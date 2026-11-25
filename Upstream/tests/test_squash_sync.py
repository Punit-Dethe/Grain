"""End-to-end: a Grain repo with NO shared history syncs from a Handy repo.

Builds two throwaway repositories so the real git machinery (graft, squash,
directory-rename detection, conflicts) is exercised, not mocked.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))

import merge_upstream  # noqa: E402
import sync_base  # noqa: E402


def run(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


def write(root: Path, rel: str, text: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def init(path: Path) -> None:
    path.mkdir()
    run(path, "init", "-q", "-b", "main")
    # Repo-local identity for throwaway repos only.
    run(path, "config", "user.name", "Test")
    run(path, "config", "user.email", "test@example.invalid")
    run(path, "config", "core.autocrlf", "false")


def commit(path: Path, message: str) -> str:
    run(path, "add", "-A")
    run(path, "commit", "-q", "--allow-empty", "-m", message)
    return run(path, "rev-parse", "HEAD")


LIB = "fn one() {}\n\nfn two() {}\n\nfn three() {}\n"


class SquashSyncTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        tmp = Path(self._tmp.name)
        self.handy, self.grain = tmp / "handy", tmp / "grain"

        init(self.handy)
        write(self.handy, "core/lib.rs", LIB)
        self.a = commit(self.handy, "handy: initial")

        # Grain starts from a SNAPSHOT of Handy A, relocated core/ -> handy/,
        # with no ancestry link at all. The marker is the only connection.
        init(self.grain)
        run(self.grain, "config", "merge.directoryRenames", "true")
        write(self.grain, "handy/lib.rs", LIB)
        write(self.grain, "grain.rs", "fn grain() {}\n")
        commit(self.grain, f"Initial commit\n\n{sync_base.marker_line(self.a)}")
        write(self.grain, "handy/lib.rs", LIB.replace("fn one() {}", "fn one() { grain(); }"))
        self.g2 = commit(self.grain, "grain: hook one()")
        run(self.grain, "remote", "add", "upstream", str(self.handy))

        patches = [
            patch.object(sync_base, "ROOT", str(self.grain)),
            patch.object(merge_upstream, "ROOT", str(self.grain)),
            patch.dict(os.environ, {"GRAIN_UPSTREAM_NO_FETCH": "1"}),
        ]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def fetch(self) -> None:
        run(self.grain, "fetch", "-q", "upstream", "main")

    def sync(self) -> int:
        cwd = os.getcwd()
        os.chdir(self.grain)  # upstream_ref runs git in the current directory
        try:
            with redirect_stdout(StringIO()):
                return merge_upstream.start(dry_run=False)
        finally:
            os.chdir(cwd)

    def test_clean_sync_is_one_single_parent_commit_without_handy_history(self) -> None:
        write(self.handy, "core/lib.rs", LIB.replace("fn three() {}", "fn three() { fixed(); }"))
        b = commit(self.handy, "handy: fix three")
        write(self.handy, "core/new.rs", "fn new() {}\n")
        c = commit(self.handy, "handy: add new")
        self.fetch()

        self.assertEqual(sync_base.sync_base(), self.a)
        self.assertEqual(self.sync(), 0)
        with redirect_stdout(StringIO()):
            self.assertEqual(merge_upstream.commit_sync(), 0)

        parents = run(self.grain, "rev-list", "--parents", "-n", "1", "HEAD").split()
        self.assertEqual(parents[1:], [self.g2])
        self.assertEqual(sync_base.sync_base(), c)
        self.assertEqual(merge_upstream.head_sync_range(), (self.a, c))
        history = set(run(self.grain, "rev-list", "HEAD").split())
        self.assertFalse({self.a, b, c} & history, "Handy commits leaked into Grain history")
        self.assertEqual(run(self.grain, "replace", "-l"), "", "graft outlived the sync")

        lib = (self.grain / "handy/lib.rs").read_text(encoding="utf-8")
        self.assertIn("fn one() { grain(); }", lib)  # Grain's edit kept
        self.assertIn("fn three() { fixed(); }", lib)  # Handy's fix arrived
        # Directory-rename detection placed Handy's new file in Grain's layout.
        self.assertTrue((self.grain / "handy/new.rs").exists())
        message = run(self.grain, "log", "-1", "--format=%B")
        self.assertIn("handy: fix three", message)
        self.assertIn(sync_base.marker_line(c), message)

    def test_conflicting_sync_stops_then_records_after_resolution(self) -> None:
        write(self.handy, "core/lib.rs", LIB.replace("fn one() {}", "fn one() { handy(); }"))
        b = commit(self.handy, "handy: change one")
        self.fetch()

        self.assertEqual(self.sync(), 2)
        self.assertEqual(merge_upstream.conflicted_paths(), ["handy/lib.rs"])
        self.assertEqual(merge_upstream.pending_sync(), b)
        self.assertEqual(run(self.grain, "replace", "-l"), "")
        with redirect_stdout(StringIO()):
            self.assertEqual(merge_upstream.commit_sync(), 1)  # refuses while conflicted

        write(self.grain, "handy/lib.rs", LIB.replace("fn one() {}", "fn one() { grain(); handy(); }"))
        run(self.grain, "add", "handy/lib.rs")
        with redirect_stdout(StringIO()):
            self.assertEqual(merge_upstream.commit_sync(), 0)
        self.assertEqual(sync_base.sync_base(), b)
        self.assertIsNone(merge_upstream.pending_sync())

    def test_second_sync_merges_against_the_new_base(self) -> None:
        write(self.handy, "core/lib.rs", LIB.replace("fn two() {}", "fn two() { v2(); }"))
        commit(self.handy, "handy: two v2")
        self.fetch()
        self.assertEqual(self.sync(), 0)
        with redirect_stdout(StringIO()):
            merge_upstream.commit_sync()

        # Handy edits the same line again; against the OLD base this would
        # conflict, against the recorded new base it is a clean update.
        write(self.handy, "core/lib.rs", LIB.replace("fn two() {}", "fn two() { v3(); }"))
        d = commit(self.handy, "handy: two v3")
        self.fetch()
        self.assertEqual(self.sync(), 0)
        with redirect_stdout(StringIO()):
            merge_upstream.commit_sync()
        self.assertEqual(sync_base.sync_base(), d)
        self.assertIn("fn two() { v3(); }", (self.grain / "handy/lib.rs").read_text(encoding="utf-8"))

    def test_up_to_date_and_missing_marker(self) -> None:
        self.fetch()
        self.assertEqual(self.sync(), 0)
        self.assertIsNone(merge_upstream.pending_sync())
        run(self.grain, "checkout", "-q", "--orphan", "bare")
        commit(self.grain, "no marker")
        with self.assertRaises(sync_base.SyncBaseError):
            sync_base.sync_base()

    def test_diverging_markers_are_refused(self) -> None:
        write(self.handy, "x", "1")
        commit(self.handy, "x1")
        run(self.handy, "checkout", "-q", "-b", "side", self.a)
        write(self.handy, "y", "1")
        side = commit(self.handy, "side")
        run(self.grain, "fetch", "-q", "upstream", "main", "side")
        main = run(self.grain, "rev-parse", "upstream/main")
        commit(self.grain, f"a\n\n{sync_base.marker_line(main)}")
        commit(self.grain, f"b\n\n{sync_base.marker_line(side)}")
        with self.assertRaises(sync_base.SyncBaseError):
            sync_base.sync_base()


if __name__ == "__main__":
    unittest.main()
