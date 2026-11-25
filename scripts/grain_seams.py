#!/usr/bin/env python3
"""Grain's seams with the Handy tree that `tether check` cannot know about.

tether guards the generic boundary (budgets, strays, relocations, ports).
These checks are Grain's own architecture, kept from the retired
Upstream/policy_check.py:

* every `use grain_X as upstream_name;` alias in lib.rs replaces an upstream
  module, so that module must be a relocation in .tether/config.toml naming
  the Grain file (or an upstream fix lands in uncompiled code unnoticed);
* the recording overlay keeps Handy's lifecycle module and Grain's owned
  renderer wired together, with review routes for the files they share;
* the transcribe.cpp fork pin matches every Cargo declaration.

    python scripts/grain_seams.py
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONFIG_PATH = os.path.join(ROOT, ".tether", "config.toml")
TRANSCRIBE_PIN_PATH = os.path.join(ROOT, "native", "transcribe-fork.json")
TAURI_MANIFEST_PATH = os.path.join(ROOT, "src-tauri", "Cargo.toml")


def load_relocations(path: str = CONFIG_PATH) -> dict[str, list[str]]:
    """Upstream path -> the fork files its code lives in."""
    with open(path, "rb") as handle:
        config = tomllib.load(handle)
    return {
        source: list(entry.get("fork", []))
        for source, entry in config.get("relocations", {}).items()
    }


def read(root: str, path: str, failures: list[str]) -> str:
    try:
        with open(os.path.join(root, path), encoding="utf-8") as handle:
            return handle.read()
    except OSError as error:
        failures.append(f"cannot read {path}: {error}")
        return ""


def alias_failures(relocations: dict[str, list[str]], root: str = ROOT) -> list[str]:
    failures: list[str] = []
    lib = read(root, "src-tauri/src/lib.rs", failures)
    for module, alias in re.findall(
        r"\buse\s+(grain_[A-Za-z0-9_]+)\s+as\s+([A-Za-z0-9_]+)\s*;", lib
    ):
        source = f"src-tauri/src/{alias}.rs"
        destination = f"src-tauri/src/{module}.rs"
        if not os.path.isfile(os.path.join(root, "src-tauri/src/handy", f"{alias}.rs")):
            continue  # not an upstream module
        if source not in relocations:
            failures.append(f"unmapped inert replacement: {source} is aliased to {destination}")
        elif destination not in relocations[source]:
            failures.append(f"{source}: alias destination missing: {destination}")
    return failures


def overlay_seam_failures(relocations: dict[str, list[str]], root: str = ROOT) -> list[str]:
    """Guard the shared overlay lifecycle and the owned renderer's review routes."""
    failures: list[str] = []
    required = {
        "src-tauri/src/overlay.rs": {
            "src-tauri/src/handy/overlay.rs", "src-tauri/src/grain_overlay.rs",
            "src/app/overlay/RecordingOverlay.tsx", "src/app/overlay/wave.ts",
            "src/app/overlay/overlay.css", "recording-overlay.html", "vite.config.ts",
        },
        "src-tauri/src/audio_toolkit/audio/sink.rs": {"src-tauri/src/grain_capture.rs"},
        "src-tauri/src/managers/audio.rs": {"src-tauri/src/grain_capture.rs"},
        "src-tauri/src/managers/transcription.rs": {"src-tauri/src/grain_overlay.rs"},
        "src-tauri/src/settings.rs": {"src/app/components/settings/ShowOverlay.tsx"},
    }
    for source, destinations in required.items():
        missing = destinations - set(relocations.get(source, []))
        if missing:
            failures.append(f"overlay review route missing: {source} -> {', '.join(sorted(missing))}")

    # Ignore comments; old historical wording must not masquerade as wiring.
    code = re.sub(r"//[^\n]*", "", read(root, "src-tauri/src/lib.rs", failures))
    if not re.search(r'#\[path\s*=\s*"handy/overlay.rs"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+overlay\s*;', code):
        failures.append("overlay seam: crate::overlay must compile handy/overlay.rs")
    if re.search(r"\buse\s+grain_overlay\s+as\s+overlay\s*;", code):
        failures.append("overlay seam: grain_overlay must not replace Handy's lifecycle module")
    for module in ("grain_overlay", "grain_capture"):
        if not re.search(rf"\bmod\s+{module}\s*;", code):
            failures.append(f"overlay seam: owned {module} adapter is not compiled")
    for path, needle in (
        ("src-tauri/src/handy/overlay.rs", '"recording-overlay.html"'),
        ("recording-overlay.html", "/src/app/overlay/main.tsx"),
        ("vite.config.ts", '"recording-overlay.html"'),
    ):
        if needle not in read(root, path, failures):
            failures.append(f"overlay seam: owned recording entry is disconnected in {path}")
    for path in ("crates/grain-pill/Cargo.toml", "src/app/overlay/matrix.ts",
                 "src/app/components/settings/PillSkinSelector.tsx"):
        if os.path.isfile(os.path.join(root, path)):
            failures.append(f"overlay seam: retired production implementation returned: {path}")
    return failures


def transcribe_pin_failures() -> list[str]:
    """Preserve the maintained fork when Handy changes its native dependency."""
    failures: list[str] = []
    try:
        with open(TAURI_MANIFEST_PATH, encoding="utf-8") as handle:
            manifest = handle.read()
        with open(TRANSCRIBE_PIN_PATH, encoding="utf-8") as handle:
            pin = json.load(handle)
        if not re.fullmatch(r"[0-9a-f]{40}", pin["upstream_revision"]):
            failures.append("native fork lacks an exact upstream commit")
        declarations = re.findall(r'transcribe-cpp\s*=\s*\{[^\n]*version\s*=\s*"=([^\"]+)"', manifest)
        if not declarations or any(value != pin["version"] for value in declarations):
            failures.append("every transcribe-cpp dependency must exactly match the fork baseline")
        native_check = subprocess.run(
            [sys.executable, os.path.join(ROOT, "scripts", "transcribe_fork.py"), "--check"],
            cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
        if native_check.returncode:
            failures.append(native_check.stderr.strip() or "native fork source check failed")
    except (OSError, ValueError, KeyError) as error:
        failures.append(f"invalid native fork pin: {error}")
    if not os.path.isfile(os.path.join(ROOT, "docs", "TRANSCRIBE-CPP-FORK.md")):
        failures.append("missing docs/TRANSCRIBE-CPP-FORK.md integration contract")
    return failures


def main() -> int:
    relocations = load_relocations()
    failures = (
        alias_failures(relocations)
        + overlay_seam_failures(relocations)
        + transcribe_pin_failures()
    )
    for failure in failures:
        print(f"[seams] FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("[seams] OK: alias coverage, overlay seams and the transcribe.cpp pin agree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
