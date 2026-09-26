# Parakeet v2 Flow: second research round

Date: 2026-09-26. Scope: Parakeet TDT 0.6B **v2**, especially Grain's installed Handy Q8_0 GGUF. This round changes research and the experiment plan only. The offline evaluator from [the first audit](FLOW-ACCURACY-AUDIT.md) is ready; no additional accuracy measurements or production changes were made here.

## Conclusions

1. **Other users report this kind of degradation with v2 itself.** Reduced future context can cause substantial deletion errors. This establishes a relevant failure mode, not the cause or magnitude of Grain's errors.
2. **NVIDIA has a stronger v2 reference than the FluidAudio layout.** Its newer buffered pipeline retains decoder state, encodes left/current/right audio, and decodes each owned frame once. Published v2 results approach offline WER with roughly 25 seconds of total context.
3. **Larger windows alone are insufficient as a prescription.** NVIDIA's older 30-second buffered method performed worse than its newer approximately 25-second method. Context geometry and decoding policy must be evaluated separately.
4. **The 15-second CoreML input shape is not the v2 GGUF model's inherent limit.** Grain's first audit already decoded complete 83-second recordings. Bound GGUF context by measured memory and compute requirements.
5. **VoiceInk does not supply an equivalent Parakeet Flow implementation.** Its v2/v3 provider processes a finished recording. Its realtime Nemotron path uses a different model.
6. **No universally optimal v2 Q8_0 desktop settings were established.** The strongest next experiment is NVIDIA-style buffered ownership and complete TDT state continuity, compared with a simpler pause-aware baseline.

Grain-owned audio conditioning remains excluded, as requested. No normalization changes are proposed. Keep the same model artifact and input samples throughout comparisons.

## Direct v2 evidence

### Model contract

NVIDIA describes v2 as an English model trained with full attention, using FastConformer and TDT. Its documented long single-pass capability depends on hardware. Independently recomputing bounded windows changes the available acoustic context, even when weights and precision are identical. Exact equivalence to complete-audio Batch cannot be assumed. [NVIDIA model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2).

### Reported deletions with less right context

A v2 user tested 10 seconds left, 2 seconds owned, and right context decreasing from 2.5 to 0.25 seconds. They reported increasing deletions, including a span of 21 words, while substitutions and insertions changed little. NVIDIA requested a reproducible issue. The inspected issue remains open and contains no established resolution; do not claim it confirms a specific defect in Grain. [Original discussion](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/60), [NeMo issue #14430](https://github.com/NVIDIA-NeMo/Speech/issues/14430).

Grain differs: its current windows independently recognize approximately 14.88 seconds and merge overlapping hypotheses. It does not use the cited stateful center-frame pipeline. The first Grain audit found substitutions well inside a window, as well as decoder endpoint differences. An explanation limited to seam deletions would therefore be incomplete.

### NVIDIA's improved buffered pipeline

In response to a v2 streaming/merging question, an NVIDIA maintainer recommended the newer pipeline: preserve decoding state and remove hypothesis merging, reporting better quality and speed. [Maintainer response](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/63).

The implementing PR explicitly identifies its `tdt-0.6b` checkpoint as **v2**. Selected reported WER values:

| Method | Context specification | LS clean | LS other | Earnings22 | AMI |
| --- | --- | ---: | ---: | ---: | ---: |
| Offline | Complete input | 1.69 | 3.20 | 11.16 | 11.17 |
| Older buffered | Total 30 s, owned 10 s | 2.06 | 4.06 | 14.19 | 29.34 |
| New stateful | Left 10 s / owned 10 s / right 4.96 s | 1.69 | 3.21 | 11.16 | 11.17 |
| New stateful | Left 10 s / owned 2 s / right 2 s | 3.14 | 4.59 | 11.22 | 11.45 |

These are NVIDIA's reported benchmark results, not Grain measurements. The old and new pipelines also differ in precision and implementation; this is not an isolated ablation proving that state retention alone caused the improvement. Published GPU batching speedups cannot predict CPU GGUF stop latency. [NeMo PR #9106](https://github.com/NVIDIA-NeMo/Speech/pull/9106).

An earlier NVIDIA v2 recommendation suggested a 30-second buffer and 10-second owned chunk, with settings adjusted for accuracy/performance. Prefer the newer stateful implementation as the architectural reference; treat that older suggestion as a candidate, not a ready-made Grain default. [v2 long-audio discussion](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/15).

## Implementation lessons from source

Inspected NVIDIA-NeMo/Speech commit `cf724ac337d1ebc7d0dda1e23fb80916f52927a5`, under ignored `Refrence/NeMo-Speech` using a sparse checkout.

- `examples/asr/asr_chunked_inference/rnnt/speech_to_text_streaming_infer_rnnt.py`: evaluates the full left/current/right buffer, removes left encoder frames, limits decoding to owned frames, passes `prev_batched_state`, and extends hypotheses. At the last chunk, its decode length uses the remaining valid encoder frames.
- `nemo/collections/asr/parts/submodules/transducer_decoding/tdt_label_looping.py`: carries predictor state **and predictor output**, previous labels, cumulative decoded lengths, and **residual duration jumps**. If a TDT duration skips beyond the current owned chunk, that overshoot initializes the next chunk's frame position.
- Blank-only chunks retain previous labels. Nonblank output is stored using its emission frame; activity after advancing does not erase an otherwise valid last-frame emission. Max-symbol rules constrain pathological frame loops.

Primary source: [buffered inference script](https://github.com/NVIDIA-NeMo/Speech/blob/cf724ac337d1ebc7d0dda1e23fb80916f52927a5/examples/asr/asr_chunked_inference/rnnt/speech_to_text_streaming_infer_rnnt.py), [TDT state implementation](https://github.com/NVIDIA-NeMo/Speech/blob/cf724ac337d1ebc7d0dda1e23fb80916f52927a5/nemo/collections/asr/parts/submodules/transducer_decoding/tdt_label_looping.py).

### Why this is more than adding an overlap

```text
Current Grain:
  [ recognized window A ]
                 [ recognized window B ]
  fresh predictor for each -> reconcile repeated token hypotheses

NVIDIA buffered ownership:
  [ left acoustic context | owned chunk | right acoustic context ]
  encode all; decode owned frames with state from the preceding owned chunk
  next chunk starts at the preceding owned end -> append native tokens
```

Important constraints for a native experiment:

1. Merely filtering ordinary decoder output by timestamps after inference does **not** reproduce center-only decoding. The predictor has already consumed context tokens. The first audit's ordinary-window diagnostic is useful but does not test this architecture.
2. Carrying only an LSTM or only the previous token omits TDT duration overshoot and can repeat or skip frames.
3. Never carry state through audio that will be decoded again as overlap. Establish disjoint owned intervals first.
4. Frame ownership, encoder subsampling phase and sample-to-frame offsets must agree. Revisit the current one-frame origin convention before introducing new geometry.
5. Logical blank/SOS names do not establish predictor equivalence across runtimes. Verify the actual embedding, initialization and update order against the chosen native decoder.
6. Preserve valid tokens emitted on the last frame before advancing. Do not run a fictitious repeated-last-frame tail to manufacture completion; test final words with the actual remaining samples and the model's supported padding behavior.
7. At stop, consume existing captured audio and finalize the last valid interval. There is no future microphone audio to wait for, no full-recording rerun required by this candidate, and no subsequent reprocessing after text delivery.

This can retain Grain's journal, serial worker, coalesced wakes and one model lease. Native per-recording predictor state requires explicit reset on finish, error, replay and cancellation. It is a material adapter change, not a constant adjustment.

## Other implementations: relevance and limits

| Source | Model/path inspected | What it contributes | Limit for Grain v2 |
| --- | --- | --- | --- |
| VoiceInk | FluidAudio v2/v3 completed recording | Confirmed ordinary offline inference | No equivalent Parakeet background Flow scheduler found |
| parakeet-mlx | Generic TDT implementation; v2 supported; current README example v3 | Cached encoder plus finalized/draft regions; predictor state retained | Defaults replace full attention with local attention; different inference behavior and Apple-only runtime |
| v2 FastAPI service | Explicit `nvidia/parakeet-tdt-0.6b-v2` | VAD-delimited ordinary inference in background | Experimental; no accuracy benchmark; unsafe to copy segmentation verbatim |
| parakeet-streaming-onnx | Explicit v3 | Left/current/right buffered inference; documents accuracy loss with lower latency | Its settings are not a measured v2 Q8_0 recipe |
| Parakeet Web | Default v3 ONNX | Demonstrates provisional live text | Final whole-audio rerun replaces live output, masking live/final accuracy difference |
| Independent streaming study | Explicit v3 | Broad context/precision sweep showing chunked penalties | Different version and runtime; its best context is not a v2 default |
| transcribe.cpp main | v2 offline greedy, distinct streaming-capable models | Confirms native model scope and model-based decoder safeguards | Grain remains pinned to reviewed 0.2.3; this audit performs no dependency upgrade |

### parakeet-mlx

Cloned at `b78130e3aa1788e89707ec164adbea8865316aaf`. `StreamingParakeet` in `parakeet_mlx/parakeet.py` changes attention to local attention unless `keep_original_attention` is true, maintains encoder caches, and revises a draft suffix. Its withheld encoder-frame count is `right_context * depth`; increasing depth changes buffering cost. Its readme's layer-equivalence explanation must not be read as a promise of parity with v2's original full-attention Batch mode. [Repository](https://github.com/senstella/parakeet-mlx), [inspected source](https://github.com/senstella/parakeet-mlx/blob/b78130e3aa1788e89707ec164adbea8865316aaf/parakeet_mlx/parakeet.py).

A separate project reports worse streaming accuracy than Batch using MLX/candle with reduced local attention. Its inspected report does not pin the evaluated checkpoint, so its numerical result is not accepted as v2-specific evidence. The direct v2 NVIDIA sources above are stronger. [Implementation author's report](https://github.com/rust-works/omni-voice/issues/25).

### Experimental v2 VAD service

Cloned `Shadowfita/parakeet-tdt-0.6b-v2-fastapi` at `31c5652b62d09653ad5ea8190c0ad0d35394174d`. Its config explicitly selects v2. `streaming_vad.py` flushes at VAD silence or an 8-second guard; the worker calls ordinary transcription on resulting WAVs. The code discards partial 512-sample blocks and the inspected WebSocket disconnect path has no final partial-audio flush. Those implementation risks disqualify copying it as a production reference. The transferable idea is to prefer natural utterance boundaries while preserving every input sample. [Source](https://github.com/Shadowfita/parakeet-tdt-0.6b-v2-fastapi/tree/31c5652b62d09653ad5ea8190c0ad0d35394174d/parakeet_service).

### v3 sources excluded from v2 settings claims

The ONNX streaming project documents 10/2/2 seconds and warns of accuracy loss with a 0.5/0.5-second owned/right configuration. It explicitly exports v3. [Repository](https://github.com/dhyuk54/parakeet-streaming-onnx).

Parakeet Web explicitly uses v3 and replaces its live hypothesis with a complete-audio result at stop. Its final accuracy claim therefore cannot establish incremental parity without a rerun. [README](https://github.com/thiswillbeyourgithub/parakeet_web/blob/main/README.md).

The April 2026 streaming study reports v3's best tested chunked WER of 9.22% versus a reported 6.32% Batch baseline. This supports investigating the context tradeoff but supplies neither Grain's expected error rate nor a v2 optimum. [Paper, sections 5.1 and A.3](https://arxiv.org/html/2604.14493v1).

### Current native upstream

Fetched transcribe.cpp main read-only at `db096815cec18f1d19dfaba722fb7fae7b010c00`. Its v2 documentation classifies this model as offline. The ordinary TDT decoder starts with a no-previous-token sentinel, emits nonblank tokens before advancing, uses metadata-based symbol rules, and returns an error on an iteration cap. Streaming RNNT support for other models does not supply a TDT-v2 state-carry adapter. [v2 documentation](https://github.com/handy-computer/transcribe.cpp/blob/db096815cec18f1d19dfaba722fb7fae7b010c00/docs/models/parakeet-tdt-0.6b-v2.md), [decoder](https://github.com/handy-computer/transcribe.cpp/blob/db096815cec18f1d19dfaba722fb7fae7b010c00/src/arch/parakeet/decoder.cpp).

VoiceInk remains at `82bbaede2562360dabb29d2817e59dd64923c6b4`; source details are in the first audit. [Inspected provider](https://github.com/Beingpax/VoiceInk/blob/82bbaede2562360dabb29d2817e59dd64923c6b4/VoiceInk/Infrastructure/Providers/Transcription/FluidAudio/FluidAudioTranscriptionService.swift).

## Revised experiment plan

The original pause-aware experiment remains a useful low-complexity baseline. This round adds a stronger model-specific candidate and rejects declaring a winning window size before evaluation.

1. **Extend the offline evaluator first.** Keep current production Flow and ordinary complete-audio Batch as controls. Record exact sample intervals, owned frame ranges, decoder policy, state resets, duration carry, padding and backend configuration in every candidate report.
2. **Independent-window baseline.** Test ordinary native greedy decoding with bounded 25- and 30-second inputs, plus pause-aware cuts near the older minimum of 12 seconds. Preserve overlap ownership and frame phase. Compare fixed versus pause-aware geometry with otherwise identical decoding. Do not pass larger inputs through the unchanged 150-token Flow cap.
3. **Stateful buffered candidate.** Add a narrowly scoped native evaluation adapter for TDT v2, preserving complete state and decoding only disjoint owned intervals. Start with NVIDIA's approximately 10/10/5-second geometry; compare 10/10/2 and 10/2/2 to separate context and cadence costs. Align every duration to native frames and report the actual values. These are experiment settings, not shipped defaults.
4. **Separate context from state effects.** For identical encoder buffers and owned ranges, compare a freshly initialized versus carried decoder state. Where possible, replay the same owned encoder outputs through one uninterrupted decoder and through chunked calls to prove state/overshoot equivalence. That isolates adapter bugs from full-attention window differences.
5. **Test structural correctness before speech scores.** All-blank intervals, duration overshoot across several short chunks, multiple tokens at one frame, empty final tail, final nonblank emission, dense speech, cancellation/replay and session reset must preserve ownership and release state. No output may be silently truncated by a successful cap return.
6. **Measure on the exact v2 Q8_0 artifact.** Use identical audio and human references when available; measure deletions, substitutions, insertions, final words and punctuation separately. Keep previously clean short recordings, continuous speech, long pauses, code/names and repeated words in the gate. Batch disagreement remains a diagnostic until intended transcripts are available.
7. **Measure operational costs.** Bounded PCM memory does not bound quadratic full-attention scratch by itself. Measure peak working set, native allocation high-water mark, warm/cold compute, worker backlog and real stop latency. NVIDIA's `owned + right` theoretical streaming latency is not a measured Grain stop delay.
8. **Choose the smallest passing change.** Keep the existing runtime capture/journal/worker architecture. If pause-aware independent windows meet the accuracy gate, ship that simpler policy. If stateful buffered decoding materially improves referenced accuracy, review that adapter and its lifetime/ABI explicitly before integration. No local-attention rewrite, additional ASR engine, full-audio final rerun, model switch or preview is selected by this research.

Any native implementation must update the explicitly permitted patch in `vendor/TRANSCRIBE-CPP.md`, regenerate bindings/ABI where needed, and satisfy its validation contract. An upstream dependency upgrade must follow that contract and `Upstream/UPSTREAM.md`; reading current reference code is not authorization to mix native releases.

## Evidence limits

The first audit's 11 clips remain the only local real-model measurements. Adding two seconds of context to its stateless diagnostic produced mixed results, so that experiment does not prove the stateful candidate will help. No numerical claim in this report is presented as a measured improvement to Grain. The outcome is a better-supported experiment plan, with the runtime accuracy gate still open.
