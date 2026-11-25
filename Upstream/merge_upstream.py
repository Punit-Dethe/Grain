#!/usr/bin/env python3
"""Drive an upstream sync, resolving the conflicts that have only one answer.

A sync is a true three-way merge against the recorded sync point
(`sync_base.py`), committed as ONE single-parent commit carrying
`Upstream-Synced: <handy sha>`. Handy's commits never become part of Grain's
history. To get git's full merge machinery (rename detection, rerere, merge
drivers) against a base the history does not contain, HEAD is given the base
as an extra parent through a temporary `git replace --graft` for the duration
of `git merge --squash` only; the replacement is deleted before this script
returns, and the commit that follows records only the real HEAD as parent.

Roughly half of every sync's conflict list is not a decision. Grain froze the
frontend on 2026-07-31 and moved it to `src/app/`; upstream keeps developing its
own `src/`. Git sees upstream editing files we deleted and raises a
modify/delete conflict for each one — `src/App.tsx`, `src/bindings.ts`,
`src/styles/theme.css`, and so on. Every sync. Forever. And the answer is always
the same one AGENTS.md already gives: never merge upstream frontend changes.

Resolving those by hand each time is not judgement, it is transcription, and it
buries the handful of conflicts that DO need a decision in noise. So this script
applies the written policy mechanically and hands back only the real ones.

    python Upstream/merge_upstream.py            # start the sync
    ...resolve what it lists, then:
    python Upstream/merge_upstream.py --finish   # commit, verify, re-baseline budgets

`--dry-run` reports what would happen without touching anything. `--commit`
records the sync commit without the review gates (CI's auto-sync PR, whose
reviews happen on the PR).

What is deliberately NOT automated: anything in `src-tauri/`, `crates/` or the
repo root. Those conflicts encode real decisions about Grain's divergence from
Handy, and a script that guessed at them would be trading a visible merge
conflict for an invisible behaviour change.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from sync_base import (  # noqa: E402
    SyncBaseError,
    marker_line,
    parse_markers,
    sync_base,
)
from upstream_ref import ensure_fresh  # noqa: E402

#: Grain owns everything under here; upstream's version is never taken.
FROZEN_PREFIX = "src/"
#: ...except Grain's own tree, which upstream has no counterpart for.
FROZEN_EXEMPT = "src/app/"
UPSTREAM_URL = "https://github.com/cjpais/handy"


def git(*args: str, check: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=check,
    )


def conflicted_paths() -> list[str]:
    out = git("diff", "--name-only", "--diff-filter=U").stdout
    return [ln.strip() for ln in out.splitlines() if ln.strip()]


def is_frozen_frontend(path: str) -> bool:
    return path.startswith(FROZEN_PREFIX) and not path.startswith(FROZEN_EXEMPT)


def exists_at(ref: str, path: str) -> bool:
    return git("cat-file", "-e", f"{ref}:{path}").returncode == 0


def frontend_adopted_from_upstream(target: str = "upstream/main") -> list[str]:
    """Frontend paths this sync changed that upstream, not Grain, authored.

    A path counts when the sync touched it AND the corresponding upstream path
    exists — `src/app/components/X.tsx` against upstream's
    `src/components/X.tsx`. That mapping is exactly what rename detection used
    to place the change, so it is also what identifies it.
    """
    changed = git("diff", "--name-only", "HEAD", "--", "src/").stdout.splitlines()
    adopted = []
    for path in (p.strip() for p in changed if p.strip()):
        upstream_path = path.replace(FROZEN_EXEMPT, FROZEN_PREFIX, 1) if path.startswith(FROZEN_EXEMPT) else path
        probe = git("cat-file", "-e", f"{target}:{upstream_path}")
        if probe.returncode == 0:
            adopted.append(path)
    return adopted


def working_tree_dirty() -> bool:
    return bool(git("status", "--porcelain").stdout.strip())


def in_merge() -> bool:
    # A linked worktree has a .git file, not a directory.
    return git("rev-parse", "--quiet", "--verify", "MERGE_HEAD").returncode == 0


def squash_msg_path() -> str:
    path = git("rev-parse", "--git-path", "SQUASH_MSG").stdout.strip()
    return path if os.path.isabs(path) else os.path.join(ROOT, path)


def pending_sync() -> str | None:
    """The Handy sha of an uncommitted sync, from the prepared SQUASH_MSG."""
    try:
        with open(squash_msg_path(), encoding="utf-8") as handle:
            body = handle.read()
    except OSError:
        return None
    found = parse_markers(body)
    return found[-1] if found else None


def resolve_target(target: str) -> str:
    resolved = git("rev-parse", "--verify", "--end-of-options", f"{target}^{{commit}}")
    if resolved.returncode or not resolved.stdout.strip():
        raise ValueError(f"not a commit: {target}")
    sha = resolved.stdout.strip()
    if git("merge-base", "--is-ancestor", sha, "upstream/main").returncode != 0:
        raise ValueError(f"target is not in fetched Handy main history: {target}")
    return sha


def sync_message(base: str, target: str) -> str:
    """Subject, the absorbed Handy subjects (credit by link, not by authorship),
    and the marker every tool reads the sync point from."""
    log = git("log", "--no-merges", "--format=  %h %s", f"{base}..{target}").stdout.rstrip()
    count = git("rev-list", "--count", "--no-merges", f"{base}..{target}").stdout.strip()
    return (
        f"sync(upstream): Handy {target[:8]} ({count} commits)\n\n"
        f"Brings in Handy ({UPSTREAM_URL}) changes {base[:12]}..{target[:12]}:\n\n"
        f"{log}\n\n"
        f"{marker_line(target)}\n"
    )


def squash_merge(base: str, target: str) -> subprocess.CompletedProcess:
    """`git merge --squash target`, three-way against `base`.

    When `base` is not in HEAD's history, HEAD is grafted onto it for the one
    git call, so merge-base resolves to the recorded sync point. The graft is
    a local replace ref and is always deleted again: it never reaches a commit
    (the squash commit's parent is HEAD's real sha) and never outlives the call.
    """
    head = git("rev-parse", "HEAD", check=True).stdout.strip()
    grafted = git("merge-base", "--is-ancestor", base, head).returncode != 0
    if grafted:
        parents = git("rev-list", "--parents", "-n", "1", head, check=True).stdout.split()[1:]
        graft = git("replace", "-f", "--graft", head, *parents, base)
        if graft.returncode:
            raise RuntimeError(f"could not graft the sync base: {graft.stderr.strip()}")
    try:
        # Reused rerere content is review input, never an auto-staged decision.
        # Force this even on clones carrying the old autoupdate=true setup.
        return git("-c", "rerere.autoupdate=false", "merge", "--squash", target)
    finally:
        if grafted:
            git("replace", "-d", head)


def start(dry_run: bool, target: str = "upstream/main") -> int:
    if ensure_fresh() is None:
        return 1
    try:
        target_sha = resolve_target(target)
        base = sync_base()
    except (ValueError, SyncBaseError) as error:
        print(f"[sync] {error}", file=sys.stderr)
        return 1
    if git("merge-base", "--is-ancestor", base, target_sha).returncode != 0:
        print(f"[sync] {target} does not contain the sync point {base[:8]}", file=sys.stderr)
        return 1

    behind = git("rev-list", "--count", f"{base}..{target_sha}").stdout.strip()
    if behind == "0":
        print(f"[sync] already synced through {target}")
        return 0
    print(f"[sync] {behind} Handy commit(s) from {base[:8]} through {target_sha[:8]}")

    if dry_run:
        print("[sync] --dry-run: not starting a sync")
        return 0

    if in_merge() or pending_sync():
        print("[sync] a merge or sync is already in progress - finish it, or abandon it with `git reset --hard`")
        return 1
    if working_tree_dirty():
        print("[sync] working tree is dirty; commit or stash first")
        return 1

    try:
        merged = squash_merge(base, target_sha)
    except RuntimeError as error:
        print(f"[sync] {error}", file=sys.stderr)
        return 1
    conflicts = conflicted_paths()
    if merged.returncode not in (0, 1) or (merged.returncode == 1 and not conflicts):
        print(f"[sync] could not start the sync: {merged.stderr.strip()}", file=sys.stderr)
        return 1
    with open(squash_msg_path(), "w", encoding="utf-8", newline="\n") as handle:
        handle.write(sync_message(base, target_sha))

    frozen = [p for p in conflicts if is_frozen_frontend(p)]
    real = [p for p in conflicts if not is_frozen_frontend(p)]

    for path in frozen:
        # `git rm` covers both shapes this takes: upstream modified a file we
        # deleted, and upstream added one under a path we no longer use.
        git("rm", "--force", "--quiet", path)
    if frozen:
        print(f"[sync] auto-resolved {len(frozen)} frozen-frontend conflict(s) by discarding upstream's copy:")
        for path in frozen:
            print(f"          {path}")

    # Conflicts are only half of it. Git's directory-rename detection saw
    # src/ -> src/app/ and helpfully routes upstream's frontend work INTO Grain's
    # tree: new files land at src/app/..., and edits to files that moved are
    # applied silently with no conflict at all. Neither shows up as a conflict,
    # and the freeze census cannot see them either — it matches on paths shared
    # with upstream's src/, and src/app/... is not one. So they arrive unnoticed.
    # This is what put upstream's slider-reset props and two translation files
    # into src/app/ on the 08-02 sync; the build caught it, but only by luck.
    adopted = frontend_adopted_from_upstream(target_sha)
    for path in adopted:
        if exists_at("HEAD", path):
            git("checkout", "HEAD", "--", path)  # our version wins
        else:
            git("rm", "--force", "--quiet", path)
    if adopted:
        print(f"[sync] reverted {len(adopted)} upstream frontend change(s) routed into src/app/:")
        for path in adopted:
            print(f"          {path}")

    if real:
        print(f"\n[sync] {len(real)} conflict(s) need a decision:")
        for path in real:
            print(f"          {path}")
        print("\n[sync] Upstream/UPSTREAM-DIVERGENCE.md says which side is authoritative.")
        print("[sync] When they are resolved:  python Upstream/merge_upstream.py --finish")
        return 2

    print("\n[sync] no conflicts left. Finish with:")
    print("[sync]   python Upstream/merge_upstream.py --finish")
    return 0


def commit_sync() -> int:
    """Record the prepared sync as one single-parent commit."""
    target = pending_sync()
    if not target:
        print("[sync] no sync in progress")
        return 1
    remaining = conflicted_paths()
    if remaining:
        print(f"[sync] {len(remaining)} conflict(s) still unresolved:")
        for path in remaining:
            print(f"          {path}")
        return 1
    # --allow-empty: a sync whose every change was declined (frozen frontend,
    # Grain-owned paths) still moves the sync point; that review happened.
    # Committing also lets rerere record the resolutions it saw.
    result = git("commit", "--allow-empty", "-F", squash_msg_path())
    if result.returncode != 0:
        print(f"[sync] commit failed: {(result.stderr or result.stdout).strip()}", file=sys.stderr)
        return 1
    if os.path.exists(squash_msg_path()):
        os.remove(squash_msg_path())
    print(f"[sync] recorded the sync to Handy {target[:8]}")
    return 0


def head_sync_range() -> tuple[str, str] | None:
    """(previous, new) sync points when HEAD is a sync commit, else None."""
    parents = git("rev-list", "--parents", "-n", "1", "HEAD").stdout.split()
    if len(parents) != 2:
        return None
    head_markers = parse_markers(git("log", "-1", "--format=%B", "HEAD").stdout)
    if not head_markers:
        return None
    try:
        previous = sync_base("HEAD^")
        new = sync_base("HEAD")
    except SyncBaseError as error:
        print(f"[sync] {error}", file=sys.stderr)
        return None
    if new != head_markers[-1] or new == previous:
        return None
    return previous, new


def finish() -> int:
    if pending_sync():
        committed = commit_sync()
        if committed:
            return committed
    elif conflicted_paths() or in_merge():
        print("[sync] a merge is in progress that this script did not start")
        return 1

    sync_range = head_sync_range()
    if sync_range is None:
        print("[sync] HEAD is not a sync commit; refusing --finish")
        return 1
    previous, new = sync_range

    frontend_review = subprocess.run(
        [sys.executable, "Upstream/frontend_freeze.py", "--review-audit", previous, new],
        cwd=ROOT,
    )
    if frontend_review.returncode != 0:
        print("[sync] frontend knowledge review incomplete; budgets were NOT changed")
        return 1

    print("[sync] running preflight before changing any baseline")
    check = subprocess.run(
        [sys.executable, "Upstream/preflight.py", "--committed", "--no-fetch"],
        cwd=ROOT,
    )
    if check.returncode != 0:
        print("[sync] gates failed; Upstream/budget.json was NOT changed")
        return 1

    print("[sync] gates passed; tightening divergence budgets")
    result = subprocess.run(
        [sys.executable, "Upstream/ratchet.py", "--update", "--no-fetch"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    print("  " + (result.stdout or result.stderr).strip())
    if result.returncode != 0:
        return 1

    print("\n[sync] Done. Remaining by hand:")
    print("  - commit the re-baselined Upstream/budget.json")
    print("  - log the sync in Upstream/UPSTREAM.md")
    print("  - record any NEW divergence in Upstream/UPSTREAM-DIVERGENCE.md")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="upstream/main", help="Tested batch boundary in fetched Handy main history")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--finish", action="store_true")
    parser.add_argument("--commit", action="store_true", help="Record the prepared sync commit without the review gates")
    parser.add_argument("--no-fetch", action="store_true", help="Use an already-fetched upstream ref")
    args = parser.parse_args()
    if args.finish or args.commit:
        if args.target != "upstream/main" or args.dry_run or (args.finish and args.commit):
            parser.error("--finish/--commit take no other mode flags")
        return finish() if args.finish else commit_sync()
    return start(args.dry_run, args.target)


if __name__ == "__main__":
    sys.exit(main())
