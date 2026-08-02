#!/usr/bin/env python3
"""Drive an upstream merge, resolving the conflicts that have only one answer.

Roughly half of every sync's conflict list is not a decision. Grain froze the
frontend on 2026-07-31 and moved it to `src/app/`; upstream keeps developing its
own `src/`. Git sees upstream editing files we deleted and raises a
modify/delete conflict for each one — `src/App.tsx`, `src/bindings.ts`,
`src/styles/theme.css`, and so on. Every sync. Forever. And the answer is always
the same one AGENTS.md already gives: never merge upstream frontend changes.

Resolving those by hand each time is not judgement, it is transcription, and it
buries the handful of conflicts that DO need a decision in noise. So this script
applies the written policy mechanically and hands back only the real ones.

    python Upstream/merge_upstream.py            # start the merge
    ...resolve what it lists, then:
    python Upstream/merge_upstream.py --finish   # verify + re-baseline budgets

`--dry-run` reports what would happen without touching anything.

What is deliberately NOT automated: anything in `src-tauri/`, `crates/` or the
repo root. Those conflicts encode real decisions about Grain's divergence from
Handy, and a script that guessed at them would be trading a visible merge
conflict for an invisible behaviour change.
"""

from __future__ import annotations

import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from upstream_ref import ensure_fresh  # noqa: E402

#: Grain owns everything under here; upstream's version is never taken.
FROZEN_PREFIX = "src/"
#: ...except Grain's own tree, which upstream has no counterpart for.
FROZEN_EXEMPT = "src/app/"


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


def working_tree_dirty() -> bool:
    return bool(git("status", "--porcelain").stdout.strip())


def in_merge() -> bool:
    return os.path.exists(os.path.join(ROOT, ".git", "MERGE_HEAD"))


def start(dry_run: bool) -> int:
    if ensure_fresh() is None:
        return 1

    behind = git("rev-list", "--count", "HEAD..upstream/main").stdout.strip()
    if behind == "0":
        print("[merge] already up to date with upstream/main")
        return 0
    print(f"[merge] {behind} commit(s) to merge")

    if dry_run:
        print("[merge] --dry-run: not starting a merge")
        return 0

    if in_merge():
        print("[merge] a merge is already in progress — resolve it or `git merge --abort`")
        return 1
    if working_tree_dirty():
        print("[merge] working tree is dirty; commit or stash first")
        return 1

    git("merge", "--no-commit", "--no-ff", "upstream/main")

    conflicts = conflicted_paths()
    frozen = [p for p in conflicts if is_frozen_frontend(p)]
    real = [p for p in conflicts if not is_frozen_frontend(p)]

    for path in frozen:
        # `git rm` covers both shapes this takes: upstream modified a file we
        # deleted, and upstream added one under a path we no longer use.
        git("rm", "--force", "--quiet", path)
    if frozen:
        print(f"[merge] auto-resolved {len(frozen)} frozen-frontend conflict(s) by discarding upstream's copy:")
        for path in frozen:
            print(f"          {path}")

    if real:
        print(f"\n[merge] {len(real)} conflict(s) need a decision:")
        for path in real:
            print(f"          {path}")
        print("\n[merge] Upstream/UPSTREAM-DIVERGENCE.md says which side is authoritative.")
        print("[merge] When they are resolved:  python Upstream/merge_upstream.py --finish")
        return 2

    print("\n[merge] no conflicts left. Finish with:")
    print("[merge]   python Upstream/merge_upstream.py --finish")
    return 0


def finish() -> int:
    remaining = conflicted_paths()
    if remaining:
        print(f"[merge] {len(remaining)} conflict(s) still unresolved:")
        for path in remaining:
            print(f"          {path}")
        return 1

    if in_merge():
        print("[merge] conflicts resolved — commit the merge, then re-run --finish")
        print("[merge]   git commit --no-edit")
        return 2

    # Budgets are measured against HEAD, so this only means anything after the
    # merge commit exists.
    print("[merge] re-baselining divergence budgets")
    result = subprocess.run(
        [sys.executable, "Upstream/ratchet.py", "--update", "--no-fetch"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    print("  " + (result.stdout or result.stderr).strip())

    print("[merge] running preflight")
    check = subprocess.run(
        [sys.executable, "Upstream/preflight.py", "--committed", "--no-fetch"],
        cwd=ROOT,
    )
    if check.returncode != 0:
        return 1

    print("\n[merge] Done. Remaining by hand:")
    print("  - commit the re-baselined Upstream/budget.json")
    print("  - log the sync in Upstream/UPSTREAM.md")
    print("  - record any NEW divergence in Upstream/UPSTREAM-DIVERGENCE.md")
    return 0


def main() -> int:
    if "--help" in sys.argv or "-h" in sys.argv:
        print(__doc__)
        return 0
    if "--finish" in sys.argv:
        return finish()
    return start("--dry-run" in sys.argv)


if __name__ == "__main__":
    sys.exit(main())
