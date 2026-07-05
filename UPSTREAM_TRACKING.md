# Upstream Tracking (Handy -> Grain)

This document tracks updates and commits from the upstream **Handy** repository since June 5, 2026. It ensures Grain stays reasonably up-to-date with upstream improvements while maintaining its own architectural independence.

## Pending Evaluation and Merge
Updates from upstream that need to be evaluated and either merged or ignored.

| Date | Upstream Commit / PR | Status / Notes |
| :--- | :--- | :--- |
| **Jul 04, 2026** | `Improve Dutch (nl) translation accuracy and consistency (#1594)` | Pending |
| **Jul 04, 2026** | `Update Japanese translations (#1593)` | Pending |
| **Jul 04, 2026** | `Add Dutch (Nederlands) translation (#1590)` | Pending |
| **Jul 03, 2026** | `faster mic initialization (#1582)` | Pending |
| **Jul 02, 2026** | `ship vsredist directly with the app (#1577)` | Pending |
| **Jul 01, 2026** | `update language selector` | Partially Merged (Frontend logic missing) |
| **Jul 01, 2026** | `edit model recs` | Pending (Grain uses a custom catalog, requires manual evaluation) |

---

## Completed / Merged
Updates from upstream that have been successfully ported, refactored, and merged into Grain.

| Date | Upstream Commit / PR | Grain Commit | Notes |
| :--- | :--- | :--- | :--- |
| **Jul 03, 2026** | `fix cyrillic (unicode) path problems (#1187)` | `pending` | Fixed VAD initialization crash on paths with Cyrillic characters. |
| **Jul 03, 2026** | `bump to transcribe-cpp-0.1.1 (#1589)` | `pending` | Bumped transcribe-cpp version across all targets. |
| **Jun 25, 2026** | `debug + perf transcribe cli (#1541)` | `7400a9b` | Ported live log viewer and perf cli. |
| **Jun 24, 2026** | `fix: stop overlay mic-level events leaking memory... (#1447)` | `8e761c3` | Fixed Tauri memory leak from overlay. |
| **Jun 24, 2026** | `fix: skip post-processing when transcription is empty (#1537)` | `8e761c3` | Applied upstream check. |
| **Jun 23, 2026** | `live debug log viewer in app (#1535)` | `7400a9b` | Ported live log viewer and CLI tool. |
| **Jun 18, 2026** | `fix: dropdown overflow in post-processing settings (#1402)` | `db42a12`* | Ported silently during Dark Mode / UI overhaul. Grain's `Dropdown` already uses the identical grid fix. |
| **Jun 11, 2026** | `fix(models): show size for downloaded models (#1484)` | `db42a12`* | Model sizes already visible in Grain UI with dynamic icons (ported during UI overhaul). |
| **Jun 11, 2026** | `fix(visualizer): scale FFT window to device sample rate (#1491)` | `353b37c`* | FFT scaling already present in `recorder.rs` (likely ported during audio perf rewrite). |

---

## Intentionally Ignored
Updates from upstream that we have evaluated and explicitly decided NOT to merge (e.g., conflicts with Grain's architecture, UI philosophy, or native implementations).

| Date | Upstream Commit / PR | Reason for Ignoring |
| :--- | :--- | :--- |
| **Jun 23, 2026** | `Clarified branding and redistribution terms for Handy` | Irrelevant to Grain (we are an independent fork with our own branding and license terms). |

