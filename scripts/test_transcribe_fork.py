"""Regression gates for mixed native pins and preservation of local work."""

import contextlib
import io
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
            manifest = '[patch.crates-io]\n' + '\n'.join(
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

    def test_bootstrap_preserves_an_existing_non_repository(self):
        path = self.root / "native/transcribe.cpp"
        path.mkdir(parents=True)
        work = path / "local-work.cpp"
        work.write_text("unfinished changes", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "preserve it"):
            fork.bootstrap(self.pin)
        self.assertEqual(work.read_text(encoding="utf-8"), "unfinished changes")


if __name__ == "__main__":
    unittest.main()
