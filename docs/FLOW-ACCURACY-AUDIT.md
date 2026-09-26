# Flow accuracy audit — 2026-09-26

Status: historical investigation and initial offline experiments complete. The follow-up [implementation plan](FLOW-ACCURACY-IMPLEMENTATION.md) and [accuracy/speed measurements](FLOW-ACCURACY-SPEED.md) supersede this report's unchanged-production status. This is not an accuracy-parity claim.

Follow-up: [Parakeet v2 research round two](PARAKEET-V2-FLOW-RESEARCH.md) identifies NVIDIA's v2-specific stateful buffered inference results and adds that candidate alongside the pause-aware baseline. The implementation and measurements linked above record the subsequent selection.

## Findings

The same Parakeet v2 Q8_0 artifact produces different text when Grain changes its input windows. The strongest measured contributor is recognition within bounded windows. The custom Fluid-style decoder also changes endings and punctuation. Neither replacing the decoder alone nor adding a fixed two seconds of future context removed the observed disagreements.

The original rewrite reproduced FluidAudio's chunking and decoding policy, but its real-model accuracy gate remained open. FluidAudio's own long-form Batch path is already chunked using the same inference path as its incremental service. Handy's ordinary GGUF Batch path runs the complete supplied recording through transcribe.cpp. Matching the CoreML application's window policy therefore does not establish parity with Handy's Batch result.

Grain gain normalization is excluded from the investigation at the owner's request. `handy/audio_toolkit/audio/conditioner.rs` is explicitly `[GRAIN]`; upstream Handy has no corresponding file. Both sides of every experiment received identical WAV samples without an additional normalization pass.

## Current execution path

```text
16 kHz continuous capture
  -> exact Float32 journal, including silence
  -> one serial worker with coalesced wakes and one model lease
  -> fixed content windows and independent decoder state
  -> native token IDs + absolute encoder timestamps
  -> overlap token merger
  -> one final dictionary/filter/LLM stage at stop
```

- Through 240,000 samples / 15 seconds inclusive: one complete prefix at stop. Short inputs can receive up to one encoder frame of zero padding.
- Above that threshold: 238,080 samples / 14.88 seconds of content, 32,000 samples / 2 seconds of overlap, and a 206,080-sample / 12.88-second stride. Later inputs include 1,280 samples / 80 ms of preceding mel context.
- The worker waits until a stable window is available, but the encoder reads only through that window's content end. Availability of later captured audio does not mean that audio is included as right context.
- Flow capture sets `VadPolicy::Disabled`; the rolling service ignores the speech argument. Boundaries have no preference for pauses, sentence ends or complete words.
- Every window starts fresh predictor state. Overlap provides a second recognition of boundary audio; it cannot make the first window's encoder see the entire recording.
- The native `PKFW` extension uses `decode_tdt_fluid_window`, while ordinary Batch uses `decode_tdt_greedy`. The Flow decoder changes SOS priming, repeated zero-duration handling, emission after duration advance, a 150-token main-loop cap, and an extra final-tail loop.
- Window failures do not commit the cursor. Stop drains recorder and journal before finishing inference; failures allow one clean replay. Source review found no evidence of deliberate audio dropping in this path. The offline evaluation does not prove live capture lifecycle behavior.

Relevant production sources: `src-tauri/src/rolling.rs`, `src-tauri/src/tdt_flow.rs`, `crates/grain-tdt/src/layout.rs`, `crates/grain-tdt/src/merge.rs`, `src-tauri/src/grain_actions.rs`, and vendored `src/arch/parakeet/{model,decoder}.cpp`.

## Controlled evidence

Model: `handy-computer/parakeet-tdt-0.6b-v2-gguf/parakeet-tdt-0.6b-v2-Q8_0.gguf`, Hugging Face snapshot `07cee0616125a08ef619729bb47f40ef747e4bc4`. Native version 0.2.3, Grain-patched library; CPU with four decoder threads. All paths use the same recorded samples and no language hint. The WAV history conversion to PCM16 is common to both paths.

The evaluator uses the actual `grain-tdt` planner and merger. Its complete-audio processing is equivalent to draining the deterministic cursor at stop; it does not simulate the microphone, journal races, live CPU contention or cancellation.

| Recording suffix | Seconds | Flow windows | Batch words | Flow/Batch word edits | Native decoder in same windows/Batch |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1790394210 | 1.62 | 1 | 5 | 0 | 0 |
| 1790394222 | 7.86 | 1 | 23 | 0 | 0 |
| 1790394198 | 8.16 | 1 | 23 | 0 | 0 |
| 1790394095 | 3.06 | 1 | 9 | 0 | 0 |
| 1790394090 | 3.39 | 1 | 9 | 0 | 0 |
| 1790395086 | 17.19 | 2 | 22 | 0 | 0 |
| 1790394256 | 23.22 | 2 | 45 | 3 | 3 |
| 1790394338 | 27.03 | 2 | 63 | 3 | 3 |
| 1790394135 | 37.65 | 3 | 23 | 0 | 0 |
| 1790395028 | 83.76 | 7 | 186 | 7 | 7 |
| 1790393887 | 82.89 | 7 | 179 | 4 | 4 |

Five short recordings had zero lexical differences, with some punctuation differences. Four of six longer recordings differed. The total is 17 word edits over 587 Batch words; this is **not WER** because Batch is not a human reference. For example, Batch contained an isolated `s` that Flow omitted. Two short clips are similar repeat utterances, and these recordings are not an independent benchmark corpus.

Window-level inspection distinguishes acoustic recognition from merging:

- In the 27.03-second recording, the first window recognized `face` where full Batch recognized `stay`, near the beginning of the clip. That difference existed before merging and was far from the seam.
- The same window ended with `very high`; the next window recognized `very hard to gauge`, and merging correctly retained that continuation.
- An isolated 14.88-second crop produced `accuracy.` with ordinary inference and `accur` with the custom decoder on exactly the same padded input. In the complete recording, later overlap can repair such a crop; this is evidence of decoder behavior, not proof that this was the dominant final-text error.
- The largest tested window produced 86 tokens. The 150-token cap did not explain these recordings, although silently returning a capped result remains a correctness concern for dense speech and other tokenizations.

Two-second extra right context, diagnostic only:

| Seconds | Current Flow/Batch edits | Native windows + 2 s right context/Batch edits |
| --- | ---: | ---: |
| 23.22 | 3 | 2 |
| 27.03 | 3 | 1 |
| 83.76 | 7 | 7 |
| 82.89 | 4 | 7 |

The total remained 17 edits across these four clips. More future context helped some recordings and hurt another. This rejects a blanket two-second lookahead change as an established fix.

Local reports, intentionally untracked because they contain transcripts:

- `%TEMP%/grain-flow-full-audit.json`
- `%TEMP%/grain-flow-long-audit.json`
- `%TEMP%/grain-flow-lookahead-audit.json`
- `%TEMP%/grain-flow-seam-audit.json`

## Reference comparison

### FluidVoice / FluidAudio

Audited local FluidVoice `0039d645aa6bec8c7c6f571785bf4b5b7fa2a29b` and its FluidAudio fork pin `3fd63887eef1dc25edea8263ce4b44aa854d898b`. The app uses the complete short prefix and incremental finalized windows for longer input. FluidAudio explicitly shares chunk layout, inference and overlap merging between long-form Batch and incremental transcription, with fresh decoder state per window. That is the useful architectural property to preserve.

The 15-second bound belongs to this CoreML model pipeline. Grain's GGUF model successfully processed complete 83-second recordings in this audit. Its input bound therefore does not need to be inferred from the CoreML layout. Increasing windows still needs RAM, CPU and stop-latency measurements.

The current `altic-dev/FluidAudio` main was fetched read-only at `09c23cce76126920b3ff6710cdb154bdf9c126b8`. Its changes since the pin add pronunciation-related processing; the window geometry and decoder policy inspected here have not been replaced by an accuracy fix. The separate `FluidInference/FluidAudio` main is a different line of development and reorganizes the ASR implementation; do not confuse it with FluidVoice's fork pin.

Sources: `FluidAudioProvider.swift`, `ParakeetIncrementalSession.swift`, `ParakeetChunkLayout.swift`, `AsrTranscription.swift`, `AsrChunkTokenMerger.swift`, and the v2/v3 TDT decoders under the ignored local reference folders. Primary repositories: [FluidVoice](https://github.com/altic-dev/FluidVoice), [its FluidAudio fork](https://github.com/altic-dev/FluidAudio).

### VoiceInk

Cloned into ignored `Refrence/VoiceInk`; audited commit `82bbaede2562360dabb29d2817e59dd64923c6b4`. Its Parakeet path calls FluidAudio's `AsrManager.transcribe` on the completed recording with fresh `TdtDecoderState`. Its separate GGUF provider uses ordinary transcribe.cpp inference and low-energy split points, but its inspected 30/35-second chunk profiles belong to SenseVoice/Cohere, **not Parakeet**. Those values must not be presented as tested Parakeet settings.

Useful lessons: keep the model's supported inference path and use natural boundaries for long-form segmentation. VoiceInk's offline Parakeet path is not evidence of an equivalent background Flow scheduler.

Sources: `FluidAudioTranscriptionService.swift`, `OfflineTranscribeCppService.swift`, `TranscribeCppModelCatalog.swift`. Primary repository: [VoiceInk](https://github.com/Beingpax/VoiceInk).

### Previous Grain architecture

The immediate predecessor of `0fa5fd69` used ordinary `session.run`, with no custom decoder family extension. Its final checked-in cursor configuration had a 25-second hard bound, two-second overlap, early finalization after at least 12 seconds with a 700 ms pause, and speech pre-roll. Earlier user-used configurations may differ.

This gives two concrete differences from today's pipeline: a larger maximum acoustic context and pause-aware boundaries. Both are plausible contributors to the owner's earlier accuracy experience. The old pipeline was not replayed in this audit; do not claim it has been benchmarked or restore its deleted engine wholesale.

## Implementation plan

1. **Keep the evaluation gate.** Use the offline evaluator for identical-sample comparisons and per-window token inspection. Add intended transcripts when supplied, allowing actual WER and endpoint/seam loss measurements. Batch disagreement alone cannot select the most accurate hypothesis. Preserve punctuation and casing measurements separately.
2. **Test boundary policy before changing production.** Evaluate a bounded pause-aware cursor, retaining the existing journal and serial worker. Test maximum context around the previous 25-second bound and early sentence/pause finalization, against continuous speech with no pauses. Use an inline RMS/pause decision or the existing VAD result if justified; no second VAD model, new background service or generic legacy engine. Avoid cutting through words, but retain a strict upper bound when there is no safe pause.
3. **Evaluate decoder unification separately.** Adapt the native window range to the same model-native greedy policy used by Batch. Verify SOS, multiple zero-duration emissions, valid end frames, dense token counts and final words. A decoder change alone did not improve the measured lexical disagreements, so it must not be sold as the complete accuracy fix. Make exhaustion explicit rather than silently returning a truncated transcript. Ordinary Batch defaults remain intact.
4. **Audit ownership and merging with new geometry.** Keep native IDs through final detokenization. Prove every owned frame is covered, validate context-relative to absolute timestamps, and test conflicts/repeated phrases at seams. Confirm an overlap cannot duplicate a word or prefer a clipped boundary hypothesis over a supported continuation. Include the current one-frame timestamp-origin distinction in the review.
5. **Verify recording lifecycle and resource bounds.** Stop during decode and during a tail, cancellation, rapid restart, model switch, journal failure and replay must release the worker, model lease and temporary file. Retain bounded audio RAM and coalesced wakes. Measure real warm/cold stop latency and memory, followed by long-recording/rapid-session soak. No preview or new controls are needed.
6. **Ship only measured gains.** Re-run short, 15-second threshold, long continuous, paused, fast, repeated, quiet/noisy and spoken-code recordings. Start with the owner's v2 Q8_0 model; validate reviewed v3 models independently. Require improved referenced accuracy without unacceptable stop latency or regressions in previously clean recordings.

The first runtime experiment should be pause-aware boundaries with a larger bounded input, not a global lookahead constant or a larger window passed through the unchanged 150-token decoder. All runtime work stays in Grain-owned code and the explicitly documented native adapter; Handy capture hooks remain narrow.

## Verification completed

- Offline evaluator unit tests: word-distance behavior and production short/long planning.
- Evaluator compilation and real-model runs: 11 baseline recordings, four lookahead comparisons, and two per-window inspection replays.
- Evaluator Clippy with warnings denied and targeted formatting.
- Production changes: none. Model v3, referenced WER, live stop latency, capture lifecycle, packaged-app checks and soak remain future gates.

Reproduction commands and report semantics: [scripts/flow-audit/README.md](../scripts/flow-audit/README.md).
