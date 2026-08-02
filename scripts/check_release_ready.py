#!/usr/bin/env python3
"""Refuse to cut a release that users could not update from.

Every failure here is one that stays silent until it is in someone else's hands:
a version mismatch tags one build and ships another, a missing `pubkey` produces
an update nobody can verify, and `createUpdaterArtifacts: false` produces a
release with no `latest.json` at all -- so the app checks, finds nothing, and
reports "up to date" forever.

None of that shows up in a local build or a test run. It shows up as an install
base that silently stops receiving updates, which is unrecoverable without
asking every user to reinstall by hand. So the release workflow runs this first.

Usage:  python scripts/check_release_ready.py
Exit:   0 all good, 1 something would break updates.
"""

from __future__ import annotations

import io
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "package.json"
CONF = ROOT / "src-tauri" / "tauri.conf.json"
CARGO = ROOT / "src-tauri" / "Cargo.toml"
CAPS = ROOT / "src-tauri" / "capabilities" / "default.json"

SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


def read_json(path: Path) -> dict:
    return json.load(io.open(path, encoding="utf-8"))


def cargo_version(path: Path) -> str | None:
    # First `version = "..."` under [package]; good enough for a fixed manifest.
    for line in io.open(path, encoding="utf-8"):
        m = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    return None


def main() -> int:
    problems: list[str] = []

    pkg = read_json(PKG)
    conf = read_json(CONF)

    versions = {
        "package.json": pkg.get("version"),
        "src-tauri/tauri.conf.json": conf.get("version"),
        "src-tauri/Cargo.toml": cargo_version(CARGO),
    }

    # 1. One version, three files. The workflow tags from tauri.conf.json while
    #    the binary reports Cargo's, so a drift ships v0.0.2 that calls itself
    #    0.0.1 -- and then never updates, because it already "has" the latest.
    distinct = set(versions.values())
    if len(distinct) != 1:
        problems.append(
            "version mismatch across manifests: "
            + ", ".join(f"{k} = {v!r}" for k, v in versions.items())
        )
    for name, value in versions.items():
        if value is None:
            problems.append(f"{name}: no version found")
        elif not SEMVER.match(value):
            problems.append(f"{name}: {value!r} is not a plain x.y.z semver")

    # 2. The updater feed must exist and be verifiable.
    updater = (conf.get("plugins") or {}).get("updater") or {}
    endpoints = updater.get("endpoints") or []
    if not endpoints:
        problems.append("tauri.conf.json: plugins.updater.endpoints is empty")
    for endpoint in endpoints:
        if not endpoint.startswith("https://"):
            problems.append(f"updater endpoint is not https: {endpoint}")
    if not updater.get("pubkey"):
        problems.append(
            "tauri.conf.json: plugins.updater.pubkey is empty -- signed updates "
            "cannot be verified and every update would be rejected"
        )

    # 3. No `latest.json` means the check silently finds nothing, forever.
    if not (conf.get("bundle") or {}).get("createUpdaterArtifacts"):
        problems.append(
            "tauri.conf.json: bundle.createUpdaterArtifacts is not true -- the "
            "release would ship without latest.json and no client could update"
        )

    # 4. A command the app is not permitted to call fails at runtime only.
    caps = read_json(CAPS)
    if not any(
        str(p) == "updater:default" or str(p).startswith("updater:")
        for p in caps.get("permissions", [])
    ):
        problems.append(
            "capabilities/default.json: no `updater:` permission -- the update "
            "check would be denied at runtime"
        )

    if problems:
        print("[release] NOT READY:")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print(f"[release] OK: v{versions['package.json']} is consistent and updatable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
