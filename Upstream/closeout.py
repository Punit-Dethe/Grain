#!/usr/bin/env python3
"""Fail-closed ancestry close-out for already-assessed upstream commits.

Default is read-only. ``--execute`` creates the ``-s ours`` merge only when
every commit in ``HEAD..<ref>`` has an explicit Merged/Ignored verdict, the
working tree is clean, preflight passes, and the target belongs to
``upstream/main``. The tool verifies the tree hash did not change afterwards.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
VERDICTS_PATH = os.path.join(HERE, "verdicts.json")
sys.path.insert(0, HERE)

from upstream_ref import ensure_fresh  # noqa: E402


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace"
    )


def main() -> int:
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    if not args or "--help" in sys.argv:
        print("python Upstream/closeout.py <upstream-tag-or-sha> [--execute]")
        return 0 if "--help" in sys.argv else 2
    target = args[0]
    if ensure_fresh() is None:
        return 1
    resolved = git("rev-parse", "--verify", f"{target}^{{commit}}")
    if resolved.returncode != 0:
        print(f"[closeout] FAIL: cannot resolve {target}", file=sys.stderr)
        return 1
    target_sha = resolved.stdout.strip()
    if git("merge-base", "--is-ancestor", target_sha, "upstream/main").returncode != 0:
        print(f"[closeout] FAIL: {target} is not in upstream/main", file=sys.stderr)
        return 1
    commits = git("rev-list", "--reverse", f"HEAD..{target_sha}").stdout.splitlines()
    with open(VERDICTS_PATH, encoding="utf-8") as handle:
        verdicts = json.load(handle)
    unassessed = [
        sha for sha in commits
        if (verdicts.get(sha) or {}).get("status") not in {"Merged", "Ignored"}
    ]
    if unassessed:
        print(
            f"[closeout] FAIL: {len(unassessed)} commit(s) lack an explicit "
            "Merged/Ignored verdict:", file=sys.stderr
        )
        for sha in unassessed:
            subject = git("show", "-s", "--format=%s", sha).stdout.strip()
            print(f"  {sha[:8]} {subject}", file=sys.stderr)
        return 1
    print(f"[closeout] OK: {len(commits)} commit(s) explicitly assessed through {target}")
    if "--execute" not in sys.argv:
        print("[closeout] read-only check complete; add --execute to record ancestry")
        return 0
    if git("status", "--porcelain").stdout.strip():
        print("[closeout] FAIL: working tree is dirty", file=sys.stderr)
        return 1
    preflight = subprocess.run(
        [sys.executable, "Upstream/preflight.py", "--committed", "--no-fetch"],
        cwd=ROOT,
    )
    if preflight.returncode != 0:
        print("[closeout] FAIL: preflight failed; no merge created", file=sys.stderr)
        return 1
    before_tree = git("rev-parse", "HEAD^{tree}").stdout.strip()
    merged = git(
        "merge", "--no-ff", "-s", "ours", target_sha,
        "-m", f"chore(upstream): close out {target}"
    )
    if merged.returncode != 0:
        print(merged.stderr, file=sys.stderr)
        return 1
    after_tree = git("rev-parse", "HEAD^{tree}").stdout.strip()
    if before_tree != after_tree:
        print("[closeout] FAIL: ours merge changed the tree; stop and inspect", file=sys.stderr)
        return 1
    update = subprocess.run(
        [sys.executable, "Upstream/ratchet.py", "--update", "--no-fetch"], cwd=ROOT
    )
    if update.returncode != 0:
        print(
            "[closeout] ancestry recorded, but budget growth needs semantic review; "
            "budget.json was not changed.", file=sys.stderr
        )
        return 1
    print("[closeout] ancestry recorded; tree unchanged; budgets tightened")
    return 0


if __name__ == "__main__":
    sys.exit(main())
