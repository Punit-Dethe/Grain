# Counterpart Audit — Grain-only modules vs upstream (Phase 3)

Audited 2026-07-19 against `upstream/main` (== v0.9.3 for all files below).
Question per module: **does upstream now ship a counterpart, and should Grain
re-layer on it?** Verdicts: **KEEP** (no counterpart, genuinely Grain),
**LAYER** (upstream owns the base; Grain rides on top), **RECLASSIFY**
(Grain rewrote an upstream file wholesale — move to Grain namespace, Phase 4).

Method: file-set diff (`git ls-tree` both sides) + capability greps. Upstream
deleted nothing Grain depends on; Grain deleted nothing from upstream's tree.

## The big one: rolling vs upstream streaming

Upstream's real-time path (transcribe-cpp `Stream`, the "Native ASR" port) and
Grain's `rolling.rs` + `crates/rolling-window` coexist and serve different
models: upstream streaming requires a **streaming-capable** model; rolling
gives real-time behavior with **any batch model** and adds RCSR seam revision
(retro-correction at chunk seams — no upstream counterpart). Both already
share ONE engine slot + the same `StreamRouter` frame fan-out after the
unification. **Verdict: KEEP both.** Rolling is a Grain capability, not a
parallel port of upstream streaming. Watch item: if upstream ever ships
chunked re-decode for non-streaming models, revisit.

## Verdict table

| Module | Upstream counterpart? | Verdict |
|---|---|---|
| `rolling.rs`, `crates/rolling-window` | Streaming exists but model-gated; no RCSR | **KEEP** (see above) |
| `stt_router.rs`, `stt_client.rs`, `commands/stt.rs` | None — upstream STT is local-only | **KEEP** |
| `post_process_router.rs`, `rotation_state.rs`, `crates/provider-router` | Upstream is single-provider (`llm_client.rs`) | **KEEP** (router); upstream's client → RECLASSIFY (Phase 4) |
| `llm_client.rs` (Grain's multi-provider rewrite) | Upstream file exists, single-provider | **RECLASSIFY** → `grain_llm_client.rs` |
| `overlay.rs` (Grain's native-pill rewrite) | Upstream webview overlay exists | **RECLASSIFY** → Grain namespace |
| `tray.rs` | Upstream tray exists; Grain diff is moderate (branding + non-panicking load) | **KEEP AS-IS** for now — divergence is small and behavioral; not worth an uncompiled shadow |
| `audio_toolkit/conditioner.rs` | None (no high-pass/AGC upstream) | **KEEP** |
| `audio_toolkit/snippets.rs` | None | **KEEP** |
| `dictionary.rs` (auto-dictionary) | Upstream has static custom words only | **KEEP** (layers on `text.rs` custom words) |
| `context_detect.rs` | None | **KEEP** |
| `prompt_record.rs`, `master_key.rs`, `voice_actions.rs` | None | **KEEP** |
| `agent.rs`, `bridge.rs`, `events_server.rs` | None | **KEEP** |
| `grain_space/**` | None | **KEEP** |
| `commands/{native_asr,post_process}.rs` | None (Grain command surface) | **KEEP** |
| `crates/grain-core` | Replaces upstream settings plumbing — upstream settings **fixes must be ported here** (already the documented rule) | **KEEP** |
| `crates/grain-pill` | Replaces upstream `RecordingOverlay.*` webview | **KEEP** |

## Things Grain lacks that upstream has (found during phases 1-2)

- `vad_enabled` setting (upstream lets users disable VAD; Grain hardcodes on).
  Port into grain-core when convenient — call sites already take `VadPolicy`.
- Upstream's overlay-style decision logic (`is_streaming` consumers) —
  deliberately not wanted; pill owns that UX.

## Net result

No Grain module needs retiring; nothing was rebuilt that upstream already
owned **except** the two Phase-4 reclassification targets, which are rewrites
of upstream files rather than parallel inventions. The fog was thin.
