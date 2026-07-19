#!/usr/bin/env python3
"""Shared rerere cache — every conflict resolved once, resolved forever.

git's rerere records each conflict resolution in ``.git/rr-cache`` (preimage →
postimage), and silently replays it whenever the same conflict recurs. But the
cache is per-clone: the maintainer's laptop, a fresh clone, and the CI runner
each start empty. This script makes the cache a shared, versioned asset by
mirroring it through ``Upstream/rr-cache/`` in the repo itself:

    python Upstream/rerere_cache.py restore   # repo -> .git/rr-cache (before a sync)
    python Upstream/rerere_cache.py save      # .git/rr-cache -> repo (after resolving)

Workflow: ``restore`` runs automatically in the upstream-sync CI job and should
be run once per fresh clone. After resolving conflicts in a sync, run ``save``
and commit the new entries alongside the merge — from then on, no human or bot
ever resolves that hunk again.

Requires (once per clone):
    git config rerere.enabled true
    git config rerere.autoupdate true
"""

import os
import shutil
import subprocess
import sys

REPO_ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
).stdout.strip()
GIT_DIR = subprocess.run(
    ["git", "rev-parse", "--git-dir"], capture_output=True, text=True, check=True
).stdout.strip()
if not os.path.isabs(GIT_DIR):
    GIT_DIR = os.path.join(REPO_ROOT, GIT_DIR)

LIVE = os.path.join(GIT_DIR, "rr-cache")
SHARED = os.path.join(REPO_ROOT, "Upstream", "rr-cache")


def merge_tree(src: str, dst: str) -> int:
    """Copy every resolution dir from src into dst (no overwrites needed —
    entries are content-addressed by conflict hash). Returns entries added."""
    if not os.path.isdir(src):
        return 0
    os.makedirs(dst, exist_ok=True)
    added = 0
    for entry in os.listdir(src):
        s, d = os.path.join(src, entry), os.path.join(dst, entry)
        if not os.path.isdir(s) or os.path.exists(d):
            continue
        # Only share complete resolutions (preimage alone is an unresolved
        # conflict snapshot — useless to other clones).
        if not os.path.exists(os.path.join(s, "postimage")):
            continue
        shutil.copytree(s, d)
        added += 1
    return added


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "restore":
        n = merge_tree(SHARED, LIVE)
        print(f"[rerere] restored {n} shared resolution(s) into .git/rr-cache")
    elif mode == "save":
        n = merge_tree(LIVE, SHARED)
        print(f"[rerere] saved {n} new resolution(s) into Upstream/rr-cache - commit them")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
