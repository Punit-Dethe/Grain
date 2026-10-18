# Parakeet TDT v3 Flow: accuracy-first buffered decoding

2026-10-18. Accuracy has priority over first-output latency. V2's tested profile
and its separate checkout/build remain unchanged. No private microphone clips
or local parameter sweep will select the settings.

## Evidence and decision

- [NVIDIA's streaming/buffered reference](https://github.com/NVIDIA-NeMo/Speech/blob/main/examples/asr/asr_chunked_inference/rnnt/speech_to_text_streaming_infer_rnnt.py)
  recommends 10s left / 10s owned / 5s right for long-form, generally similar to
  offline recognition. The encoder sees the full buffer; only owned frames enter
  decoding, with predictor state carried forward and one final detokenization.
- [V3's model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
  demonstrates 10/2/2, a lower-latency operating point, not an accuracy optimum.
  Punctuation/capitalization are learned model outputs; no text prompt is exposed.
- [Microsoft's April 2026 study, appendix A.3](https://arxiv.org/html/2604.14493v1#A3)
  tests 18 v3 configurations. Best tested average WER is 9.22% at 5.6/8.8/1.6;
  8.4/11.6/1.6 yields 9.24%, and 7.2/21.6/7.2 yields 16.94%, against a 6.32%
  offline reference. Larger is not monotonically better. Its table uses CUDA,
  batch 16; CPU numbers elsewhere use a 32-core EPYC, not a laptop. The
  [evaluation code](https://github.com/nenad1002/open_asr_leaderboard/blob/main/parakeet/run_parakeet.py)
  carries decoder state, so these results cannot be dismissed as independent
  utterance resets. It does not test Grain's 10/10/4.96 GGUF profile.
- [Native maintainer benchmarks](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet-tdt-0.6b-v3.md)
  provide relevant personal-computer context. V3 Q8, 11s / 35.3s clips: M4 Max
  CPU 286ms / 1.00s, Metal 60ms / 164ms; Ryzen 4750U laptop CPU 729ms / 2.89s,
  integrated Radeon Vulkan 453ms / 1.37s. These warm batch-one timings are not
  Flow latency or an accuracy/window sweep. Quantized test-clean WER is 1.94%
  Q8 and 1.98% Q4, without proving equivalence across languages.
- [CrispASR's long-form matrix](https://github.com/CrispStrobe/CrispASR/blob/main/PERFORMANCE.md)
  uses character coverage on a few clips, not human-reference WER. Its 30s
  chunk/3s overlap result does not establish an accuracy plateau.
- [NVIDIA's native runtime](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/asr/models.md)
  currently exposes v3 as full-utterance ASR, not cache-aware streaming. Its
  Nemotron streaming performance numbers do not describe Parakeet v3.

Selected policy: the NVIDIA long-form recommendation, aligned down to native
80ms frames: **125 left / 125 owned / 62 right** = **10 / 10 / 4.96 seconds**.
This is a documented accuracy-oriented operating point, not a proved v3
optimum or a measured GGUF WER plateau. There is no adequate published sweep
to claim that 28, 30, 32 or 35 seconds makes no further accuracy difference.
Reusing the existing v2 policy avoids speculative knobs and a second scheduler.

The first stable run needs more than 14.96s of real audio; subsequent runs
advance 10s. At most 24.96s / 399,360 Float32 samples (~1.6MB PCM, excluding
model/encoder memory) enter one inference. Stop decodes the remaining real
frames immediately, without waiting for right context or adding synthetic
silence. There is no recording-duration cap at the window length. Full capture
is journaled to disk and replay uses one fresh continuation.

Decoder carry removes forced fresh-utterance starts and text deduplication at
boundaries. It cannot guarantee correct capitals, punctuation, language choice
or full-context accuracy: the encoder still has bounded context. Conventional
normalized WER often excludes punctuation/case; it cannot validate those alone.

## Execution order

1. Review native model capability, state/reset and worker boundaries. Enable the
   already-tested PKBF decoder for the exact reviewed v3 architecture/durations.
   Preserve v2 geometry, greedy algorithm and resource cleanup.
2. Test v3 continuation against ordinary decoding on identical encoder inputs,
   including finalization, discontinuity rejection, reset, cancellation and reuse.
   This checks implementation correctness, not parameter tuning.
3. Use the buffered cursor for both models. Delete the unused Fluid cursor,
   padding, boundary drain and overlap-merger paths from Grain; retain native
   SDK protocol support where still independently used.
4. Revise native contract identity, regenerate bindings/digest, test and push the
   fork; update both immutable Cargo pins and lockfiles together.
5. Run native, Rust, journal and real-worker gates on public English and European
   language fixtures, including multi-window input and CPU/Vulkan. Keep all
   outputs/builds in the v3 worktree or C:/gtv3, away from the user's C:/gtc build.
6. Record results and limits, commit and push Grain. No UI or Handy feature edits.

## Verification

- Native source `eeffcc3887018015d04d20e61d3698a33d790f24`, contract 5,
  `grain-flow-tdt`, ABI `19ba4aa29b4b28e4`. Both workspace pins/locks and the
  official metadata check agree. The source-built Windows CPU/Vulkan library
  passes contract/layout checks and actual backend discovery; staged DLL hashes
  agree with the source-build output. No prebuilt override selects shipping code.
- Windows application build passes. Backend suite: **601 passed, 22 ignored**;
  root workspace: **340 passed**. Geometry tests cover exact-boundary tails,
  one-hour scheduling, partition independence and bounded PCM capacity. The
  failed-decode/replay test exercises both model variants and preserves repeated
  token IDs. Native safe Rust: 14 library + 10 no-model tests pass. Real v2
  English and v3 French lifecycle gates pass identical-encoder split decoding,
  cancellation, reset, finalization, discontinuity rejection and model reuse.
- V3 Q4 public fixtures: JFK (11s), dots (35.3s), FLEURS Russian (33.84s),
  Ukrainian (28.92s), and threefold FLEURS French/German/Spanish clips
  (30.60/33.48/29.16s). Both CPU and Vulkan pass full-text incremental journal
  versus fresh replay checks, with bounded audio capacity. Actual executable
  worker tests pass all seven against the offline policy with matching backend,
  PCM conversion and default session options, including cancellation epochs and
  unloaded-worker cleanup. FLEURS provenance/licence/IDs are in the native
  fork's `samples/README.md`; repeated clips are transport/continuation fixtures,
  not independent speech samples or a corpus benchmark.
- CPU and Vulkan have identical normalized words on these seven fixtures. One
  Ukrainian phrase differs in comma/lowercase versus full-stop/capitalization.
  The same difference remains with the native automatic thread setting; it is
  not evidence that cross-backend punctuation/case is guaranteed.
- A worker-test mismatch was traced to different PCM16 divisors (32767 in the
  generic Handy WAV helper versus 32768 in the oracle). Reproducing that exact
  scaling offline reproduces the proper-name difference. The Grain test now
  uses identical PCM; production capture and Handy's helper are untouched.
  Audit diagnostics accept thread count 0 to match the worker; 0 versus 4 makes
  no text difference on these seven clips. No settings were fitted to this audio.
- V2's original 11/35/83s regression clips retain exact cased/punctuated text
  and owned token IDs. Real worker gates also pass all three on CPU and Vulkan.
  V2's testing checkout and C:/gtc artifacts were not modified by this work.
- Generated Python/Rust/TypeScript bindings, Swift digest, package audit,
  formatting, ten pin/tooling tests, 28 upstream tooling tests and the divergence
  ratchet pass. No Handy tree changes. App Clippy succeeds with the existing
  87-warning test baseline and no warnings in changed code; audit Clippy passes
  with warnings denied. Native dynamic-feature Clippy requires allowing the
  pre-existing `model.rs:444` redundant cast; no unrelated source fix is included.

Native cross-platform CI: [run 39114437519](https://github.com/Punit-Dethe/transcribe.cpp/actions/runs/39114437519).
macOS (including Metal) and Linux (including source archives without Git) are
green; Windows' final shared-library build is still running at this checkpoint.
Public-fixture conformance is not multilingual
corpus WER, a punctuation accuracy study, a recording soak or proof of a plateau.

## Launch without replacing the v2 test build

Fully quit Grain first. V2 testing remains in the handy-sync worktree using
`CARGO_TARGET_DIR=C:/gtc`. To test this v3 candidate separately:

```powershell
Set-Location 'C:/Users/watrm/.codex/worktrees/parakeet-v3-flow/grain'
$env:CARGO_TARGET_DIR = 'C:/gtv3'
bun run dev:asr
```

Select a reviewed Parakeet v3 GGUF with translation off, and use Standard.
Flow is selected automatically for the reviewed model; there is no Flow toggle.
