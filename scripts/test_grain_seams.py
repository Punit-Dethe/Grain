from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import grain_seams  # noqa: E402


class SeamTests(unittest.TestCase):
    def setUp(self) -> None:
        self.workspace = TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.relocations = grain_seams.load_relocations()
        self.write(
            "src-tauri/src/lib.rs",
            '#[path = "handy/overlay.rs"]\nmod overlay;\nmod grain_overlay;\nmod grain_capture;\n',
        )
        self.write("src-tauri/src/handy/overlay.rs", 'WebviewUrl::App("recording-overlay.html".into())')
        self.write("recording-overlay.html", "/src/app/overlay/main.tsx")
        self.write("vite.config.ts", 'recordingOverlay: resolve(__dirname, "recording-overlay.html")')

    def write(self, path: str, text: str) -> None:
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")

    def failures(self) -> list[str]:
        return grain_seams.overlay_seam_failures(self.relocations, str(self.root))

    def test_shared_lifecycle_and_owned_entry_are_valid(self) -> None:
        self.assertEqual(self.failures(), [])

    def test_replacing_the_shared_module_with_an_alias_fails(self) -> None:
        self.write("src-tauri/src/lib.rs", "pub(crate) use grain_overlay as overlay;\nmod grain_overlay;\nmod grain_capture;\n")
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

    def test_an_unmapped_alias_of_an_upstream_module_fails(self) -> None:
        self.write("src-tauri/src/lib.rs", "use grain_widget as widget;\n")
        self.write("src-tauri/src/handy/widget.rs", "")
        failures = grain_seams.alias_failures(self.relocations, str(self.root))
        self.assertEqual(
            failures,
            ["unmapped inert replacement: src-tauri/src/widget.rs is aliased to src-tauri/src/grain_widget.rs"],
        )

    def test_an_alias_needs_its_grain_destination(self) -> None:
        self.write("src-tauri/src/lib.rs", "use grain_settings as settings;\n")
        self.write("src-tauri/src/handy/settings.rs", "")
        self.relocations["src-tauri/src/settings.rs"] = ["crates/grain-core/src/settings.rs"]
        failures = grain_seams.alias_failures(self.relocations, str(self.root))
        self.assertTrue(any("alias destination missing" in item for item in failures))


if __name__ == "__main__":
    unittest.main()
