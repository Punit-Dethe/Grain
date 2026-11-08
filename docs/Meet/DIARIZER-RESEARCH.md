# Grain Meet: transcribe.cpp version and diarizer decision

**Prepared:** 2 November 2026, on `feat/grain-meet` at `9694fd44`.
**Status:** research complete. **Decision (2 November 2026, user): keep Sortformer v2.1 for the first implementation.** The Nemotron 3 backport in section 5 is the recorded route for a later upgrade, not current work. No native change is made.

## 1. Questions

1. Which transcribe.cpp source does Grain actually build?
2. Does that source already expose a working Rust diarization interface for Sortformer v2.1?
3. Should Grain Meet start on Nemotron 3 Diarization, the model FluidMeet uses, instead of Sortformer v2.1?
4. If so, which existing port do we reuse (the Glimpse fork or upstream PR #175), and how do we do it without moving off Handy's transcribe.cpp release?

## 2. Which transcribe.cpp Grain is on

| Fact | Evidence |
| --- | --- |
| Grain's Cargo manifest declares `transcribe-cpp = "=0.4.0"` and patches both `transcribe-cpp` and `transcribe-cpp-sys` to `Punit-Dethe/transcribe.cpp` at `eeffcc3887018015d04d20e61d3698a33d790f24`. | `src-tauri/Cargo.toml` `[patch.crates-io]`; `src-tauri/Cargo.lock` resolves both packages to that git revision. |
| That revision is upstream tag **v0.4.0** (`f1dcd15`, "release: 0.4.0 (#204)", 2026-10-12) plus 5 Grain commits. | `git log f1dcd15..eeffcc3` in the reference clone. |
| The 5 Grain commits touch only Parakeet TDT buffered decoding, detokenization, ABI IDs 29/30 and their tests (38 files). They do not touch diarization. | `git diff --stat f1dcd15 eeffcc3`; `native/transcribe-fork.json` (`contract_revision` 5, `grain-flow-tdt`). |
| Handy `main` (`7bfb2e2c`, 2026-10-27) uses crates.io `transcribe-cpp 0.4.0`, the same upstream release. | `git show upstream/main:src-tauri/Cargo.toml` and its lockfile. |
| Upstream transcribe.cpp has since released **v0.4.1** (`4d7c1cc`, 2026-10-22); `main` is at `5700f12` (2026-10-27). Handy has not adopted 0.4.1. | Reference clone tags. |

**Answer 1:** the plan's "transcribe.cpp 0.4" statement is correct. Grain is on its own fork, which is exactly upstream 0.4.0 plus the Flow TDT patch, and Handy is on the same 0.4.0 release.

## 3. Sortformer v2.1 in Grain's pinned source

transcribe.cpp 0.4.0 ships the DIARIZE role (`include/transcribe/diarize.h`), the `sortformer` architecture, the Sortformer run extension (`include/transcribe/sortformer.h`, presets Default/VeryHigh/High/Low latency) and the Rust `DiarizeSession` (`bindings/rust/transcribe-cpp/src/diarize.rs`). There is no push-audio (live) diarization API in 0.4.0.

Executed evidence on this machine (Windows, RTX 3080, Vulkan SDK 1.4.350): the native crate's own `tests/diarize.rs`, built from a copy of the exact pinned source `eeffcc3` with the `vulkan` feature and the pinned Q8_0 artifact (`handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf@60ebab3`, sha256 `a5dacdc6…2442a9`):

- 4 passed, 0 failed. Three tests ran with the real model, on CPU and on `Vulkan0`. `asr_only_model_refuses_diarize` skipped its real-model part because no ASR smoke model was provided.
- Covered: diarize-only roles, 16 kHz / 4 speakers, preset application, input rejection, cancel token, session keeps its model alive.

**Answer 2:** yes. Grain's pinned source exposes a working Rust Sortformer v2.1 interface on Windows CPU and Vulkan. Grain does not call it yet, and Handy's catalog test deliberately keeps `sortformer` out of the ASR model list.

## 4. Nemotron 3 Diarization options

### What FluidMeet uses

FluidVoice `dd37664` (2026-11-01; the reference clone was updated to it) installs `altic-dev/nemotron-3-diarization-coreml@8976c3c` (`nemotron_3_diarization.mlpackage`). That is a Core ML conversion of `nvidia/Nemotron-3-Diarization` (Streaming Sortformer v3, 8 speakers, 10 ms output frames, OpenMDW-1.1). It runs on Apple Core ML through FluidAudio and cannot run in Grain's runtime.

### Glimpse

[Glimpse](https://github.com/glimpse-hq/Glimpse) (`5b05ef1`, AGPL-3.0) is a shipping Tauri/Rust meeting app.

- It diarizes with Nemotron 3 through `glimpse-speech 3.0.1` (MIT), which depends on the fork `glimpse-hq/transcribe.cpp@352b27f` (MIT).
- It **retired Sortformer v2.1** in favor of Nemotron 3.
- It runs the offline file path at `VeryHighLatency` and a live push-audio path at `LowLatency`.
- It treats the microphone track and the system-audio track separately.
- It caps whole-clip diarization at 3 hours, and documents about **1.5 GB peak per hour** of audio, because the native run holds the whole clip and its spectrogram.

The Glimpse fork:

- **Base:** it merges upstream through v0.4.1 + 2 commits (`5700f12`), with 589 commits of its own on top. Most of these are Core ML, Whisper and Parakeet speed work.
- **The diarization work** is in these commits: `a1e9f53` (add Nemotron 3), `b893ed2` (validate against Transformers and AMI), `7cd197e` (push-audio streaming) and `ba087c2` (Rust live streaming). They were made 2026-09-23 on September main and carried forward through Glimpse's 0.4.0 merge `4b3f456`. No diarization file changed after that merge.
- **The diarization slice at Glimpse's 0.4.0 merge**, in about 2.7k lines:
  - `src/arch/nemotron3_diar/{model,stream,weights}.cpp` and the arch header (about 1.6k lines).
  - `src/transcribe-diarize.{cpp,h}`, which adds the stream API.
  - Registration in `transcribe-arch.cpp`, `CMakeLists.txt` and `transcribe.cpp`.
  - `include/transcribe/{diarize.h,sortformer.h}`: the stream API, the `SFLV` live extension, and the VERY_LOW and ULTRA_LOW presets.
  - Rust `diarize.rs`, `family.rs` and sys FFI, plus tests.
  - The offline path reuses the 0.4.0 `transcribe_diarize_run` and Sortformer `SFDR` extension unchanged, so a caller written for Sortformer drives Nemotron 3 unchanged.
- **Published validation:**
  - Tensor parity within 2e-4 against HF Transformers.
  - On AMI IHM test (16 meetings, 9.06 h, collar 0, overlap scored), the Q8_0 port scores **DER 9.23%** against **9.22%** for the reference.
  - Sortformer v2.1 Q8_0 scores **15.99%** through the same pipeline. Speaker confusion is 0.86% against 3.60%.
  - The streaming path is bit-identical to the batch run.
  - Measured on Apple CPU and Metal only.
- **Artifact:** `Glimpse-Dictation/Nemotron-3-Diarization-gguf@273ebb0`, file `nemotron-3-diarization-Q8_0.gguf`, 105,936,416 bytes, sha256 `877ff9e7…31688`. Its LICENSE is OpenMDW-1.1. It is a conversion of `nvidia/Nemotron-3-Diarization` at revision `a435e98`; NVIDIA's current revision is `f667ed7`.

Executed evidence on this machine: the Glimpse fork at `352b27f`, built with `vulkan`, with the pinned Q8_0 artifact.

- Its Rust test `nemotron3_diar_live_rows_match_run` **passed**.
- A disposable probe outside every repository ran the offline `VeryHighLatency` path on `samples/sortformer-2spk-mix.wav` (12 s) on CPU and on Vulkan. Both backends returned **identical** segments: 2 speakers and 5 segments, matching Sortformer's turns to within about 130 ms.
  - CPU encode took 137 ms.
  - The first Vulkan call took 3.5 s, dominated by first-use pipeline setup. Warm GPU throughput was not measured.

This shows that the port runs on Windows CPU and Vulkan. It does not establish accuracy on our own meetings or the memory envelope.

### Upstream PR #175

[handy-computer/transcribe.cpp#175](https://github.com/handy-computer/transcribe.cpp/pull/175) "nemotron3 diarization port", by CJ Pais (the Handy and transcribe.cpp maintainer). Issue #170 is answered with "will come in #175".

- **Status:** open, not mergeable, last updated 2026-09-26 (head `e6672a8`). It is based on early-September main (around v0.2.3), so it predates 0.4.0.
- **Its own documentation:** "Stage 4 complete pending tolerance review". The OpenMDW license is "still to be interpreted before ship". It has a BF16 reference GGUF, and no published artifact.
- **It is an independent implementation with an incompatible GGUF schema under the same arch key `nemotron3_diar`.** For example, PR tensor names are `enc.pre_encode.proj`, `diar.encoder_proj` and `diar.single_spk_head`; Glimpse names are `enc.embed.proj`, `diar.proj` and `diar.head.out`. A Glimpse GGUF will not load in a future upstream release built from #175, and the reverse is also true.
- It has no Windows or Vulkan evidence.

## 5. Upgrade route (implemented 2 November 2026)

**Done:** option B below, as `grain/diarize-nemotron3` on the native fork, commit `c8cf6b11dd5e67131b264025112293bda1bb8c86` (contract 6, ABI `54c13920a14bb4b6`, `TRANSCRIBE_ABI_DIARIZE_STREAM_PARAMS` = 31). Grain downloads `Glimpse-Dictation/Nemotron-3-Diarization-gguf@273ebb0` (sha256 `877ff9e77e829e30158528cfaabf56188fca11d05349e09821b3033a97d31688`, 105,936,416 bytes). Mirroring the artifact to a Grain-controlled host (divergence 4) is still open before release.

Recorded so the later upgrade starts from evidence. The user kept Sortformer v2.1 for now. The analysis below was the proposal at research time: **when upgrading, backport the Glimpse diarization slice onto Grain's own fork, which stays on upstream 0.4.0 plus the Flow patch.** Do not move Grain to the Glimpse fork, to upstream 0.4.1, or to PR #175.

| Option | Speakers / AMI DER | Native work | Handy constraint | Verdict |
| --- | --- | --- | --- | --- |
| A. Sortformer v2.1 now, Nemotron later | 4 / 15.99% | None | Unchanged | Works today, but we would do two diarizer integrations, tune assignment twice and migrate artifacts. Glimpse already retired this model. |
| **B. Backport the Glimpse slice to 0.4.0 + Grain patch** | **8 / 9.23%** | About 2.7k lines, mostly additive new files | Unchanged: still upstream 0.4.0; Handy is unaffected | **Preferred upgrade route.** Validated source; runs on our CPU and Vulkan; same API as Sortformer; includes the live API needed later. |
| C. Rebase PR #175 onto 0.4.0 | 8 / not published | Port a stale, conflicting branch; convert our own GGUF; finish its validation | Unchanged | The upstream maintainer's implementation, but unfinished and with no artifact. |
| D. Switch Grain to the Glimpse fork | 8 / 9.23% | Replace the whole native base (0.4.1 + 589 commits) | Breaks it: base moves past Handy | Rejected. |
| E. Upgrade to upstream 0.4.1 | No Nemotron 3 there | Rebase the Grain patch | Breaks it | Rejected, and it doesn't solve the problem. |

Why B over A: Nemotron 3 has 8 speaker slots instead of 4, its speaker confusion is about 4x lower, and it is the exact model family FluidMeet uses. Its offline call is the same `DiarizeSession::run` call with the same `SFDR` preset that Sortformer uses. So the worker role bridge, storage, assignment and UI contracts do not change. The extra cost is a bounded native backport, done once now instead of a full second integration later.

### Accepted divergences (to record in `docs/TRANSCRIBE-CPP-FORK.md` and `native/transcribe-fork.json`)

1. **The native fork gains a second Grain patch, `grain-diarize-nemotron3`**, ported from `glimpse-hq/transcribe.cpp`. Source attribution: commits `a1e9f53`, `b893ed2`, `7cd197e` and `ba087c2`, taken at the Glimpse 0.4.0 merge `4b3f456`. MIT; keep the copyright and attribution. Exclude Core ML and every non-diarization Glimpse change.
2. **ABI renumbering.** Glimpse appends `TRANSCRIBE_ABI_DIARIZE_STREAM_PARAMS = 29`, but Grain already owns 29 (PKFW) and 30 (PKBF). In Grain it becomes **31**. `TRANSCRIBE_EXT_SLOT_DIARIZE_STREAM = 3` does not collide. Bump the contract revision **5 → 6** and the ABI digest, and add the size/alignment pair to the Rust contract check.
3. **Arch key collision risk.** We keep `nemotron3_diar` and the Glimpse GGUF schema. If upstream later ships #175's schema and Handy adopts that release, Grain replaces the backport with upstream's code and switches the artifact in one change. Grain is unreleased, so the clean-break policy applies. Until then, the Grain-owned diarizer catalog pins the artifact by sha256, so a mismatched file fails validation instead of loading wrongly.
4. **Third-party artifact host.** The model is currently pinned at `Glimpse-Dictation@273ebb0`. Before release, mirror the exact bytes (same sha256, with the OpenMDW LICENSE) to a Grain-controlled location. Do not reconvert.
5. **No new Handy-side change for the model choice.** The worker role bridge is diarizer-agnostic. Handy's catalog stays ASR-only; the diarizer is a Grain-owned meeting artifact.
6. **Sortformer v2.1 is not offered to users.** It stays available in the native library only because upstream 0.4.0 ships it. Do not keep a fallback model path (YAGNI, and the clean-break policy).

### Licensing guard

- The Glimpse application is **AGPL-3.0** and FluidVoice is **GPL-3.0**. Use both as behavioral references only. Copy no application code.
- `glimpse-hq/transcribe.cpp` and `glimpse-speech` are MIT and may be ported with attribution.
- The model weights are OpenMDW-1.1; ship its LICENSE with the artifact.

### Consequences for the execution plan

- The baseline diarizer becomes Nemotron 3, with 8 speakers per analyzed track. Sortformer is removed from the critical path. "Nemotron 3 upgrade" leaves the improvements table and becomes part of Step 0/2.
- **Memory gate:** Glimpse documents a peak of about 1.5 GB per hour of audio for whole-clip diarization. Tracks are processed one at a time, so the diarization phase of a 60-minute meeting should peak near 1.5 GB. That is under the plan's 2 GiB target, but with little margin, and a 2-hour track would exceed it. Step 0 must measure this. If whole-clip memory fails the gate, the live push-audio API is the fallback for bounded long-meeting diarization: it keeps only per-frame activity (about 11.5 MB per hour, per the port's documentation). Its LowLatency preset scores DER 9.48% against 9.23% offline.
- **New Step 0 items:** native backport and CI on Windows, Linux and macOS; Rust and C tests at the pin; CPU/Vulkan parity on fixtures; warm throughput; 10/60/120-minute peak memory.

## 6. Local state

- Reference clones under `grain/Refrence/` (ignored): `FluidVoice-latest` updated to `dd37664`; `Glimpse` (`5b05ef1`) and `Glimpse-Speech` (`42d601e`) added. `transcribe.cpp` now has the `glimpse` and `grain` remotes and `origin/pr-175`.
- Evidence workspace `C:/Projects/Grain/meet-fixtures/` (outside every repository) holds:
  - The pinned Sortformer and Nemotron Q8_0 artifacts.
  - A copy of the `eeffcc3` source.
  - A detached worktree of the Glimpse fork at `352b27f`, containing the disposable `tests/grain_probe.rs`.
- The role-aware worker bridge work started earlier on `feat/grain-meet` is **paused and uncommitted** in the `C:/Projects/Grain/grain-meet` worktree. It covers `grain_engine.rs`, `grain_transcription.rs` and small `[GRAIN]` hooks in `engine_supervisor/{worker,supervisor}.rs` and `managers/transcription.rs`. It is diarizer-agnostic (`DiarizeSession` and `max_speakers` come from the model), so it applies unchanged to Nemotron 3. It is now implemented and validated with Sortformer v2.1; see PROGRESS.md, Increment 2.
