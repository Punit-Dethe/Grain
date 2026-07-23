"""Refresh the upstream commit ledger (data.json) and audit ancestry drift.

Two records of "what upstream work have we absorbed" must agree:

  * the LEDGER (data.json) — the human verdict per commit, rendered by
    index.html;
  * git ANCESTRY — what `git rev-list HEAD..upstream/main` believes.

They drift apart when work is applied by cherry-pick or by hand, because
neither records ancestry. The content lands, the ledger says Merged, and git
still reports us "behind" forever — replaying those commits (and their
conflicts) into every future merge. This script flags that drift so it gets
closed out with `git merge -s ours` instead of festering.

Measured 2026-07-20: four i18n commits sat applied-but-unrecorded, which is
what made every trial merge conflict on es/translation.json.

Outputs (all under Upstream/):

  data.json    the ledger — the only human-edited file (verdicts + notes)
  status.json  sync health: upstream head, behind count, trial-merge result
  data.js      both of the above baked into a script, so index.html opens
               straight off the filesystem (file:// forbids fetch())
"""

import urllib.request
import json
import os
import subprocess
from datetime import datetime, timezone
import re

REPO = "cjpais/handy"
# Pull the ledger in pages: a single 30-commit page silently dropped every
# commit past the 30th whenever upstream landed a burst between runs, and
# nothing ever went back for them.
PER_PAGE = 100
MAX_PAGES = 10
# Script runs from root via GitHub Actions, or from the Upstream/ folder locally
script_dir = os.path.dirname(os.path.abspath(__file__))
DATA_FILE = os.path.join(script_dir, "data.json")
STATUS_FILE = os.path.join(script_dir, "status.json")
BUNDLE_FILE = os.path.join(script_dir, "data.js")


def fetch_page(page):
    url = f"https://api.github.com/repos/{REPO}/commits?per_page={PER_PAGE}&page={page}"
    req = urllib.request.Request(url)
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")

    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode())


def commit_ts(commit_obj):
    return commit_obj["commit"]["committer"]["date"]


def fetch_upstream_commits(floor_ts):
    """Newest-first upstream commits back to `floor_ts`, paging as needed.

    A single 30-commit page silently dropped everything past the 30th whenever
    upstream landed a burst between runs, and nothing ever went back for them.
    Paging fixes that — but the walk must stop at the oldest commit the ledger
    already holds, or it would keep reaching further back and import all of
    Handy's history, which this tracker was never meant to cover.
    """
    collected = []
    for page in range(1, MAX_PAGES + 1):
        try:
            batch = fetch_page(page)
        except Exception as e:
            print(f"Error fetching page {page} from GitHub API: {e}")
            break
        if not batch:
            break
        collected.extend(batch)
        if len(batch) < PER_PAGE:
            break
        # This page reached past the start of the ledger — everything older is
        # out of scope, so there is nothing left to catch up on.
        if floor_ts and any(commit_ts(c) < floor_ts for c in batch):
            break
        # Fresh ledger: seed from one page rather than the whole history.
        if not floor_ts:
            break
    return collected


def normalize(msg):
    """Subject with PR numbers/backticks stripped — the join key between the
    ledger, upstream commits, and our own git log. Adapted cherry-picks keep
    the subject even when the patch changed, so subject matching finds them
    where `git cherry` (patch-id based) cannot.

    It is NOT unique, though: upstream reuses subjects like "update catalog"
    and "bump tauri global shortcut". Deduplication keys on SHA for exactly
    that reason; subjects are only a fallback for pre-SHA ledger rows.
    """
    clean = re.sub(r"\(#\d+\)", "", msg)
    clean = clean.replace("`", "")
    return clean.strip().lower()


# Kept as an alias: check_ancestry_drift() and the docs both refer to it.
normalize_subject = normalize


def sort_key(item):
    """Sort by full timestamp when we have one. Legacy rows only carry a
    day-granularity date, which scrambled same-day ordering on every re-sort."""
    ts = item.get("ts")
    if ts:
        return ts
    try:
        day = datetime.strptime(item["date"], "%b %d, %Y")
    except (KeyError, ValueError):
        return ""
    return day.strftime("%Y-%m-%dT00:00:00Z")


def pop_legacy(index, key):
    """Claim a pre-SHA ledger row under `key`, if one is still unclaimed."""
    if not key:
        return None
    rows = index.get(key)
    while rows:
        row = rows.pop(0)
        if not row.get("sha"):  # a subject lookup may re-offer a PR-matched row
            return row
    return None


def is_ancestor(sha):
    """True when `sha` is already in our history (arrived through a merge)."""
    try:
        return (
            subprocess.run(
                ["git", "merge-base", "--is-ancestor", sha, "HEAD"],
                capture_output=True,
            ).returncode
            == 0
        )
    except FileNotFoundError:
        return False


def update_data():
    try:
        with open(DATA_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
    except FileNotFoundError:
        print(f"Could not find {DATA_FILE}. Starting fresh.")
        data = []

    known_shas = {item["sha"] for item in data if item.get("sha")}
    # Rows written before SHAs were recorded can only be matched by content.
    # Match them once, backfill the SHA, and they join the SHA-keyed path.
    # The PR number is the stronger key: hand-written rows abbreviated long
    # subjects ("...events leaking memory... (#1447)"), which no amount of
    # subject normalising will match.
    legacy_by_pr = {}
    legacy_by_subject = {}
    for item in data:
        if item.get("sha"):
            continue
        if item.get("pr"):
            legacy_by_pr.setdefault(item["pr"], []).append(item)
        legacy_by_subject.setdefault(normalize(item["commit"]), []).append(item)

    # The ledger starts where it starts; commits older than its oldest row
    # predate tracking and must never be pulled in.
    floor_ts = min((sort_key(item) for item in data), default="")

    new_commits = fetch_upstream_commits(floor_ts)
    if not new_commits:
        print("No upstream commits returned; leaving the ledger untouched.")
        return data

    added_count = 0
    backfilled = 0

    for commit_obj in reversed(new_commits):  # oldest first
        sha = commit_obj["sha"]
        if sha in known_shas:
            continue

        date_str = commit_ts(commit_obj)
        if floor_ts and date_str < floor_ts:
            continue

        msg = commit_obj["commit"]["message"].split("\n")[0]
        dt = datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%SZ")

        pr_match = re.search(r"\(#(\d+)\)", msg)
        pr_num = pr_match.group(1) if pr_match else ""

        row = pop_legacy(legacy_by_pr, pr_num) or pop_legacy(
            legacy_by_subject, normalize(msg)
        )
        if row is not None:
            row["sha"] = sha
            row["ts"] = date_str
            # The upstream subject is authoritative; abbreviated hand-written
            # ones are what made this row hard to match in the first place.
            row["commit"] = msg
            known_shas.add(sha)
            backfilled += 1
            continue

        entry = {
            "date": dt.strftime("%b %d, %Y"),
            "ts": date_str,
            "sha": sha,
            "commit": msg,
            "status": "Pending",
            "notes": "",
            "pr": pr_num,
        }
        # A commit already in our history needs no review — it arrived through
        # a merge. Defaulting those to Pending inflates the review queue with
        # work that is provably done.
        if is_ancestor(sha):
            entry["status"] = "Merged"
            entry["notes"] = "Absorbed by an upstream merge (in our ancestry)."
        data.append(entry)
        known_shas.add(sha)
        added_count += 1

    if added_count or backfilled:
        data.sort(key=sort_key, reverse=True)
        with open(DATA_FILE, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
        print(
            f"Ledger: +{added_count} new commit(s), "
            f"{backfilled} legacy row(s) matched to a SHA."
        )
    else:
        print("Ledger: no new commits found.")

    return data


def git(*args):
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def check_ancestry_drift(recorded_shas):
    """Report upstream commits that git counts as unmerged but whose work is
    already in our tree (applied by cherry-pick / by hand).

    Returns (unmerged_count, already_applied_subjects). A non-empty second
    value means: close out with `git merge -s ours upstream/main` so git stops
    replaying resolved work. See Upstream/UPSTREAM.md → "Closing out".
    """
    try:
        unmerged = [
            line.split(" ", 1)
            for line in git("log", "--format=%h %s", "HEAD..upstream/main").splitlines()
            if line.strip()
        ]

        if not unmerged:
            return 0, []

        # Our own subjects since the merge base: a cherry-picked upstream commit
        # keeps its subject, so this finds work that landed without ancestry.
        base = git("merge-base", "HEAD", "upstream/main").strip()
        ours = {
            normalize(s) for s in git("log", "--format=%s", f"{base}..HEAD").splitlines()
        }
    except (subprocess.CalledProcessError, FileNotFoundError):
        # No upstream remote (fresh clone, or a local run) — the ledger is
        # still valid, so never let this take the whole job down.
        print("  (no upstream remote — skipping ancestry check)")
        return 0, []

    applied = [
        (sha, subj)
        for sha, subj in unmerged
        if normalize(subj) in ours and sha not in recorded_shas
    ]
    return len(unmerged), applied


def report_ancestry(ledger):
    recorded_shas = {
        item["sha"][:8]
        for item in ledger
        if item.get("sha") and item.get("status") != "Pending"
    }
    unmerged_count, applied = check_ancestry_drift(recorded_shas)
    if not unmerged_count:
        print("Ancestry: in sync with upstream/main (0 unmerged).")
        return unmerged_count, applied
    print(f"Ancestry: {unmerged_count} upstream commit(s) not in our history.")
    if applied:
        # ASCII only: this runs on the Windows console (cp1252), where a stray
        # arrow or warning glyph raises UnicodeEncodeError and kills the job.
        print(
            f"  WARNING: {len(applied)} of them are ALREADY APPLIED here "
            f"(cherry-picked - same subject, no ancestry):"
        )
        for sha, subj in applied:
            print(f"      {sha} {subj}")
        print(
            "  -> Verify the content, then record it:\n"
            "        git merge -s ours upstream/main\n"
            "     Until then git replays these commits - and their conflicts -\n"
            "     into every merge. See Upstream/UPSTREAM.md."
        )
    return unmerged_count, applied


def load_status():
    try:
        with open(STATUS_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def write_status(unmerged_count, applied):
    """Merge the ancestry audit into status.json (the trial-merge step writes
    the rest). This is what tells the dashboard whether tracking is actually
    keeping up, rather than only what verdicts were recorded."""
    status = load_status()
    status["behind"] = unmerged_count
    status["drift"] = [{"sha": sha, "commit": subj} for sha, subj in applied]
    status["checked_at"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    with open(STATUS_FILE, "w", encoding="utf-8") as f:
        json.dump(status, f, indent=2)
    return status


def write_bundle(data, status):
    """Bake the ledger into a plain script.

    index.html prefers fetch('data.json'), but browsers refuse fetch() on a
    file:// origin — opening the dashboard by double-clicking it showed only
    "Couldn't reach data.json". A <script> tag has no such restriction, so
    this sidecar is what makes the page work off the filesystem.
    """
    with open(BUNDLE_FILE, "w", encoding="utf-8") as f:
        f.write("// Generated by Upstream/sync_upstream.py — do not edit.\n")
        f.write("// Lets Upstream/index.html open directly from disk (file://),\n")
        f.write("// where the browser blocks fetch('data.json').\n")
        f.write("window.UPSTREAM_DATA = ")
        json.dump(data, f, indent=2)
        f.write(";\n")
        f.write("window.UPSTREAM_STATUS = ")
        json.dump(status, f, indent=2)
        f.write(";\n")


if __name__ == "__main__":
    ledger = update_data()
    unmerged_count, applied = report_ancestry(ledger)
    status = write_status(unmerged_count, applied)
    write_bundle(ledger, status)
