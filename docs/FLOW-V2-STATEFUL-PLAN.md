# Parakeet TDT v2 Flow: published buffered policy

2026-10-18. Scope is Parakeet TDT 0.6B v2 only. The user selects published
NVIDIA results rather than parameter fitting on private microphone recordings.
V3 keeps its current reviewed decoder and layout.

## Reference and selected configuration

NVIDIA-NeMo/Speech PR #9106 explicitly benchmarks v2 with 10 seconds left,
10 seconds owned and 4.96 seconds right context. Its stateful pipeline reports
near-offline WER on the listed datasets. The encoder processes the complete
buffer, but the decoder consumes each owned frame once, carrying prediction
state and duration overshoot. This differs from independent-window merging.

https://github.com/NVIDIA-NeMo/Speech/pull/9106
https://github.com/NVIDIA-NeMo/Speech/blob/cf724ac337d1ebc7d0dda1e23fb80916f52927a5/examples/asr/asr_chunked_inference/rnnt/speech_to_text_streaming_infer_rnnt.py

Selected geometry: 125 left / 125 owned / 62 right encoder frames at 80 ms,
maximum 24.96 seconds of input. Full stable steps advance 10 seconds. At Stop,
decode all remaining real frames, including available right context. No silence
removal, signal conditioning, speculative text or extra final full-file pass.

## Execution order

1. Audit native run/session/reset boundaries and the worker's existing transport.
   Add a v2-only typed buffered run extension to the maintained native fork.
   Preserve committed/pending predictor state, previous token, duration overshoot,
   zero-duration symbol count and absolute frame continuity. Reject invalid or
   out-of-order windows and clear state on finalization, reset, failure and cancel.
2. Verify decoder continuation against uninterrupted decoding on identical
   encoded frames, including blank-only chunks, repeated labels, zero durations,
   duration overshoot, final-frame emission and compute failures. These are
   implementation checks, not tuning or microphone/WER experiments.
3. Replace the v2 independent-window cursor with the fixed published buffered
   geometry. Append owned v2 tokens directly and detokenize the complete sequence.
   Keep the existing journal, worker, lease, cancellation and replay architecture.
   Remove v2 pause heuristics, configurable ceiling sweep and overlap merging.
   Retain shared merging only for v3; do not preserve compatibility-only v2 paths.
4. Regenerate native bindings and ABI digest, update the contract revision, test
   the safe wrapper and worker serialization, then commit/push the native fork.
   Update Grain's paired immutable native pins and both lockfiles together.
5. Run geometry, lifecycle, reset/cancellation, transport and public-fixture
   conformance checks plus relevant builds and upstream boundary checks. Do not
   select settings using local timing or private recording agreement. Published
   NeMo WER and GPU speed do not establish identical GGUF results or Stop latency.
6. Record implementation evidence and limits, commit and push the Grain branch.

## Acceptance limits

No finite input duration guarantees that WER equals full-context recognition.
This implementation adopts a published operating point and decoding policy;
it cannot claim the paper's accuracy, punctuation correctness or speed without
equivalent evaluation. Automated verification remains required for correctness.

## Personal-computer evidence (checked 2026-10-18)

The native maintainer publishes v2 Q8 batch-one, warm mel+encode+decode timings:

| Personal computer / backend | 11s JFK | 35.3s dots |
| --- | ---: | ---: |
| Apple M4 Max / CPU | 278ms | 987ms |
| Apple M4 Max / Metal | 55ms | 142ms |
| Ryzen 7 PRO 4750U laptop / CPU | 696ms | 2.77s |
| Same Ryzen / integrated Radeon Vulkan | 416ms | 1.24s |

Source and reproduction: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet-tdt-0.6b-v2.md
M4 source 77b0c93; Ryzen source 218aeae3, both dated 2026-09-14. One warmup,
mean of three runs. These are short-clip native measurements, not Flow windows,
Grain transport/capture, cold loading, battery/thermal results or Stop latency.
The native CPU predictor/joint remains CPU even with encoder GPU offload.

NVIDIA's stateful WER comparison provides the decoding-policy evidence;
its batch-32 RTX 6000 Blackwell throughput is not a laptop latency estimate.
No discovered published desktop CPU/CUDA/Metal/Vulkan sweep establishes an
exact 24/28/30/35-second Flow optimum for this GGUF implementation. Do not
invent one or transfer other models' timings. The fixed published policy stays
backend-independent; acceleration changes compute placement, not ownership,
text state or accuracy settings. Steady-state encoding reads about 25 seconds of context for each 10 seconds
advanced; buffering and re-encoding are bounded, not free. At Stop, pending real audio still needs inference, especially if CPU
has fallen behind; no universal subsecond Stop promise is justified.

FluidAudio also publishes laptop-scale v2 results using CoreML: MacBook Pro M4
Pro / full LibriSpeech test-clean reports 2.1% average WER and 145.8x overall
RTFx; its MacBook Air M2 100-file regression set reports 3.8% and 81.7x on the
listed main revision. These differ in runtime, precision and evaluation scope;
they are corroborating on-device evidence, not GGUF policy or GPU-only timing.
https://github.com/FluidInference/FluidAudio/blob/main/Documentation/Benchmarks.md
https://github.com/FluidInference/FluidAudio/blob/main/Documentation/ASR/benchmarks100.md

## Verification and observed limits

- Native source `e77845e7de1f6aff6f7e24473b70d9e5a0565000`, contract 4,
  `grain-flow-v2`, ABI `4fb918e2c49684a4`. Windows CPU: 59 tests passed,
  one unavailable diarization smoke skipped. Safe wrapper: 14 library and 10
  no-model tests passed; serde/postcard, all 31 layouts, generated bindings,
  Swift digest, package audit and strict native-wrapper Clippy passed.
- Native CI passed Linux, Windows and macOS, including shared consumption,
  source-archive identity and Metal compilation:
  https://github.com/Punit-Dethe/transcribe.cpp/actions/runs/39110777759
- Deterministic real-ggml tests compare uninterrupted greedy decoding with
  multiple partitions of identical encoder frames. They cover zero durations,
  repeated labels, alternating blanks, pending/committed LSTM state, last label,
  duration overshoot and failed projection/predictor/joint dispatches.
- Grain source-built CPU/Vulkan backend: 601 tests passed, 22 opt-in tests
  ignored; workspace: 356 passed. Audit Clippy is strict and clean; application
  Clippy completed with existing repository warnings. Changed Rust files pass
  formatting. Paired Git pins and staged DLL identity/backend discovery pass.
- Explicit public 11/35/83-second JFK fixtures pass incremental journal vs
  offline-policy agreement and fresh replay on both CPU and RTX 3080 Vulkan.
  Real isolated-worker checks pass on both backends, including cancelled epochs,
  unload, and cleanup that must not reload a model. V3's existing three CPU
  fixture strings remain unchanged. Audio capacity stays at or below the
  model-specific bound. These are conformance tests, not performance samples.

The old v2 fixture expectations came from independent 24-second runs and cannot
serve as the oracle for a different decoding policy. The new offline stateful
reference is checked separately against journal partitioning and replay.
There is an observed accuracy limitation: the 35-second public repeat changes
punctuation/case, and the 83-second repeat returns 150 words rather than the
historical 154. No settings were adjusted to fit those samples, no casing
heuristics were added, and those differences are not presented as improvements.
The published NeMo operating point does not establish identical quantized GGUF
WER, universally better casing, or a unique hardware-independent optimum.

The Windows source build required bypassing this host's unusable junction
working directory by unsetting LOCALAPPDATA/TEMP only in the build subprocess
(TMP stayed set). The shipping source, compiler flags and native features were
unchanged. The disposable Rust test executable required the existing Common
Controls v6 manifest after linking. No production code or Cargo configuration
was added for either local environment issue.
