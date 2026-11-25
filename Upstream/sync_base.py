#!/usr/bin/env python3
"""Where Grain last synced with Handy, read from a commit-message marker.

Grain does not share git history with Handy. Each upstream sync lands as ONE
ordinary commit whose message records the Handy commit it synced to:

    sync(upstream): Handy 1a2b3c4d (12 commits)

    Upstream-Synced: <full 40-char Handy sha>

That line is the whole contract. Every tool that used to ask git for
`merge-base HEAD upstream/main` asks `sync_base()` instead, and "is this
upstream commit already in Grain?" becomes "is it an ancestor of the sync
base in Handy's own history?" (Handy's commits are fetched from the `upstream`
remote; they are simply never reachable from Grain's branches.)

Why a marker instead of shared history: a real merge makes every Handy commit
part of Grain's log, which credits Handy's contributors with Grain's history
and Grain with theirs. One squashed commit per sync keeps the attribution
honest in both directions while every merge stays a true three-way merge
against the recorded base.

    python Upstream/sync_base.py          # print the sync point
    python Upstream/sync_base.py --sha    # just the sha (for scripts and CI)
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

MARKER = "Upstream-Synced"
_MARKER_RE = re.compile(rf"(?im)^{MARKER}:[ \t]*([0-9a-f]{{40}})[ \t]*$")


class SyncBaseError(RuntimeError):
    """The sync point cannot be determined; every caller must stop."""


def _git(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if check and result.returncode != 0:
        raise SyncBaseError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result


def is_ancestor(older: str, newer: str) -> bool:
    return _git("merge-base", "--is-ancestor", older, newer, check=False).returncode == 0


def marker_line(sha: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise SyncBaseError(f"a sync marker needs a full 40-char sha, got {sha!r}")
    return f"{MARKER}: {sha}"


def parse_markers(message: str) -> list[str]:
    """The Handy shas a commit message records, in order."""
    return [sha.lower() for sha in _MARKER_RE.findall(message)]


def markers(rev: str = "HEAD") -> list[tuple[str, str]]:
    """Every (grain_commit, handy_sha) marker reachable from `rev`, newest first."""
    out = _git(
        "log", rev, "-E", f"--grep=^{MARKER}:", "--format=%H%x00%B%x01"
    ).stdout
    found = []
    for record in out.split("\x01"):
        commit, _, body = record.strip("\n").partition("\x00")
        if not commit:
            continue
        found.extend((commit.strip(), sha) for sha in parse_markers(body))
    return found


def sync_point(rev: str = "HEAD") -> tuple[str, str]:
    """(grain_commit, handy_sha) of the latest sync reachable from `rev`.

    "Latest" is decided in Handy's history, not by commit date: the winning
    sha must descend from every other recorded one. Two markers that diverge
    mean two syncs disagree about what Grain contains — that is refused, never
    guessed at.
    """
    found = markers(rev)
    if not found:
        raise SyncBaseError(
            f"no '{MARKER}:' marker reachable from {rev}. Every Grain history "
            "must record its Handy sync point - see Upstream/UPSTREAM.md."
        )
    unique: dict[str, str] = {}
    for commit, sha in found:
        unique.setdefault(sha, commit)  # newest Grain commit for each sha
    for sha in unique:
        if _git("cat-file", "-e", f"{sha}^{{commit}}", check=False).returncode != 0:
            raise SyncBaseError(
                f"sync point {sha[:12]} is not in this clone - run: git fetch upstream"
            )
    best = next(iter(unique))
    for sha in unique:
        if sha != best and is_ancestor(best, sha):
            best = sha
    stray = [sha for sha in unique if sha != best and not is_ancestor(sha, best)]
    if stray:
        raise SyncBaseError(
            f"sync markers diverge: {best[:12]} does not contain "
            + ", ".join(s[:12] for s in stray)
        )
    return unique[best], best


def sync_base(rev: str = "HEAD") -> str:
    """The Handy commit Grain last synced to (as seen from `rev`)."""
    return sync_point(rev)[1]


def absorbed(rev: str = "HEAD") -> set[str]:
    """Every Handy commit already contained in Grain at `rev`."""
    return set(_git("rev-list", sync_base(rev)).stdout.split())


def main() -> int:
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    rev = args[0] if args else "HEAD"
    try:
        commit, sha = sync_point(rev)
    except SyncBaseError as error:
        print(f"[sync-base] {error}", file=sys.stderr)
        return 1
    if "--sha" in sys.argv:
        print(sha)
        return 0
    subject = _git("log", "-1", "--format=%s", sha, check=False).stdout.strip()
    print(f"[sync-base] Handy {sha[:12]}  {subject}")
    print(f"[sync-base] recorded by {commit[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
