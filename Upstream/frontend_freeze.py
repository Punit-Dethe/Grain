#!/usr/bin/env python3
"""Frontend freeze — guards Grain's UI against upstream drift.

Companion to ``ratchet.py``. The ratchet guards the *backend* boundary, where
Grain deliberately keeps merging Handy's code. This guards the *frontend*, where
Grain deliberately stopped: since the UI 2.0 freeze, ``src/`` is Grain-owned
(``src/** merge=ours`` in .gitattributes) and upstream's React tree is no longer
a base we build on. See ``docs/UI 2.0/PLAN.md``.

``merge=ours`` alone is NOT a freeze — it only wins *conflicts*:

  * a file upstream modified that Grain never touched merges cleanly, so
    upstream's version lands silently;
  * a brand-new upstream file under ``src/`` is an add, not a conflict, so it
    lands too.

So the freeze needs a guard, and this is it. Two checks:

**Census** (default). ``shared = upstream's src/ files ∩ ours``. It may shrink,
never grow. A new shared path means an upstream file landed in our tree — delete
it, or (if it is genuinely wanted) run ``--update`` and say why in the commit.
The set shrinks to zero as UI 2.0 replaces the last Handy-derived screens.

**Sync purity** (``--sync-purity``). On the auto-sync branch, a merge should
change nothing under ``src/``. While the census is non-empty this is a warning,
because an upstream edit to a still-shared file we never touched can legitimately
land; blocking the sync PR over it would stall *backend* intake for a frontend
reason. Flip ``"strict": true`` in frontend_allow.json once the census reaches
zero — from then on any frontend change in a sync is a hard failure.

``--report`` lists upstream commits touching ``src/`` since the merge base, so a
reviewer can spot the rare one carrying backend knowledge worth porting to Rust
by hand (see the plan's §3 principle: facts about the machine belong in the
engine, screens are ours).
"""

import json
import os
import subprocess
import sys

SCOPE = "src/"
ALLOW_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "frontend_allow.json"
)


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def tree(ref: str) -> set:
    return set(git("ls-tree", "-r", "--name-only", ref, "--", SCOPE).splitlines())


def shared() -> list:
    """Files that exist in BOTH upstream's tree and ours, under src/."""
    return sorted(tree("upstream/main") & tree("HEAD"))


def load_allow() -> dict:
    with open(ALLOW_PATH) as f:
        return json.load(f)


def census() -> int:
    allow = load_allow()
    baseline = set(allow["shared"])
    current = set(shared())

    added = sorted(current - baseline)
    gone = sorted(baseline - current)

    for path in gone:
        print(f"[freeze] released: {path} - run frontend_freeze.py --update to lock it in")
    if added:
        print(
            "\n".join(
                f"[freeze] FAIL: upstream file landed under src/: {p} - the frontend "
                f"is frozen (docs/UI 2.0/PLAN.md). Delete it, or run "
                f"frontend_freeze.py --update and justify it in the commit."
                for p in added
            ),
            file=sys.stderr,
        )
        return 1
    print(
        f"[freeze] OK: {len(current)} file(s) still shared with upstream "
        f"(target: 0 at UI 2.0 cutover)"
    )
    return 0


def sync_purity(base: str) -> int:
    """A sync merge must not change the frontend."""
    changed = [
        f
        for f in git("diff", "--name-only", f"{base}...HEAD", "--", SCOPE).splitlines()
        if f.strip()
    ]
    if not changed:
        print("[freeze] OK: this merge changes nothing under src/")
        return 0

    strict = load_allow().get("strict", False)
    label = "FAIL" if strict else "WARN"
    print(
        f"[freeze] {label}: this merge changes {len(changed)} frontend file(s):\n"
        + "\n".join(f"    {f}" for f in changed)
        + "\n  The frontend is frozen. Review each one — upstream content should "
        "not be entering src/.",
        file=sys.stderr if strict else sys.stdout,
    )
    return 1 if strict else 0


def report() -> int:
    base = git("merge-base", "HEAD", "upstream/main").strip()
    log = git(
        "log", "--oneline", f"{base}..upstream/main", "--", SCOPE
    ).strip()
    if not log:
        print("[freeze] no upstream frontend commits since the merge base")
        return 0
    print("[freeze] upstream commits touching src/ since the merge base:")
    print("\n".join(f"    {line}" for line in log.splitlines()))
    print(
        "  These are NOT taken. If one carries backend knowledge (host "
        "capabilities, locale resolution, permissions), port it into Rust by "
        "hand and record it:\n"
        '    python Upstream/verdict.py <sha> --note "frontend frozen; ported to <file>"'
    )
    return 0


def main() -> int:
    if "--update" in sys.argv:
        existing = load_allow() if os.path.exists(ALLOW_PATH) else {}
        current = shared()
        with open(ALLOW_PATH, "w", newline="\n") as f:
            json.dump(
                {
                    "note": "Files still shared with upstream under src/. May shrink, never grow. See docs/UI 2.0/PLAN.md.",
                    "strict": existing.get("strict", False),
                    "shared": current,
                },
                f,
                indent=2,
            )
            f.write("\n")
        print(f"frontend_allow.json updated: {len(current)} shared file(s)")
        return 0

    if "--sync-purity" in sys.argv:
        i = sys.argv.index("--sync-purity")
        base = sys.argv[i + 1] if len(sys.argv) > i + 1 else "main"
        return sync_purity(base)

    if "--report" in sys.argv:
        return report()

    return census()


if __name__ == "__main__":
    sys.exit(main())
