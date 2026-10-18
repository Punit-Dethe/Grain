from __future__ import annotations

import os
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import policy_check  # noqa: E402


class RelocationSourceTests(unittest.TestCase):
    recorder = "src-tauri/src/audio_toolkit/audio/recorder.rs"
    worker = "src-tauri/src/engine_supervisor.rs"

    def source_files(self, integrated: set[str], pending: set[str]) -> set[str]:
        def fake_git(*args: str) -> SimpleNamespace:
            if args == ("merge-base", "HEAD", "upstream/main"):
                return SimpleNamespace(returncode=0, stdout="integrated-sha\n")
            if args[:3] == ("ls-tree", "-r", "--name-only"):
                self.assertEqual(args[4:], ("--", "src-tauri/src/"))
                paths = {"integrated-sha": integrated, "upstream/main": pending}[args[3]]
                return SimpleNamespace(returncode=0, stdout="\n".join(sorted(paths)))
            raise AssertionError(args)

        with patch.object(policy_check, "git", side_effect=fake_git):
            return policy_check.upstream_source_files()

    def test_pending_recorder_deletion_does_not_break_existing_route(self) -> None:
        self.assertIn(self.recorder, self.source_files({self.recorder}, {self.worker}))

    def test_merged_recorder_deletion_requires_retiring_old_route(self) -> None:
        self.assertNotIn(self.recorder, self.source_files({self.worker}, {self.worker}))

    def test_new_worker_route_can_be_added_before_merge_commit(self) -> None:
        self.assertIn(self.worker, self.source_files({self.recorder}, {self.worker}))

    def test_unknown_source_is_not_accepted(self) -> None:
        self.assertNotIn("src-tauri/src/typo.rs", self.source_files({self.recorder}, {self.worker}))

    def test_missing_merge_base_fails_closed(self) -> None:
        with patch.object(policy_check, "git", return_value=SimpleNamespace(returncode=1, stdout="")):
            with self.assertRaisesRegex(RuntimeError, "integrated Handy baseline"):
                policy_check.upstream_source_files()

    def test_empty_merge_base_fails_closed(self) -> None:
        with patch.object(policy_check, "git", return_value=SimpleNamespace(returncode=0, stdout="\n")):
            with self.assertRaisesRegex(RuntimeError, "integrated Handy baseline"):
                policy_check.upstream_source_files()

    def test_unreadable_source_tree_fails_closed(self) -> None:
        with patch.object(policy_check, "git", side_effect=[
            SimpleNamespace(returncode=0, stdout="integrated-sha\n"),
            SimpleNamespace(returncode=1, stdout=""),
        ]):
            with self.assertRaisesRegex(RuntimeError, "cannot inspect Handy source tree"):
                policy_check.upstream_source_files()


if __name__ == "__main__":
    unittest.main()
