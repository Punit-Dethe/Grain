#!/usr/bin/env python3
"""Divergence audit — checks the *claims* in UPSTREAM-DIVERGENCE.md against real blobs.

The ratchet ([ratchet.py](ratchet.py)) measures divergence against the **merge
base**, which is the right yardstick for "is feature code creeping into the
Handy tree". But it is blind to two things that make syncs harder:

  1. **Ancestry drift** — a file byte-identical to `upstream/main` that is still
     billed, because the change was applied by hand/cherry-pick instead of
     merged. Git does not know we have it, so it replays it (and re-raises its
     conflict) into every future merge. `handy/tray_i18n.rs` was exactly this
     when this script was written: identical blob, 60 lines of phantom budget,
     and a delete/modify conflict every sync because the file also moved and
     similarity fell to 36% — under git's 50% rename threshold.

  2. **Stale documentation** — UPSTREAM-DIVERGENCE.md claiming a file is
     "converged (byte-identical)" long after it stopped being so. Four files
     were mis-filed that way when this script was written, one of them carrying
     a real 179-line bug fix.

Both are documentation problems, not code problems, which is why nothing caught
them. This script is the check. It is read-only and needs `upstream/main`
fetched.

    python Upstream/audit_divergence.py           # report
    python Upstream/audit_divergence.py --check   # exit 1 on drift/stale claims
"""

import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from upstream_ref import ensure_fresh  # noqa: E402

HANDY_DIR = "src-tauri/src/handy/"
HANDY_PREFIX = "src-tauri/src/"
HERE = os.path.dirname(os.path.abspath(__file__))
BUDGET_PATH = os.path.join(HERE, "budget.json")
MAP_PATH = os.path.join(HERE, "UPSTREAM-DIVERGENCE.md")

# Only source-ish trees. Binary rebrand assets (icons, tray PNGs) diverge by
# design and say nothing about mergeability.
SCOPES = ["src-tauri/", "scripts/", ".nix/", "package.json", ".gitignore"]
SKIP = re.compile(r"(icons/|resources/.*\.png$|\.icns$|\.ico$|Cargo\.lock$)")


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True).stdout


def to_upstream(path):
    """Our path -> the path the same file has in upstream's tree."""
    if path.startswith(HANDY_DIR):
        return HANDY_PREFIX + path[len(HANDY_DIR):]
    return path


def tree(ref, scope):
    out = {}
    for line in git("ls-tree", "-r", ref, "--", scope).splitlines():
        meta, path = line.split("\t", 1)
        sha = meta.split()[2]
        if not SKIP.search(path):
            out[path] = sha
    return out


def converged_claims():
    """Files UPSTREAM-DIVERGENCE.md claims are byte-identical to upstream."""
    claimed = set()
    try:
        with open(MAP_PATH, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return claimed
    for line in text.splitlines():
        if not line.startswith("|") or "onverged" not in line:
            continue
        # A row documenting a file as NOT converged is the correction, not a
        # claim. Without this, fixing a stale row keeps its warning alive and
        # the check could never go green.
        if re.search(r"\bnot converged\b", line, re.IGNORECASE):
            continue
        # Only the row's FIRST cell names files; later cells are prose that may
        # mention other paths in passing.
        for name in re.findall(r"`([^`]+)`", line.split("|")[1]):
            if name.endswith((".rs", ".json", ".toml", ".js")):
                claimed.add(name)
    return claimed


def main():
    # Blob-for-blob against `upstream/main` — a stale ref audits the wrong
    # upstream and can report convergence that no longer holds.
    if ensure_fresh(quiet=True) is None:
        return 1
    if not git("rev-parse", "--verify", "--quiet", "upstream/main").strip():
        print("upstream/main not found — run: git fetch upstream", file=sys.stderr)
        return 2

    base = git("merge-base", "HEAD", "upstream/main").strip()
    ours, up, base_t = {}, {}, {}
    for scope in SCOPES:
        ours.update(tree("HEAD", scope))
        up.update(tree("upstream/main", scope))
        base_t.update(tree(base, scope))

    mapped = {to_upstream(p): sha for p, sha in ours.items()}
    rev = {to_upstream(p): p for p in ours}
    budget = json.load(open(BUDGET_PATH))

    drift, real, deleted = [], [], []
    for upath, usha in sorted(up.items()):
        if upath not in mapped:
            if upath in base_t:
                deleted.append(upath)
            continue
        opath = rev[upath]
        if mapped[upath] == usha:
            # Identical to upstream's CURRENT tip. Billed anyway => the change
            # is in our tree but not our ancestry.
            if opath in budget:
                drift.append((opath, budget[opath]))
        elif mapped[upath] != base_t.get(upath):
            real.append(opath)

    claimed = converged_claims()
    stale = sorted(
        name for name in claimed
        if any(p.endswith("/" + name) or p == name for p in real)
    )

    print(f"merge base {base[:8]} | {len(up)} upstream files in scope")
    print(f"  diverged from upstream/main by our own edits : {len(real)}")
    print(f"  deleted by us                                : {len(deleted)}")
    print()

    if drift:
        print("ANCESTRY DRIFT - byte-identical to upstream/main, still billed:")
        for path, cost in drift:
            print(f"  {path}  (budget {cost})")
        print("  Fix: merge upstream (or `git merge -s ours` per UPSTREAM.md D),")
        print("  then `python Upstream/ratchet.py --update`.")
        print()

    if stale:
        print("STALE MAP CLAIMS - documented 'converged' but actually diverged:")
        for name in stale:
            print(f"  {name}")
        print("  Fix: correct the row in UPSTREAM-DIVERGENCE.md.")
        print()

    if not drift and not stale:
        print("OK: no ancestry drift, and every 'converged' claim holds.")

    if "--check" in sys.argv and (drift or stale):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
