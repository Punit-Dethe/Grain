#!/usr/bin/env python3
"""Divergence ratchet — guards the Grain/Handy boundary (see Upstream/UPSTREAM.md).

Guards the boundary between the Handy-derived STT core and Grain's own code:
every file under ``src-tauri/`` that also exists in upstream's tree has a
line-budget (added+removed lines vs the upstream merge base) recorded in
``budget.json``. CI fails any push that

  * grows an existing budget (feature code creeping into a Handy file), or
  * diverges a file that has no budget entry yet (new entanglement).

Budgets only ever move down without ceremony: after shrinking a diff (or
deliberately accepting a new hook), run ``python Upstream/ratchet.py --update``
and commit the tightened ``budget.json`` alongside the change.

**Measure after committing.** The diff is taken against ``HEAD``, not the
working tree, so running ``--update`` with the change still unstaged records
the *old* numbers. Commit the code first, then ``--update``, then amend or
follow up with the budget commit.

The reference is the *merge base* with ``upstream/main`` (not upstream's HEAD),
so upstream activity never shifts Grain's numbers; only Grain commits and
release close-outs (``git merge -s ours vX.Y.Z``) do. After a close-out the
budgets must be regenerated with ``--update`` as part of the sync commit.

Grain-only files (no upstream counterpart) are invisible to the ratchet —
that is the point: new features belong there.

The ratchet also flags **stray upstream files**: after the phase-7 folder move,
a merge places new upstream files inside a fully-moved directory (e.g.
``managers/``) into ``src/handy/`` automatically via directory-rename
detection, but a new file at the ``src-tauri/src/`` ROOT lands at the root
(the root was never fully renamed — ``lib.rs``/``main.rs`` and the Grain
modules still live there). Such a file must be ``git mv``'d into ``handy/``
(+ its ``#[path]`` declaration if it is a new module). Verified empirically
2026-07-20.
"""

import json
import os
import subprocess
import sys

SCOPE = "src-tauri/"
# Regenerated artifacts under merge=ours — churn there says nothing about the
# code boundary this ratchet guards.
EXCLUDE = {"src-tauri/Cargo.lock"}
# Phase 7 moved the Handy-derived tree into src-tauri/src/handy/ (declared from
# lib.rs with #[path]). Upstream still keeps those files at src-tauri/src/, so
# map our path back to upstream's before deciding whether a file is
# upstream-derived. Without this every moved file looks Grain-only and silently
# leaves the ratchet.
HANDY_DIR = "src-tauri/src/handy/"
HANDY_PREFIX = "src-tauri/src/"


def to_upstream_path(path: str) -> str:
    """Our path -> the path the same file has in upstream's tree."""
    if path.startswith(HANDY_DIR):
        return HANDY_PREFIX + path[len(HANDY_DIR) :]
    return path
BUDGET_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "budget.json")


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def measure() -> dict:
    """Divergence per file, measured blob-to-blob against the merge base.

    Deliberately NOT a `git diff base HEAD` over the tree: once phase 7 moved the
    Handy files into `src/handy/`, rename detection pairs each move as R100 and
    reports zero changed lines, which would silently retire every moved file from
    the ratchet. Comparing the two blobs by explicit path is immune to that.
    """
    base = git("merge-base", "HEAD", "upstream/main").strip()
    base_files = set(
        git("ls-tree", "-r", "--name-only", base, "--", SCOPE).splitlines()
    )
    our_files = [
        f
        for f in git("ls-tree", "-r", "--name-only", "HEAD", "--", SCOPE).splitlines()
        if f not in EXCLUDE
    ]

    current: dict[str, int] = {}
    for path in our_files:
        upstream_path = to_upstream_path(path)
        if upstream_path not in base_files:
            continue  # Grain-only file — outside the ratchet by design
        out = git(
            "diff",
            "--numstat",
            f"{base}:{upstream_path}",
            f"HEAD:{path}",
        ).strip()
        if not out:
            continue  # byte-identical to upstream
        added, removed, _ = out.split("\t", 2)
        cost = (0 if added == "-" else int(added)) + (
            0 if removed == "-" else int(removed)
        )
        if cost:
            current[path] = cost

    # A Handy file we deleted outright still counts as divergence.
    for upstream_path in base_files:
        if upstream_path in EXCLUDE:
            continue
        ours = {to_upstream_path(f) for f in our_files}
        if upstream_path not in ours:
            body = git("show", f"{base}:{upstream_path}")
            current[upstream_path + " (deleted)"] = len(body.splitlines())
    return current


# The only upstream-tree files that legitimately live OUTSIDE src/handy/: the
# composition roots stay at src/ so the crate root is Grain's.
STRAY_ALLOWED = {
    "src-tauri/src/lib.rs",
    "src-tauri/src/main.rs",
}


def strays() -> list:
    """Upstream-tree source files sitting outside src/handy/.

    After an upstream merge, a NEW file at upstream's src-tauri/src/ root lands
    at OUR src root (directory-rename detection only fires for fully-moved
    directories). Anything this finds should be `git mv`'d into src/handy/ and,
    if it is a new module, declared in lib.rs with #[path = "handy/..."].
    """
    upstream_files = set(
        git(
            "ls-tree", "-r", "--name-only", "upstream/main", "--", "src-tauri/src/"
        ).splitlines()
    )
    ours = git("ls-tree", "-r", "--name-only", "HEAD", "--", "src-tauri/src/").splitlines()
    return [
        f
        for f in ours
        if not f.startswith(HANDY_DIR) and f in upstream_files and f not in STRAY_ALLOWED
    ]


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
                f"NEW divergence: {path} ({cost} lines) - no budget entry. "
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

    for path in strays():
        failures.append(
            f"STRAY upstream file outside src/handy/: {path} - "
            f"`git mv` it into src-tauri/src/handy/ (and add its #[path] "
            f"declaration in lib.rs if it is a new module)."
        )

    for note in improvements:
        print(f"[ratchet] {note} - run ratchet.py --update to lock it in")
    if failures:
        print("\n".join(f"[ratchet] FAIL: {f}" for f in failures), file=sys.stderr)
        return 1
    print(f"[ratchet] OK: {len(current)} diverged file(s), all within budget")
    return 0


if __name__ == "__main__":
    sys.exit(main())
