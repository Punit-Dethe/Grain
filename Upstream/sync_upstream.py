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
"""

import urllib.request
import json
import os
import subprocess
from datetime import datetime
import re

API_URL = "https://api.github.com/repos/cjpais/handy/commits?per_page=30"
# Script runs from root via GitHub Actions, or from the Upstream/ folder locally
# Determine path to data.json
script_dir = os.path.dirname(os.path.abspath(__file__))
DATA_FILE = os.path.join(script_dir, "data.json")

def fetch_upstream_commits():
    req = urllib.request.Request(API_URL)
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
        
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode())
    except Exception as e:
        print(f"Error fetching from GitHub API: {e}")
        return []

def update_data():
    try:
        with open(DATA_FILE, 'r', encoding='utf-8') as f:
            data = json.load(f)
    except FileNotFoundError:
        print(f"Could not find {DATA_FILE}. Starting fresh.")
        data = []
        
    def normalize(msg):
        # Remove PR numbers and backticks for robust matching
        clean = re.sub(r'\(#\d+\)', '', msg)
        clean = clean.replace('`', '')
        return clean.strip().lower()
        
    existing_normalized = {normalize(item['commit']) for item in data}
    
    new_commits = fetch_upstream_commits()
    added_count = 0
    
    for commit_obj in reversed(new_commits): # Reverse to add oldest first from the page
        msg = commit_obj['commit']['message'].split('\n')[0]
        date_str = commit_obj['commit']['committer']['date']
        
        # Parse date to 'Jul 09, 2026' format
        dt = datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%SZ")
        formatted_date = dt.strftime("%b %d, %Y")
        
        norm_msg = normalize(msg)
        
        if norm_msg not in existing_normalized:
            pr_match = re.search(r'\(#(\d+)\)', msg)
            pr_num = pr_match.group(1) if pr_match else ""
            
            data.append({
                'date': formatted_date,
                'commit': msg,
                'status': 'Pending',
                'notes': '',
                'pr': pr_num
            })
            existing_normalized.add(norm_msg)
            added_count += 1
            
    if added_count > 0:
        # Re-sort data by date descending just to be safe
        data.sort(key=lambda x: datetime.strptime(x['date'], "%b %d, %Y"), reverse=True)
        with open(DATA_FILE, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2)
        print(f"Added {added_count} new commits to {DATA_FILE}.")
    else:
        print("No new commits found.")

def normalize_subject(msg):
    """Subject with PR numbers/backticks stripped — the join key between the
    ledger, upstream commits, and our own git log. Adapted cherry-picks keep
    the subject even when the patch changed, so subject matching finds them
    where `git cherry` (patch-id based) cannot."""
    clean = re.sub(r"\(#\d+\)", "", msg)
    clean = clean.replace("`", "")
    return clean.strip().lower()


def git(*args):
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def check_ancestry_drift():
    """Report upstream commits that git counts as unmerged but whose work is
    already in our tree (applied by cherry-pick / by hand).

    Returns (unmerged_count, already_applied_subjects). A non-empty second
    value means: close out with `git merge -s ours upstream/main` so git stops
    replaying resolved work. See Upstream/UPSTREAM.md → "Closing out".
    """
    try:
        unmerged = [
            line.split(" ", 1)
            for line in git(
                "log", "--format=%h %s", "HEAD..upstream/main"
            ).splitlines()
            if line.strip()
        ]
    except subprocess.CalledProcessError:
        print("  (no upstream remote — skipping ancestry check)")
        return 0, []

    if not unmerged:
        return 0, []

    # Our own subjects since the merge base: a cherry-picked upstream commit
    # keeps its subject, so this finds work that landed without ancestry.
    base = git("merge-base", "HEAD", "upstream/main").strip()
    ours = {
        normalize_subject(s)
        for s in git("log", "--format=%s", f"{base}..HEAD").splitlines()
    }

    applied = [(sha, subj) for sha, subj in unmerged if normalize_subject(subj) in ours]
    return len(unmerged), applied


def report_ancestry():
    unmerged_count, applied = check_ancestry_drift()
    if not unmerged_count:
        print("Ancestry: in sync with upstream/main (0 unmerged).")
        return
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


if __name__ == "__main__":
    update_data()
    report_ancestry()
