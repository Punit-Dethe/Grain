#!/usr/bin/env python3
"""Validate machine/human upstream policy and replacement coverage.

`relocations.json` is canonical for inert, relocated, and parallel code. This
gate validates its schema and paths, proves every `grain_* as upstream_name`
module alias is mapped, and checks that the generated table in
UPSTREAM-DIVERGENCE.md exactly matches the JSON.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
RELOCATIONS_PATH = os.path.join(HERE, "relocations.json")
DIVERGENCE_PATH = os.path.join(HERE, "UPSTREAM-DIVERGENCE.md")
LIB_PATH = os.path.join(ROOT, "src-tauri", "src", "lib.rs")
TRANSCRIBE_CONTRACT_PATH = os.path.join(ROOT, "vendor", "TRANSCRIBE-CPP.md")
TAURI_MANIFEST_PATH = os.path.join(ROOT, "src-tauri", "Cargo.toml")
BEGIN = "<!-- BEGIN GENERATED RELOCATION POLICY -->"
END = "<!-- END GENERATED RELOCATION POLICY -->"
KINDS = {"inert", "relocated", "parallel"}


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace"
    )


def load_relocations() -> dict:
    with open(RELOCATIONS_PATH, encoding="utf-8") as handle:
        raw = json.load(handle)
    return {key: value for key, value in raw.items() if not key.startswith("_")}


def render(relocations: dict) -> str:
    lines = [
        BEGIN,
        "| Upstream source | Kind | Grain runtime destinations |",
        "| --- | --- | --- |",
    ]
    for source, entry in sorted(relocations.items()):
        destinations = "<br>".join(f"`{path}`" for path in entry["grain"])
        lines.append(f"| `{source}` | {entry['kind']} | {destinations} |")
    lines.append(END)
    return "\n".join(lines)


def aliased_replacements() -> dict[str, str]:
    with open(LIB_PATH, encoding="utf-8") as handle:
        body = handle.read()
    return {
        alias: module
        for module, alias in re.findall(
            r"\buse\s+(grain_[A-Za-z0-9_]+)\s+as\s+([A-Za-z0-9_]+)\s*;", body
        )
    }


def main() -> int:
    relocations = load_relocations()
    failures: list[str] = []

    # transcribe.cpp is upstream-owned at its pristine baseline but carries a
    # narrowly-scoped Grain TDT patch. Its contract is therefore part of
    # upstream management, not optional vendor prose.
    if not os.path.exists(TRANSCRIBE_CONTRACT_PATH):
        failures.append("missing vendor/TRANSCRIBE-CPP.md upstream contract")
    else:
        with open(TRANSCRIBE_CONTRACT_PATH, encoding="utf-8") as handle:
            contract = handle.read()
        baseline = re.search(r"Published crates: `transcribe-cpp ([^`]+)`", contract)
        commit = re.search(r"Upstream commit: `([0-9a-f]{40})`", contract)
        with open(TAURI_MANIFEST_PATH, encoding="utf-8") as handle:
            manifest = handle.read()
        if not baseline:
            failures.append("transcribe contract lacks a published-crate baseline")
        else:
            declarations = re.findall(r'transcribe-cpp\s*=\s*\{[^\n]*version\s*=\s*"=([^\"]+)"', manifest)
            if not declarations or any(value != baseline.group(1) for value in declarations):
                failures.append(
                    "every transcribe-cpp dependency must exactly match the contract baseline"
                )
            for package in ("transcribe-cpp", "transcribe-cpp-sys"):
                vendor_dir = os.path.join(ROOT, "vendor", f"{package}-{baseline.group(1)}")
                if not os.path.isdir(vendor_dir):
                    failures.append(f"missing vendored contract baseline: {package}-{baseline.group(1)}")
                    continue
                vcs_path = os.path.join(vendor_dir, ".cargo_vcs_info.json")
                try:
                    with open(vcs_path, encoding="utf-8") as handle:
                        vendor_commit = json.load(handle)["git"]["sha1"]
                except (FileNotFoundError, KeyError, json.JSONDecodeError):
                    failures.append(f"{package}: invalid .cargo_vcs_info.json")
                    continue
                if commit and vendor_commit != commit.group(1):
                    failures.append(
                        f"{package}: vendored commit {vendor_commit} differs from contract"
                    )
                expected_path = f'../vendor/{package}-{baseline.group(1)}'
                if expected_path not in manifest:
                    failures.append(f"{package}: Cargo patch does not select {expected_path}")
        if not commit:
            failures.append("transcribe contract lacks its exact upstream commit")
    upstream_files = set(
        git("ls-tree", "-r", "--name-only", "upstream/main", "--", "src-tauri/src/")
        .stdout.splitlines()
    )

    for source, entry in sorted(relocations.items()):
        if entry.get("kind") not in KINDS:
            failures.append(f"{source}: invalid kind {entry.get('kind')!r}")
        destinations = entry.get("grain")
        if not isinstance(destinations, list) or not destinations:
            failures.append(f"{source}: grain must be a non-empty path list")
            continue
        if not str(entry.get("why", "")).strip():
            failures.append(f"{source}: why is required")
        if source not in upstream_files:
            failures.append(f"{source}: source does not exist in upstream/main")
        for destination in destinations:
            if not os.path.exists(os.path.join(ROOT, destination)):
                failures.append(f"{source}: destination does not exist: {destination}")

    # Alias coverage is derivable: do not rely on a reviewer remembering to
    # register a newly-created inert replacement.
    for alias, module in sorted(aliased_replacements().items()):
        source = f"src-tauri/src/{alias}.rs"
        destination = f"src-tauri/src/{module}.rs"
        if source in upstream_files and source not in relocations:
            failures.append(
                f"unmapped inert replacement: {source} is aliased to {destination}"
            )
        elif source in relocations and destination not in relocations[source]["grain"]:
            failures.append(f"{source}: alias destination missing: {destination}")

    with open(DIVERGENCE_PATH, encoding="utf-8") as handle:
        document = handle.read().replace("\r\n", "\n")
    expected = render(relocations)
    if BEGIN not in document or END not in document:
        failures.append("UPSTREAM-DIVERGENCE.md lacks the generated relocation table")
    else:
        actual = BEGIN + document.split(BEGIN, 1)[1].split(END, 1)[0] + END
        if actual.strip() != expected.strip():
            failures.append(
                "generated relocation table is stale; update it from relocations.json"
            )

    if failures:
        for failure in failures:
            print(f"[policy] FAIL: {failure}", file=sys.stderr)
        return 1
    print(
        f"[policy] OK: {len(relocations)} relocation rules; alias coverage, "
        "transcribe.cpp contract, and human policy agree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
