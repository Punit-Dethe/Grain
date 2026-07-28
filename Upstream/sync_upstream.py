"""Rebuild the upstream commit ledger and audit ancestry drift.

The ledger is DERIVED, never accumulated. Every run recomputes it from three
inputs:

  * upstream's commit list (GitHub API, back to TRACKING_FLOOR_TS);
  * our own git ancestry — a commit reachable from HEAD arrived through a
    merge, so it is Merged whether or not anyone wrote that down;
  * verdicts.json — the one human-owned file: the notes, and the statuses
    ancestry cannot infer (Ignored, or work applied by cherry-pick).

That makes the output a pure function of its inputs: idempotent, impossible to
drift, and nothing to "regenerate". The previous design hand-edited data.json
and separately generated data.js from it, so the two silently disagreed
whenever a verdict was recorded without re-running this script.

None of the outputs are committed — they are gitignored build products,
regenerated in CI and published to Pages by the same job. See UPSTREAM.md.

Outputs (all under Upstream/, all derived):

  data.json    the ledger index.html renders
  status.json  sync health: upstream head, behind count, trial-merge result
  data.js      both of the above baked into a script, so index.html opens
               straight off the filesystem (file:// forbids fetch())
"""

import urllib.request
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
import re

REPO = "cjpais/handy"
# Where tracking begins. Commits older than this predate the tracker and are
# out of scope — without a floor the walk would reach back through all of
# Handy's history. It used to be inferred from the oldest row of a committed
# data.json; now that the ledger is derived, the floor has to be a stated fact.
TRACKING_FLOOR_TS = "2026-06-11T00:50:36Z"
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
VERDICTS_FILE = os.path.join(script_dir, "verdicts.json")

ANCESTRY_NOTE = "Absorbed by an upstream merge (in our ancestry)."


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
    Paging fixes that; the floor is what stops the walk before it imports all
    of Handy's history.

    The ledger is rebuilt rather than accumulated, so a walk that stops short
    of the floor does not merely miss new commits — it drops rows that were on
    the board yesterday. That has to be an error, never a quiet truncation.
    """
    collected = []
    reached_floor = False
    for page in range(1, MAX_PAGES + 1):
        try:
            batch = fetch_page(page)
        except Exception as e:
            print(f"Error fetching page {page} from GitHub API: {e}")
            break
        if not batch:
            reached_floor = True  # upstream has no more history to walk
            break
        collected.extend(batch)
        # This page reached past the floor — everything older is out of scope.
        if any(commit_ts(c) < floor_ts for c in batch):
            reached_floor = True
            break
        if len(batch) < PER_PAGE:
            reached_floor = True
            break

    if collected and not reached_floor:
        raise SystemExit(
            f"Walked {MAX_PAGES} pages ({len(collected)} commits) without reaching "
            f"TRACKING_FLOOR_TS ({TRACKING_FLOOR_TS}) — the board would silently "
            f"lose its oldest rows. Raise MAX_PAGES, or move the floor forward "
            f"once those commits are closed out."
        )
    return collected


def normalize(msg):
    """Subject with PR numbers/backticks stripped — the join key between our
    git log and upstream's commits. Adapted cherry-picks keep the subject even
    when the patch changed, so subject matching finds them where `git cherry`
    (patch-id based) cannot.

    It is NOT unique — upstream reuses subjects like "update catalog" — which
    is why the ledger itself keys on SHA and this is confined to drift
    detection.
    """
    clean = re.sub(r"\(#\d+\)", "", msg)
    clean = clean.replace("`", "")
    return clean.strip().lower()


# Kept as an alias: check_ancestry_drift() and the docs both refer to it.
normalize_subject = normalize


def git(*args):
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def load_verdicts():
    """The human overrides: {sha: {"status"?: str, "notes"?: str}}.

    `status` is omitted whenever ancestry already implies it, so the file only
    ever holds what a human actually decided.
    """
    try:
        with open(VERDICTS_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        print(f"No {os.path.basename(VERDICTS_FILE)} — every verdict from ancestry.")
        return {}


def ancestry_shas():
    """Every commit reachable from HEAD.

    One rev-list beats a `git merge-base --is-ancestor` per commit (that was
    80+ subprocesses a run). A failure here is fatal on purpose: with an empty
    set every row would file as Pending, and publishing that wipes the board.
    """
    try:
        return set(git("rev-list", "HEAD").split())
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        raise SystemExit(f"Cannot read git ancestry ({e}) — refusing to build a ledger.")


def build_ledger(commits, ancestry, verdicts):
    """One row per upstream commit, verdict resolved in three steps.

    A human verdict outranks ancestry: a commit can sit in our history because
    a merge carried it while we deliberately kept our own version of the file,
    and `Ignored` says so. Ancestry then fills in everything nobody wrote down
    — which is what makes a finished sync show up on the dashboard with no
    bookkeeping at all.
    """
    rows = []
    for commit_obj in commits:
        sha = commit_obj["sha"]
        ts = commit_ts(commit_obj)
        if ts < TRACKING_FLOOR_TS:
            continue

        msg = commit_obj["commit"]["message"].split("\n")[0]
        # Backports carry both numbers — "…prompt (#1261) (#1310)". The last is
        # the PR that actually landed it upstream, which is what the dashboard
        # should link to; taking the first pointed at a closed fork PR.
        prs = re.findall(r"\(#(\d+)\)", msg)
        override = verdicts.get(sha, {})
        in_ancestry = sha in ancestry

        status = override.get("status") or ("Merged" if in_ancestry else "Pending")
        notes = override.get("notes")
        if notes is None:
            notes = ANCESTRY_NOTE if in_ancestry else ""

        rows.append(
            {
                "date": datetime.strptime(ts, "%Y-%m-%dT%H:%M:%SZ").strftime(
                    "%b %d, %Y"
                ),
                "ts": ts,
                "sha": sha,
                "commit": msg,
                "status": status,
                "notes": notes,
                "pr": prs[-1] if prs else "",
            }
        )

    rows.sort(key=lambda r: r["ts"], reverse=True)
    return rows


def check_ancestry_drift(recorded_shas):
    """Report upstream commits that git counts as unmerged but whose work is
    already in our tree (applied by cherry-pick / by hand).

    Returns (unmerged_count, already_applied_subjects, checked). A non-empty
    second value means: close out with `git merge -s ours upstream/main` so git
    stops replaying resolved work. See Upstream/UPSTREAM.md → "Closing out".
    """
    try:
        unmerged = [
            line.split(" ", 1)
            for line in git("log", "--format=%h %s", "HEAD..upstream/main").splitlines()
            if line.strip()
        ]

        if not unmerged:
            return 0, [], True

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
        return 0, [], False

    applied = [
        (sha, subj)
        for sha, subj in unmerged
        if normalize(subj) in ours and sha not in recorded_shas
    ]
    return len(unmerged), applied, True


def report_ancestry(ledger):
    recorded_shas = {
        item["sha"][:8]
        for item in ledger
        if item.get("sha") and item.get("status") != "Pending"
    }
    unmerged_count, applied, checked = check_ancestry_drift(recorded_shas)
    if not checked:
        return unmerged_count, applied, checked
    if not unmerged_count:
        print("Ancestry: in sync with upstream/main (0 unmerged).")
        return unmerged_count, applied, checked
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
    return unmerged_count, applied, checked


def load_status():
    try:
        with open(STATUS_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def write_status(unmerged_count, applied, checked):
    """Merge the ancestry audit into status.json (the trial-merge step writes
    the rest). This is what tells the dashboard whether tracking is actually
    keeping up, rather than only what verdicts were recorded."""
    status = load_status()
    # A local run without the upstream remote must not overwrite CI's real
    # behind-count with a fabricated zero.
    if checked:
        status["behind"] = unmerged_count
        status["drift"] = [{"sha": sha, "commit": subj} for sha, subj in applied]
    status["generated_at"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    # Kept under its old name too: an already-deployed dashboard reads this.
    status["checked_at"] = status["generated_at"]
    run_url = None
    server, repo, run_id = (
        os.environ.get("GITHUB_SERVER_URL"),
        os.environ.get("GITHUB_REPOSITORY"),
        os.environ.get("GITHUB_RUN_ID"),
    )
    if server and repo and run_id:
        run_url = f"{server}/{repo}/actions/runs/{run_id}"
    status["run_url"] = run_url
    with open(STATUS_FILE, "w", encoding="utf-8") as f:
        json.dump(status, f, indent=2)
    return status


def write_outputs(data, status):
    """Write the ledger and its offline twin from the same objects.

    index.html prefers fetch('data.json'), but browsers refuse fetch() on a
    file:// origin — opening the dashboard by double-clicking it showed only
    "Couldn't reach data.json". A <script> tag has no such restriction, so
    data.js is what makes the page work off the filesystem. Writing both here,
    from one in-memory ledger, is what keeps them from disagreeing.
    """
    with open(DATA_FILE, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
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


def refresh():
    """Rebuild every derived output. Returns the ledger."""
    verdicts = load_verdicts()
    ancestry = ancestry_shas()

    commits = fetch_upstream_commits(TRACKING_FLOOR_TS)
    if not commits:
        # Publishing an empty board would look exactly like "upstream went
        # quiet", so this has to be loud rather than silent.
        raise SystemExit(
            "GitHub returned no upstream commits — refusing to publish an empty "
            "ledger. Check the API status or GITHUB_TOKEN and re-run."
        )

    ledger = build_ledger(commits, ancestry, verdicts)
    pending = sum(1 for r in ledger if r["status"] == "Pending")
    print(
        f"Ledger: {len(ledger)} commit(s) tracked, {pending} pending, "
        f"{len(verdicts)} human verdict(s) applied."
    )

    unmerged_count, applied, checked = report_ancestry(ledger)
    status = write_status(unmerged_count, applied, checked)
    write_outputs(ledger, status)
    return ledger


if __name__ == "__main__":
    if "--help" in sys.argv or "-h" in sys.argv:
        print(__doc__)
        sys.exit(0)
    refresh()
