# Final divergence audit (pre folder-move)

Every file that still differs from upstream, examined with a real diff and
given a verdict. Run before Phase 7 so the `handy/` folder move happens over a
tree whose every divergence is intentional and written down.

- **Audited:** 2026-07-20 against upstream `cdbc2239` (latest `main`).
- **Result:** 26 files, 3550 lines. No unexplained divergence remains.
- **Upstream is 4 commits ahead**, all frontend/i18n (`#1697`, `#1701`,
  `#1708`, `#1709`) — no Rust changes, so every number here is current.

Regenerate the inventory any time with
`python "Handy Isolation/ratchet.py" --update`.

## What this pass fixed

Nine files were carrying divergence that nobody had chosen — drift, not design:

| File | What was wrong | Now |
|---|---|---|
| `apple_intelligence.rs` | `c"..."` literal rewritten as `CStr::from_bytes_with_nul(...).unwrap()` | identical to upstream |
| `commands/audio.rs` | `is_ok_and` rewritten as `map_or` | identical |
| `audio_toolkit/text.rs` | `is_some_and` rewritten as `map_or` | identical |
| `transcription_coordinator.rs` | same `map_or` regression | that hunk converged |
| `portable.rs` | module docs changed `//!` → `///` (dangles onto the next item) | identical |
| `cli.rs` | stale doc text, inlined `PathBuf`, **`--list-models` flag dropped entirely** | identical; flag + handler restored in `lib.rs` |
| `input.rs` | `get_cursor_position` deleted | restored in upstream's position, `#[allow(dead_code)]` |
| `recorder.rs` | **regression from this effort's own phase 1**: the `mem::replace` that avoids a per-chunk heap alloc on the realtime callback had been lost when the file was re-baselined | restored |
| `managers/audio.rs` | **regression from phase 1**: Native ASR's `StreamRouter` was moved onto upstream's *post-VAD* callback; Grain feeds it every frame | restored, with a comment on why neither live consumer may use the post-VAD hook |

The last two are the reason this audit was worth doing: a re-baseline silently
drops Grain behaviour that the compiler and the tests cannot miss.

## Verdicts — remaining 26 files

### Deliberate product decisions (permanent; documented in the divergence map)

| File | Lines | Why it diverges |
|---|---:|---|
| `managers/transcription.rs` | 732 | ONNX/transcribe-rs removal (every family ships GGUF); shared-engine `with_engine_session`; `transcribe_rolling_chunk`; `initiate_model_load_for`. **Not extracted on purpose** — see below. |
| `lib.rs` | 581 | The composition root. It is *supposed* to diverge: it wires Grain's services (rolling, routers, events server, pill supervisor, Grain Space). Cheap to merge, honest about what Grain adds. |
| `actions.rs` | 548 | Upstream's actions + thin `[GRAIN]` hooks after phase 6. One **deliberate hole** where upstream's `post_process_transcription` was — it cannot compile against Grain's `llm_client`; expect a modify/delete conflict and port by hand into `grain_post_process.rs`. |
| `managers/model.rs` | 487 | Legacy ONNX model entries removed. **Not restorable** — the `transcribe-rs` dependency is gone, so upstream's text would not compile. |
| `shortcut/mod.rs` | 206 | Dynamic shortcut lifecycle (send-to-AI, agent follow-up), Grain Space gating so the feature is zero-overhead when off. |
| `recorder.rs` | 197 | Additive `[GRAIN]` hooks: `with_sample_callback`, conditioning, `recorded_len`, plus the `mem::replace` fix (item 3 in UPSTREAMABLE). |
| `resampler.rs` | 179 | The delay-line drain fix (item 1 in UPSTREAMABLE). |
| `Cargo.toml` | 85 | Grain identity + the four Grain crates + `[patch]` pins. |
| `managers/audio.rs` | 83 | Prompt Record mark, conditioning toggle, live-consumer fan-out. |
| `tray.rs` | 82 | Single branded icon; no Windows taskbar-theme icon variants. |
| `transcription_coordinator.rs` | 58 | Grain's capture modes join the serialized record/transcribe lifecycle so they can never overlap. |
| `shortcut/handy_keys.rs`, `shortcut/tauri_impl.rs` | 49 / 41 | Dynamic-shortcut skips + Grain Space gating in both keyboard backends. |
| `commands/models.rs` | 47 | Category guard: `selected_model` may not be a streaming model. |
| `input.rs` | 43 | Agent copy-chord helpers (`release_modifiers`, `send_copy_ctrl_c`). |
| `managers/history.rs` | 34 | Windows delete retry (item 2 in UPSTREAMABLE). |
| `clipboard.rs` | 22 | Dictation into the Agent panel routes as an event, not an OS paste. |
| `commands/history.rs` | 15 | Re-transcribe only; never silently re-run AI on a redo. |
| `capabilities/*.json` | 16 | No `recording_overlay` window; window controls for the custom title bar. |
| `utils.rs` | 14 | Pill `SessionCancelled` instead of `hide_recording_overlay`. |
| `tauri.conf.json` | 13 | `com.grain.app`, Grain product name, **no auto-updater** (never re-add). |
| `main.rs` | 7 | `--pill` multicall entry point. |
| `commands/mod.rs`, `audio_toolkit/mod.rs`, `audio_toolkit/audio/mod.rs` | 11 | Module declarations + re-exports for Grain-owned files. |

### Files now byte-identical to upstream (0 divergence)

`settings.rs`, `llm_client.rs`, `overlay.rs` (all three inert — Grain's versions
are `grain_settings.rs` / `grain_llm_client.rs` / `grain_overlay.rs`),
`vad/{mod,silero,smoothed}.rs`, `audio_toolkit/bin/cli.rs`, `cli.rs`,
`portable.rs`, `apple_intelligence.rs`, `commands/audio.rs`,
`audio_toolkit/text.rs`, `catalog/*`, `managers/gguf_meta.rs`,
`managers/model_capabilities.rs`, `resampler`-adjacent mod files.

### Deliberately not extracted

`transcribe_rolling_chunk` and `initiate_model_load_for` stay inside
`managers/transcription.rs`. Moving them needs `with_engine_session` — the
engine-lifecycle heart (takes the engine out of the mutex, `catch_unwind`,
returns it) — plus `touch_activity`, both manager fields and two upstream
helpers made `pub(crate)`. That widens the encapsulation of the most
safety-critical code in the ASR path and creates external dependencies on
upstream internals, making future merges *harder*. Bad trade for ~90 lines.

## Verification at the time of audit

- `cargo check --lib` / `--bins`: clean, no warnings
- `cargo test --lib` (handy): **229 passed**
- `cargo test --workspace --lib`: **105 passed** across the 4 Grain crates
- `tsc --noEmit`: clean
- Tauri command surface: **180 before → 180 after, identical**
- Shortcut binding ids: **20 before → 20 after, identical**
- Event surface: +`StreamPhaseEvent` only (restored upstream type, additive)

## Conclusion

Every remaining divergence is either a documented product decision, a bug fix
queued for upstream in [UPSTREAMABLE.md](UPSTREAMABLE.md), or the composition
root doing its job. **The tree is ready for the Phase 7 folder move.**
