#!/usr/bin/env python3
"""Guarantee `upstream/main` is current before anything measures against it.

Every upstream tool here answers a question of the form "how do we compare to
upstream?" — how far behind, which files diverge, what a merge would conflict
on. All of them read the *remote-tracking ref* `upstream/main`, which is a local
cache that only changes when somebody runs `git fetch`.

That cache going stale is silent and total. Nothing warns you; the tools keep
answering confidently, just about a version of upstream from days ago.

This is not hypothetical. On 2026-08-02 a sync merged 22 commits and every check
reported "0 behind upstream" — while `upstream/main` had last been fetched on
2026-07-28. Sixteen commits that already existed upstream were invisible to the
merge, to the ratchet, and to the divergence audit, and the sync was declared
complete. The CI dashboard, which fetches on every run, correctly said 16
pending; the discrepancy looked like a broken dashboard rather than a real gap.

So freshness is not checked here, it is *enforced*: fetch first, every time. A
fetch with nothing to bring down costs a few KB and removes the entire class of
error. Measuring staleness and warning about it would leave the same trap open
for anyone who skims the warning.

Usage from a tool:

    from upstream_ref import ensure_fresh
    ensure_fresh()          # fetches, or explains why the answer may be stale

CI passes `--no-fetch` (the workflow has already fetched); offline runs degrade
to a loud, specific warning rather than a silent wrong answer.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import time

REMOTE = "upstream"
BRANCH = "main"
REF = f"refs/remotes/{REMOTE}/{BRANCH}"

#: Age past which an un-fetched ref is called out by name in the warning.
STALE_AFTER_SECONDS = 30 * 60


def _git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def ref_exists() -> bool:
    return (
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", REF],
            capture_output=True,
        ).returncode
        == 0
    )


def last_fetch_epoch() -> int | None:
    """When the remote-tracking ref last moved, from its reflog.

    Returns `None` when the ref has no reflog — a fresh clone, or a repo where
    reflogs for remote refs are disabled.
    """
    out = _git("reflog", "show", "--date=raw", "-n", "1", REF, check=False)
    match = re.search(r"@\{(\d+)", out)
    return int(match.group(1)) if match else None


def describe_age() -> str:
    epoch = last_fetch_epoch()
    if epoch is None:
        return "unknown (no reflog for the ref)"
    age = max(0, int(time.time()) - epoch)
    if age < 90:
        return "just now"
    if age < 3600:
        return f"{age // 60} minutes ago"
    if age < 86400:
        return f"{age // 3600} hours ago"
    return f"{age // 86400} days ago"


def ensure_fresh(*, allow_fetch: bool | None = None, quiet: bool = False) -> str | None:
    """Fetch `upstream/main`, then return its sha.

    `allow_fetch` defaults to True, and to False when `GRAIN_UPSTREAM_NO_FETCH`
    is set or `--no-fetch` is on the command line — CI has already fetched, and
    a second fetch per tool would be waste, not safety.

    Returns the resolved sha, or `None` when the ref does not exist at all (no
    `upstream` remote configured), which callers should treat as fatal.
    """
    if allow_fetch is None:
        allow_fetch = not (
            os.environ.get("GRAIN_UPSTREAM_NO_FETCH") or "--no-fetch" in sys.argv
        )

    if allow_fetch:
        before = _git("rev-parse", REF, check=False).strip() if ref_exists() else ""
        fetched = subprocess.run(
            ["git", "fetch", REMOTE, BRANCH, "--quiet"],
            capture_output=True,
            text=True,
        )
        if fetched.returncode != 0:
            # Offline is a legitimate state to work in; a wrong number reported
            # as fact is not. Say exactly how old the answer is.
            print(
                f"[upstream] WARNING: could not fetch {REMOTE}/{BRANCH} "
                f"({fetched.stderr.strip().splitlines()[-1] if fetched.stderr.strip() else 'unknown error'}).\n"
                f"[upstream] Everything below is measured against a ref last "
                f"updated {describe_age()} and may be wrong.",
                file=sys.stderr,
            )
        else:
            after = _git("rev-parse", REF, check=False).strip()
            if before and after != before and not quiet:
                moved = _git(
                    "rev-list", "--count", f"{before}..{after}", check=False
                ).strip()
                print(f"[upstream] fetched: {REMOTE}/{BRANCH} advanced {moved} commit(s)")
    elif not quiet:
        age = describe_age()
        note = " - STALE" if _is_stale() else ""
        print(f"[upstream] not fetching (last updated {age}){note}")

    if not ref_exists():
        print(
            f"[upstream] ERROR: {REF} does not exist. Add the remote first:\n"
            f"[upstream]   git remote add {REMOTE} https://github.com/cjpais/Handy.git",
            file=sys.stderr,
        )
        return None

    return _git("rev-parse", REF).strip()


def _is_stale() -> bool:
    epoch = last_fetch_epoch()
    return epoch is not None and (time.time() - epoch) > STALE_AFTER_SECONDS


def main() -> int:
    sha = ensure_fresh()
    if sha is None:
        return 1
    behind = _git("rev-list", "--count", f"HEAD..{REF}").strip()
    ahead = _git("rev-list", "--count", f"{REF}..HEAD").strip()
    subject = _git("log", "-1", "--format=%s", REF).strip()
    print(f"[upstream] {REMOTE}/{BRANCH} = {sha[:8]}  {subject}")
    print(f"[upstream] behind {behind}, ahead {ahead}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
