#!/usr/bin/env python3
"""Fail-closed close-out for already-assessed upstream commits.

Moves the recorded sync point (`sync_base.py`) forward without changing a
single file: for Handy commits whose work Grain already has (cherry-picked,
ported by hand) or deliberately declined. Default is read-only. ``--execute``
records an empty commit carrying ``Upstream-Synced: <sha>`` only when every
commit in ``<sync point>..<target>`` has an explicit Merged/Ignored verdict,
the working tree is clean, preflight passes, and the target belongs to
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

from sync_base import SyncBaseError, marker_line, sync_base  # noqa: E402
from upstream_ref import ensure_fresh  # noqa: E402


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace"
    )


def main() -> int:
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    if not args or "--help" in sys.argv:
        print(
            "python Upstream/closeout.py <upstream-tag-or-sha> "
            "[--execute] [--accept-growth]"
        )
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
    try:
        base = sync_base()
    except SyncBaseError as error:
        print(f"[closeout] FAIL: {error}", file=sys.stderr)
        return 1
    if base == target_sha or git("merge-base", "--is-ancestor", base, target_sha).returncode != 0:
        print(f"[closeout] FAIL: {target} is not past the sync point {base[:8]}", file=sys.stderr)
        return 1
    commits = git("rev-list", "--reverse", f"{base}..{target_sha}").stdout.splitlines()
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
    semantic_gates = [
        [sys.executable, "Upstream/frontend_freeze.py", "--review-audit", base, target_sha],
        [sys.executable, "Upstream/suppressed_review.py", base, target_sha],
    ]
    semantic_results = [subprocess.run(argv, cwd=ROOT).returncode for argv in semantic_gates]
    if not all(result == 0 for result in semantic_results):
        print(
            "[closeout] FAIL: frozen/suppressed upstream changes lack semantic review",
            file=sys.stderr,
        )
        return 1
    print(f"[closeout] OK: {len(commits)} commit(s) explicitly assessed through {target}")
    if "--execute" not in sys.argv:
        print("[closeout] read-only check complete; add --execute to move the sync point")
        return 0
    if git("status", "--porcelain").stdout.strip():
        print("[closeout] FAIL: working tree is dirty", file=sys.stderr)
        return 1
    preflight = subprocess.run(
        [
            sys.executable,
            "Upstream/preflight.py",
            "--committed",
            "--no-fetch",
            "--skip-ratchet",
        ],
        cwd=ROOT,
    )
    if preflight.returncode != 0:
        print("[closeout] FAIL: preflight failed; sync point not moved", file=sys.stderr)
        return 1
    before_tree = git("rev-parse", "HEAD^{tree}").stdout.strip()
    recorded = git(
        "commit", "--allow-empty",
        "-m", f"chore(upstream): close out Handy {target_sha[:8]}",
        "-m", f"Every Handy commit through {target_sha[:12]} is assessed; nothing to apply.",
        "-m", marker_line(target_sha),
    )
    if recorded.returncode != 0:
        print(recorded.stderr, file=sys.stderr)
        return 1
    after_tree = git("rev-parse", "HEAD^{tree}").stdout.strip()
    if before_tree != after_tree:
        print("[closeout] FAIL: close-out changed the tree; stop and inspect", file=sys.stderr)
        return 1
    ratchet_args = [sys.executable, "Upstream/ratchet.py", "--update", "--no-fetch"]
    if "--accept-growth" in sys.argv:
        ratchet_args.append("--accept-growth")
    update = subprocess.run(ratchet_args, cwd=ROOT)
    if update.returncode != 0:
        print(
            "[closeout] sync point moved, but budget growth needs semantic review; "
            "budget.json was not changed.", file=sys.stderr
        )
        return 1
    print("[closeout] sync point moved; tree unchanged; budgets tightened")
    return 0


if __name__ == "__main__":
    sys.exit(main())
