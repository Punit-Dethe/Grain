# Audit: upstream hunks lost to rerere-cache rename pairing

Date: 2026-11-26. Read-only audit; nothing ported.

## How they were lost

`Upstream/rr-cache/` was tracked and its plain preimages were near copies of
Handy files. In each sync below, git's rename detection paired the upstream
path (on Grain's side of the merge base) with a cache file instead of the moved
`src-tauri/src/handy/...` file. Upstream's edits then merged into the cache
file, and `Upstream/** merge=ours` threw them away silently: no conflict, no
diff in the merge commit.

Pairings (from `git diff -M --name-status <base> <merge>^1`):

| Upstream path | Cache entry | Syncs |
|---|---|---|
| `src-tauri/src/actions.rs` | 6d678c01 | all three |
| `src-tauri/src/shortcut/mod.rs` | 6800714d | all three |
| `src-tauri/src/managers/transcription.rs` | 28c1b6bd | all three |
| `src-tauri/src/audio_toolkit/audio/recorder.rs` | d2244365 | 97482bda9 only |
| tray locale JSONs fr/de/zh-TW/es | 3f1e0f88, 5600eb27, 59ec1cf3, eff31944 | all three (not applicable: English only) |

Syncs and upstream ranges:

- 97482bda9 (2026-09-23): 0e503672..8f9cf53c
- ff13b6fc7 (2026-10-17): 8ef8dd43..29bd2c0d
- 1cf50ec18 (2026-10-17): 29bd2c0d..9e57a0d7

Method: each merge was redone in a scratch worktree twice with rerere off,
once as it was and once with `Upstream/rr-cache` removed first. The first run
reproduces the "Auto-merging Upstream/rr-cache/.../preimage" lines. The second
run conflicts in the real `handy/` files (`actions.rs`, `shortcut/mod.rs`,
`managers/*`, `tray.rs`, ...). Each upstream commit that touched a paired file
in its range was then checked against `main` today (79734811).

## Result per hunk

Present = Grain has the behaviour today (re-ported by hand later, or done in a
`grain_*` module). Columns: upstream commit | file | present today | worth porting.

### actions.rs (6d678c01)

- 5ec2276a single-writer tray icon | actions.rs (`change_tray_icon` → `set_tray_state`, 16 call sites) | **MISSING**. The whole commit is missing, not just these call sites: Grain's `handy/tray.rs` still has the old `CurrentTrayIconState` + `change_tray_icon`, which rebuilds the full menu on every state change. The 97482bda9 resolution kept our `tray.rs`, `secure_input.rs` and `utils.rs` to match the stale `actions.rs`. The trial branch only adds a `set_tray_state` shim. | **Yes**: fixes the macOS tray disappearing (tauri#12060, Handy #1948) and the racing menu rebuilds; Windows benefits from fewer native menu rebuilds. Port the whole commit (tray.rs + all callers, including `grain_actions.rs` and `lib.rs`).
- 258899a2 redact transcription in log | actions.rs | Present (`utils::redact_text`) | —
- dc5bdc9d unload model after empty recordings | actions.rs (`FinishGuard(AppHandle, Arc<TranscriptionManager>)` + `maybe_unload_immediately`) | Present | —
- eea1f5f4 don't start recording when no model can transcribe | actions.rs | Present as `grain_capture::require_transcribable_model` (called from TranscribeAction::start) | —
- 0bb0428e Chinese script selector: removes `maybe_convert_chinese_variant`, `resolve_effective_language`, the OpenCC import and the `else if final_text != transcription` branch | actions.rs | **MISSING**. The new path (`chinese_script.rs`, called from `managers/transcription.rs`) is present, but the old OpenCC pass still runs in `process_transcription_output`. | **Yes**: dead legacy path that can convert text a second time when the language intent is `zh-Hans`/`zh-Hant`, overriding the `chinese_script` choice (e.g. AsTranscribed). Delete the old function and its call.
- c8262ab3 finalize-stream comment | actions.rs | Present (superseded by 9e57a0d7) | —
- a42131d2 `recording_error_type` / `anyhow::Error` | actions.rs | Present | —
- 9e57a0d7 `live_decode` gate + `transcribe_recording` | actions.rs | Present | —

### shortcut/mod.rs (6800714d)

- 141f981d compound-key parse test | shortcut/mod.rs (test only) | **MISSING** | Low: test only; the fix itself is in frontend `keyboard.ts` (Grain-owned). Cheap to add as a regression guard.
- a6eed754 reject unknown reset binding IDs | shortcut/mod.rs | Present (Grain's `grain_settings::get_stored_binding` returns Err) | —
- c62a5fcd `change_shortcut_activation_setting` | shortcut/mod.rs | Present (`grain_shortcuts.rs`) | —
- c62a5fcd `change_hold_threshold_ms_setting` | shortcut/mod.rs | **MISSING** (the setting exists and the coordinator reads it; there is no setter command) | No: upstream only uses it from a debug UI panel.
- b0266607 Nix update-check guard | shortcut/mod.rs | N/A: updater removed from Grain | No
- 20ada47d `change_vad_backend_setting` | shortcut/mod.rs | Present (`grain_transcription.rs`) | —
- 5ec2276a `update_tray_menu(&app)` signature | shortcut/mod.rs | **MISSING**: part of the tray single-writer port above | With 5ec2276a
- 9e57a0d7 `change_long_form_chunking_setting` | shortcut/mod.rs | Present (`grain_transcription.rs`) | —
- 5197faf0 remove `change_ort_accelerator_setting` | shortcut/mod.rs | Present (command gone). A stale `ort_accelerator` field is left in `handy/settings.rs`. | Low: drop the field when settings are next touched.
- 14ae9c6b `change_post_process_api_format_setting` | shortcut/mod.rs | Present by other means: Grain's provider form (`PpProviderForm.tsx`) writes `api_format` through its own provider command | —
- c8262ab3 `retry_transcribe_gpu` on accelerator/device change + `get_available_accelerators(app)` | shortcut/mod.rs | Present (via `save_accelerator_and_reload_next_use`) | —
- b38987a9 cancel-shortcut reconcile (`CANCEL_REQUESTED`) | shortcut/mod.rs | Present | —
- 0bb0428e `change_chinese_script_setting` | shortcut/mod.rs | Present (`grain_transcription.rs`) | —

### managers/transcription.rs (28c1b6bd)

- e4ae0d44 emit `loading_failed` for deleted model files | transcription.rs | Present | —
- 6fa85061 Norwegian `no` alias (`canonical_language_code`) | transcription.rs | Present (+ test) | —
- 258899a2 redact transcription result log | transcription.rs | Present | —
- 29bd2c0d `report_compute_devices` off startup | transcription.rs | Present in transcription.rs, but **never called**: the `lib.rs` calls (prewarm thread + CLI path) are missing, so the device list is never logged. lib.rs was not cache-paired; this was dropped by an ordinary conflict resolution. | Low: diagnostics only. Wire the call into the prewarm thread.
- 0bb0428e, c8262ab3, 5197faf0, 4150a4a8, 9e57a0d7 (Chinese script, worker isolation, ONNX removal, vocabulary API, long-form chunking) | transcription.rs | Present (re-ported later: `EngineSupervisor`, `transcription/{chunked,job,live,policy}.rs`) | —

### audio_toolkit/audio/recorder.rs (d2244365, 97482bda9 only)

- d54c88eb real-time-safe mic callback, df216832 tail audio on stop, 20ada47d earshot VAD | recorder.rs | Present by supersession: Grain records through the `handy-recorder` crate (upstream a42131d2), which carries these fixes. `recorder.rs` is gone. | —

### Tray locale JSONs (3f1e0f88, 5600eb27, 59ec1cf3, eff31944)

- All locale edits | fr/de/zh-TW/es `translation.json` | N/A: Grain is English only | No

## Worth porting (summary)

1. **5ec2276a single-writer tray**: full commit (tray.rs + callers). Biggest real loss.
2. **0bb0428e leftover**: delete the old OpenCC pass in `handy/actions.rs`.
3. Low: wire up `report_compute_devices` (29bd2c0d lib.rs part); add the 141f981d test; drop the stale `ort_accelerator` field.

`trial/tether-full-sync` fixes none of these. It only adds a `set_tray_state`
shim over the old tray, and still has `maybe_convert_chinese_variant`.
