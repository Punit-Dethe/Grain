#!/usr/bin/env python3
"""Check immutable native inputs or bootstrap the ignored development checkout.

uv run --no-project scripts/transcribe_fork.py --check --metadata
uv run --no-project scripts/transcribe_fork.py --bootstrap
Official builds add --official; deliberate prebuilt diagnostics use --runtime-dir.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
WORKSPACES = (ROOT / "src-tauri", ROOT / "scripts/flow-audit")
PACKAGES = ("transcribe-cpp", "transcribe-cpp-sys")


def run(*args: str, cwd: Path = ROOT) -> str:
    return subprocess.check_output(args, cwd=cwd, text=True, encoding="utf-8").strip()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def source(pin: dict) -> str:
    return f"git+{pin['repository']}?rev={pin['revision']}#{pin['revision']}"


def check(pin: dict, metadata: bool, official: bool, target: str | None = None) -> None:
    if official:
        for key in ("TRANSCRIBE_DIR", "TRANSCRIBE_CMAKE_ARGS", "CMAKE_ARGS"):
            require(not os.environ.get(key), f"Official native source build rejects {key}; unset it.")
    for workspace in WORKSPACES:
        manifest = tomllib.loads((workspace / "Cargo.toml").read_text(encoding="utf-8"))
        patches = manifest.get("patch", {}).get("crates-io", {})
        for name in PACKAGES:
            require(patches.get(name) == {"git": pin["repository"], "rev": pin["revision"]},
                    f"{workspace.name}: {name} must use the canonical paired Git pin")
        locked = tomllib.loads((workspace / "Cargo.lock").read_text(encoding="utf-8"))
        for name in PACKAGES:
            packages = [p for p in locked["package"] if p["name"] == name]
            require(len(packages) == 1 and packages[0].get("version") == pin["version"]
                    and packages[0].get("source") == source(pin),
                    f"{workspace.name}: {name} lock must resolve exactly one canonical fork revision")
        if metadata:
            host = target or next(line[6:] for line in run("rustc", "-vV").splitlines() if line.startswith("host: "))
            resolved = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1",
                                      "--filter-platform", host, cwd=workspace))
            nodes = {node["id"]: node for node in resolved["resolve"]["nodes"]}
            for name in PACKAGES:
                packages = [p for p in resolved["packages"] if p["name"] == name]
                require(len(packages) == 1 and packages[0]["source"] == source(pin),
                        f"{workspace.name}: {name} metadata has a path/registry/duplicate source")
                features = set(nodes[packages[0]["id"]]["features"])
                required = {"dynamic-backends", "shared"}
                if workspace.name == "src-tauri":
                    required = ({"metal"} if "apple-darwin" in host else set()
                                if host == "aarch64-pc-windows-msvc" else required | {"vulkan"})
                require(required <= features, f"{workspace.name}: {name} missing features {required - features}")
                require("default" not in features, f"{workspace.name}: {name} accidentally enables defaults")
    print(f"Native source pair verified: {pin['revision']}")


def bootstrap(pin: dict) -> None:
    checkout = ROOT / "native/transcribe.cpp"
    if checkout.exists():
        require((checkout / ".git").exists(), f"{checkout} exists without an independent .git; preserve it and inspect manually")
        require(Path(run("git", "rev-parse", "--show-toplevel", cwd=checkout)).resolve() == checkout.resolve(),
                "The native path resolves to another repository")
        require(run("git", "remote", "get-url", "origin", cwd=checkout) == pin["repository"],
                "Native origin differs from the canonical fork; inspect remotes manually")
        actual = run("git", "rev-parse", "HEAD", cwd=checkout)
        require(actual == pin["revision"],
                f"Native checkout is {actual}; expected {pin['revision']}. Preserve work and switch revisions manually.")
        require(not run("git", "status", "--porcelain", cwd=checkout),
                "Native checkout has local edits; preserved. Commit or stash them before using a pinned checkout.")
        print(f"Existing native checkout verified: {checkout}")
        return
    checkout.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "clone", "--filter=blob:none", "--no-checkout", pin["repository"], str(checkout)], check=True)
    subprocess.run(["git", "fetch", "origin", pin["revision"]], cwd=checkout, check=True)
    subprocess.run(["git", "checkout", "--detach", pin["revision"]], cwd=checkout, check=True)
    subprocess.run(["git", "remote", "add", "upstream", pin["upstream_repository"]], cwd=checkout, check=True)
    print(f"Native development checkout created: {checkout}")


def runtime(pin: dict, directory: Path) -> None:
    directory = directory.resolve(strict=True)
    filenames = ("transcribe.dll", "libtranscribe.so", "libtranscribe.dylib")
    libraries = [directory / name for name in filenames if (directory / name).is_file()]
    require(len(libraries) == 1, "Expected exactly one platform libtranscribe in --runtime-dir")
    # Keep the DLL search handle alive until every query finishes. This CLI
    # exits after inspection; no library or discovery service survives it.
    search = os.add_dll_directory(str(directory)) if sys.platform == "win32" else None
    try:
        lib = ctypes.CDLL(str(libraries[0]))
        lib.transcribe_grain_contract_revision.restype = ctypes.c_uint32
        require(lib.transcribe_grain_contract_revision() == pin["contract_revision"], "Runtime contract revision mismatch")
        for function, expected in (("transcribe_grain_patch_id", pin["patch_identity"]),
                                   ("transcribe_runtime_header_hash", pin["abi_hash"]),
                                   ("transcribe_version", pin["version"])):
            query = getattr(lib, function)
            query.argtypes = []
            query.restype = ctypes.c_char_p
            require(query() == expected.encode(), f"Runtime {function} mismatch")
        lib.transcribe_init_backends.argtypes = [ctypes.c_char_p]
        lib.transcribe_init_backends.restype = ctypes.c_int
        require(lib.transcribe_init_backends(os.fsencode(directory)) == 0, "Runtime backend module initialization failed")
        lib.transcribe_device_count.restype = ctypes.c_int
        require(lib.transcribe_device_count() > 0, "Runtime has no compute devices")
        lib.transcribe_backend_available.argtypes = [ctypes.c_int]
        lib.transcribe_backend_available.restype = ctypes.c_bool
        require(lib.transcribe_backend_available(1), "Runtime is missing the required CPU fallback")
        print(f"Runtime contract and backend discovery verified: {libraries[0]}")
    finally:
        if search:
            search.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--metadata", action="store_true")
    parser.add_argument("--official", action="store_true")
    parser.add_argument("--target", help="Cargo target triple (defaults to rustc host)")
    parser.add_argument("--bootstrap", action="store_true")
    parser.add_argument("--runtime-dir", type=Path)
    args = parser.parse_args()
    try:
        pin = json.loads((ROOT / "native/transcribe-fork.json").read_text(encoding="utf-8"))
        require(bool(re.fullmatch(r"[0-9a-f]{40}", pin["revision"])), "Native revision must be a full commit SHA")
        if args.bootstrap:
            bootstrap(pin)
        if args.check or args.metadata or args.official:
            check(pin, args.metadata, args.official, args.target)
        if args.runtime_dir:
            runtime(pin, args.runtime_dir)
        require(args.bootstrap or args.check or args.metadata or args.official or args.runtime_dir,
                "Choose --bootstrap, --check, or --runtime-dir")
        return 0
    except (ValueError, OSError, KeyError, AttributeError, subprocess.CalledProcessError) as error:
        print(f"Native fork check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
