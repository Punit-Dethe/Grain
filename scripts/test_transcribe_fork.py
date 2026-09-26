"""Regression gates for mixed native pins and preservation of local work."""

import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import transcribe_fork as fork


class ForkInputsTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspaces = (self.root / "src-tauri", self.root / "scripts/flow-audit")
        self.pin = {"repository": "https://github.com/Punit-Dethe/transcribe.cpp.git",
                    "revision": "1" * 40, "version": "0.2.3"}
        self.context = contextlib.ExitStack()
        self.addCleanup(self.context.close)
        self.context.enter_context(patch.object(fork, "ROOT", self.root))
        self.context.enter_context(patch.object(fork, "WORKSPACES", self.workspaces))
        self.context.enter_context(contextlib.redirect_stdout(io.StringIO()))
        for workspace in self.workspaces:
            workspace.mkdir(parents=True)
            manifest = ('[dependencies]\ntranscribe-cpp = {version = "=0.2.3", default-features = false}\n'
                        '[patch.crates-io]\n') + '\n'.join(
                f'{name} = {{git = "{self.pin["repository"]}", rev = "{self.pin["revision"]}"}}'
                for name in fork.PACKAGES)
            locked = '\n'.join(
                f'[[package]]\nname = "{name}"\nversion = "0.2.3"\nsource = "{fork.source(self.pin)}"\n'
                for name in fork.PACKAGES)
            (workspace / "Cargo.toml").write_text(manifest, encoding="utf-8")
            (workspace / "Cargo.lock").write_text(locked, encoding="utf-8")

    def test_same_version_mixed_revision_is_rejected(self):
        fork.check(self.pin, False, False)
        path = self.workspaces[1] / "Cargo.lock"
        path.write_text(path.read_text(encoding="utf-8").replace("1" * 40, "2" * 40, 1), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "lock must resolve"):
            fork.check(self.pin, False, False)

    def test_path_override_is_rejected(self):
        path = self.workspaces[0] / "Cargo.toml"
        path.write_text('[patch.crates-io]\ntranscribe-cpp = {path = "../native/transcribe.cpp"}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "canonical paired Git pin"):
            fork.check(self.pin, False, False)

    def test_official_build_rejects_prebuilt_bypass(self):
        with patch.dict(os.environ, {"TRANSCRIBE_DIR": "unreviewed-native-prefix"}):
            with self.assertRaisesRegex(ValueError, "rejects TRANSCRIBE_DIR"):
                fork.check(self.pin, False, True)

    def resolution(self, app_features: str):
        def run(*args, cwd):
            if args[1] == "metadata":
                return json.dumps({"packages": [
                    {"id": name, "name": name, "source": fork.source(self.pin)}
                    for name in fork.PACKAGES], "resolve": {"nodes": [
                    {"id": name, "features": ["metal", "dynamic-backends", "shared", "vulkan"]}
                    for name in fork.PACKAGES]}})
            self.assertEqual(args[1], "tree")
            return app_features if cwd.name == "src-tauri" else "dynamic-backends,shared"
        return patch.object(fork, "run", side_effect=run)

    def test_target_features_do_not_use_union_in_metadata(self):
        with self.resolution("dynamic-backends,shared,vulkan"):
            fork.check(self.pin, True, False, "x86_64-pc-windows-msvc")
        with self.resolution(""):
            fork.check(self.pin, True, False, "aarch64-pc-windows-msvc")
        with self.resolution("metal"):
            fork.check(self.pin, True, False, "aarch64-apple-darwin")

    def test_unintended_backend_is_rejected(self):
        with self.resolution("cuda,dynamic-backends,shared,vulkan"):
            with self.assertRaisesRegex(ValueError, "target features"):
                fork.check(self.pin, True, False, "x86_64-pc-windows-msvc")

    def test_shared_arm_posture_is_rejected(self):
        with self.resolution("shared"):
            with self.assertRaisesRegex(ValueError, "target features"):
                fork.check(self.pin, True, False, "aarch64-pc-windows-msvc")

    def test_dependency_defaults_and_loose_version_are_rejected(self):
        manifest = self.workspaces[1] / "Cargo.toml"
        original = manifest.read_text(encoding="utf-8")
        for altered in (original.replace('version = "=0.2.3"', 'version = "0.2.3"'),
                        original.replace('default-features = false', 'default-features = true')):
            manifest.write_text(altered, encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "exact version and disabled defaults"):
                fork.check(self.pin, False, False)

    def test_bootstrap_preserves_an_existing_non_repository(self):
        path = self.root / "native/transcribe.cpp"
        path.mkdir(parents=True)
        work = path / "local-work.cpp"
        work.write_text("unfinished changes", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "preserve it"):
            fork.bootstrap(self.pin)
        self.assertEqual(work.read_text(encoding="utf-8"), "unfinished changes")

    def test_linux_packaged_soname_is_inspectable(self):
        library = self.root / "libtranscribe.so.0"
        library.write_bytes(b"packaged core")
        (self.root / "libtranscribe.so.debug").write_bytes(b"not a runtime library")
        self.assertEqual(fork.runtime_library(self.root), library.resolve())

    def test_multiple_native_cores_are_rejected(self):
        (self.root / "libtranscribe.so.0").write_bytes(b"core one")
        (self.root / "libtranscribe.so.1").write_bytes(b"core two")
        with self.assertRaisesRegex(ValueError, "exactly one"):
            fork.runtime_library(self.root)


if __name__ == "__main__":
    unittest.main()
