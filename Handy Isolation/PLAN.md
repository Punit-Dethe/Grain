# Handy Isolation — Plan

Goal: make the Handy-derived foundation of Grain *legible and isolated* so a
contributor can tell at a glance what is Handy (don't touch, updated by
upstream merges) and what is Grain (build here freely). End state: the
Handy-derived STT core lives in its own folder, byte-close to upstream, with
only small marked hooks; everything else is Grain-owned.

Scope: **isolation only.** The extension/plugin system and marketplace are a
separate effort, planned later. Nothing here designs them — but every hook
extracted in Phase 6 is a future extension point for free.

## Ground rules (apply to every phase)

1. **Upstream's text is canonical in the STT core.** Names, structure,
   comments — where Grain paraphrased, converge back to Handy's text.
2. **Grain deltas are additive, marked `[GRAIN]` blocks.** Never restructure
   upstream code to accommodate a Grain feature when an additive hook works.
3. **The check that can't lie:** after touching a Handy-derived file,
   `git diff upstream/main main -- <file>` must show *only* marked blocks.
   The divergence map (`Upstream Tracking/UPSTREAM-DIVERGENCE.md`) is a
   hypothesis; the diff is the truth. Update the map when the diff changes.
4. **Commit each verified chunk immediately** (per repo protocol). One
   phase-chunk per commit, `cargo check --lib` + `cargo test --lib` green
   before each.
5. Never rebase; never hand-edit `src/bindings.ts`; build with
   `CARGO_TARGET_DIR=C:\gtc` (the running app locks `C:/gt`).

## Measured baseline (2026-07-19)

- 63 backend files differ from upstream; merge base v0.9.3; trial-merge
  conflicts currently 0.
- `managers/transcription.rs`: **not a paraphrase.** Function names match
  upstream (3 differ per side); deltas are deliberate and marked (ONNX
  removal, `with_engine_session`, rolling chunk path, scrap-that, bridge
  mirror). Needs *parity restoration*, not re-baselining.
- `audio_toolkit/audio/recorder.rs` + `managers/audio.rs`: **the real
  paraphrase.** Grain renamed `with_audio_callback`→`with_sample_callback`
  and, worse, *lacks* upstream features that arrived after the port:
  `VadPolicy` (offline/streaming hangover), `RecordingState::Stopping`,
  cancellable extra-buffer sleep.
- `audio_toolkit/vad/*`: Grain is a pure regression — upstream's
  `SileroVad::reset` (LSTM state clear between sessions), `SmoothedVad`
  inner reset, `set_hangover_frames`, and the named VAD constants are
  missing on our side. No Grain feature lives here.

## Phases

### Phase 1 — Re-baseline the audio chain (NOW)

Adopt upstream's current text and re-thread Grain hooks additively.

- `audio_toolkit/vad/{mod,silero,smoothed}.rs`: take upstream **verbatim**
  (zero Grain content; recovers the Silero LSTM reset fix).
- `audio_toolkit/audio/recorder.rs`: upstream text (VadPolicy, VadConfig,
  `with_vad(detector, offline_hangover, streaming_hangover)`,
  `with_audio_callback`, `start(VadPolicy)`, `Error::other`) + `[GRAIN]`
  additive blocks:
  - `conditioning` atomic + per-frame HighPass + stop-time `normalize_gain`;
  - `recorded_len` (Prompt Record split mark);
  - `with_sample_callback(Fn(&[f32], Option<bool>))` — fires for EVERY frame
    with the VAD decision (rolling needs the continuous timeline); upstream's
    `with_audio_callback` (post-VAD speech frames → stream router) stays
    exactly as upstream wrote it. `handle_frame` gains only a marked
    `-> Option<bool>` return.
- `managers/audio.rs`: upstream text (VAD constants, Stopping state,
  cancellable buffer sleep, `try_start_recording(_, VadPolicy)`,
  `stop_recording(_, cancel_generation)`) + `[GRAIN]` blocks: conditioning
  seed + `set_conditioning`, `prompt_mark`/`arm_prompt_record`/
  `take_prompt_mark`, sample-callback wiring to rolling.
- Call sites (`actions.rs`, coordinator, any Grain mode paths): pass the
  VadPolicy chosen upstream-style (Disabled if !vad_enabled, Streaming for
  streaming-capable Native ASR, Offline otherwise — Rolling uses Offline;
  its sample callback sees every frame regardless of policy).

### Phase 2 — transcription.rs parity restoration

Keep the deliberate deltas (ONNX removal, `with_engine_session`, rolling
chunk, scrap-that). Restore upstream surfaces we deleted but could carry
inertly to shrink the merge surface: `stream_active` flag + `is_streaming()`,
`StreamPhase`/`StreamWorkKind`/`emit_stream_working` (emitting a Tauri event
nobody listens to is a no-op). Re-verify names against upstream.

### Phase 3 — Counterpart audit of Grain-only modules

One session. For each Grain-only file (`rolling.rs`, `stt_router.rs`,
`post_process_router.rs`, `agent.rs`, `context_detect.rs`, snippets, etc.):
does upstream now ship a counterpart? Verdict per file: keep / layer on
upstream's version / retire. The big question: rolling+RCSR vs upstream's
native streaming — decide what rolling still owns (RCSR seam revision is
genuinely ours) and what upstream's stream now covers.

### Phase 4 — Reclassify wholesale-replaced files

Files Grain rewrote entirely stop masquerading as Handy files:
- `llm_client.rs` (multi-provider) → move under a Grain module name; restore
  upstream's single-provider file verbatim, unused (or feature-gated off).
- `overlay.rs` → Grain pill glue moves out; upstream file restored inert.
- `tray.rs` → judgment call during the phase.
Effect: their divergence drops to ~0 with zero behavior change.

### Phase 5 — Divergence ratchet (CI)

Extend the existing 2-hourly trial-merge workflow: a checked-in budget file
(`Handy Isolation/budget.json`) maps each Handy-derived file to its current
`git diff upstream/main main -- <file>` line count. CI fails any push that
*increases* a budget. Shrinking auto-updates the budget. This makes Phases
2/4/6 un-regressable and removes any need for a feature freeze.

### Phase 6 — Extract [GRAIN] blocks from the composition files

`actions.rs` (68 markers) and `shortcut/mod.rs` (24): convert each block to
a one-line typed-event emission or hook call; feature bodies move to
Grain-owned modules; registration happens in `lib.rs` (the composition root,
which is *allowed* to diverge). Opportunistic, ratchet-enforced, no deadline.

### Phase 7 — The folder move (LAST)

When the ratchet shows the STT core down to hook-only diffs: one branch, one
`git mv` of the Handy-derived core into `handy/`, rely on merge-ort
directory-rename detection (fallback `git merge -s subtree`), and let the CI
trial-merge prove upstream tags still land before merging. Update
UPSTREAM.md / divergence map to the new layout. Contributor rule becomes:
**inside `handy/` = upstream + thin hooks, don't add features; outside =
Grain.**

## Status log

| Date | Phase | Done |
|---|---|---|
| 2026-07-19 | — | Plan written; baseline measured |
| 2026-07-19 | 1 | Audio chain re-baselined (`cca5ae45`): vad/* + cli.rs at divergence **0**; recorder.rs + managers/audio.rs additive `[GRAIN]` hooks only. Recovered upstream's VadPolicy profiles, Silero LSTM reset, Stopping state, cancellable buffer. Follow-up noted: port upstream's `vad_enabled` setting into grain-core. |
| 2026-07-19 | 2 | transcription.rs parity restored (`ced99522`): StreamPhase/StreamWorkKind/StreamPhaseEvent, stream_active + is_streaming + emit_stream_working back, event registered in lib.rs. Remaining divergence = deliberate (ONNX removal, with_engine_session, rolling chunk, scrap-that). |
| 2026-07-19 | 3 | Counterpart audit written (AUDIT.md, `78da1e76`): nothing to retire; rolling+RCSR stays; watch items recorded. |
| 2026-07-19 | 4 | llm_client.rs + overlay.rs reclassified (`78da1e76`): Grain rewrites now `grain_llm_client.rs`/`grain_overlay.rs` with `pub(crate) use` aliases; upstream files restored byte-identical, un-compiled. Divergence for both paths: **0**. tray.rs verdict: keep as-is. |
| 2026-07-19 | 5 | Divergence ratchet live: `ratchet.py` + `budget.json` (32 files, measured vs merge base, Cargo.lock excluded) + `.github/workflows/divergence-ratchet.yml` on push/PR. Close-out step added to UPSTREAM.md. |
| 2026-07-19 | 6.1 | Grain shortcut actions extracted to `grain_actions.rs` (`8cc3df14`): rolling/NativeAsr/switcher/master-chord/Agent/Grain Space actions + SESSION_ID; ACTION_MAP keeps one `register` hook. actions.rs budget 1760 → 875. |
| 2026-07-19 | 6.2 | Post-process rotation machinery → `post_process_router.rs` (`7f9c5bb6`): post_process_rotated, timeout wrapper, ROLLING_SEAM_PROMPT. actions.rs budget 875 → **722**. Remaining actions.rs divergence: TranscribeAction pill/warm-up/cancel threading + post_process_transcription context layers + `run_one_provider` reasoning/Apple extensions. Next targets: those threads, then `shortcut/mod.rs` (562). |
