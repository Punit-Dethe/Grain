# Flow accuracy implementation plan

2026-09-26. Target: Handy Parakeet TDT 0.6B v2 GGUF, initially Q8_0. Preserve the journal, one serial worker/model lease, existing final processing and bounded audio memory. Grain-owned conditioning is excluded.

## Order of work

1. Extend the existing offline evaluator with frame-aligned independent native windows at 15, 24.96 and 30 seconds. Compare with complete-audio Batch and the current Fluid-style decoder. Keep exact PCM, native token IDs and full per-window measurements.
2. Evaluate the simpler baseline before building a new native state protocol. If larger native windows reduce observed disagreement without regressions, implement a bounded pause-aware cursor using the existing audio buffer. Prefer natural quiet boundaries after a minimum context, keep a hard ceiling, and preserve overlap and subsampling phase. Use ordinary native inference; short recordings retain the complete supplied audio and Batch's exact decoder policy.
3. If that baseline is insufficient, evaluate the stateful buffered candidate from the second research report. Native state must include predictor state/output, previous token, duration overshoot and absolute owned-frame continuity. Decode each owned frame once; do not combine state carry with repeated overlap. Update the native patch/ABI contract and validate split versus uninterrupted decoding before runtime integration.
4. Integrate only the selected candidate for v2. Keep v3 on its existing reviewed contract until measured independently. Cancellation, failure and replay must preserve the recording and release temporary/native state. No preview, extra model, new worker/service or final full-recording rerun.
5. Compare pause-aware 20/24/28/30-second ceilings using identical recordings and serial processes. Cross-check published v2 context/latency settings before choosing the smallest ceiling that retains the measured accuracy benefit. Verify sample/frame coverage, exact boundary conditions, quiet and continuous speech, conflicting overlap tokens, final words, rapid session reset and failure replay. Run targeted Rust checks/tests, native checks if touched, and the real-model evaluator. Record process memory and real stop-path limits separately from total inference time.
6. Document measured outcomes and remaining live-app gates, commit and push. Batch disagreement is a diagnostic; do not call it WER or claim perfect accuracy without human reference transcripts.

Sources and hypotheses: [initial audit](FLOW-ACCURACY-AUDIT.md), [v2-specific research](PARAKEET-V2-FLOW-RESEARCH.md).

## Expected code areas

- `scripts/flow-audit/`: offline candidate measurements and reproducibility.
- `crates/grain-tdt/`: bounded cursor and native-token merge policies.
- `src-tauri/src/tdt_flow.rs`: v2 inference selection, exact timestamps and final detokenization.
- `src-tauri/src/rolling.rs`: existing worker integration and cleanup only.
- `vendor/transcribe-cpp*` only if the stateful candidate is required; ordinary upstream Batch must remain unchanged.

## Selection gate

Structural correctness is mandatory. Retain clean short clips and assess every changed hypothesis rather than optimizing one aggregate score. Require a useful reduction in the observed long-recording errors with acceptable resource use; referenced WER and real application feedback remain explicit follow-up gates when those inputs are unavailable.

## Selected implementation

The broader ceiling sweep selects ordinary native inference with **24 seconds maximum input**, two seconds overlap, and quiet cuts after 720 ms low energy in the last 3.04 seconds (earliest cut 21.68 seconds). Unlike the initial 30-second candidate, this balances the expanded recording set's agreement with lower worst-case input and memory. Exact PCM is preserved; short audio is decoded whole without extra padding. Every stable window runs once and only the remaining bounded tail is decoded at stop.

The Grain-owned cursor and model-specific adapter retain the existing journal, serial worker, native lease, failure replay and cancellation path. Only v2 selects this policy; v3 keeps the existing adapter. No native ABI, vendor decoder, upstream Handy tree, frontend, normalization or post-processing changes are required.

The independent-window baseline provides a useful measured improvement, so the native state-carry candidate is deferred. Introducing that protocol solely to match another runtime's published results would require a separate state/ownership and accuracy gate. Selection evidence, changed individual hypotheses and hardware limitations are recorded in [accuracy/speed selection](FLOW-ACCURACY-SPEED.md).

## Verification completed

- Pure geometry/merger: 22 tests pass; evaluator: two tests pass; both Clippy checks pass with warnings denied.
- Full backend `cargo check` passes, with existing unused-import warnings outside this change.
- Adapter: five routing/timestamp tests pass. Opt-in real-model test passes on all 12 local v2 Q8_0 fixtures, exercising 8,001-sample appends and fresh-accumulator journal replay for every case (24 app-adapter renderings). Input buffer capacity remains bounded by the selected ceiling.
- Existing five journal tests and the rolling model-load barrier test pass. These verify journal cleanup and the load barrier; they do not constitute a new slow-worker/cancellation/disk-error soak.
- CPU four/two-thread and explicit Vulkan comparisons are recorded in the measurements report. Six surviving original inputs through 24 seconds match exact CPU Batch native IDs; all tested short GPU inputs match GPU Batch text.
- Graph impact/change/review tools were consulted. Their indexed impact output does not cover the new cursor sufficiently; direct source review and the real journal regression supply that evidence.
- Windows lib-test loading required a Common Controls v6 manifest on the disposable test executable, as in the prior rewrite. No application source or package manifest changed.

The owner finalized 24 seconds after these measurements. Further tuning is deferred until later pre-release work. Live stop-to-paste timing, actual lower-powered hardware, referenced WER and packaging/platform validation remain explicitly unmeasured.
