#!/usr/bin/env python3
"""One command that answers "is the upstream relationship healthy?".

Before this, the answer lived in four scripts that had to be run in the right
order, with the right flags, at the right moment in the commit cycle. Running
three of them and forgetting the fourth looked exactly like running all four.

    python Upstream/preflight.py             # before committing
    python Upstream/preflight.py --committed # after committing (what CI runs)

What it checks, in the order that matters:

  1. `upstream/main` is actually current. Everything below is measured against
     it, so a stale ref makes every later check confidently wrong. This is the
     failure that let a sync report "0 behind" while 16 commits were missing.
  2. How far behind we are, and whether a merge would conflict.
  3. The divergence ratchet — feature code creeping into the Handy tree.
  4. The divergence audit — ancestry drift and stale "converged" claims.
  5. The frontend freeze — files whose path is shared with upstream's src/.

Exit code is 0 only when everything passes. Informational findings (commits
pending, a merge that would conflict) are reported but do NOT fail the run:
being behind upstream is a normal state, not a broken one.
"""

from __future__ import annotations

import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from upstream_ref import ensure_fresh  # noqa: E402


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    ).stdout


def run_gate(label: str, argv: list[str]) -> bool:
    result = subprocess.run(
        [sys.executable, *argv],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    lines = [ln for ln in (result.stdout or "").strip().splitlines() if ln.strip()]
    tail = lines[-1] if lines else (result.stderr or "").strip()[:200]
    print(f"  {'PASS' if result.returncode == 0 else 'FAIL'}  {label}: {tail}")
    if result.returncode != 0 and len(lines) > 1:
        for line in lines[:-1][-12:]:
            print(f"        {line}")
    return result.returncode == 0


def trial_merge_conflicts() -> list[str] | None:
    """Files a merge with upstream/main would conflict on.

    Uses `merge-tree`, which computes the merge in memory — the working tree and
    the index are never touched, so this is safe to run with uncommitted work.
    Returns `None` when git is too old for the `--write-tree` form.
    """
    out = subprocess.run(
        ["git", "merge-tree", "--write-tree", "--name-only", "HEAD", "upstream/main"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if out.returncode == 0:
        return []  # clean
    if "unknown option" in (out.stderr or "") or "usage:" in (out.stderr or "").lower():
        return None
    # Format: tree oid, then the conflicted paths, then a BLANK LINE, then
    # git's own "Auto-merging ..." / "CONFLICT ..." chatter. Only the block
    # before the blank line is the answer; reading past it inflates the count
    # with every file that merged cleanly.
    lines = (out.stdout or "").splitlines()
    conflicted: list[str] = []
    for line in lines[1:]:
        if not line.strip():
            break
        conflicted.append(line.strip())
    return conflicted


def main() -> int:
    committed = "--committed" in sys.argv
    skip_ratchet = "--skip-ratchet" in sys.argv
    print("=== upstream preflight ===")

    if ensure_fresh() is None:
        return 1

    behind = git("rev-list", "--count", "HEAD..upstream/main").strip() or "?"
    ahead = git("rev-list", "--count", "upstream/main..HEAD").strip() or "?"
    tip = git("log", "-1", "--format=%h %s", "upstream/main").strip()
    print(f"  upstream/main {tip}")
    print(f"  behind {behind}, ahead {ahead}")

    if behind not in ("0", "?"):
        conflicts = trial_merge_conflicts()
        if conflicts is None:
            print("  (trial merge skipped: git too old for merge-tree --write-tree)")
        elif conflicts:
            print(f"  a merge would conflict in {len(conflicts)} file(s):")
            for path in conflicts[:20]:
                print(f"        {path}")
            if len(conflicts) > 20:
                print(f"        ... and {len(conflicts) - 20} more")
        else:
            print("  a merge would apply cleanly")

    print("  --")
    ratchet = ["Upstream/ratchet.py", "--no-fetch"]
    if not committed:
        ratchet.append("--worktree")
    ok = [
        run_gate("divergence audit", ["Upstream/audit_divergence.py", "--check", "--no-fetch"]),
        run_gate("frontend freeze", ["Upstream/frontend_freeze.py"]),
        run_gate("port audit", ["Upstream/port_audit.py", "--no-fetch"]),
        run_gate("policy consistency", ["Upstream/policy_check.py"]),
    ]
    if skip_ratchet:
        print("  SKIP  ratchet: close-out moves the merge base; checked after ancestry")
    else:
        ok.insert(0, run_gate("ratchet", ratchet))

    if all(ok):
        print("=== preflight OK ===")
        return 0
    print("=== preflight FAILED ===")
    return 1


if __name__ == "__main__":
    sys.exit(main())
