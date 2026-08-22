#!/usr/bin/env python3
"""Port audit — the guard the ratchet and the faithfulness diff cannot be.

Grain keeps Handy's STT core byte-identical inside `src-tauri/src/handy/`, so a
plain diff against `upstream/main` reliably proves the *shared* surface is on
par with Handy. That check has one structural blind spot, and it is exactly the
one that bites:

  * Some upstream files are **inert** in Grain — byte-identical to upstream but
    UNCOMPILED (`llm_client.rs`, `settings.rs`, `overlay.rs`). They merge
    cleanly and match upstream perfectly, so a diff reports "in sync" — while
    the code that actually runs (`grain_llm_client.rs`, grain-core settings,
    the native pill) is a SEPARATE file with no upstream counterpart to diff.
  * Some logic was **relocated** out of an otherwise-merged file
    (`post_process_transcription` → `grain_post_process.rs`).
  * Some subsystems are **parallel** Grain-only implementations that share a
    bug class with upstream but have no upstream file at all (`stt_client.rs`,
    the rolling engine).

For all three, an upstream fix can merge "successfully" and never reach the
code Grain runs. A diff cannot see it: the upstream-shaped file matches, and
the Grain file has nothing to compare against. This is not hypothetical — the
v0.9.5 sync merged #1823 and #2211 into the inert `llm_client.rs` while
`grain_llm_client` silently lacked both fixes.

So this tool does the one thing the diff can't: it reads `relocations.json`
(the machine-readable twin of UPSTREAM-DIVERGENCE.md), and for every upstream
commit that touched a mapped file, it demands a human have RECORDED that the
change reached the Grain destination — a note in `verdicts.json`. A flagged
commit with no such note is a commit that MIGHT have been silently dropped, and
the audit fails until someone confirms it (`verdict.py <sha> --note "..."`).

    python Upstream/port_audit.py              # audit all history since the graft
    python Upstream/port_audit.py --pending    # only commits not yet merged (sync gate)
    python Upstream/port_audit.py --no-fetch    # CI has already fetched

Exit code is non-zero when a merged commit touched a relocated file and no note
records what happened to the port — the same "block the auto-PR rather than
merge a lie" stance as the ancestry-drift detector.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from upstream_ref import ensure_fresh  # noqa: E402

RELOCATIONS_PATH = os.path.join(HERE, "relocations.json")
VERDICTS_PATH = os.path.join(HERE, "verdicts.json")

# The import baseline (grafted). Everything upstream has done since lives in
# `IMPORT_BASELINE..upstream/main`.
IMPORT_BASELINE = "0392b7b"
STRUCTURED_AUDIT_BASE = "98a4d80cce8ad41efec2a419b59d9e81229a35d7"
# ...but only commits at or after the ledger's tracking floor can be annotated
# with verdict.py, so the audit floors here too — older commits are the settled
# import baseline, outside the ongoing-sync verdict system. Kept in step with
# sync_upstream.TRACKING_FLOOR_TS.
from sync_upstream import TRACKING_FLOOR_TS  # noqa: E402


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    ).stdout


def load_relocations() -> dict:
    with open(RELOCATIONS_PATH, encoding="utf-8") as f:
        data = json.load(f)
    return {k: v for k, v in data.items() if not k.startswith("_")}


def load_verdicts() -> dict:
    try:
        with open(VERDICTS_PATH, encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return {}


def is_merged(sha: str) -> bool:
    """True when `sha` is already in our history (an ancestor of HEAD)."""
    return (
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", sha, "HEAD"],
            cwd=ROOT,
            capture_output=True,
        ).returncode
        == 0
    )


def requires_structured_audit(sha: str) -> bool:
    return subprocess.run(
        ["git", "merge-base", "--is-ancestor", sha, STRUCTURED_AUDIT_BASE],
        cwd=ROOT, capture_output=True,
    ).returncode != 0


def valid_port_records(verdict: dict, touched: list[str]) -> tuple[bool, list[str]]:
    ports = verdict.get("ports") or {}
    missing = []
    for source in touched:
        record = ports.get(source) or {}
        if record.get("outcome") not in {"ported", "not-applicable"} or not str(
            record.get("evidence", "")
        ).strip():
            missing.append(source)
    return not missing, missing


def commits_touching(sources: set[str]) -> list[tuple[str, str, list[str]]]:
    """Every upstream commit since the baseline that touched a mapped source.

    Returns (sha, subject, [touched mapped sources]) newest-first. One
    `git log` with `--name-only` is cheap even over the whole fork history.
    """
    out = git(
        "log",
        f"{IMPORT_BASELINE}..upstream/main",
        f"--since={TRACKING_FLOOR_TS}",
        "--no-merges",
        "--name-only",
        "--format=%x00%H%x00%s",
    )
    results: list[tuple[str, str, list[str]]] = []
    sha = subject = None
    touched: set[str] = set()

    def flush() -> None:
        if sha and touched:
            results.append((sha, subject, sorted(touched)))

    for line in out.splitlines():
        if line.startswith("\x00"):
            flush()
            _, sha, subject = line.split("\x00")
            touched = set()
        elif line.strip() in sources:
            touched.add(line.strip())
    flush()
    return results


def main() -> int:
    # Commit subjects and verdict notes carry em-dashes and accented text; the
    # Windows console defaults to cp1252, which would crash on them.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

    pending_only = "--pending" in sys.argv
    if ensure_fresh(quiet=True) is None:
        return 1

    relocations = load_relocations()
    verdicts = load_verdicts()
    sources = set(relocations)

    flagged = commits_touching(sources)

    review: list = []      # merged, no note — the dangerous silent case
    acknowledged: list = []  # merged, has a note (or an explicit verdict)
    pending: list = []     # not merged yet

    for sha, subject, touched in flagged:
        merged = is_merged(sha)
        verdict = verdicts.get(sha, {})
        note = (verdict.get("notes") or "").strip()
        status = verdict.get("status")
        structured = requires_structured_audit(sha)
        ports_ok, missing_ports = valid_port_records(verdict, touched)
        acknowledged_port = ports_ok if structured else bool(note or status == "Ignored")
        entry = (sha, subject, touched, note, status, missing_ports, structured)
        if not merged:
            pending.append(entry)
        elif acknowledged_port:
            acknowledged.append(entry)
        else:
            review.append(entry)

    def render(entry) -> None:
        sha, subject, touched, note, status, missing_ports, structured = entry
        print(f"  {sha[:8]}  {subject[:60]}")
        for src in touched:
            dests = ", ".join(relocations[src]["grain"])
            print(f"            {src}  [{relocations[src]['kind']}] -> verify: {dests}")
        if note:
            print(f"            note: {note[:100]}")
        elif status:
            print(f"            verdict: {status}")
        if structured and missing_ports:
            print(f"            structured port evidence missing: {', '.join(missing_ports)}")

    print("=== port audit (relocated / inert / parallel files) ===")
    print(
        f"  {len(flagged)} upstream commit(s) since {IMPORT_BASELINE} touched a mapped file"
    )

    if pending:
        print(f"\n  PENDING ({len(pending)}) — not merged; confirm the port when you sync:")
        for e in pending:
            render(e)

    if not pending_only:
        if acknowledged:
            print(f"\n  acknowledged ({len(acknowledged)}) — a verdict note records the port:")
            for e in acknowledged:
                render(e)
        if review:
            print(
                f"\n  !! REVIEW ({len(review)}) -- merged, but NO required evidence records whether the "
                f"fix reached the Grain destination. Verify each, then record it:\n"
                f"      python Upstream/verdict.py <sha> --port <source> "
                f"<ported|not-applicable> \"test/review evidence\""
            )
            for e in review:
                render(e)

    gate = review if not pending_only else []
    if gate:
        print(f"\n=== port audit FAILED: {len(gate)} unverified relocated-file commit(s) ===")
        return 1
    print("\n=== port audit OK ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
