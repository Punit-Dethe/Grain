# GRAIN — Transition Log (read me first)

> You are an AI continuing work on **Grain** with no memory of prior sessions.
> Read this whole file before touching anything. It tells you what exists, what
> just shipped, what's next, and the traps. Companion docs:
> - `docs/transcribe-cpp Migration Plan.md` — the migration plan + owner decisions.
> - `docs/Improved Model-Agnostic Native ASR Architecture Plan.md` — older ASR design history.
> - SQLite `agent_logs.db` (via the `sqlite` MCP) — dated one-liners of past decisions/bugs.
> - `AGENTS.md` / `CLAUDE.md` — house rules (Rust+TS, low-RAM, "destroy if not in use",
>   frontend↔backend decoupled: commands one way, events the other).

---

## 0. What Grain is (30-second orientation)

- **Grain = a fork of `cjpais/handy`** (a local, offline speech-to-text app).
  Tauri **Rust backend** (`src-tauri/`) + **React/TS frontend** (`src/`).
- Remotes: `origin` = `Punit-Dethe/Grain` (ours), `upstream` = `cjpais/handy`.
  Branch: `main`. We MUST stay upstream-mergeable.
- **Grain-specific architecture we added on top of Handy** (KEEP these — they're
  our differentiators and are engine-agnostic):
  - `crates/grain-core` — a **headless typed event bus** (`DaemonEvent` enum) +
    owned settings, broadcast over a local WebSocket (`ws://127.0.0.1:7124`).
  - `crates/grain-pill` — a **native winit + tiny-skia "pill" overlay** (its own
    process, launched by the app). Renders recording/processing/idle states and
    the **Studio Window** (the live streaming caption card). It subscribes to
    `DaemonEvent`s over the WS. Handy uses a React webview overlay; **we do not** —
    we use this native pill.
  - Provider rotation (STT + post-process), an Agent, a Quick Panel, a rolling
    (real-time whisper) engine — all ours.
- **Three capture shortcuts** (this is a deliberate UX we keep):
  - `transcribe` = Batch (record → transcribe once → paste).
  - `transcribe_realtime` = Rolling (whisper rolling-window).
  - `transcribe_native_asr` = **Native ASR / live streaming** (the thing we just
    migrated). Default binding `ctrl+alt+shift+space` (see
    `crates/grain-core/src/settings.rs` `get_default_settings`).

---

## 1. What JUST shipped (this session): sherpa → transcribe.cpp migration

**We replaced the Native ASR (live streaming) engine from Sherpa-ONNX to Handy's
`transcribe-cpp` crate, and VERIFIED it end-to-end on the GPU.** transcribe-cpp is
a unified GGUF/ggml ASR runtime (crates.io `0.1.0`, git-vendored by Handy). It is
model-agnostic (whisper, parakeet, nemotron, voxtral, moonshine, canary, cohere,
qwen3, granite…), does **batch AND streaming**, and **stabilizes streaming text
internally** (committed/tentative via `CommitPolicy::Auto`) — so our old SAPrefix
stabilizer is gone from this path.

### Verified state (not just "compiles")
- `cargo build` (default features) is **clean**; app **boots**; **78 tests pass**.
- On boot the log shows: `transcribe-cpp initialized with 2 compute device(s):
  [Vulkan0 (vulkan), CPU (cpu)]` — the Vulkan GPU backend registered.
- A real streaming smoke test loaded `moonshine-streaming-small` GGUF, fed a
  16 kHz wav through **our** `drive_stream` worker, and produced the exact
  transcript on the **Vulkan GPU**:
  `After early nightfall, the yellow lamps would light up here and there, the
  squalid quarter of the brothels.`
- Frontend `tsc --noEmit` clean.

### The DLLs (Windows) — how the native runtime ships
- `transcribe-cpp` built with `dynamic-backends,vulkan` ships ggml as **separate
  DLLs**, which is WHY it coexists with our `transcribe-rs` (whisper) with **no
  duplicate-symbol clash** (the pain sherpa had). The `transcribe-cpp-sys` build
  script **auto-copies** them next to the exe: `transcribe.dll`, `ggml.dll`,
  `ggml-base.dll`, `ggml-vulkan.dll`, `ggml-cpu-<ISA>.dll` (alderlake, haswell,
  icelake, skylakex, sandybridge, sse42, x64, …). They're in `C:\gt\debug\`
  next to `handy.exe` (dev). **Release bundling of these DLLs is NOT done yet
  (see Next Steps / Gotchas).**

---

## 2. Exact codebase state — file map for the Native ASR / streaming path

**Cargo (`src-tauri/Cargo.toml`):**
- `default = []` — **sherpa is now opt-in** (`native-asr-sherpa = ["dep:grain-asr-sherpa"]`),
  effectively RETIRED from the normal build. Do not re-enable unless comparing.
- Base deps added: `transcribe-cpp = { version = "0.1.0", default-features = false }`
  and `hf-hub = { git = "https://github.com/cjpais/hf-hub", branch = "cancellable-downloads", features = ["tokio"] }`.
- Per-platform transcribe-cpp features (mirror upstream EXACTLY):
  - `[target.'cfg(all(windows, target_arch = "x86_64"))'.dependencies]` → `["dynamic-backends","vulkan"]`
  - Windows aarch64 → base default (static CPU-only).
  - macOS → `["metal"]` (static, ships no DLLs).
  - Linux → `["dynamic-backends","vulkan"]` (needs `$ORIGIN/../lib` rpath in build.rs — NOT yet ported).
- `transcribe-rs` still has `whisper-cpp`/`whisper-vulkan`/`whisper-metal` (Batch/
  Rolling still use it). They coexist fine.

**Engine lifecycle / streaming worker (all under `src-tauri/src/`):**
- `lib.rs` (setup, ~line 288): calls **`native_asr::init_transcribe_backend();`**
  ONCE before creating the native manager. **This is mandatory** (see Gotchas).
  Also constructs `native_input`, `native_manager`, and the `engine_lifecycle`
  arbiter (which enforces ≤1 heavyweight engine across Batch/Rolling/NativeAsr).
- `native_asr/mod.rs` — exports `NativeAsrInput`, `NativeAsrManager`, and
  `init_transcribe_backend()` (calls `transcribe_cpp::init_logging()` +
  `init_backends_default()`; logs registered devices).
- `native_asr/input.rs` — `NativeAsrInput`: the atomic-gated mic frame sink
  (pre-roll ring + bounded drop-on-overflow queue). Unchanged by the migration.
  The recorder fans frames into it (see `managers/audio.rs:~159` `input.feed(frame)`).
- `native_asr/worker.rs` — **REWRITTEN.** `drive_stream(gguf_path, language,
  session_id, next: FnMut()->FrameCmd, emit: FnMut(DaemonEvent)) -> Result<String>`.
  Loads `Model::load(gguf_path)` → `model.session()` → `session.stream(&RunOptions,
  &StreamOptions::default())` → loop `stream.feed(&pcm)`; on `update.committed_changed`
  read `stream.text().committed` and emit `DaemonEvent::AsrStreamText { committed }`;
  on Stop `stream.finalize()` then emit final `AsrStreamText` + `AsrSessionFinal`.
  `FrameCmd` is now just `Frame(AudioFrame)` | `Stop`. Has a `#[cfg(test)]`
  smoke test `streams_a_real_gguf_when_present` (gated on `GRAIN_TC_GGUF` +
  `GRAIN_TC_WAV`). NO stabilizer, NO backend trait.
- `native_asr/manager.rs` — **REWRITTEN.** `NativeAsrManager::start(gguf_path:
  PathBuf, language: Option<String>) -> u64` spawns the worker thread (owns the
  `Session`+`Stream` on that thread — required, `Stream<'a>` borrows `Session`),
  drains `NativeAsrInput`, emits via `crate::bridge::emit`. `stop() -> Option<String>`
  joins + returns the final transcript. `cancel()` disarms + stops. `is_running()`.
- `managers/asr_model.rs` — **REWRITTEN** into a GGUF manager. In-code
  `GGUF_CATALOG` (3 streaming models: `nemotron-3.5-streaming-0.6b`,
  `parakeet-unified-en-0.6b`, `moonshine-streaming-small`). Downloads the single
  `.gguf` **directly from HF resolve URL** (no hf-hub yet, no extraction) into
  `<app_data>/models/asr-gguf/<id>/<file>.gguf`. Emits `asr-model-download-progress`.
  Key methods: `list() -> Vec<AsrModelInfo>`, `download(id)`, `cancel_download`,
  `delete`, **`get_gguf_path(id) -> Option<PathBuf>`** (the file the worker loads).
- `commands/native_asr.rs` — **REWRITTEN** (see the file; it's the current one).
  Tauri commands: `list_asr_models`, `download_asr_model`, `cancel_asr_model_download`,
  `delete_asr_model`, `select_asr_model` (persists `selected_asr_model`),
  `start_native_asr`, `stop_native_asr`, `native_asr_running`. Helper
  `language_hint(app) -> Option<String>` (settings `selected_language`, "auto"→None).
- `actions.rs` — `NativeAsrAction` (the `transcribe_native_asr` shortcut handler):
  resolves `asr_models.get_gguf_path(selected)`; if absent, emits
  `DaemonEvent::ModelError` (pill shows "Install and select a streaming model…")
  and returns; else opens mic (binding `"native_asr"`), emits
  `RecordingStarted { mode: SessionMode::NativeAsr }`, calls
  `manager.start(gguf_path, language_hint(app))`. `stop()` finalizes → paste +
  history. `CancelAction` also calls `native_manager.cancel()`.
- `transcription_coordinator.rs` — `is_transcribe_binding` includes
  `transcribe_native_asr` (so it goes through the serialized record/transcribe
  lifecycle, same as Batch/Rolling).

**Events (`crates/grain-core/src/event.rs`):**
- Added `DaemonEvent::AsrStreamText { session_id, committed }` — **the committed
  transcript so far (cumulative, flicker-free)**. This is what the pill renders.
- Legacy `AsrPartial`/`AsrCommit`/`AsrSegmentFinal` still exist (sherpa-era) but
  are unused by the transcribe-cpp path. `AsrSessionFinal` still emitted at end.
- `SessionMode::NativeAsr` tells the pill to show the Studio Window (vs the small pill).

**Pill (`crates/grain-pill/src/lib.rs`):**
- `apply_event`: on `AsrStreamText { committed }` (while `Recording`) → `r.asr.committed = committed`.
- **Committed-only rendering**: `AsrDisplay::committed_runs()` filters to committed
  words; the Studio Window draws committed text crisp and does **NOT** draw a
  tentative/blurred tail (we matched Handy — removed the blur). `paint_studio_card`
  is windowing-free + has a PNG render test (`studio_card_renders_to_png`, writes
  to temp — inspect it to see the design). Studio card is 452×156, 3 lines,
  a small dot-matrix "equalizer" top-right (recording = grey→white density by
  voice level; processing = orange sparkle; both re-roll on a quantized cadence).

**Frontend (streaming model UI — already works, unchanged types):**
- `src/stores/asrModelStore.ts` — zustand store: `listAsrModels`, `downloadAsrModel`,
  progress via `asr-model-download-progress` event, cancel/delete. (It also listens
  for `asr-model-extraction-*` events which **no longer fire** for GGUF — harmless.)
- `src/components/settings/AsrModelLibrary.tsx` — renders the streaming models via
  the shared `ModelCard` (adapts `AsrModelInfo`→`ModelInfo`).
- `src/components/settings/speech-to-text/AsrModelSection.tsx` — the "Streaming
  model" collapsible section (twin of `LocalModelSection` = "Standard/Batch model").
  Both live in `SpeechToTextSettings.tsx`. This is the streaming-vs-batch split.
- `src/stores/settingsStore.ts` — `selected_asr_model` updater calls
  `commands.selectAsrModel`.
- Shortcut UI: `src/components/settings/general/GeneralSettings.tsx` shows
  `transcribe`/`transcribe_realtime`/`transcribe_native_asr` shortcuts. Quick Panel
  trio in `src/components/quick-panel/ModuleA.tsx`.
- `src/bindings.ts` is **specta-generated** on app startup in debug (see below).

---

## 3. Immediate NEXT STEPS (where to continue)

1. **Manually verify the live UI flow** (the code is verified via smoke test, but
   not through the running GUI): `bun tauri dev` → Settings → Speech to Text →
   Streaming section → download **Moonshine Streaming Small** (only 189 MB) →
   select it → press the Streaming Transcribe shortcut → speak → confirm the
   Studio Window shows committed text growing live → release → confirm paste.
2. **Regenerate `src/bindings.ts`** if any command/type signature changed. It
   auto-exports on debug startup (`lib.rs` specta `.export(... "../src/bindings.ts")`).
   The migration did NOT change `AsrModelInfo` shape or command names, so bindings
   are current — but re-run the app once to be safe.
3. **Release bundling of transcribe-cpp DLLs** (NOT done): port Handy's
   `build.rs` `stage_transcribe_runtime_libs()` (copies runtime libs from
   `DEP_TRANSCRIBE_CPP_RUNTIME_DIR`/`DEP_TRANSCRIBE_CPP_MODULE_DIR` env into
   `src-tauri/transcribe-libs/`) + add that dir to `tauri.conf.json` bundle
   resources, + the Linux `$ORIGIN/../lib` rpath (`cargo:rustc-link-arg`). Dev
   works without this because the sys crate copies DLLs next to the exe.
4. **Batch + Rolling unification (the big next phase, owner wants it):** move
   `transcribe` (batch) and `transcribe_realtime` (rolling) onto the SAME
   transcribe-cpp `Session` (`session.run` for batch; chunked run / streaming for
   rolling). Owner rules for this phase:
   - **Keep the 3 shortcuts.** Shortcut → per-category model: ASR shortcut loads
     the selected **ASR/streaming** model; Batch loads the **batch** model;
     Rolling loads the **batch** model but through our **rolling engine**.
   - **Rolling is OURS (Handy has none).** Unify onto one engine but keep the
     rolling driver **isolated** in its own module (no edits inside ported
     upstream files) so upstream syncs stay clean manual merges.
   - **Rolling live-preview (LATER):** we already VAD/silence-segment; on each
     commit-at-silence, surface that text as a toggleable live preview in rolling.
   - Adopt Handy's **full GGUF model catalog** (`upstream/main:src-tauri/src/catalog/catalog.json`)
     and split UI **streaming vs batch by `supports_streaming`** (capability from
     GGUF metadata — port `gguf_meta.rs` + `model_capabilities.rs` if you want the
     runtime-truth capability, or keep the hardcoded per-entry flag for now).
5. **Cleanup (low priority):** delete `crates/grain-asr-sherpa` and the now-dead
   `crates/grain-asr-core` bits (stabilizer, `NativeAsrBackend`, `AsrModelSpec`,
   `AsrTuning`, sherpa registry). `grain-asr-core::session::AudioFrame`/`AudioFormat`
   are STILL USED by `native_asr/input.rs` + `worker.rs` — keep those (or move them
   into `grain-core`) before deleting the crate.

---

## 4. GOTCHAS / unwritten rules / fragile deps (READ)

- **`transcribe_cpp::init_backends_default()` MUST run once at startup** (we call
  it in `lib.rs` via `native_asr::init_transcribe_backend()`), BEFORE any
  `Model::load`. With `dynamic-backends` it dlopen's the ggml DLLs next to the exe;
  skip it and the engine registers **zero compute devices** and every model load
  fails. This bit me — it's the #1 trap.
- **Build target dir is `C:\gt` (and `C:\t`), NOT the repo `target/`.** There are
  `.cargo/config.toml` files setting `target-dir`. This is a deliberate Windows
  **MAX_PATH** workaround (whisper.cpp/ggml Vulkan CMake nested paths exceed 260
  chars otherwise). `handy.exe` builds to `C:\gt\debug\handy.exe`. Do NOT "fix"
  the target dir.
- **Running a `cargo test` that touches transcribe-cpp needs the DLLs next to the
  TEST binary** (`C:\t\debug\deps\`), which the build script does NOT auto-populate
  (it only stages next to `handy.exe` in the profile dir). Before running such a
  test: `cp C:/gt/debug/transcribe.dll C:/gt/debug/ggml*.dll C:/t/debug/deps/`
  (or the `C:\t` equivalent for the test profile). Same trick was needed for the
  old sherpa smoke test. If a test crashes with an access violation or "0 devices",
  this is why.
- **transcribe-cpp is v0.1.0** — pin it; expect API churn. Its streaming API:
  `Model::load(path)`/`load_with(&ModelOptions{backend,gpu_device})` →
  `model.session()`/`session_with(&SessionOptions)` → `session.stream(&RunOptions{
  task:Task::Transcribe, language, ..Default::default()}, &StreamOptions::default())`
  → `Stream<'a>` (borrows the Session — keep both on ONE thread) → `stream.feed(&[f32])
  -> StreamUpdate{committed_changed,tentative_changed,is_final,revision,audio_committed_ms,..}`;
  read text via `stream.text() -> StreamText{committed, tentative, display()}`;
  `stream.finalize() -> StreamUpdate`; then `stream.text().committed` = full text.
  Audio is **16 kHz mono f32**; feed any chunk size (it buffers). Batch =
  `session.run(&[f32], &RunOptions) -> Transcript`.
- **Do NOT re-add the SAPrefix stabilizer** to the transcribe-cpp path.
  transcribe-cpp already commits (CommitPolicy::Auto). `AsrStreamText.committed` is
  cumulative — the pill SETS it (does not append).
- **Pill shows committed-only** (owner decision, matches Handy). Do not re-add the
  blurred tentative tail.
- **The pill is a separate process** launched by the app; it talks over the WS at
  `127.0.0.1:7124`. UI state flows: backend `crate::bridge::emit(app, DaemonEvent)`
  → WS → pill `apply_event`. Frontend↔backend is Tauri commands (frontend→backend)
  and events (backend→frontend); don't blur that.
- **Single-instance:** launching a 2nd `handy.exe` while one runs makes the new
  one forward + exit early (empty log) — that's not a crash. Kill leftovers
  (`taskkill //F //IM handy.exe //T; taskkill //F //IM grain-pill.exe //T`) before
  a clean boot test.
- **`transcribe-rs` and `transcribe-cpp` coexist** ONLY because transcribe-cpp
  uses `dynamic-backends` (ggml in DLLs). If you ever switch transcribe-cpp to a
  STATIC build, you'll get ggml duplicate-symbol LNK2005 like sherpa did. Don't.
- **Downloads:** GGUF is a single file (no extraction). We download directly from
  `https://huggingface.co/<repo>/resolve/main/<file>.gguf` with our reqwest
  streamer. Handy uses the `hf-hub` crate instead — we added the dep but don't use
  it yet; switching to it later is fine (cancellable/resumable/cache).
- **CI / light builds** can `--no-default-features` to skip transcribe-cpp's native
  build (though nothing else currently depends on that).
- **Model store extraction listeners** in `asrModelStore.ts` are now dead (GGUF has
  no extract phase). Harmless; clean up when convenient.
- **`_manager` param in `select_asr_model`** is intentionally unused (kept so the
  command signature/state-injection stays stable); don't delete it thinking it's dead.

---

## 5. How to build / run / verify

- Build backend: `cd src-tauri && cargo build` (default features; transcribe-cpp on,
  sherpa off). First build compiles ggml native (~3–4 min) then caches.
- Run the app: `bun tauri dev` (frontend `vite` + backend). Or run
  `C:\gt\debug\handy.exe` directly after a build.
- Tests: `cargo test` in `src-tauri` (78 pass). Pure crates:
  `cargo test -p grain-core -p grain-pill` etc.
- Format: `cargo fmt` (src-tauri) + `bun run format` / prettier for TS.
- Frontend typecheck: `node node_modules/typescript/bin/tsc --noEmit`.
- The real streaming smoke test:
  `GRAIN_TC_GGUF=<path.gguf> GRAIN_TC_WAV=<16k.wav> cargo test --lib
  streams_a_real_gguf_when_present -- --nocapture` (after staging DLLs into deps/).

---

## 6. TL;DR for the next you
The **live Native ASR path is fully migrated to transcribe-cpp and works on the
GPU**. Backend is done + tested. Your job: (a) sanity-check the live flow in the
actual GUI, (b) do release DLL bundling, then (c) the big one — **unify Batch +
Rolling onto the same transcribe-cpp engine** per the owner rules in §3.4, keeping
the 3 shortcuts and isolating our Rolling engine. Mind the init + DLL + MAX_PATH
gotchas in §4. Read `docs/transcribe-cpp Migration Plan.md` for the decision trail.
