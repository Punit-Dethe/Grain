from __future__ import annotations

import json
import io
import os
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import mock_open, patch

UPSTREAM_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, UPSTREAM_DIR)

import frontend_freeze  # noqa: E402
import merge_upstream  # noqa: E402
import port_audit  # noqa: E402
import policy_check  # noqa: E402
import ratchet  # noqa: E402
import review_evidence  # noqa: E402
import suppressed_review  # noqa: E402


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
    def audit_legacy_record(self, record: dict) -> int:
        sha = "a" * 40
        with (
            patch.object(port_audit, "ensure_fresh", return_value=True),
            patch.object(port_audit, "load_relocations", return_value={"a.rs": {"kind": "parallel", "grain": ["b.rs"]}}),
            patch.object(port_audit, "load_verdicts", return_value={sha: record}),
            patch.object(port_audit, "commits_touching", return_value=[(sha, "historical fix", ["a.rs"])]),
            patch.object(port_audit, "is_merged", return_value=True),
            patch.object(port_audit, "requires_structured_audit", return_value=False),
            patch.object(port_audit, "deferred_commit_ids", return_value=set()),
            patch.object(sys, "argv", ["port_audit.py"]),
            redirect_stdout(io.StringIO()),
        ):
            return port_audit.main()

    def test_new_structured_evidence_is_accepted_for_a_legacy_commit(self) -> None:
        self.assertEqual(self.audit_legacy_record({"ports": {"a.rs": {"outcome": "ported", "evidence": "source review"}}}), 0)

    def test_a_legacy_note_cannot_bypass_the_deferred_queue(self) -> None:
        self.assertEqual(self.audit_legacy_record({"notes": "historical note", "ports": {"a.rs": {"outcome": "deferred", "evidence": "pending review"}}}), 1)

    def test_deferred_ports_require_a_tracked_queue_row(self) -> None:
        verdict = {"ports": {"a.rs": {"outcome": "deferred", "evidence": "pending hardware review"}}}
        self.assertEqual(port_audit.valid_port_records(verdict, ["a.rs"]), (False, ["a.rs"]))
        self.assertEqual(
            port_audit.valid_port_records(verdict, ["a.rs"], deferred_tracked=True), (True, [])
        )

    def test_historical_prose_does_not_count_as_a_deferred_queue_entry(self) -> None:
        prose, queued = "a" * 40, "b" * 40
        with patch("builtins.open", mock_open(read_data=f"Historical {prose}\n| `{queued}` | Pending | Verify |\n")):
            self.assertEqual(port_audit.deferred_commit_ids(), {queued})

    def test_every_touched_source_needs_structured_evidence(self) -> None:
        verdict = {
            "ports": {
                "a.rs": {"outcome": "ported", "evidence": "unit test"},
                "b.rs": {"outcome": "not-applicable", "evidence": "path is inert"},
            }
        }
        self.assertEqual(port_audit.valid_port_records(verdict, ["a.rs", "b.rs"]), (True, []))
        self.assertEqual(port_audit.valid_port_records(verdict, ["a.rs", "c.rs"]), (False, ["c.rs"]))


class OverlaySeamTests(unittest.TestCase):
    def setUp(self) -> None:
        self.workspace = TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.relocations = policy_check.load_relocations()
        self.write("src-tauri/src/lib.rs", '#[path = "handy/overlay.rs"]\nmod overlay;\nmod grain_overlay;\nmod grain_capture;\n')
        self.write("src-tauri/src/handy/overlay.rs", 'WebviewUrl::App("recording-overlay.html".into())')
        self.write("recording-overlay.html", '/src/app/overlay/main.tsx')
        self.write("vite.config.ts", 'recordingOverlay: resolve(__dirname, "recording-overlay.html")')

    def write(self, path: str, text: str) -> None:
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")

    def failures(self) -> list[str]:
        return policy_check.overlay_seam_failures(self.relocations, str(self.root))

    def test_shared_lifecycle_and_owned_entry_are_valid(self) -> None:
        self.assertEqual(self.failures(), [])

    def test_replacing_the_shared_module_with_an_alias_fails(self) -> None:
        self.write("src-tauri/src/lib.rs", 'pub(crate) use grain_overlay as overlay;\nmod grain_overlay;\nmod grain_capture;\n')
        failures = self.failures()
        self.assertTrue(any("must compile handy/overlay.rs" in item for item in failures))
        self.assertTrue(any("must not replace" in item for item in failures))

    def test_missing_readiness_review_route_fails(self) -> None:
        del self.relocations["src-tauri/src/managers/audio.rs"]
        self.assertTrue(any("managers/audio.rs" in item for item in self.failures()))

    def test_disconnected_native_entry_fails(self) -> None:
        self.write("src-tauri/src/handy/overlay.rs", 'WebviewUrl::App("index.html".into())')
        self.assertTrue(any("entry is disconnected" in item for item in self.failures()))

    def test_a_retired_native_renderer_fails(self) -> None:
        self.write("crates/grain-pill/Cargo.toml", "[package]")
        self.assertTrue(any("retired production implementation" in item for item in self.failures()))


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
            if args[0] == "diff-tree":
                return "src/components/Fix.tsx\n"
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

    def test_frontend_review_requires_problem_and_real_destination(self) -> None:
        failures = review_evidence.validation_failures(
            {
                "outcome": "adapted",
                "problem": "phrases were rejected",
                "destinations": ["src/app/lib/customWords.ts"],
                "evidence": "unit test",
            },
            os.path.dirname(UPSTREAM_DIR),
        )
        self.assertEqual(failures, [])


class SuppressedReviewTests(unittest.TestCase):
    def test_matches_only_grain_owned_suppressed_paths(self) -> None:
        self.assertTrue(suppressed_review.is_suppressed("BUILD.md"))
        self.assertTrue(suppressed_review.is_suppressed("docs/guide.md"))
        self.assertTrue(suppressed_review.is_suppressed("scripts/gen_catalog.py"))
        self.assertFalse(suppressed_review.is_suppressed("src-tauri/src/managers/audio.rs"))


if __name__ == "__main__":
    unittest.main()
