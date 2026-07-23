# Grain-architecture-driven fixes (NOT upstream candidates)

Fixes in Handy-owned files made **because of Grain's architecture**. Do NOT send upstream.

| # | Fix | File | Marker | Why Grain | Why not Handy |
|---|-----|------|--------|-----------|---------------|
| 1 | Resampler tail-drop drain | `handy/audio_toolkit/audio/resampler.rs` | `output_delay` | Cloud STT clips tail word | Local models tolerate missing ~30-60ms |
| 2 | Windows delete retry | `handy/managers/history.rs` | `remove_file_with_retry` | Rapid create→delete in dev hit file locks | Normal deletes happen hours later |
| 3 | Per-chunk `mem::replace` | `handy/audio_toolkit/audio/recorder.rs` | `std::mem::replace` | Rolling-window tightens hot path | Batch clone is invisible on desktop |

Tracked as deliberate divergences in [UPSTREAM-DIVERGENCE.md](./UPSTREAM-DIVERGENCE.md).

---

## Extension points (worth discussing upstream)

Behaviour-preserving hooks that would let forks stop patching Handy's files:

| Hook | File | Enables |
|---|---|---|
| Per-frame sample callback (frame + VAD decision) | `recorder.rs` | Rolling/live transcription without touching recorder |
| Post-transcript hook | `actions.rs` | Post-processing/routing (voice actions, snippets) |
| Typed event tap | `lib.rs` | Out-of-process UIs (native pill) |

---

## Deliberately NOT upstreamable

ONNX/transcribe-rs removal, multi-provider routers, native pill, grain-core settings, all `grain_*` modules. Product decisions, not fixes.
