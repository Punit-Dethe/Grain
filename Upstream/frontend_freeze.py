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

from review_evidence import validation_failures

SCOPE = "src/"
ALLOW_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "frontend_allow.json"
)
VERDICTS_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "verdicts.json"
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


def adopted_from_upstream() -> list:
    """Files under `src/app/` byte-identical to upstream's counterpart.

    The census above matches on PATH, which is exactly what Grain's `src/app/`
    move made it blind to: git's directory-rename detection routes upstream's
    frontend work into `src/app/...`, a path upstream does not have, so nothing
    is "shared" and the census stays at zero while upstream code walks in. That
    happened on the 2026-08-02 sync; only the type-checker noticed.

    Content is the signal instead. Most hits are legitimate -- components Grain
    inherited and has not rewritten yet -- so this is a RATCHET, not a rule: the
    set may shrink freely, and any NEW member means a file just became
    upstream's again.
    """
    # Read the files on DISK, not HEAD blobs: the check has to be usable while
    # a merge is still uncommitted, which is the moment an adoption can be
    # undone cheaply. In CI the working tree is the commit, so this covers both.
    ours = git("ls-tree", "-r", "--name-only", "HEAD", "--", "src/app").splitlines()
    hits = []
    for path in ours:
        counterpart = "src/" + path[len("src/app/") :]
        theirs = subprocess.run(
            ["git", "cat-file", "-p", f"upstream/main:{counterpart}"],
            capture_output=True,
        )
        if theirs.returncode != 0:
            continue
        try:
            with open(path, "rb") as handle:
                mine = handle.read()
        except OSError:
            continue
        # splitlines() compares content independently of line endings, which a
        # checkout can rewrite without anyone authoring anything.
        if mine.splitlines() == theirs.stdout.splitlines():
            hits.append(path)
    return sorted(hits)


def adoption_ratchet() -> int:
    allow = load_allow()
    baseline = set(allow.get("adopted", []))
    current = set(adopted_from_upstream())

    added = sorted(current - baseline)
    if added:
        for path in added:
            print(
                f"[freeze] FAIL: {path} is now byte-identical to upstream's copy "
                f"- a sync adopted upstream's frontend into src/app/. Revert it, "
                f"or run frontend_freeze.py --update if the match is deliberate.",
                file=sys.stderr,
            )
        return 1
    for path in sorted(baseline - current):
        print(f"[freeze] diverged from upstream: {path} - run --update to lock it in")
    print(
        f"[freeze] OK: {len(current)} src/app file(s) still match upstream "
        f"(ratchet: no growth)"
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


def frontend_review_audit(base: str, head: str = "HEAD") -> int:
    """Require problem-level review for every upstream UI change, including merges."""
    candidates = git("rev-list", "--reverse", f"{base}..{head}").splitlines()
    shas = [
        sha for sha in candidates
        if git(
            "diff-tree", "-m", "--no-commit-id", "--name-only", "-r", sha, "--", SCOPE
        ).strip()
    ]
    upstream_shas = [
        sha
        for sha in shas
        if subprocess.run(
            ["git", "merge-base", "--is-ancestor", sha, "upstream/main"],
            capture_output=True,
        ).returncode == 0
    ]
    with open(VERDICTS_PATH, encoding="utf-8") as handle:
        verdicts = json.load(handle)
    missing = []
    for sha in upstream_shas:
        review = (verdicts.get(sha) or {}).get("frontend_review")
        failures = validation_failures(review, os.path.dirname(os.path.dirname(__file__)))
        if failures:
            missing.append(
                (sha, git("show", "-s", "--format=%s", sha).strip(), failures)
            )
    if missing:
        for sha, subject, failures in missing:
            print(f"[freeze] FAIL: frontend problem not reviewed: {sha[:8]} {subject}")
            print(f"  review: {', '.join(failures)}")
        print(
            "[freeze] Use: verdict.py <sha> --frontend-review "
            "<adapted|already-covered|not-applicable> \"problem\" \"evidence\" [Grain paths...]"
        )
        return 1
    print(f"[freeze] OK: {len(upstream_shas)} upstream frontend review(s) recorded")
    return 0


def update_allow(accept_growth: bool) -> int:
    existing = load_allow() if os.path.exists(ALLOW_PATH) else {}
    current_shared = shared()
    current_adopted = adopted_from_upstream()
    growth = sorted(
        (set(current_shared) - set(existing.get("shared", [])))
        | (set(current_adopted) - set(existing.get("adopted", [])))
    )
    if growth and not accept_growth:
        for path in growth:
            print(f"[freeze] FAIL: refusing allowlist growth: {path}", file=sys.stderr)
        print("[freeze] Revert it or use --update --accept-growth after review.", file=sys.stderr)
        return 1
    if existing.get("strict", False) and current_shared:
        print("[freeze] FAIL: strict freeze cannot contain shared paths.", file=sys.stderr)
        return 1
    with open(ALLOW_PATH, "w", newline="\n") as f:
        json.dump(
            {
                "note": existing.get("note", "Frontend freeze allowlist."),
                "strict": existing.get("strict", False),
                "shared": current_shared,
                "adopted": current_adopted,
            }, f, indent=2,
        )
        f.write("\n")
    print(f"frontend_allow.json updated: {len(current_shared)} shared file(s)")
    return 0


def main() -> int:
    if "--update" in sys.argv:
        return update_allow("--accept-growth" in sys.argv)

    if "--sync-purity" in sys.argv:
        i = sys.argv.index("--sync-purity")
        base = sys.argv[i + 1] if len(sys.argv) > i + 1 else "main"
        return sync_purity(base)

    if "--report" in sys.argv:
        return report()

    if "--review-audit" in sys.argv:
        i = sys.argv.index("--review-audit")
        if len(sys.argv) <= i + 1:
            print("[freeze] --review-audit needs a base ref", file=sys.stderr)
            return 2
        head = sys.argv[i + 2] if len(sys.argv) > i + 2 else "HEAD"
        return frontend_review_audit(sys.argv[i + 1], head)

    rc = census()
    # Path-based and content-based checks answer different questions; the second
    # is the only one that can see upstream code arriving at a Grain-only path.
    return max(rc, adoption_ratchet())


if __name__ == "__main__":
    sys.exit(main())
