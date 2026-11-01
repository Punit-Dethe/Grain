"""Windows Cargo runner regression checks using actual Rust test executables."""

import concurrent.futures
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(
    os.name == "nt" and shutil.which("rustc") and shutil.which("powershell.exe"),
    "Requires Windows, Rust and Windows PowerShell",
)
class WindowsRustRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="grain runner probe ")
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name).resolve()
        cls.runner = Path(__file__).with_name("run-rust-test.ps1").resolve()
        source = cls.root / "probe.rs"
        source.write_text(
            "#[test] fn succeeds() {}\n"
            '#[test] fn fails() { panic!("expected negative control"); }\n',
            encoding="utf-8",
        )
        (cls.root / "deps").mkdir()
        cls.binary = cls.root / "deps" / "handy_app_lib-runner-probe.exe"
        subprocess.run(
            ["rustc", "--test", str(source), "-o", str(cls.binary)],
            check=True, capture_output=True, text=True, timeout=120,
        )
        cls.original_hash = hashlib.sha256(cls.binary.read_bytes()).digest()
        # A sibling belongs to another invocation and must survive cleanup.
        cls.sibling = cls.root / "grain-rust-tests-unrelated"
        cls.sibling.mkdir()
        (cls.sibling / "marker").write_text("preserve", encoding="utf-8")

    def invoke(self, case, binary=None):
        return subprocess.run(
            ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
             "-File", str(self.runner), str(binary or self.binary), case, "--exact"],
            capture_output=True, text=True, timeout=30,
        )

    def assert_clean(self):
        self.assertEqual(
            set(self.root.glob("grain-rust-tests-*")), {self.sibling},
            "Runner leaked staging files or removed another invocation",
        )
        self.assertEqual((self.sibling / "marker").read_text(), "preserve")
        self.assertEqual(hashlib.sha256(self.binary.read_bytes()).digest(), self.original_hash)

    def test_forwards_filter_and_arguments_without_modifying_cargo_binary(self):
        result = self.invoke("succeeds")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("1 passed; 0 failed", result.stdout)
        self.assert_clean()

    def test_failed_test_preserves_exit_code_and_cleans_staging(self):
        result = self.invoke("fails")
        self.assertEqual(result.returncode, 101, result.stdout + result.stderr)
        self.assertIn("0 passed; 1 failed", result.stdout)
        self.assert_clean()

    def test_non_tauri_binary_passes_through(self):
        binary = self.root / "deps" / "ordinary-probe.exe"
        shutil.copyfile(self.binary, binary)
        result = self.invoke("succeeds", binary)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_clean()

    def test_parallel_runs_have_separate_owned_staging(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(self.invoke, ["succeeds", "succeeds"]))
        for result in results:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_clean()


if __name__ == "__main__":
    unittest.main()
