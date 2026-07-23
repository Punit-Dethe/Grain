# Handy Isolation — Plan

> **Historical document.** All seven phases shipped (2026-07-19/20). The living
> infrastructure this plan produced — ratchet.py, budget.json, UPSTREAMABLE.md,
> the runbook — now lives in `Upstream/`.

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

**Three kinds of divergence, three different treatments.** The ratchet counts
added **and removed** lines, which makes the distinction load-bearing:

1. **Grain additions** woven into upstream functions (pill emits, extra
   params, feature hooks). Extracting them into a Grain module genuinely
   removes lines from the diff — this is where phase 6 pays.
2. **Grain rewrites** of upstream code (e.g. `run_one_provider` returning
   `CallOutcome` for rotation). Moving these out leaves a same-sized hole:
   net zero. The only real fixes are to restore upstream's version and layer
   on top, or accept the divergence and mark it clearly.
3. **Grain deletions** of upstream code. Restoring them is free divergence
   reduction — and, as chunk 3 found, often restores a lost upstream fix.
   Audit these first; they are pure win.

Before extracting, classify the hunk. If it is kind 2, prefer parity
restoration over relocation.

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
| 2026-07-19 | 6.2 | Post-process rotation machinery → `post_process_router.rs` (`7f9c5bb6`): post_process_rotated, timeout wrapper, ROLLING_SEAM_PROMPT. actions.rs budget 875 → 722. |
| 2026-07-19 | 6.3 | **Parity restoration** (`2ddca402`) — kind-3 divergence, all accidental losses: upstream's effective-language gate for OpenCC (real bug: a stale zh intent rewrote CJK from non-Chinese models) + `resolve_effective_language`; two cancellation guards in `TranscribeAction::stop` deleted with the overlay calls they sat beside (a cancel ran a full decode before anyone noticed); silent transcription failures (`debug!`, no toast) → upstream's `error!` + `transcription-error`; `is_blank_transcription` + its 2 tests. budget 722 → 646, tests 227 → 229. |
| 2026-07-19 | 6.4 | Pill session events → `grain_actions` helpers (`5f38b0f4`): 6 ProcessingComplete blocks, 3 start pairs, 3 stop reads, CancelAction's 18-line teardown all become one-liners; SESSION_ID no longer referenced from actions.rs. Shortcut order preserved (deferral lives in master_key/shortcut — deadlock-safe). budget 646 → **571**. |
| 2026-07-19 | 6.5 | 24 Grain-only `#[tauri::command]` settings mutators (+ `DetectedApp`) → `grain_commands.rs` (`c48d0878`): context awareness, auto-dictionary, scrap-that, snippets, voice actions, app modes, Agent ×5, Grain Space ×8, rolling preview, audio conditioning. Command *names* unchanged → frontend/bindings unaffected. Registration/dispatch machinery untouched. `shortcut/mod.rs` budget 562 → **206**. |
| 2026-07-20 | 6.6 | Grain's post-processing → `grain_post_process.rs` (`3420d033`): `post_process_transcription` + `run_one_provider`. **Found the limit of the phase-4 inert pattern**: it works file-level (un-compiled) but NOT function-level — inline code must still typecheck, and upstream's version calls `llm_client::send_chat_completion*` with a signature Grain's client dropped. So actions.rs carries a documented hole + marker comment there. actions.rs 1245 → 754 lines. |
| 2026-07-20 | — | **`settings.rs` made inert** — the single biggest win. Grain's 103-line facade → `grain_settings.rs`; upstream's 1487-line file restored un-compiled with a `pub(crate) use` alias. Divergence **1470 → 0**. |
| 2026-07-20 | 6.7 | `finalize_transcript` + its 3 tests → `audio_toolkit/grain_text.rs`. text.rs **105 → 4**. |
| | | Noted for upstreaming: `resampler.rs`'s `finish()` fix (drains the FFT delay line; upstream silently drops the recording's last fraction of a second — audible as a clipped tail word on cloud STT). |
| | | **Totals: 5561 → 3621 diverged lines, 32 → 31 files.** Remaining: `managers/transcription.rs` (749 — next target; `transcribe_rolling_chunk` + `initiate_model_load_for` want a Grain extension trait, needs `pub(crate)` accessors for `model_manager`/`with_engine_session`; it is the ASR hot path, so do it with a clear head), `lib.rs` (621 — composition root, *allowed* to diverge), `actions.rs` (548 — now mostly upstream text + thin hooks), `managers/model.rs` (487 — deliberate ONNX removal, not restorable: the transcribe-rs dep is gone). |
| | | ~~**Totals after this session: 5561 → 5206 diverged lines**~~; the four files worked on fell 1760→569 (actions), 562→206 (shortcut), plus vad/cli/llm_client/overlay at 0. Remaining top offenders are `settings.rs` (1470 — a deliberate facade over grain-core), `managers/transcription.rs` (749) and `managers/model.rs` (487, deliberate ONNX removal), `lib.rs` (612 — the composition root, *allowed* to diverge). Remaining actions.rs divergence is mostly kind 2 (`run_one_provider`'s CallOutcome rewrite, post-process context layers) + Prompt Record / cloud warm-up threading. Next: phase 7 folder move once these settle. |
