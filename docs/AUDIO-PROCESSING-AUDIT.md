# Audio processing audit — 2026-09-26

Compared with fetched Handy `8f9cf53cd1410cda26beea39ff802ac306e39585`.

Grain added two default-on enhancements absent from Handy: an 85 Hz high-pass
filter on every resampled capture frame, and boost-only, noise-gated automatic
gain on the finished Batch buffer. Live ASR/Flow received the high-passed frames;
Batch received both stages. Enhancement was applied inside the shared recorder,
including stop-time pending buffers, channel drain and resampler flush.

Removed both implementations, filter state/scratch allocations, toggle atomics,
audio-manager configuration, persisted setting and command registration. Removed
the Voice Processing control, quick-panel entry, store mutation and retired
English copy. Regenerated bindings from Rust; preserved existing unrelated
bindings whitespace edits and Cargo manifest changes.

Every capture path now forwards ordinary resampler output directly to VAD,
Batch and live consumers without Grain filtering or gain changes. Old saved
`audio_conditioning` values are ignored and omitted on the next settings save.

Handy's normal mono/channel conversion, 16 kHz resampling and VAD remain.
Grain's Flow journaling, bounded capture, Prompt Record positions and existing
resampler tail-preservation fix also remain. This removes enhancement policy
divergence; the separately deferred recorder rewrite and optional Earshot VAD
are not implemented by this change. Text post-processing is unchanged.

Verification:

- 561 backend tests pass, 3 fixture/export tests remain ignored. Five tests for
  the deleted enhancer were removed; one capture regression was added outside
  the Handy tree. It checks quiet speech/DC input at 16/48 kHz against ordinary
  resampling, for live frames, pending stop buffers, drain and final Batch audio.
- 203 Grain core tests pass, including old conditioning settings loading without
  losing other fields and omitting the retired key on serialization.
- 104 frontend tests pass; TypeScript, targeted ESLint and Prettier checks pass.
  Binding export and upstream policy/port audits pass.
- Shared-tree measured divergence falls by 82 lines: recorder 1,337 → 1,270,
  audio manager 394 → 382, audio exports 2 → 0, composition root 1,237 → 1,236.
  The audio manager returns within its existing budget. Six unrelated ratchet
  failures remain; no new failure or budget increase is introduced.
- No live microphone or cross-platform runtime test was performed. The capture
  regression exercises the real consumer without a physical device.
