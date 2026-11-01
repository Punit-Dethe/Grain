# Grain Meet implementation progress

## Increment 1: structured bounded TDT evidence

Implemented in `src-tauri/src/tdt_flow.rs` on `feat/grain-meet`, based on
`30a4e74d4b189b374b2664121de0def2e56e0b25`.

- Added an opt-in accumulator constructor and structured final result. Native
  transcripts retain token IDs/text, raw text, confidence (including unavailable
  NaN), timing (including zero-duration points), word/segment membership and
  original word rows. Each batch retains its exact analysis-audio window.
- Final text still uses one detokenization of the complete ordered token ID
  sequence, followed by the existing trim policy. No per-window text joining,
  word timestamp fabrication, or repeated-token deduplication was introduced.
- Existing Flow construction retains compact tokens only. Full evidence is
  collected only when requested before decoding. Audio buffers remain bounded
  by the existing window geometry; retained transcript evidence grows with output.
- Invalid timing commits no partial batch. Failed native calls and finalization
  make an accumulator terminal; replay requires a new accumulator so the native
  decoder resets instead of reusing potentially advanced state.
- No Handy-derived file, model pin, application manifest, lockfile, UI, or capture
  path changed. The structured API is a foundation for the pending meeting
  pipeline; it is not yet connected to a meeting command.

Native word rows are **window-local evidence**, not final joined meeting words.
Words/byte-fallback sequences may cross windows. Canonical word assembly needs
native tokenization semantics and real-model boundary fixtures before use for
speaker assignment. Source/track/session mapping belongs to the later meeting
pipeline; current window coordinates refer only to its 16 kHz analysis journal.

## Validation

The actual production adapter and journal modules compiled against a clean
checkout of the exact pinned `transcribe.cpp` source, with its real CPU native
library. No native API replacement or Tauri UI mock was used.

Reproducible command (Cargo, CMake and a C++ toolchain required):

```sh
python3 scripts/test_tdt_adapter.py --native-source /path/to/transcribe.cpp
```

The runner verifies the native commit and tracked-source cleanliness, creates a
disposable Cargo test target outside the checkout, imports the production
modules, and preserves Cargo's exit status. It leaves application dependency
files unchanged. Test-only dependencies resolve separately from the desktop
application; this is not a full application build or packaging check.

Result: **16 passed, 0 failed, 1 ignored**. The ignored test requires explicitly
provided local Parakeet models and WAV fixtures. Tests cover compact Flow replay,
repeated tokens, incremental versus saved-journal window partitioning, native
metadata preservation, point timing, malformed timing, empty input, failed-call
reuse rejection, buffer capacity, and journal lifetime/range behavior.

Also passed: `python3 scripts/transcribe_fork.py --check`, Python compilation of
the runner, Rust formatting of the changed module, and `git diff --check`.

The tests inject inference outputs to exercise adapter logic. Compiling/linking
the native library does not establish successful model execution or real-model
text parity. Frontend files were untouched; no new frontend acceptance is claimed.

## Increment 2: worker role bridge (Sortformer v2.1)

Implemented on `feat/grain-meet`; Sortformer v2.1 kept as the first diarizer (user decision, 2 November 2026).

- **Worker roles.**
  - The existing worker holds either Handy's ASR session or a Grain-owned Sortformer `DiarizeSession`, never both.
  - The role travels in `LoadSpec`, so crash, hang and GPU-loss recovery reloads the requested role.
  - Diarization reads a validated 16 kHz mono float32 analysis WAV by absolute path. Audio is never copied through the PCM frame. The worker re-checks the header, the declared length and that every sample is finite.
  - The processing cap is 90 minutes per track (see the decisions below). It bounds one allocation and is not a supported-duration promise.
- **Deadlines.** Grain requests that carry audio use the supervisor's device-aware `run` deadline. The fixed 30 s floor still applies to queries.
- **Wrong roles fail explicitly and are not retried:**
  - An ASR query against a diarizer.
  - Diarization against an ASR model.
  - Loading a model in a role it does not serve.
- **Ownership.**
  - `Use::Meeting(attempt)` is an exclusive inference lease. `cancel_grain_meeting` cancels only the current attempt.
  - `with_grain_meeting_engine` releases the dictation model first and freezes the device policy. On every exit it unloads the last role, releases the lease, and restores readiness from the current settings.
- **Evidence validation.** Native speaker segments are checked: speaker ids must be in 1..=max and intervals well formed. Overshoot of up to one 80 ms diarizer frame past the audio end is clamped; anything else is rejected.
- **Handy hooks:** supervisor 81 → 90, worker 7 → 8, transcription manager 90 → 91. Reviewed in `Upstream/UPSTREAM-DIVERGENCE.md`.
- **Not yet wired:** these are not connected to any command. The saved-audio pipeline (Step 3) is their caller.

### Validation (Windows 11, i5-14600KF, RTX 3080, Vulkan SDK 1.4.350)

- 27 non-ignored engine, lease, supervisor-protocol and TDT unit tests pass through `scripts/run-rust-test.ps1`. This includes 8 new tests covering role mapping, analysis-WAV rules, worker dispatch without a model, serde round trips with NaN confidence, lease and stale cancellation, and segment validation.
- Real-worker tests passed on **CPU and Vulkan**. They used the debug `handy.exe` worker, pinned Sortformer `diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf` (sha256 `a5dacdc6…2442a9`) and Parakeet `parakeet-tdt-0.6b-v3-Q8_0.gguf` (sha256 `5859f779…2cc7`). The audio was the native `samples/sortformer-2spk-mix.wav` (provenance unrecorded upstream; engineering fixture only).
  - The worker goes ASR → diarizer → ASR with one resident role.
  - Wrong-role loads leave nothing loaded.
  - Refused ASR work does not drop the diarizer.
  - The 12 s fixture gives 2 speakers and 5 segments.
  - Cancelling a 10-minute run in flight kills the worker and rejects the old epoch. A fresh attempt respawns the **diarizer** role and completes.
  - A GPU fault injected during diarization (`abort@diarize`, GPU only) is retried in a CPU-only worker with the diarizer role.
- Reproduce the role and fault tests with `cargo test --lib -- --ignored --test-threads=1 meeting_worker_tests`. Set `HANDY_TRANSCRIBE_WORKER_EXE`, `GRAIN_MEET_TEST_{DIARIZER,ASR,AUDIO,BACKEND}`, and use the Windows runner config.

### Diarization resources (one track, repeated fixture audio)

Measured with `scripts/grain_meet_diarize_probe.ps1`. The figures are the worker process's peak working set / peak private commit, which include the analysis buffer, native features and model weights. VRAM was not measured. Repeated audio stresses memory, not recognition quality.

| Audio | CPU peak WS / private | CPU speed | Vulkan host peak WS / private | Vulkan speed |
| ---: | ---: | ---: | ---: | ---: |
| 10 min | 422 / 611 MiB | 29x | 254 / 964 MiB | 340x |
| 60 min | 1,335 / 1,328 MiB | 29x | 1,244 / 1,365 MiB | 323x |
| 120 min | 2,523 / 2,519 MiB | 27x | 2,432 / 2,555 MiB | 341x |

Host memory grows about **1.2 GiB per hour of one track** on both backends. Of that, 230 MB per hour is the float32 analysis buffer; the rest is native whole-clip feature and activation memory.

- A 60-minute track fits the plan's proposed 2 GiB incremental processing target.
- A **120-minute track does not**. Tracks are processed one at a time, so a 60-minute dual-track meeting stays within the target.
- **Decision (2 November 2026, user): follow FluidMeet's numbers instead of running more experiments.** FluidMeet runs speaker detection after recording, not live; its live captions do no diarization. It diarizes each track's whole audio in **one run** ("one continuous state per track"), joining capture epochs with 0.5 s of silence. It never cuts a track into independently diarized pieces, because each piece would restart speaker numbering (speaker 1 in one piece is not speaker 1 in the next).
  - Grain now uses FluidMeet's ceiling: **256 Mi samples** (1 GiB of float32 PCM), about 280 minutes of one track or 140 minutes across two.
  - Like FluidMeet's, this bounds retained audio, not total memory. By the measurements above, a 140-minute track peaks near 3 GiB of host memory. Tune later if needed.
  - **Superseded the same day:** the user then set a **90-minute per-track cap** in place of the 256 Mi-sample ceiling (see the plan's memory section).
  - The other FluidMeet numbers to adopt are listed in the plan's [FluidMeet tuning table](EXECUTION-PLAN.md#fluidmeet-tuning-adopted-for-the-first-release).
- These are debug-build host measurements on one machine; a low-memory 8 GiB reference machine is still untested.


**2 November 2026 research update:** the transcribe.cpp pin, its Sortformer v2.1 support, and the Nemotron 3 options are documented in [DIARIZER-RESEARCH.md](DIARIZER-RESEARCH.md).

- The pinned source passes the native Sortformer Rust tests on Windows CPU and Vulkan.
- The Glimpse Nemotron 3 port runs on the same machine on CPU and Vulkan with identical fixture segments.
- The proposal to start on Nemotron 3 was declined for now: **the first implementation keeps Sortformer v2.1**. Nemotron 3 is a later upgrade.
- Work on the role-aware worker bridge is implemented and validated (Increment 2 above).

No Sortformer/Parakeet model execution, real-worker role switching, final word
assembly, cancellation during native execution, accelerated fallback, recognition
quality, or 10/60/120-minute process-tree memory measurements have passed yet.
No production duration or RAM envelope is established.

Next required work is a role-aware extension of the existing worker, followed
by saved-audio model tests. Resolve the buildable real-worker prerequisite first:
this cloud image lacks the documented Linux desktop development libraries and
administrator access. A licensed/reproducible meeting fixture set and verified
Sortformer/Parakeet artifacts are also needed for the model gates. Neither the
CPU adapter runner nor documentation closes those gates. Do not expose capture
or UI as ready until the execution plan's dependencies and acceptance checks pass.
