# Flow and FluidVoice architecture parity pass

Source audit: 2026-09-25. Scope is Grain **Flow** only. The reference is
`Refrence/FluidVoice-latest`, whose `Package.resolved` pins FluidAudio at
`3fd63887eef1dc25edea8263ce4b44aa854d898b`. The earlier decoder and
window-by-window audit is in `docs/FLOW-TDT-REWRITE.md`. FluidVoice is a macOS
CoreAudio/CoreML app; Grain uses a cross-platform CPAL/GGUF backend. This audit
compares behavior and work placement, not identical hardware throughput. No
FluidVoice application source is copied into Grain.

## End-to-end path

| Stage | FluidVoice-latest | Grain Flow | Result |
| --- | --- | --- | --- |
| Shortcut release | `GlobalHotkeyManager.stopRecordingInternal` serializes one stop and calls the output pipeline (`GlobalHotkeyManager.swift:2307`). | The shortcut handler calls `RealtimeTranscribeAction.stop`; the coordinator prevents overlapping processing (`shortcut/handler.rs:101`, `grain_actions.rs:578`). | Equivalent single-session ownership. |
| Capture start | `ASRService.start` arms capture and waits for first PCM before reporting running (`ASRService.swift:2175`, `:2495`). | The recorder reports success after CPAL stream start; the first callback arrives later (`managers/audio.rs:625`, `:729`). | First-PCM acknowledgement is a correctness gap on cold/USB/Bluetooth starts. It requires one shared capture contract, not a Flow-only visual shim. |
| Idle capture | Direct CoreAudio prewarms a prepared but inactive IOProc; the legacy AVAudioEngine is not kept warm because it can degrade Bluetooth playback (`ASRService.swift:1215`). | On-demand mic is closed after stop unless the user chose always-on or lazy close (`managers/audio.rs:1012`). | Intentional low-RAM/device policy. Porting prewarm needs a cross-platform lifecycle and measured benefit. |
| Audio ownership | `ThreadSafeAudioBuffer` retains recording PCM in memory (`ASRService.swift:1551`). | A buffered, temporary Float32 journal retains exact audio on disk; one bounded decode window lives in RAM (`grain_audio_journal.rs`, `rolling.rs:378`). | Grain's low-RAM choice is intentional; copying Fluid's full buffer would scale RAM with dictation length. |
| VAD and conditioning | Parakeet dictation does not wait for speech VAD; capture may use its own direct-audio preprocessing (`ASRService.swift:2510`). | Flow runs with `VadPolicy::Disabled`, though the shared recorder constructs Silero on its first cold open (`grain_actions.rs:529`, `managers/audio.rs:290`, `:696`). | No per-frame Flow VAD. Lazy construction for the shared recorder would need Standard/Native compatibility work. |
| Model readiness | Provider preparation runs at startup/selection; stop checks readiness and loads if cold (`ASRService.swift:1881`, `:3017`). | Model selection preloads unless unload is Immediate; Flow initiates loading on press and captures while it loads (`commands/models.rs:99`, `rolling.rs:118`). | Both overlap loading with useful work. Immediate unloading deliberately makes each short session cold. |
| Work during capture | A completion-driven streaming scheduler coalesces busy work (`ASRService.swift:5322`, `:5413`). | One serial worker coalesces wakes. Stable windows decode even with preview off; optional previews revisit the mutable tail 600 ms after the previous pass (`rolling.rs:390`, `:496`). | Same bounded scheduling idea. Always decoding short prefixes when preview is off would consume more CPU without removing Fluid's own short final decode. |
| TDT geometry and output | Pinned FluidAudio uses a 240,000-sample short path and 238,080-sample content windows, 32,000 overlap, token-ID/time merging (`ParakeetIncrementalSession.swift`). | `grain-tdt` ports that layout/merger; `tdt_flow.rs` uses a stateless native window adapter and complete token detokenization. | Source-level parity was audited in `FLOW-TDT-REWRITE.md`; real CoreML/GGUF text and latency parity is still unmeasured. |
| Stop boundary | Marks end at host time, drains the direct capture, then retires legacy audio engine off the main path (`ASRService.swift:2878`, `:2903`). | `stop_recording` drains the recorder and closes an on-demand mic before rolling finalization (`managers/audio.rs:964`). | Potential release-to-final gap. Deferring close needs a race-safe next-start gate and must not leave capture active. Do not copy macOS engine handling into CPAL. |
| Stop feedback and UI | A stop cue runs after capture; final-transcription status is deferred 100 ms for fast finishes (`ContentView.swift:2605`, `ASRService.swift:3059`). | Flow has no stop cue by user choice. The pill receives `RecordingStopped` immediately. Recording and Transcribing tray presentations are identical, so this pass removes the redundant Flow tray rebuild before capture stop (`grain_actions.rs:588`, `tray.rs:95`, `:251`). | No sound work or redundant tray I/O on Flow's stop-to-text path. |
| Final work | A serialized executor drains any active streaming pass, then performs final transcription (`ASRService.swift:2926`, `:3045`). | The recorder closes the journal and joins the dedicated TDT worker. Equal-sample preview can be reused; a failed decode gets one clean journal replay (`rolling.rs:167`, `:274`, `:416`). | Correct serial ownership; Grain's exact-count cache is an improvement over FluidVoice's short final path. |
| Model release | The provider normally remains prepared; teardown is lifecycle driven (`ASRService.swift:3017`). | The user-selected Immediate policy unloads during `finish_session`, before text delivery; other policies retain the model for a bounded interval (`rolling.rs:175`, `managers/transcription.rs:507`). | A deliberate RAM/latency tradeoff. Moving unload behind paste would retain a large model through optional AI work and history saving. Measure before changing the contract. |
| Text processing | Formatting and optional AI run after final ASR (`ContentView.swift:2605`, `:2780`). | One final dictionary/filler/snippet pass, then optional post-processing/Prompt Record (`grain_actions.rs:635`, `:708`). | Both have optional work that can dominate stop-to-text. Do not bypass user-selected processing. |
| Text delivery | `TypingService` queues a user-initiated worker and can post to a captured target PID with zero settle delay; no-PID fallback waits (`TypingService.swift:361`, `:414`). | Flow schedules `utils::paste` on Tauri's main thread; clipboard methods honor the configured before/after delays (`grain_actions.rs:720`, `clipboard.rs:766`, `:55`). | Plausible delivery gap. A Flow-only bypass of explicit paste settings or moving shared platform paste work off main needs real-target reliability tests. |
| History | Audio saving is detached, and database writes are queued (`ContentView.swift:3351`, `TranscriptionHistoryStore.swift:553`). | Paste is now dispatched before WAV export and SQLite history/cleanup, but Flow still waits for persistence before signalling pipeline completion (`grain_actions.rs:715`, `:735`). | Current text delivery no longer waits for disk. Fully detaching persistence would allow overlapping journal saves and needs bounded ordering and shutdown draining. |
| Recovery and cleanup | Buffer-handoff generations, route retries, and first-PCM readiness handle device changes (`ASRService.swift:2205`, `:2878`). | Session generations, a bounded wake channel, capture-worker death detection, one device-open retry, and cancellation/journal cleanup cover the shared backend (`rolling.rs:167`, `managers/audio.rs:625`). | Grain has the main ownership guards; first-PCM and host-time boundary semantics remain open. |
| Timing evidence | `ASR_BENCH`, `HISTORY_BENCH`, and typing benchmarks split capture, inference, delivery, and persistence (`ASRService.swift:2829`, `TypingService.swift:382`). | Grain logs stream open and total Flow finalization, but has no release-to-paste phase trace (`managers/audio.rs:729`, `rolling.rs:435`). | Stage timing is needed before changing capture teardown, worker priority, or paste reliability behavior. |

## Changes made for stop-to-text

- `07884f62` removed the Flow stop sound and scheduled text delivery before WAV
  and history I/O. Standard and Native ASR remain unchanged.
- This branch removes the Flow Recording-to-Transcribing tray refresh. Both
  states select the same icon and menu, while `RecordingStopped` still updates
  the pill. The Idle refresh still runs after text delivery.

## Existing speed controls

- A non-Immediate model unload setting can keep the selected TDT model ready
  between sessions. Immediate unload saves resident RAM but repeats model load
  and teardown for each session.
- Lazy microphone close skips device teardown on the stop path and reuses the
  stream for a restart within 30 seconds. The microphone stream remains open
  during that idle window, so this must remain a user choice.
- Clipboard paste delays are configurable. The delay before the paste shortcut
  directly affects visible insertion time; reducing it needs verification in
  the actual target applications. The after-paste delay affects cleanup, not
  the initial appearance of text.

## Next evidence gate

The source pass found no safe reason to add a second engine, an unbounded PCM
buffer, or a background persistence queue. The next improvements depend on
measured stage costs and target-app reliability:

1. Measure key release to recorder stop, rolling finish, final text, paste
   request, and actual insertion, with 1 s, 5 s, 15 s, 30 s, and long recordings.
   Report p50/p95 separately for warm/cold models and preview on/off.
2. Test on-demand microphone close, first-PCM readiness, device reconnects,
   fast stop/restart, and cancellation on Windows, macOS, and Linux before
   changing shared capture lifecycle.
3. Compare configured clipboard delays against zero delay in real editors,
   terminals, browsers, remote desktops, and accessibility targets. A speed
   gain that drops or misroutes text is not parity.
4. Measure resident/peak RAM and journal lifetime over 50 sessions and a
   two-hour dictation. Keep the Immediate unload policy meaningful.

No reviewed Parakeet TDT GGUF is present in this checkout or the local Grain
app-data model directory, so this pass cannot claim measured inference speed or
accuracy parity. The pinned FluidAudio source and existing pure/native tests
support the algorithm comparison; they do not replace the real-model gate.
