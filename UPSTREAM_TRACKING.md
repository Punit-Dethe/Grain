# Upstream Tracking (Handy -> Grain)

This document tracks updates and commits from the upstream **Handy** repository since June 5, 2026. It ensures Grain stays reasonably up-to-date with upstream improvements while maintaining its own architectural independence.

## Pending Evaluation and Merge
Updates from upstream that need to be evaluated and either merged or ignored.

| Date | Upstream Commit / PR | Status / Notes |
| :--- | :--- | :--- |
| **Jul 09, 2026** | `fix: tray icon invisible on Windows with dark taskbar + light apps (#1423)` | Pending (Grain's native architecture heavily modified `tray.rs` and `lib.rs` for the Pill UI. Requires manual analysis and integration.) |
| **Jul 09, 2026** | `fix: cancel stalled output processing (#1614)` | Pending (Grain's ASR adaptation stripped the `cancel_generation` token in `actions.rs`. Needs manual rebuild using Grain's `SESSION_ID` architecture.) |
| **Jul 09, 2026** | `[bugfix] keep track of current tray icon state (#1158)` | Pending (Grain's native architecture modified this part of the code for the Pill UI. Kept pending for manual analysis and merge.) |
| **Jul 08, 2026** | `salvage valid settings instead of resetting store on parse failure (#1631)` | Pending |
| **Jul 07, 2026** | `fix: preserve ampersands in custom words (#1569)` | Pending (complex change: adds `build_match_key` + dual-lookup struct to `text.rs` + `CustomWords.tsx` frontend changes) |
| **Jul 05, 2026** | `move to auto timestamps for all models (#1602)` | Reverted / Pending (Porting this strictly caused Whisper models to collapse/return empty text when queried with an initial prompt. This broke the Agent voice query completely, leading to an empty embedding and a `Null distance` SQLite crash.) |
| **Jul 03, 2026** | `faster mic initialization (#1582)` | Pending |
| **Jul 02, 2026** | `ship vsredist directly with the app (#1577)` | Pending |


---

## Completed / Merged
Updates from upstream that have been successfully ported, refactored, and merged into Grain.

| Date | Upstream Commit / PR | Notes |
| :--- | :--- | :--- |
| **Jul 09, 2026** | `fix(tray): log tray icon failures instead of panicking (#1355)` | Safely merged into `tray.rs` |
| **Jul 08, 2026** | `fix(build): auto-fall-back to AI stub on Command Line Tools-only macOS (#1510)` | Added CLT fallback logic in `build.rs`. |
| **Jul 08, 2026** | `fix: prevent abort on quit by handling poisoned mutexes in Drop impls (#1354)` | Recover poisoned mutexes inside Drop impls using `match` + logged warnings. |
| **Jul 08, 2026** | `handy keys 0.3.0 (#1623)` | Bumped in `Cargo.toml`. |
| **Jul 08, 2026** | `fix: throttle mic-level IPC to mitigate WebKitWebProcess memory leak (#1444)` | Throttled `emit_levels` to 30 FPS in `overlay.rs`. |
| **Jul 08, 2026** | `fix: reset resampler state between recordings to prevent audio crosstalk (#1344)` | Added `reset()` to `FrameResampler` on `Cmd::Start`. |
| **Jul 08, 2026** | `Apply paste delay after key press and increase slider range (#1465)` | Added dual paste delays (before/after), bumped slider to 500ms, and updated translations. |
| **Jul 08, 2026** | `Add Nepali translation (#1632)` | Added `ne` translation. |
| **Jul 08, 2026** | `bump version (#1634)` | Bumped `transcribe-cpp` to 0.1.2. |
| **Jul 08, 2026** | `add openblas to ci and packaging for linux (#1621)` | Added OpenBLAS checks to Linux CI/packaging. |
| **Jul 01, 2026** | `edit model recs` | Skipped (Grain manages custom catalog). |
| **Jul 07, 2026** | `Update Italian translations (#1604)` | Updated `it` translations. |
| **Jul 06, 2026** | `Fix GigaAM v3 description. (#1613)` | Corrected descriptions to "Russian speech-to-text". |
| **Jul 05, 2026** | `fix: gate whisper run extension on model arch, not Feature::InitialPrompt (#1603)` | Gated family extension on `model.arch() == "whisper"`. |
| **Jul 04, 2026** | `Improve Dutch (nl) translation accuracy and consistency (#1594)` | Improved `nl` translations. |
| **Jul 04, 2026** | `Update Japanese translations (#1593)` | Fixed encoding and translated English strings in `ja`. |
| **Jul 04, 2026** | `Add Dutch (Nederlands) translation (#1590)` | Added `nl` translation. |
| **Jul 03, 2026** | `fix cyrillic (unicode) path problems (#1187)` | Fixed VAD crash on Cyrillic paths. |
| **Jul 03, 2026** | `bump to transcribe-cpp-0.1.1 (#1589)` | Bumped transcribe-cpp to 0.1.1. |
| **Jul 01, 2026** | `update language selector` | Ported language selector improvements. |
| **Jun 25, 2026** | `debug + perf transcribe cli (#1541)` | Ported live log viewer and perf cli. |
| **Jun 24, 2026** | `fix: stop overlay mic-level events leaking memory... (#1447)` | Fixed Tauri memory leak from overlay. |
| **Jun 24, 2026** | `fix: skip post-processing when transcription is empty (#1537)` | Applied upstream check. |
| **Jun 23, 2026** | `live debug log viewer in app (#1535)` | Ported live log viewer and CLI tool. |
| **Jun 18, 2026** | `fix: dropdown overflow in post-processing settings (#1402)` | Ported silently during Dark Mode / UI overhaul. Grain's `Dropdown` already uses the identical grid fix. |
| **Jun 11, 2026** | `fix(models): show size for downloaded models (#1484)` | Model sizes already visible in Grain UI with dynamic icons (ported during UI overhaul). |
| **Jun 11, 2026** | `fix(visualizer): scale FFT window to device sample rate (#1491)` | FFT scaling already present in `recorder.rs` (likely ported during audio perf rewrite). |

---

## Technical Debt & Future Architectural Fixes
* **Remove `candle 0.9.2` f16 CPU probe workaround:** The current upstream fix for the f16 NaN bug uses a "probe" that loads the model, tests it, and falls back to f32 if it fails. This violates lazy-loading principles and wastes initialization time. 
  * **The Proper Fix:** Replace the standard f32/f16 models with a properly quantized `int8` (or similar) model. A structurally quantized model will natively use integer matrix multiplication (bypassing the broken f16 float math on CPU), load significantly faster, and allow us to rip out the hacky probe/fallback logic completely.

---

## Intentionally Ignored
Updates from upstream that we have evaluated and explicitly decided NOT to merge (e.g., conflicts with Grain's architecture, UI philosophy, or native implementations).

| Date | Upstream Commit / PR | Reason for Ignoring |
| :--- | :--- | :--- |
| **Jul 07, 2026** | `fix: preserve active overlay during post-processing (#1597)` | Irrelevant (Grain uses a native Rust pill architecture). |
| **Jul 07, 2026** | `fix: add prompt injection defense to default post-processing prompt (#1310)` | Irrelevant (Grain manages its own custom prompts). |
| **Jun 23, 2026** | `Clarified branding and redistribution terms for Handy` | Irrelevant (Grain is an independent fork). |

