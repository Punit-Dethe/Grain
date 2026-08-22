#!/usr/bin/env python3
"""Require semantic review for upstream changes hidden by ``merge=ours``.

These files stay Grain-owned, but their fixes remain problem reports. A review
must say what broke, whether Grain has the same problem, where it was adapted,
and how that decision was verified.
"""

from __future__ import annotations

import fnmatch
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
VERDICTS_PATH = os.path.join(HERE, "verdicts.json")
PATTERNS = (
    "README.md",
    "AGENTS.md",
    "CLAUDE.md",
    "CRUSH.md",
    "BUILD.md",
    "CONTRIBUTING.md",
    "CONTRIBUTING_TRANSLATIONS.md",
    "docs/*",
    "website/*",
    ".github/workflows/*",
    "src-tauri/tauri.conf.json",
    "src-tauri/tauri.windows.conf.json",
    "bun.lock",
    "Cargo.lock",
    "src-tauri/Cargo.lock",
    ".nix/bun.nix",
    ".nix/bun-lock-hash",
    # Deliberately deleted in Grain; upstream edits surface as delete/modify
    # conflicts and are reviewed for generated-output/runtime consequences.
    "scripts/gen_catalog.py",
    "scripts/ci/stage-transcribe-libs.sh",
)
sys.path.insert(0, HERE)
from review_evidence import validation_failures  # noqa: E402


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
        encoding="utf-8",
        errors="replace",
    ).stdout


def touched_paths(sha: str) -> set[str]:
    return set(
        git("diff-tree", "-m", "--no-commit-id", "--name-only", "-r", sha).splitlines()
    )


def is_suppressed(path: str) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in PATTERNS)


def audit(base: str, head: str = "HEAD") -> int:
    commits = git("rev-list", "--reverse", f"{base}..{head}").splitlines()
    upstream = [
        sha
        for sha in commits
        if subprocess.run(
            ["git", "merge-base", "--is-ancestor", sha, "upstream/main"],
            cwd=ROOT,
            capture_output=True,
        ).returncode == 0
    ]
    with open(VERDICTS_PATH, encoding="utf-8") as handle:
        verdicts = json.load(handle)
    failures: list[tuple[str, list[str], list[str]]] = []
    reviewed = 0
    for sha in upstream:
        paths = sorted(path for path in touched_paths(sha) if is_suppressed(path))
        if not paths:
            continue
        reviewed += 1
        problems = validation_failures(
            (verdicts.get(sha) or {}).get("suppressed_review"), ROOT
        )
        if problems:
            failures.append((sha, paths, problems))
    for sha, paths, problems in failures:
        subject = git("show", "-s", "--format=%s", sha).strip()
        print(f"[suppressed] FAIL: {sha[:8]} {subject}")
        print(f"  paths: {', '.join(paths)}")
        print(f"  review: {', '.join(problems)}")
    if failures:
        print(
            "[suppressed] Use: verdict.py <sha> --suppressed-review "
            "<adapted|already-covered|not-applicable> \"problem\" \"evidence\" [Grain paths...]"
        )
        return 1
    print(f"[suppressed] OK: {reviewed} suppressed upstream change(s) reviewed")
    return 0


def main() -> int:
    if len(sys.argv) < 2 or sys.argv[1] in {"-h", "--help"}:
        print("python Upstream/suppressed_review.py <base> [head]")
        return 0 if len(sys.argv) >= 2 else 2
    return audit(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "HEAD")


if __name__ == "__main__":
    sys.exit(main())
