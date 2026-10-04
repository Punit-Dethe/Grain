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
TRANSCRIBE_PIN_PATH = os.path.join(ROOT, "native", "transcribe-fork.json")
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


def overlay_seam_failures(relocations: dict, root: str = ROOT) -> list[str]:
    """Guard the active shared lifecycle and the owned renderer's review routes."""
    failures = []
    required = {
        "src-tauri/src/overlay.rs": {
            "src-tauri/src/handy/overlay.rs", "src-tauri/src/grain_overlay.rs",
            "src/app/overlay/RecordingOverlay.tsx", "src/app/overlay/wave.ts",
            "src/app/overlay/overlay.css", "recording-overlay.html", "vite.config.ts",
        },
        "src-tauri/src/audio_toolkit/audio/recorder.rs": {"src-tauri/src/grain_capture.rs"},
        "src-tauri/src/managers/audio.rs": {"src-tauri/src/grain_capture.rs"},
        "src-tauri/src/managers/transcription.rs": {"src-tauri/src/grain_overlay.rs"},
        "src-tauri/src/settings.rs": {"src/app/components/settings/ShowOverlay.tsx"},
    }
    for source, destinations in required.items():
        missing = destinations - set(relocations.get(source, {}).get("grain", []))
        if missing:
            failures.append(f"overlay review route missing: {source} -> {', '.join(sorted(missing))}")

    def read(path: str) -> str:
        try:
            with open(os.path.join(root, path), encoding="utf-8") as handle:
                return handle.read()
        except OSError as error:
            failures.append(f"overlay seam: cannot read {path}: {error}")
            return ""

    lib = read("src-tauri/src/lib.rs")
    # Ignore comments; old historical wording must not masquerade as wiring.
    code = re.sub(r"//[^\n]*", "", lib)
    if not re.search(r'#\[path\s*=\s*"handy/overlay.rs"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+overlay\s*;', code):
        failures.append("overlay seam: crate::overlay must compile handy/overlay.rs")
    if re.search(r'\buse\s+grain_overlay\s+as\s+overlay\s*;', code):
        failures.append("overlay seam: grain_overlay must not replace Handy's lifecycle module")
    for module in ("grain_overlay", "grain_capture"):
        if not re.search(rf'\bmod\s+{module}\s*;', code):
            failures.append(f"overlay seam: owned {module} adapter is not compiled")
    for path, needle in (
        ("src-tauri/src/handy/overlay.rs", '"recording-overlay.html"'),
        ("recording-overlay.html", '/src/app/overlay/main.tsx'),
        ("vite.config.ts", '"recording-overlay.html"'),
    ):
        if needle not in read(path):
            failures.append(f"overlay seam: owned recording entry is disconnected in {path}")
    for path in ("crates/grain-pill/Cargo.toml", "src/app/overlay/matrix.ts",
                 "src/app/components/settings/PillSkinSelector.tsx"):
        if os.path.isfile(os.path.join(root, path)):
            failures.append(f"overlay seam: retired production implementation returned: {path}")
    return failures


def main() -> int:
    relocations = load_relocations()
    failures: list[str] = overlay_seam_failures(relocations)

    # Preserve the maintained fork when Handy changes its native dependency.
    # The shared checker validates both workspaces/pins/locks without fetching.
    with open(TAURI_MANIFEST_PATH, encoding="utf-8") as handle:
        manifest = handle.read()
    try:
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

    # Optional frozen rollback trees retain their own original provenance.
    # They must never dictate the active source pin or the next native upgrade.
    if os.path.exists(TRANSCRIBE_CONTRACT_PATH):
        with open(TRANSCRIBE_CONTRACT_PATH, encoding="utf-8") as handle:
            contract = handle.read()
        baseline = re.search(r"Published crates: `transcribe-cpp ([^`]+)`", contract)
        commit = re.search(r"Upstream commit: `([0-9a-f]{40})`", contract)
        if not baseline:
            failures.append("transcribe contract lacks a published-crate baseline")
        else:
            for package in ("transcribe-cpp", "transcribe-cpp-sys"):
                vendor_dir = os.path.join(ROOT, "vendor", f"{package}-{baseline.group(1)}")
                if not os.path.isdir(vendor_dir):
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
        "overlay seams, transcribe.cpp contract, and human policy agree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
