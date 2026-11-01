#!/usr/bin/env python3
"""Run the production journal/TDT tests without desktop dependencies.

Requires Cargo, a C++ toolchain, CMake, and a clean checkout of the exact native
pin. Compiles the real native CPU library, not a replacement API. The adapter's
unit tests use injected inference; real-model tests remain explicitly ignored
unless requested with Cargo arguments and the documented fixture environment.
This does not validate the Tauri application or its subprocess worker.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-source', required=True, type=Path)
    parser.add_argument('cargo_args', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    native = args.native_source.resolve(strict=True)
    pin = json.loads((root / 'native/transcribe-fork.json').read_text())
    revision = subprocess.check_output(
        ['git', '-C', str(native), 'rev-parse', 'HEAD'], text=True
    ).strip()
    if revision != pin['revision']:
        parser.error('native checkout does not match native/transcribe-fork.json')
    subprocess.run(['git', '-C', str(native), 'diff', '--quiet', 'HEAD', '--'], check=True)
    # This is a disposable build target importing the production modules. It
    # neither edits the application manifest/lockfile nor substitutes its tests.
    with tempfile.TemporaryDirectory(prefix='grain-tdt-tests-') as directory:
        project = Path(directory)
        dependencies = {
            'grain-core': root / 'crates/grain-core',
            'grain-tdt': root / 'crates/grain-tdt',
            'transcribe-cpp': native / 'bindings/rust/transcribe-cpp',
        }
        manifest = '[package]\nname = "grain-tdt-adapter-tests"\nversion = "0.0.0"\nedition = "2021"\n[workspace]\n[dependencies]\n'
        for name, path in dependencies.items():
            manifest += f'{name} = {{ path = {json.dumps(str(path))}, default-features = false }}\n'
        manifest += 'tempfile = "3"\nhound = "3"\nserde_json = "1"\nanyhow = "1"\n'
        (project / 'Cargo.toml').write_text(manifest)
        (project / 'src').mkdir()
        modules = ['grain_audio_journal', 'tdt_flow']
        (project / 'src/lib.rs').write_text(''.join(
            f'#[path = {json.dumps(str(root / "src-tauri/src" / (name + ".rs")))}]\nmod {name};\n'
            for name in modules
        ))
        environment = os.environ.copy()
        environment.setdefault('CARGO_TARGET_DIR', str(root / 'target/tdt-adapter-tests'))
        command = ['cargo', 'test', '--manifest-path', str(project / 'Cargo.toml')]
        command.extend(args.cargo_args)
        return subprocess.run(command, env=environment).returncode


if __name__ == '__main__':
    raise SystemExit(main())
