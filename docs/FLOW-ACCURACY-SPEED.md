# Parakeet v2 accuracy and speed selection

2026-09-26. Exact Handy v2 Q8_0 GGUF, Grain's pinned native 0.2.3. Audio conditioning is excluded. This report measures disagreement with complete-input Batch; it does not measure human-referenced WER.

## External starting points

- NVIDIA's current buffered inference script recommends approximately 10 seconds left / 10 owned / 5 right for near-offline quality, or 10/2/2 for a shorter cadence. Those are **25 and 14 seconds of total encoder input**, respectively. Predictor state is carried and only owned frames are decoded. They are not interchangeable with independent windows plus merging. [Source](https://github.com/NVIDIA-NeMo/Speech/blob/cf724ac337d1ebc7d0dda1e23fb80916f52927a5/examples/asr/asr_chunked_inference/rnnt/speech_to_text_streaming_infer_rnnt.py).
- NVIDIA's v2 maintainer recommended 30 seconds of buffer with 10-second chunks as an older starting point, adjustable for accuracy and performance. The newer stateful implementation is preferred by another maintainer. [Older v2 guidance](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/15), [newer guidance](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/63).
- A v2 user reported longer deletions when reducing right context from 2.5 to 0.25 seconds with 10 seconds left / 2 owned. This is a reported failure mode, not a demonstrated cause of Grain's errors. [Issue](https://github.com/NVIDIA-NeMo/Speech/issues/14430).
- Sherpa documents v2 INT8 simulated streaming with VAD and an RK3588 Cortex-A76 CPU test: a 7.435-second utterance took 1.639 seconds with one thread. This is useful evidence that desktop speed cannot establish edge-device latency, but uses ONNX rather than Grain's GGUF runtime. [Device test](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html#rtf-on-rk3588-with-cortex-a76-cpu).
- An independent Core AI v2 port uses a roughly 30-second fixed encoder bucket and reports matching its reference decoder on 152 chunks. That establishes its conversion/decoder fidelity to its own reference, not full-recording accuracy parity or a cross-platform optimum. [Author's report](https://john-rocky.github.io/coreai-model-zoo/models/parakeet-v2/).
- Current transcribe.cpp classifies v2 as offline; its native buffered streaming support targets another Parakeet variant. We retain the reviewed pinned backend and model. [Native scope](https://github.com/handy-computer/transcribe.cpp/blob/db096815cec18f1d19dfaba722fb7fae7b010c00/docs/models/parakeet-tdt-0.6b-v2.md).

These sources justify testing roughly 20–30 seconds of acoustic context. They do not establish a universal optimum for Grain. In particular, NVIDIA's owned-plus-right latency is waiting for future audio, whereas Grain has no live text preview and the user primarily experiences work remaining after stop.

## Local sweep method

Host: Intel i5-14600KF (14 cores / 20 logical processors), RTX 3080 12 GiB. CPU measurements force CPU inference with four native threads. Run one pipeline per process for peak working set; keep the model, backend, samples, decoder, overlap and quiet policy fixed while changing the ceiling. Scan only the final 3.04 seconds for 720 ms of consecutive low energy. Do not remove silence or change normalization. All starts/cuts preserve the 80 ms encoder phase and adjacent windows share exactly two seconds.

The initial 11 recordings showed 17 word edits versus Batch for historical Flow and 2 for the pause-aware 30-second candidate. All eight recordings at or below 30 seconds matched exact Batch text and native IDs. A subsequent production-policy replay, without post-decode endpoint filtering, retained all 11 results.

Two history WAVs became unavailable during the ceiling sweep (one short clip and one 82.89-second clip). Freeze the remaining nine as private temporary fixtures; compare every subsequent ceiling on that identical subset. The historical baseline for this subset is 13 word edits. Add three other long recordings for validation before choosing a default. No private transcripts or audio enter version control.

| Ceiling | Batch word edits, nine clips | Peak CPU working set, MiB |
| --- | ---: | ---: |
| 20 s | 14 | 1202.95 |
| 24 s | 6 | 1224.26 |
| 26 s | 7 | Not isolated |
| 28 s | 6 | 1258.68 |
| 29.04 s | 2 | Pending |
| 30 s | 2 | 1272.42 |

The additional recordings are 50.13, 79.47 and 83.43 seconds. Compare per-clip results and previously clean recordings rather than selecting from a single total. Similar aggregate edit counts can hide different substitutions.

| Policy | Nine original clips | Three additional clips | Combined edits |
| --- | ---: | ---: | ---: |
| Historical Flow | 13 | 25 | 38 |
| Native quiet 24 s | 6 | 9 | 15 |
| Native quiet 26 s | 7 | 12 | 19 |
| Native quiet 28 s | 6 | 17 | 23 |
| Native quiet 29.04 s | 2 | 16 | 18 |
| Native quiet 30 s | 2 | 23 | 25 |

**Selected starting point: 24 seconds**, ordinary native decoding, two seconds overlap, quiet cuts in the final 3.04 seconds. It gives the strongest combined Batch agreement among the evaluated policies, a smaller worst-case encoder/tail input than 29–30 seconds, and lower measured CPU working set. This is an empirical starting point, not a universal optimum or a guarantee of superior human-referenced accuracy.

Counterevidence: the previously matching 37.65-second clip changes one word at 24 seconds (a recognizer substitution within a speech segment, not a duplicated seam). The 27.03-second clip improves from three edits to one but still differs from Batch. The longer additional clips retain two and seven disagreements. Short inputs through 24 seconds are passed whole to Batch's native decoder, with no extra padding or post-decode token filtering. The aggregate improvement does not imply every utterance improves.

## Operational interpretation

Audio RAM is bounded by one window plus the reusable journal read buffer. Native encoder scratch still grows with input duration; the isolated 83.76-second recording increased CPU peak from 1167.12 MiB (historical Flow) to 1271.42 MiB (30-second candidate). There is one native model/session and one serial worker; no second ASR pass at stop or persistent audio copy is introduced in the application. The offline evaluator alone loads complete WAVs and records diagnostic JSON, so its peak is not the full application's footprint.

Total compute, final-window decode time and actual stop-to-paste latency are different measurements. A smaller ceiling can require more overlapping calls and increase CPU work, while sometimes shortening the final tail. Tail size depends on when the user stops; one tiny tail does not establish a latency improvement. Warmup, desktop load, thread scheduling, thermals and GPU precision also affect results.

Two CPU threads on this host are a constrained-thread stress check, not an emulated low-end laptop. GPU results must be compared with Batch on that same backend. Real laptop/ARM measurements, live worker backlog, sustained thermal behavior and human reference transcripts remain practical validation limits.

## Selected policy: hardware checks

- CPU with two threads completed all 12 cases and reproduced every four-thread candidate transcript. Peak process working set was 1232.10 MiB. This does not model an older CPU's instruction set, sustained power or memory bandwidth.
- Explicit Vulkan loaded `Vulkan0` on the RTX 3080 and completed all 12 cases. Host working set peaked at 356.44 MiB in the isolated process; **VRAM peak was not measured**. Four candidate transcripts differed from CPU, so GPU agreement was evaluated against GPU Batch.
- On a repeated, warm pass of the same 12 cases with four CPU threads, median/max final-window decode were 0.810/1.376 seconds; largest individual window was 1.380 seconds. GPU warm median/max final-window decode were 0.130/0.191 seconds; largest individual window was 0.254 seconds. These are fixture observations, not a production latency SLA or stop-to-paste measurements.
- The first isolated GPU run included approximately 3.6–5.1-second first-use decodes at some input shapes. Subsequent processes/passes used already-warmed driver shader caches. Report cold-shape cost separately; the fast warm figures do not erase startup stalls.
- The selected GPU candidate reduced differences from GPU Batch from 38 to 18 words over the 12 cases. All inputs at or below 24 seconds matched exact GPU Batch text. Six surviving original short CPU fixtures also matched exact Batch native token IDs. Referenced WER, live stop-to-paste timing, inference backlog under sustained slow hardware and package/device validation remain open.

The owner finalized **24 seconds** after these checks. Further ceiling/architecture tuning is deferred. The actual application adapter also passed incremental journal processing and fresh-accumulator replay on all 12 fixtures, matching the selected offline candidate in both cases.
