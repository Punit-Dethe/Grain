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

## Increment 3: durable meeting storage (Step 1)

Implemented in Grain-owned `src-tauri/src/grain_meet/` (`storage.rs`, `wav.rs`). Not yet called by capture (Step 4) or processing (Step 3).

- **Layout.** One directory per meeting, named by a UUID that is validated on every open (path syntax is refused). It holds a versioned `manifest.json` and per-track float32 WAV chunks at the native capture rate and channel count.
- **FluidMeet values.** Chunks rotate every 60 s, and early when the format changes, when capture reports a discontinuity, or when capture resumes more than 0.5 s late.
- **Timeline.** Within a run, frame counts place the audio and capture-timestamp jitter is ignored. Missing intervals are explicit `Gap`s with a reason (source lost, overflow, device change, pause, missing, unexplained), never silence. A clock that jumps backwards never overlaps recorded audio.
- **Durability.**
  - The manifest is replaced atomically (write, sync, rename).
  - A chunk is listed before its file exists, and marked sealed only after the file is synced and renamed.
  - The active chunk is checkpointed every 5 s, so a process crash loses at most about 5 s per track.
  - Power-loss safety is claimed only for sealed chunks.
- **Recovery.** `recover` salvages every whole frame of an interrupted chunk, drops a torn partial frame and seals the session. Absent sealed audio becomes an explicit `Missing` gap. Recovery never deletes recorded audio, and running it twice changes nothing.
- **Listing.** `list_sessions` reads manifests only, never audio. Corrupt or foreign entries are skipped and logged.
- **Handy hook:** `lib.rs` 1296 → 1297, one `mod` declaration.

Validation: 9 new unit tests pass.
- WAV: sealed files read back through `hound`; checkpoints make an open chunk readable; salvage handles a torn tail and refuses a mismatched format.
- Storage: 60 s rotation over 130 s of jittered blocks; gap and discontinuity handling; format change and a backwards clock; crash recovery after a simulated crash (no flush or drop); missing-audio gaps; manifest validation and listing.
- With the engine, lease, supervisor-protocol and TDT tests, 36 pass in total.

## Status of Step 0 and decisions

- **transcribe.cpp and diarizer:** see [DIARIZER-RESEARCH.md](DIARIZER-RESEARCH.md). The pin is upstream 0.4.0 plus the Grain Flow patch, the same release Handy uses. Sortformer v2.1 is kept for now; Nemotron 3 is a later upgrade.
- **Done:** Sortformer through the real worker, role switching, cancellation, GPU-loss recovery on CPU, and 10/60/120-minute worker memory on CPU and Vulkan (Increment 2).
- **Decided:** whole-track diarization, as in FluidMeet; a 90-minute processing cap per track; live preview and live transcription skipped.
- **Still open:** Parakeet word timing across bounded windows and final word assembly (Step 3); speaker-assignment quality on representative real meetings, which needs a licensed meeting fixture set; Linux/macOS worker runs; a low-memory reference machine.

## Increment 4: saved-audio pipeline (Step 3)

Implemented in `src-tauri/src/grain_meet/` (`analysis.rs`, `transcript.rs`, `processing.rs`). `processing::process` turns a recorded session into a published transcript revision through a `MeetingEngine`. It is not yet exposed as a Tauri command.

- **Analysis audio.**
  - Each track becomes one disposable 16 kHz mono float32 file (Grain's `PcmJournal`, deleted when the attempt ends). The diarizer and bounded Parakeet both read this file. The worker's analysis contract changed from WAV to these raw float32 files.
  - Runs split at gaps and format changes. Runs are joined with FluidMeet's 0.5 s silence; separator time maps to no meeting time.
  - Channels are averaged to mono. Resampling uses rubato with Handy's configuration (one 1024-frame FFT block, BlackmanHarris2), restarted per run. The leading filter delay is removed, so the length is exactly `floor(frames x 16000 / rate)` and the measured lag is zero at 16, 22.05, 44.1 and 48 kHz.
  - The 90-minute cap is checked from the manifest before any audio is read.
- **Phases.**
  - Every track's analysis file is prepared first.
  - Sortformer then runs over each whole track, one run per track, at the offline `VeryHighLatency` preset.
  - Loading Parakeet releases the diarizer. Each track is transcribed through the existing structured `TdtAccumulator`, with a fresh decoder per track.
  - Words are then attributed and the revision is published.
- **Word assembly across windows.** A native word opens at every window's first token. A first token without the SentencePiece word marker continues the previous word, and the joined ids are detokenized once through the native tokenizer. All other words keep their native text.
- **Attribution (FluidMeet values).**
  - A coverage floor of min(80 ms, 20% of the word); same-slot intervals are unioned.
  - Dominance: the leader covers at least 50%, the runner-up at most 20%, and the leader at least 2x the runner-up; otherwise the word is ambiguous.
  - One-word boundary recovery: within 80 ms, words up to 1.2 s, vetoed by any competing onset; recovered words never anchor further words.
  - A slot with fewer than 10 words and under 5 s beside a substantial speaker is left unassigned.
  - Grain addition: point-timed words use containment, so nothing is divided by zero.
- **Failure isolation.**
  - A diarizer failure keeps the text and marks the track `SpeakersUnavailable`. FluidMeet would skip ASR here; Grain's plan keeps the text.
  - An ASR failure marks the track `Failed` and keeps its speaker evidence.
  - An over-cap track is `TooLong`; an empty track is `Empty`.
  - Cancellation publishes nothing.
- **Revisions.** `transcripts/NNNNNN.json` is written atomically and never overwritten; the newest revision is current. Each revision records the raw diarizer activity, the full-sequence ASR text, the model files and backends, and anonymous labels ("Speaker N") numbered across the meeting.
- **Handy hook:** `Cargo.toml` 152 → 153 (a direct `rubato` dependency, same locked version).

Validation:
- 43 unit tests pass:
  - analysis: resampler length and lag, run and separator mapping, cap before I/O;
  - transcript: window-split words, coverage and dominance, union, point words, boundary recovery and vetoes, brief-voice folding, immutable revisions;
  - storage, engine, lease and TDT.
- The real-model pipeline test passes on **CPU (16.3 s) and Vulkan (6.5 s)** for a 96 s saved meeting (12 s two-speaker fixture repeated: a 72 s run across a 60 s chunk boundary, a 2 s gap, then a 24 s run):
  - With a missing diarizer, revision 1 has text and every word `Unavailable`.
  - With Sortformer, revision 2 is `Complete`: 96 words, 94 attributed, 2 speakers. The assembled words equal the reviewed full text, and no word falls in the gap.
  - The role, cancellation and GPU-fault tests still pass with the raw analysis files.
- Reproduce with `cargo test --lib -- --ignored --test-threads=1 saved_meeting_becomes`, using the same environment as Increment 2.
- Quality evidence is limited to the repeated public fixture; representative meetings are still needed.

## Increment 5: microphone recording and Meet commands (Step 4)

Implemented in `grain_meet/recorder.rs` and `grain_meet/commands.rs`, plus Grain-owned seams in `grain_audio.rs` and `grain_audio_manager.rs`.

- **One microphone stream.** A meeting records through the shared dictation recorder: Handy's 16 kHz mono speech stream, without VAD, mute, cues or RAM retention. No second device stream is opened.
  - The recorder's `Idle` check makes dictation, Flow and Agent refuse while a meeting records, and a meeting refuses while they record. Meeting start takes the same `capture_start_guard` as dictation.
  - **Deliberate choice:** the microphone track is stored at the models' 16 kHz instead of 48 kHz. Room-mode playback is therefore speech quality. 48 kHz application audio is a Step 5 capture concern.
- **Delivery path.** On the recorder's delivery thread, `feed` only copies each frame into a bounded queue (about 10 s) and never blocks. A full queue drops the frame and counts it.
- **Writer thread.**
  - Within a run, frame counts are the clock. A delivery stall that the recorder's ring absorbed is not a gap.
  - A known loss becomes an explicit gap placed by arrival time: `Overflow` after dropped frames, or `SourceLost` after 3 s without audio. A silent source is restarted at most every 5 s.
  - A restart can never revive a stopped meeting: `live` is held across start, stop and restart.
  - A write failure stops capture and keeps committed audio.
- **Commands.**
  - Recording: `meet_status`, `meet_start_recording` and `meet_stop_recording`.
  - Listing: `meet_list` lists meetings newest first, and recovers sessions interrupted by a crash on first listing (it never restarts capture).
  - Processing: `meet_process` runs in the background under the meeting inference lease, with stage reporting. `meet_cancel_processing` keeps the recording. `meet_transcript` returns the latest revision.
  - `meet_delete` is refused while the meeting is recording or processing.
  - Speaker model: `meet_diarizer_installed`, and `meet_download_diarizer`, which fetches the pinned Sortformer Q8_0 and verifies its size and SHA-256 before installing it.
  - Processing requires a reviewed Parakeet TDT v2/v3 model selected and downloaded.
- **Events:** `meet-status-changed` (the full status) and `meet-diarizer-progress`. TypeScript bindings are regenerated.
- **Handy hook:** `lib.rs` 1297 → 1310 (command and event registration only).

Validation:
- 56 unit tests pass. The recorder tests cover burst delivery placed by frame count, single-meeting refusal, routing ending at finish, source-loss restart with a `SourceLost` gap, an overflow gap, and a stall without a gap.
- The real-model pipeline test still passes (Vulkan: 96 s in 2.7 s).
- **Not yet run:** a recording from a physical microphone in the running app, including device unplug, sleep/resume and low disk. It needs the Meet UI (Step 6) or the agent harness, and remains a Step 4 acceptance item. Dictation refusal during a meeting is enforced but only logged; the visible reason belongs to the Meet UI.

## Increment 6: meeting workspace UI (Step 6, first pass)

Built on `feat/grain-meet` against the current interface. The user-facing name is the single string `ui2.meet.name` ("Recall"), used by the sidebar, page title and Quick Panel. Everything internal stays Meet.

- **Navigation.** The `#/meet` route, a sidebar entry with a microphone icon (below History), and a Quick Panel entry with meeting keywords.
- **Page** (`src/app/pages/MeetPage.tsx`, `src/app/meet/`):
  - Recorder: an optional title, start and stop, elapsed time, a no-audio warning while the microphone is silent, and write errors.
  - Speaker-model notice with a download and its progress.
  - Last processing error.
  - Meeting list: date, duration and state (recording, processing, transcribed, not processed).
  - Detail view:
    - transcribe, transcribe again and cancel, with the live stage;
    - copy the transcript as `[time] Speaker: text`;
    - delete, after a native confirmation;
    - on-demand audio, where clicking a turn's timestamp seeks the player;
    - transcript search;
    - per-track notes for missing speakers, failure, over-length or empty tracks.
  - Turns group consecutive words by speaker and split on a 1.5 s pause. Words from different tracks interleave by time and are never merged. Overlap is labelled "Several speakers".
- **Playback backend.** `meet_playback_audio` writes a 16 kHz 16-bit WAV per track on the meeting clock, so gaps play as silence and player time equals transcript time. It streams one block at a time and caps the file at 12 hours. It opens exactly that file to the asset protocol (and the fs scope on Linux).
- **Strings.** English only, under `ui2.meet.*`; other locales fall back by design.

Validation:
- 107 frontend unit tests pass, including new turn-building, formatting, export and filter tests and the route round-trip.
- `tsc`, the production `vite build`, ESLint on the changed files, Prettier, `ui_parity.py` and the divergence ratchet all pass.
- 24 Meet backend tests pass, including the playback-timeline test.
- **Not yet done:** speaker renaming (needs separate user-edit storage, per the plan), export formats beyond copied text, and a review by the user of the running app.

## Increment 7: Recall's own sources, system audio, and the polished workspace (2026-11-02)

User request: choose the microphone, system audio, or both for meetings, each with its own device, without touching dictation's microphone setting.

- **Recall owns its capture** (`grain_meet/sources.rs`, `grain_meet/recorder.rs`). This supersedes the shared-recorder design of Increment 5; the `grain_audio.rs`/`grain_audio_manager.rs` meeting seams are removed.
  - Settings live in `<app_data>/meetings/sources.json`: microphone on/off plus device, system audio on/off plus output device. At least one must be on. They never read or write dictation settings.
  - Microphone: a separate `handy-recorder` instance at 48 kHz mono (the plan's common operating format), opened at Record and closed at Stop.
  - System audio: WASAPI loopback of one output device through cpal 0.18 (already in the tree through rodio), on its own thread, Windows only. Other platforms show it as unavailable. Xrun, real-time-denied and default-route changes are not failures; other stream errors reopen the source.
  - Because nothing is shared, dictation, Flow and Agent are no longer blocked while a meeting records.
- **One writer, one clock.** Every source feeds one bounded queue (about 10 s for two sources) and one writer thread, each source to its own track.
  - A microphone with no audio for 3 s is reopened and the loss is a `SourceLost` gap.
  - Loopback delivers nothing while nothing plays, so a late resume is a new `GapReason::Silent` gap (known silence, compact on disk) and never reopens the device.
  - Any source that fails to open fails the start and leaves no session.
  - Peak levels are held between status reads (`fetch_max` on the f32 bits, `swap` on read) for the meters.
  - A second Record while the first is still opening devices is refused.
- **Playback** is one mixed file per meeting (`playback/meeting.wav`): each track is written on the meeting clock, then the tracks are summed sample by sample. `meet_playback_audio` lost its track argument.
- **Commands:** `meet_sources` (settings plus current device lists, off the async runtime) and `meet_set_sources`. `MeetingSummary.sources` and `RecordingStatus.sources` (per-source device, level, recorded time, active) are new.
- **Plan deviation (user-directed):** Step 5 asked for per-application loopback first, with whole-device loopback as a separately labelled choice. System audio here is the whole output device, labelled "System audio" with the device name. Per-application capture remains open in Step 5.
- **UI** (`MeetPage.tsx`, `meet/meetFormat.ts`):
  - Studio panel: title, Record, and two source tiles with switch and device picker; the last source left on cannot be switched off. With both on, a headphones hint covers echo.
  - While recording: timer, Stop, and a live level meter and state per source. Only this panel polls status (every 100 ms); the rest of the page updates from events.
  - Meeting list grouped by day (Today, Yesterday, date) with time, duration, source icons and state.
  - Detail: date, duration and sources; Transcribe, Transcribe again, Cancel, Copy, Delete; processing as four steps; a custom player with scrubber; transcript with numbered speaker avatars, a speaker legend, search highlighting, the turn being heard highlighted, timestamp seek, and a per-turn source icon when the meeting has two sources.
  - The eyebrow label above the page title is removed.

Validation:
- 27 Meet backend tests pass (737 lib tests in total). New: both sources to their own tracks, a failed open leaving nothing, loopback silence as a `Silent` gap with no reopen, a silent microphone reopened with `SourceLost`, a device failure reopening only that source, and the mixed playback file.
- The ignored `real_devices_open_and_deliver` test passed on this machine: the default microphone delivered 48,000 frames in 1 s, and loopback of the default output delivered 48,960 frames at 48 kHz while a test tone played. Output device ids round-trip through `parse`.
- Frontend: tsc, ESLint, Prettier, all vitest suites (including new `meetFormat` tests), the production build, `ui_parity.py` and the translation check pass. The impeccable detector reports nothing for the page and stylesheet.
- **Not yet done:** a real meeting in the running app (two sources, device unplug, sleep/resume, low disk), the user's visual review, per-application capture, speaker renaming and export formats.
