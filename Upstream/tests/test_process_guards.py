from __future__ import annotations

import json
import os
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import mock_open, patch

UPSTREAM_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, UPSTREAM_DIR)

import frontend_freeze  # noqa: E402
import merge_upstream  # noqa: E402
import port_audit  # noqa: E402
import ratchet  # noqa: E402


class MergeFrontendTests(unittest.TestCase):
    def test_exists_at_uses_git_tree_not_filesystem(self) -> None:
        with patch.object(
            merge_upstream, "git", return_value=SimpleNamespace(returncode=1)
        ) as mocked:
            self.assertFalse(merge_upstream.exists_at("HEAD", "src/app/new.tsx"))
            mocked.assert_called_once_with("cat-file", "-e", "HEAD:src/app/new.tsx")


class RatchetTests(unittest.TestCase):
    def test_budget_growth_finds_new_and_grown_entries(self) -> None:
        self.assertEqual(
            ratchet.budget_growth({"grown": 4, "new": 2, "shrunk": 1}, {"grown": 3, "shrunk": 2}),
            [("grown", 3, 4), ("new", None, 2)],
        )


class PortEvidenceTests(unittest.TestCase):
    def test_every_touched_source_needs_structured_evidence(self) -> None:
        verdict = {
            "ports": {
                "a.rs": {"outcome": "ported", "evidence": "unit test"},
                "b.rs": {"outcome": "not-applicable", "evidence": "path is inert"},
            }
        }
        self.assertEqual(port_audit.valid_port_records(verdict, ["a.rs", "b.rs"]), (True, []))
        self.assertEqual(port_audit.valid_port_records(verdict, ["a.rs", "c.rs"]), (False, ["c.rs"]))


class FrontendFreezeTests(unittest.TestCase):
    def test_allowlist_growth_is_fail_closed(self) -> None:
        existing = {"strict": True, "shared": [], "adopted": []}
        with (
            patch.object(frontend_freeze, "load_allow", return_value=existing),
            patch.object(frontend_freeze, "shared", return_value=[]),
            patch.object(frontend_freeze, "adopted_from_upstream", return_value=["src/app/new.tsx"]),
            patch("frontend_freeze.os.path.exists", return_value=True),
            patch("builtins.open", mock_open()) as opened,
        ):
            self.assertEqual(frontend_freeze.update_allow(False), 1)
            opened.assert_not_called()

    def test_frontend_review_requires_evidence(self) -> None:
        sha = "a" * 40

        def fake_git(*args: str) -> str:
            if args[0] == "rev-list":
                return sha + "\n"
            if args[0] == "show":
                return "frontend change\n"
            raise AssertionError(args)

        with (
            patch.object(frontend_freeze, "git", side_effect=fake_git),
            patch.object(
                frontend_freeze.subprocess,
                "run",
                return_value=SimpleNamespace(returncode=0),
            ),
            patch("builtins.open", mock_open(read_data=json.dumps({}))),
        ):
            self.assertEqual(frontend_freeze.frontend_review_audit("main"), 1)


if __name__ == "__main__":
    unittest.main()
