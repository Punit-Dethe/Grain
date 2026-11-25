#!/usr/bin/env python3
"""Golden test: tether against the Python tools on Grain's real history.

Builds two throwaway worktrees at HEAD (plus tether's converted config, see
tether_migrate.py), runs the same step with both tools and compares:

  inbox     tether inbox --all       vs data.json (ledger rows and states)
  applied   hand-applied detection   vs sync_upstream.check_ancestry_drift
  sync      tether sync              vs merge_upstream.py (conflicts, index,
                                        status, working tree, message)
  budgets   tether check --update    vs ratchet.measure() / strays()
  adopted   tether check             vs frontend_freeze.adopted_from_upstream()
  ports     tether check             vs port_audit (acknowledged / pending)
  reviews   tether closeout          vs frontend_freeze --review-audit +
                                        suppressed_review over the pending range

    python Upstream/tether_golden.py --tether <path to tether binary>

Needs a fresh `upstream/main` and a current Upstream/data.json
(`python Upstream/sync_upstream.py`). Never touches this checkout; the
worktrees are removed afterwards.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def git(cwd: str, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)}: {result.stderr.strip()}")
    return result.stdout


def py(cwd: str, *args: str) -> subprocess.CompletedProcess:
    env = dict(os.environ, GRAIN_UPSTREAM_NO_FETCH="1", PYTHONIOENCODING="utf-8")
    return subprocess.run(
        [sys.executable, *args], cwd=cwd, capture_output=True, text=True,
        encoding="utf-8", errors="replace", env=env,
    )


def tether(binary: str, cwd: str, *args: str) -> dict:
    out = subprocess.run(
        [binary, *args, "--json"], cwd=cwd, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    return json.loads(out.stdout)


def in_worktree(cwd: str, code: str) -> str:
    """Run Python code inside a worktree, with Upstream/ importable."""
    prelude = "import sys; sys.path.insert(0, 'Upstream'); sys.argv = ['x', '--no-fetch']\n"
    result = py(cwd, "-c", prelude + code)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip())
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tether", required=True, help="path to the tether binary")
    args = parser.parse_args()
    binary = os.path.abspath(args.tether)

    results: list[tuple[str, bool, str]] = []

    def record(name: str, ok: bool, detail: str = "") -> None:
        results.append((name, ok, detail))
        print(f"  {'PASS' if ok else 'FAIL'}  {name}  {detail}")

    scratch = tempfile.mkdtemp(prefix="tether-golden-")
    ours, theirs = os.path.join(scratch, "t"), os.path.join(scratch, "p")
    try:
        git(ROOT, "worktree", "add", "-q", "--detach", ours, "HEAD")
        migrate = os.path.join(HERE, "tether_migrate.py")
        shutil.copy(migrate, os.path.join(ours, "Upstream", "tether_golden_migrate.py"))
        py(ours, "Upstream/tether_golden_migrate.py")
        os.remove(os.path.join(ours, "Upstream", "tether_golden_migrate.py"))
        git(ours, "add", ".tether")
        git(ours, "commit", "-q", "-m", "golden: tether config")
        head = git(ours, "rev-parse", "HEAD").strip()
        git(ROOT, "worktree", "add", "-q", "--detach", theirs, head)
        # The ledger is a gitignored build product; both sides read this copy.
        shutil.copy(os.path.join(HERE, "data.json"), os.path.join(theirs, "Upstream", "data.json"))

        # inbox vs the ledger
        with open(os.path.join(HERE, "data.json"), encoding="utf-8") as handle:
            ledger = {row["sha"]: row for row in json.load(handle)}
        items = {i["sha"]: i for i in tether(binary, ours, "inbox", "--all")["data"]["items"]}
        same_rows = set(ledger) == set(items)
        same_state = all(ledger[s]["status"].lower() == items[s]["state"] for s in ledger if s in items)
        record("inbox", same_rows and same_state, f"{len(items)} rows")

        drift = in_worktree(theirs, (
            "import json, sync_upstream as s\n"
            "led = json.load(open(s.DATA_FILE, encoding='utf-8'))\n"
            "rec = {i['sha'][:8] for i in led if i['status'] != 'Pending'}\n"
            "print(' '.join(sorted(a[0][:8] for a in s.check_ancestry_drift(rec)[1])))\n"
        )).split()
        applied = sorted(s[:8] for s, i in items.items() if i.get("applied_here"))
        record("applied", drift == applied, " ".join(applied))

        # sync vs merge_upstream.py
        py(theirs, "Upstream/merge_upstream.py", "--no-fetch")
        sync = tether(binary, ours, "sync", "--no-fetch")
        same = all(
            git(ours, *cmd) == git(theirs, *cmd)
            for cmd in (["ls-files", "-s"], ["status", "--porcelain"], ["diff", "HEAD"])
        )
        msg = lambda w: open(git(w, "rev-parse", "--path-format=absolute", "--git-path", "SQUASH_MSG").strip(), encoding="utf-8").read()  # noqa: E731
        record("sync", same and msg(ours) == msg(theirs), f"{len(sync['data']['conflicts'])} conflicts")
        for worktree in (ours, theirs):
            git(worktree, "reset", "-q", "--hard")
            squash = git(worktree, "rev-parse", "--path-format=absolute", "--git-path", "SQUASH_MSG").strip()
            if os.path.exists(squash):
                os.remove(squash)

        # budgets, strays, adoption
        tether(binary, ours, "check", "--update", "--accept-growth")
        with open(os.path.join(ours, ".tether", "budgets.toml"), "rb") as handle:
            baseline = tomllib.load(handle)
        git(ours, "checkout", "-q", ".tether/budgets.toml")
        python_side = json.loads(in_worktree(theirs, (
            "import json, ratchet, frontend_freeze\n"
            "print(json.dumps({'budget': ratchet.measure(), 'strays': ratchet.strays(),"
            " 'adopted': frontend_freeze.adopted_from_upstream()}))\n"
        )))
        record("budgets", python_side["budget"] == baseline["files"], f"{len(baseline['files'])} files")
        check = tether(binary, ours, "check")["data"]
        record("strays", python_side["strays"] == check["strays"])
        record("adopted", sorted(python_side["adopted"]) == sorted(baseline["adopted"]),
               f"{len(baseline['adopted'])} files")

        audit = py(theirs, "Upstream/port_audit.py", "--no-fetch").stdout
        pending = f"PENDING ({check['ports']['pending']})" in audit or check["ports"]["pending"] == 0
        ack = f"acknowledged ({check['ports']['acknowledged']})" in audit
        record("ports", pending and ack and not check["ports"]["unverified"],
               f"{check['ports']['acknowledged']} acknowledged, {check['ports']['pending']} pending")

        base = py(theirs, "Upstream/sync_base.py", "--sha").stdout.strip()
        frontend = py(theirs, "Upstream/frontend_freeze.py", "--review-audit", base, "upstream/main").stdout
        suppressed = py(theirs, "Upstream/suppressed_review.py", base, "upstream/main").stdout
        python_missing = sorted(
            [f"frontend {line.split()[6][:8]}" for line in frontend.splitlines()
             if line.startswith("[freeze] FAIL: frontend problem not reviewed:")]
            + [f"suppressed {line.split()[2][:8]}" for line in suppressed.splitlines()
               if line.startswith("[suppressed] FAIL:")]
        )
        closeout = tether(binary, ours, "closeout", "upstream/main", "--no-fetch")["data"]
        tether_missing = sorted(f"{m['area']} {m['sha'][:8]}" for m in closeout["missing_reviews"])
        record("reviews", python_missing == tether_missing, f"{len(tether_missing)} missing")
    finally:
        for worktree in (ours, theirs):
            git(ROOT, "worktree", "remove", "--force", worktree, check=False)
        shutil.rmtree(scratch, ignore_errors=True)

    failed = [name for name, ok, _ in results if not ok]
    print(f"\n=== golden: {len(results) - len(failed)}/{len(results)} identical ===")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
