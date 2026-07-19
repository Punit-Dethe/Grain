#!/usr/bin/env python3
"""Divergence ratchet — Handy Isolation phase 5.

Guards the boundary between the Handy-derived STT core and Grain's own code:
every file under ``src-tauri/`` that also exists in upstream's tree has a
line-budget (added+removed lines vs the upstream merge base) recorded in
``budget.json``. CI fails any push that

  * grows an existing budget (feature code creeping into a Handy file), or
  * diverges a file that has no budget entry yet (new entanglement).

Budgets only ever move down without ceremony: after shrinking a diff (or
deliberately accepting a new hook), run ``python "Handy Isolation/ratchet.py"
--update`` and commit the tightened ``budget.json`` alongside the change.

The reference is the *merge base* with ``upstream/main`` (not upstream's HEAD),
so upstream activity never shifts Grain's numbers; only Grain commits and
release close-outs (``git merge -s ours vX.Y.Z``) do. After a close-out the
budgets must be regenerated with ``--update`` as part of the sync commit.

Grain-only files (no upstream counterpart) are invisible to the ratchet —
that is the point: new features belong there.
"""

import json
import os
import subprocess
import sys

SCOPE = "src-tauri/"
# Regenerated artifacts under merge=ours — churn there says nothing about the
# code boundary this ratchet guards.
EXCLUDE = {"src-tauri/Cargo.lock"}
BUDGET_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "budget.json")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def measure() -> dict:
    base = git("merge-base", "HEAD", "upstream/main").strip()
    upstream_files = set(
        git("ls-tree", "-r", "--name-only", "upstream/main", "--", SCOPE).splitlines()
    )
    current: dict[str, int] = {}
    for line in git("diff", "--numstat", base, "HEAD", "--", SCOPE).splitlines():
        added, removed, path = line.split("\t", 2)
        if path not in upstream_files or path in EXCLUDE:
            continue  # Grain-only file — outside the ratchet by design
        cost = (0 if added == "-" else int(added)) + (
            0 if removed == "-" else int(removed)
        )
        if cost:
            current[path] = cost
    return current


def main() -> int:
    current = measure()

    if "--update" in sys.argv:
        with open(BUDGET_PATH, "w", newline="\n") as f:
            json.dump(dict(sorted(current.items())), f, indent=2)
            f.write("\n")
        print(f"budget.json updated: {len(current)} file(s) carry divergence")
        return 0

    with open(BUDGET_PATH) as f:
        budget = json.load(f)

    failures = []
    improvements = []
    for path, cost in sorted(current.items()):
        allowed = budget.get(path)
        if allowed is None:
            failures.append(
                f"NEW divergence: {path} ({cost} lines) — no budget entry. "
                f"Move the change into a Grain-owned module, or (for a deliberate "
                f"hook) run ratchet.py --update and justify it in the commit."
            )
        elif cost > allowed:
            failures.append(
                f"GREW: {path} {allowed} -> {cost} lines vs merge base. "
                f"Feature code belongs outside the Handy-derived core."
            )
        elif cost < allowed:
            improvements.append(f"shrunk: {path} {allowed} -> {cost}")

    for path in sorted(set(budget) - set(current)):
        improvements.append(f"converged: {path} {budget[path]} -> 0")

    for note in improvements:
        print(f"[ratchet] {note} — run ratchet.py --update to lock it in")
    if failures:
        print("\n".join(f"[ratchet] FAIL: {f}" for f in failures), file=sys.stderr)
        return 1
    print(f"[ratchet] OK: {len(current)} diverged file(s), all within budget")
    return 0


if __name__ == "__main__":
    sys.exit(main())
