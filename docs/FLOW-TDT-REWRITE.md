# Parakeet TDT Flow rewrite

Status: plan/final audit and pure TDT foundation implemented on `core/rolling-window`, 2026-09-10. Native upgrade and production integration are not implemented yet.
Owner intent: reproduce FluidVoice's Parakeet TDT v2/v3 recording behavior, simplify Grain's Flow implementation, and upgrade transcribe.cpp. This document is the durable execution and handoff record; update checkboxes and evidence as work lands. Do not infer completion from a design decision.

## Scope and invariants

- Only reviewed Parakeet TDT 0.6B v2/v3 GGUF catalog artifacts and their supported quantizations belong to Flow. Reject unsupported models and translation before capture. Standard and Native ASR retain their existing architectures.
- Replace the generic rolling implementation. Delete unused runtime code after migration; no generic fallback or parallel legacy Flow implementation at completion.
- Grain owns scheduling, audio storage, merging, and lifecycle. The Handy-derived backend gets only the minimum marked `[GRAIN]` engine lease hook. Preserve unrelated user changes, including the pre-existing `src/app/bindings.ts` change.
- Correctness, then bounded RAM/CPU, then maintainability. Keep one serial inference worker off the capture callback. Release the worker, native session lease, buffers, and temporary journal at termination.
- Matching the algorithm does not establish matching speed or accuracy across CoreML and GGUF backends. Real-model measurements are required before claiming parity or release readiness.

## Local sources and provenance

Use the local repositories; no GitHub browsing is needed.

- FluidVoice: `Refrence/FluidVoice-main/FluidVoice-main`. App orchestration is GPL-3.0: inspect behavior, implement Grain orchestration independently; do not copy source.
- FluidAudio: `Refrence/FluidAudio`, FluidVoice-pinned commit `3fd63887eef1dc25edea8263ce4b44aa854d898b`. Apache-2.0 algorithm ports require retained attribution/license.
  - `Sources/FluidAudio/ASR/Parakeet/ParakeetChunkLayout.swift`
  - `Sources/FluidAudio/ASR/Parakeet/ParakeetIncrementalSession.swift`
  - `Sources/FluidAudio/Shared/AsrChunkTokenMerger.swift`
  - `Sources/FluidAudio/ASR/Parakeet/Decoder/` and `Shared/ASRConstants.swift`
- transcribe.cpp: `Refrence/transcribe.cpp`; target stable `v0.2.3` (`63a44d9`), not the later unrelated Voxtral commit. Current vendor base is published 0.2.0, upstream `93151602c670f0dbf6703b1d87a8822f87581a0b`.

## Intended behavior

1. Journal exact 16 kHz mono Float32 capture samples on disk. Preserve causal capture high-pass if enabled; remove per-window gain normalization because it changes shared overlap samples. Convert to PCM16 only when saving history WAV. Approximate temporary disk cost: 230.4 MB/hour; model input RAM stays bounded.
2. Through **240,000 samples inclusive**, preview/final use the complete prefix with isolated decoder state. Never finalize a 238,080-sample chunk before crossing the 240,000-sample short-path threshold. Pad this short input to the next 1,280-sample frame only when that stays <=240,000. This alignment does not change journal count or reported duration. Fluid's short call leaves decoder `isLastChunk=false`, including final transcription.
3. Above 240,000 samples, use Fluid's 238,080-sample content windows, 32,000-sample overlap, 206,080-sample stride, and 1,280-sample mel context on later windows. A long-path window is stable when `available > chunk_start + 238080`.
4. Decode each window independently. Finalized windows run once; only the unfinished tail is revisited. Merge token IDs with absolute encoder-frame timestamps using contiguous overlap, LCS fallback, then timestamp-midpoint fallback. Carry confidence/duration; do not match text fragments.
5. Cache preview by exact accepted sample count. Finish at that count reuses the result. Stable windows still process while previews are disabled. Short-prefix caching is an intentional Grain improvement: FluidAudio's incremental session supports it, but FluidVoice's short app path calls regular transcription again on finalization. Isolate short decodes so caching cannot carry earlier predictor state.
6. One completion-driven worker: wait 600 ms **after inference completes** before the next preview attempt, minimum 1 second of audio, unchanged sample count produces no inference, slow work coalesces updates without a backlog. Minimum-duration handling belongs in the session, not the geometry helper.
7. Normal stop drains recorder then closes journal and drains inference; cancellation aborts inference and joins/cleans up. Session generations reject stale events. Preserve Prompt Record, history, final post-processing, and model unloading.
8. A non-cancellation failure invalidates accumulation and allows one clean replay using the same TDT pipeline and journal. No generic/Standard fallback. Specify replay timing and error propagation in lifecycle tests before wiring it.

## Native adapter: audit gate

One concrete adapter should contain transcribe-specific capability checks, run options, local-to-absolute timestamp conversion, and full-ID detokenization. The app-facing append/preview/finish session contract is the future seam. Avoid a plugin/provider hierarchy.

transcribe.cpp 0.2.3 retains the existing safe Rust API and releases offline scratch after each run; it does not expose native incremental v2/v3 TDT. Re-vendor pristine sources, then keep only justified, reviewed extensions. Full-sequence detokenization is required for v3 UTF-8 byte-fallback correctness; concatenated per-token strings are unsafe.

**Audit correction:** detokenization alone is insufficient. Add a stateless, explicitly opted-in Parakeet TDT window run contract through the native family-extension mechanism (or similarly narrow API after inspecting its constraints). Every call starts fresh predictor state; no opaque long-lived Flow handle, state carry, descriptor sequence, queue, or journal policy in C++. Ordinary run/stream defaults must remain unchanged. Full-sequence detokenization is a separate generic tokenizer API.

The decoder contract must carry context start frame, valid content frames, and last-tail status. Fluid begins at the context frame and limits decoding to `min(encoderLength, ceil(contentSamples/1280))`; it does not add context to the limit. Filtering context tokens after an ordinary native decode cannot reproduce this because those tokens already affected predictor state. Fluid timestamps add `chunkStart/1280` to the local decoder frame, not `readStart/1280`; preserve this 80 ms distinction in fixtures.

Fluid's main decoder uses duration bins `[0,1,2,3,4]`, a 10-symbol step limit, a 150-token main-loop cap, repeated nonblank duration-zero advancement, and a post-advance active check before emission. Long-path final tails get up to ten extra boundary steps and stop after five consecutive blanks. These controls differ from current native greedy TDT. Before changing C++, make scripted-logit tests for emission/state updates, zero durations, limit crossings, and tail frames; include fast-speech and repeated-tail real audio to detect truncation or duplication. Preserve the reference policy as an explicit opt-in; do not silently change Standard decoding or globally replace GGUF metadata defaults.

Still to measure: fixed CoreML padded input/original-length preprocessing versus ggml mel/encoder normalization and frame lengths. Do not blindly add 240,000 samples of padding to ordinary native run. Reproduce separate semantic input/valid-length handling in the adapter and validate encoder geometry before asserting equivalent behavior. Reference implementation differences can be deliberate only when documented with evidence.

Optional FluidVoice CTC vocabulary rescoring and pronunciation customization are separate from this TDT-window refactor. They require an additional acoustic path/model; Grain text replacements are not equivalent. Keep them recorded as a parity gap, not silently included in speed/accuracy claims. Any later decision to add that acoustic feature must account for its model/RAM cost.

## Planned code shape and deletion inventory

- Small Grain-owned Flow service (`grain_flow.rs` or equivalent) for capture lifecycle and worker scheduling.
- Pure TDT window/token core with no Tauri/model dependency; expose only the data/functions needed by Flow. Keep it directly testable without compiling the app.
- One private transcribe adapter and TDT session accumulator, storing merged stable tokens plus cached tail and window cursor. Do not retain every finalized window or whole audio in memory.
- Retain/refine `src-tauri/src/grain_audio_journal.rs` as Float32 journal with reusable read/write buffers.
- Replace `src-tauri/src/rolling.rs` and `src-tauri/src/tdt_flow.rs`.
- Delete `crates/rolling-window` and its dependency; remove cursor, timeline assembler, lexical seam repair/canonicalization, VAD cut policy, descriptor channel/queue, debt/RTF bookkeeping, generic fallback, native-text booleans, and per-window AGC.
- Remove old vendor TDT state-carry ABI, feature, options, generated symbols, projector/validation files and tests.
- Remove `TranscriptionManager::transcribe_rolling_chunk`; retain a minimal engine-lease hook. Remove Flow-specific VAD preload/consumption without refactoring the shared upstream recorder.
- Preserve `rolling_live_preview` storage/command compatibility; update its description and active docs. Supersede `docs/TDT-FLOW-PUNCTUATION-PLAN.md`; historical archived docs remain historical.

## Execution checklist

- [x] Initial local architecture audit and deletion map.
- [x] Create isolated branch `core/rolling-window`; preserve existing bindings change.
- [x] Persist this plan before source edits.
- [x] Final local decoder/latency/accuracy audit; incorporate corrections below.
- [ ] Capture baseline tests and available real-model fixtures (legacy pure tests captured; real fixtures still needed).
- [x] Pure TDT geometry/merger implementation and partition/boundary tests (`crates/grain-tdt`; integration pending).
- [ ] Pristine transcribe.cpp 0.2.3 vendor replacement, exact dependencies/lockfiles/provenance.
- [ ] Minimal native API extensions justified by final audit; regenerated bindings/ABI digest and tests.
- [ ] Float32 journal, exact overlap reads, wake/close/cancel lifecycle and cleanup tests.
- [ ] TDT decoder/session, short path, cached preview, stable windows, finalization and recovery.
- [ ] Flow service and action wiring; early model gate and explicit start errors.
- [ ] Delete superseded runtime/crate/native APIs; update active docs and licenses.
- [ ] App/native checks, real-model parity, latency/memory soak, packaged backend regression.
- [ ] Final integrated review, commit and push only task-owned files. Keep branch isolated from main; commit completed phases separately.

## Acceptance evidence required

- Geometry: 0, 15,999, 16,000, 238,080, 238,081, 240,000, 240,001, stride boundaries, exact and unaligned tails. Random append partitions must produce the same finalized windows/results.
- Merger: no overlap, empty windows, repeated tokens, contiguous match, LCS, midpoint fallback, same-time ordering, multilingual IDs. Full-sequence byte-fallback decoding must preserve Unicode.
- Lifecycle: previews disabled produce no tentative-tail work; same-count finish performs zero extra inference; recorder drain, slow inference, cancellation, repeated stop/start, stale completion, disk/native errors, single replay and cleanup.
- Real v2/v3: incremental equals a batch reference using identical native model/backend/options and chunk algorithm for IDs/text/timestamps/confidences/durations. Compare cross-backend Fluid outputs separately; do not promise bit-identical CoreML/GGUF output.
- Speed/accuracy: short speech, boundary words, repeated phrases, silence, fast speech, multilingual v3; measure WER/CER and stop-to-final p50/p95 on identical recordings/hardware against current Grain. Report preview-enabled versus disabled and post-processing separately.
- Memory: >2h recording and 50 sequential sessions; model + transcript + one window RAM, bounded scratch, no temp-file leaks. Account for disk growth and disk failure.
- Regression: Standard, Native ASR, Prompt Record, history persistence, post-processing, cancellation/model changes, immediate unload. Windows x64 Vulkan/dynamic modules and ARM CPU, macOS Metal, Linux Vulkan; package loading and linker checks (ggml changed from 0.15.2 to 0.20.2).
- Verify with targeted pure/native tests first, then relevant Cargo/app checks and upstream divergence policy. No browser/computer UI automation; user runs real Tauri app for any visual confirmation.

## Progress and discoveries

### 2026-09-10

- Graph queried first; reference Swift code is not indexed, so local file inspection is the fallback. RTK is unavailable; underlying commands are used.
- Final audit corrected the initial plan: first long-path window starts only when total samples exceed 240,000, not at 238,081 (`ParakeetIncrementalSession.processStableWindows`).
- Implemented `crates/grain-tdt` with no dependencies: sample-count window cursor, short-prefix alignment, decoder-window geometry, and an in-place port of Fluid's token merger. Stable accumulation is not sorted; only a result view may be stably sorted. Contiguous ties choose first; LCS traceback ties decrement right index; matched tokens keep left metadata; gap ties choose left; midpoint uses Fluid's exact Double comparison order. A review caught that mathematically equivalent integer cuts can select different tokens at ordinary timestamps; the port preserves reference Float64 behavior and includes a regression fixture.
- The new crate is not connected to production yet. Existing Flow remains operational until the native contract and integration land; it must be deleted when replacement is complete. The separate pure crate enables tests without loading Tauri or a model and adds no runtime engine/service.
- Baseline: `cargo test -p rolling-window --offline` passed all 66 tests. Initial new core tests and `cargo clippy -p grain-tdt --all-targets --offline -- -D warnings` passed; final counts recorded before commit below.
- Read the actual decoder controls in `ParakeetChunkLayout.swift:125`, `TdtDecoderV3.swift:153`, `:291`, `:371`, `:426`, and native `src/arch/parakeet/decoder.cpp:1075`. Ordinary native run is not a drop-in equivalent. Native extension scope above supersedes the earlier detokenize-only proposal.
- FluidVoice schedules previews 600 ms after completion; short final cache reuse is a Grain improvement, not existing FluidVoice app behavior. FluidAudio already trims retained audio; the disk journal supports Grain persistence/replay while keeping capture RAM bounded.
- Current branch started from `main`; `src/app/bindings.ts` was already modified. Do not stage that unrelated change.
- No speed/accuracy parity has been measured for the new implementation yet.
- Foundation verification: 16 new tests pass, including 32 randomized append partitions over a >2h sample timeline (geometry only, not an audio/inference soak); Clippy passes with warnings denied; formatting passes. The source review corrected short-path tail policy and Float64 midpoint parity. Graph review cannot establish coverage of newly added, unindexed files; direct source review and targeted tests supply this phase's evidence.
- `docs/` is ignored repository-wide. Explicitly track this requested plan with `git add -f docs/FLOW-TDT-REWRITE.md`; do not unignore all existing docs.

### Next concrete implementation step

Prepare pristine transcribe.cpp v0.2.3 wrapper/sys sources and verify provenance. Before wiring Grain Flow, implement and test the stateless Parakeet TDT run extension described above (existing public family extension header: `include/transcribe/parakeet.h`) and generic full-ID detokenization. V2 delegates to Fluid's V3 decoder with blank ID 1024 instead of 8192; do not implement two separate greedy loops. Inspect native mel valid-length handling before selecting padding. Keep legacy Flow compiling until the new adapter/session can replace it atomically, then remove legacy code and update the deletion checklist.

## Resume procedure

Read this document, `AGENTS.md`, `git status`, and the latest task commits. Inspect unchecked phases and evidence above; do not restart completed phases or assume planned code exists. Check local reference pins before changing algorithms. Update this document in every meaningful implementation commit with tests actually run, unresolved blockers, and the next concrete step.
