# GRAIN — Transition Log (read me first)

> You are an AI continuing work on **Grain** with no memory of prior sessions.
> Read this whole file before touching anything. It tells you what exists, what
> just shipped, what's next, and the traps. Companion docs:
> - `docs/transcribe-cpp Migration Plan.md` — the original migration plan + owner decisions.
> - SQLite `agent_logs.db` (via the `sqlite` MCP) — dated one-liners of past decisions/bugs.
> - `AGENTS.md` / `CLAUDE.md` — house rules (Rust+TS, low-RAM, "destroy if not in use",
>   frontend↔backend decoupled: commands one way, events the other).

---

## 0. What Grain is (30-second orientation)

- **Grain = a fork of `cjpais/handy`** (a local, offline speech-to-text app).
  Tauri **Rust backend** (`src-tauri/`) + **React/TS frontend** (`src/`).
- Remotes: `origin` = `Punit-Dethe/Grain` (ours), `upstream` = `cjpais/handy`.
  Branch: `main`. **No shared git history with upstream** — upstream syncs are
  strictly MANUAL file ports (copy upstream files, re-apply `[GRAIN]` hooks).
- **Grain-specific architecture on top of Handy** (KEEP — our differentiators):
  - `crates/grain-core` — headless typed event bus (`DaemonEvent`) + owned
    settings, broadcast over a local WebSocket (`ws://127.0.0.1:7124`).
  - `crates/grain-pill` — native winit + tiny-skia "pill" overlay (own process)
    + the **Studio Window** (live streaming caption card). Handy uses a React
    webview overlay (`src/overlay/`); **we do not** — native pill only.
  - Provider rotation (STT + post-process), Agent, Quick Panel, and the
    **rolling** capture mode — all ours.
- **Three capture shortcuts** (deliberate UX, keep):
  - `transcribe` = Batch (record → transcribe once → paste). Uses `selected_model`.
  - `transcribe_realtime` = Rolling (chunk-at-silence window). Uses `selected_model`.
  - `transcribe_native_asr` = Live streaming into the Studio Window. Uses
    `selected_asr_model`. Default `ctrl+alt+shift+space`.

---

## 1. What JUST shipped (this session): FULL upstream ASR unification

**Grain's entire ASR subsystem now mirrors Handy upstream (their `main` at
"introduce transcribe.cpp (#1529)" / v0.9.0): ONE `TranscriptionManager`, ONE
engine slot, one model registry with the full 65-model GGUF catalog. All three
capture modes share that single resident engine — the separate rolling
(grain-transcribe) and native-ASR (worker/manager) engines are GONE, along with
their idle RAM remnants.**

### Engine routing — transcribe-cpp ONLY (owner decision, diverges from upstream)
- `EngineType::TranscribeCpp` = every GGUF/GGML model → the `transcribe-cpp`
  crate (batch `session.run` AND streaming `session.stream`). It is the ONLY
  inference engine.
- **transcribe-rs (ONNX) was REMOVED ENTIRELY** in a follow-up session (owner
  decision — dead weight, all families ship as GGUF). Precondition verified
  before deletion: whisper (12), parakeet (10), moonshine (17) — and in fact
  every legacy family — exist as GGUF in the catalog. The `EngineType` ONNX
  variants remain as inert enum tags for upstream-diff parity; loading one
  errors with "download its GGUF version". The 11 legacy ONNX table entries
  were removed from `model.rs` (frontend shows only GGUF models now), along
  with the GigaAM migration, the `ort_accelerator` setting/enum/command, and
  the ORT dropdown in `AccelerationSelector`. NOTE: upstream still HAS
  transcribe-rs — on future syncs, do NOT reintroduce it; re-apply this removal.

### What was ported / replaced (upstream files, verbatim + marked hooks)
- `src-tauri/src/catalog/` — **NEW**: bundled `catalog.json` (65 models) +
  `mod.rs`. Copied verbatim.
- `src-tauri/src/managers/model.rs` — replaced wholesale (0 Grain edits).
  Producers: catalog, legacy URL table, custom models dir scan, shared HF cache
  scan. Downloads via `hf-hub` (cancellable fork) into the shared HF cache.
  Custom `.gguf`/`.bin` dropped into `<app_data>/models/` are discovered with a
  GGUF header capability probe (id = filename stem).
- `src-tauri/src/managers/{gguf_meta,model_capabilities}.rs` — **NEW**, verbatim.
- `src-tauri/src/managers/transcription.rs` — replaced with upstream's unified
  manager (engine slot + `StreamRouter` + stream worker with engine lease +
  idle watcher + accelerator resolution). **[GRAIN] hooks inside (grep
  `[GRAIN]`):**
  1. `initiate_model_load_for(model_id)` — per-category model selection
     (Batch/Rolling load `selected_model`; Native ASR loads
     `selected_asr_model`); swaps the resident engine when a different
     category's model is loaded. `initiate_model_load()` delegates to it.
  2. `emit_stream_text` also mirrors `DaemonEvent::AsrStreamText { committed,
     tentative }` over the WS bus to the pill.
  3. `rolling_hold` (+`set_rolling_hold`) — while a rolling session is live,
     per-chunk custom-word/filler post-processing is skipped and
     "Immediately" unload is deferred to session end.
- `src-tauri/src/commands/models.rs` — upstream version + our
  `has_any_models_*` onboarding helpers. `rescan_local_models` is new.
- `src-tauri/src/commands/native_asr.rs` — now TINY: `list_asr_models`
  (= unified list filtered `supports_streaming`) + `select_asr_model`
  (persists `selected_asr_model`). Download/delete/cancel go through the
  unified model commands.
- `src-tauri/src/managers/audio.rs` — recorder constructor takes
  `tm.stream_router()`; the per-frame fan-out feeds rolling + the router.
- `src-tauri/src/actions.rs` — `NativeAsrAction` = load selected streaming
  model via `initiate_model_load_for` → `tm.start_stream()` → on stop
  `tm.finalize_stream()` (30 s timeout; batch-fallback on `Ok(None)`, mirrors
  upstream's TranscribeAction) → paste + history + `AsrSessionFinal`.
  `CancelAction` calls `tm.cancel_stream()`.
- `src-tauri/src/rolling.rs` — rewritten as a pure DRIVER (isolated Grain
  module): keeps `rolling-window` chunking/assembly, transcribes each chunk via
  `tm.transcribe()` (word timings no longer available → fuzzy text seam only),
  brackets the session with `set_rolling_hold(true/false)` +
  `maybe_unload_immediately` at end.
- `src-tauri/build.rs` — upstream's, incl. `stage_transcribe_runtime_libs()`
  (staged 13 DLLs into `src-tauri/transcribe-libs/`, gitignored) +
  `tauri.windows.conf.json` (new) + linux `/usr/lib` bundle entries in
  `tauri.conf.json`. **Release DLL bundling is DONE.**
- Settings (`crates/grain-core/src/settings.rs`):
  `WhisperAcceleratorSetting`→`TranscribeAcceleratorSetting` (serde alias
  `whisper_accelerator` migrates old JSON), `whisper_gpu_device`→
  `transcribe_gpu_device` (deliberately NOT aliased — semantics changed to a
  transcribe-cpp registry index; legacy values reset to auto). Commands renamed
  to `change_transcribe_accelerator_setting` / `change_transcribe_gpu_device`.
- Frontend: `modelStore` unchanged (event names all still match).
  `asrModelStore.ts` DELETED — `AsrModelSection`/`AsrModelLibrary` now read the
  unified `useModelStore` filtered `supports_streaming`;
  `ModelLibrary` (Standard/Batch section) filters `!supports_streaming`.
  **That's the streaming-vs-batch list split.** `bindings.ts` regenerated.
- **DELETED** (destroy if not in use): `src-tauri/src/native_asr/`,
  `managers/asr_model.rs`, `managers/transcription_mock.rs`,
  `engine_lifecycle.rs`, `commands/native_asr.rs`'s start/stop commands, crates
  `grain-transcribe`, `grain-asr-core`, `grain-asr-sherpa`, `engine-lifecycle`.
  (Single engine slot makes the lifecycle arbiter structurally unnecessary.)

### Pill live-preview freeze — ROOT-CAUSED and fixed
Symptom: live text froze mid-dictation (classically right after a full stop);
final paste was always complete. Cause: the pill rendered **committed-only**
text, but transcribe-cpp's auto-commit goes long stretches without committing.
Measured on a real 90 s dictation via the gated smoke test: **34 s tail with
ZERO committed updates** while tentative updates kept flowing; `finalize()`
then commits everything (hence the perfect paste). Handy never shows this
because its overlay renders committed + tentative. Fix (Handy parity):
`AsrStreamText` now carries `tentative` (serde-default for compat); the pill
renders the tail dimmed-but-crisp (no blur) after the solid committed prefix.
Files: `grain-core/src/event.rs`, `managers/transcription.rs` hook,
`grain-pill/src/lib.rs` (`display_runs`, style map, `apply_event`).

### Verified state (not just "compiles")
- `cargo test --lib` (src-tauri): **96 pass**. Workspace crates: pass. `tsc
  --noEmit` clean. App boots; catalog seeds 65 models; devices = Vulkan0 (RTX
  3080) + CPU.
- Headless E2E (`--transcribe-file`): parakeet ONNX ("onnx" backend, 18×RT) ✔;
  nemotron GGUF via transcribe-cpp on **Vulkan0** (5.7×RT) ✔.
- Streaming E2E (no mic needed): `src-tauri/tests/streaming_smoke.rs`, gated on
  `GRAIN_TC_GGUF`+`GRAIN_TC_WAV` — streamed 90 s at ~9×RT, produced the exact
  transcript, and captured the commit-gap numbers above. DLL trick: copy
  `C:/gt/debug/transcribe.dll` + `ggml*.dll` into `C:/gt/debug/deps/` first.
- NOT yet verified live: mic dictation through the GUI (all three shortcuts) —
  needs a human speaking. Everything up to the mic is exercised.

### Follow-up session: streaming panic fix + category separation + ONNX removal
- **Streaming was silently dead in the GUI**: `StreamTextEvent.emit` PANICKED
  ("Event stream-text-event not found in registry!") because the event wasn't
  in our `collect_events![]` — tauri-specta rc.21 panics on unregistered
  emits. The panic killed the stream worker mid-lease (engine dropped → even
  the batch fallback found nothing loaded → no pill text, no paste). FIX:
  `StreamTextEvent` + `StreamPhaseEvent` registered in `lib.rs`
  (`mount_events` already ran at setup). GOTCHA for future syncs: any
  tauri_specta event a ported file emits MUST be in `collect_events![]`.
- **Strict per-category model selection** (`selected_model` = standard only,
  `selected_asr_model` = streaming only), enforced at every layer:
  - Hard guards at the two choke points: `switch_active_model` rejects
    streaming models; `select_asr_model` rejects non-streaming (empty id =
    clear is allowed).
  - `auto_select_model_if_needed` only auto-picks non-streaming, and CLEARS a
    legacy streaming `selected_model`.
  - Tray model submenu + Quick Panel `LocalModelSelect` + Onboarding filter to
    non-streaming. Settings lists were already split by `supports_streaming`.
  - The footer's model SELECTOR was replaced by a READ-ONLY status indicator
    (`model-selector/ModelSelector.tsx`): shows whichever model currently
    occupies the shared engine slot (follows `model-state-changed` model_id,
    so it flips batch↔streaming naturally) + loaded/unloaded. Its dropdown,
    `ModelDropdown.tsx`/`ModelStatusButton.tsx`, and its
    auto-select-on-download behavior were deleted.

### User-machine migration notes (this dev machine)
- The user has since downloaded + selected catalog GGUF models for BOTH
  categories (batch = parakeet-tdt-0.6b-v2-gguf, streaming = nemotron
  catalog entry), so nothing is stranded by the ONNX removal.
- Redundant files that can be deleted manually: `<app_data>/models/asr-gguf/`
  (~700 MB), `<app_data>/models/asr/` (sherpa bundles),
  `<app_data>/models/parakeet-tdt-0.6b-v2-int8/` (retired ONNX parakeet,
  ~450 MB), and `<app_data>/models/nemotron-3.5-streaming-0.6b.gguf` (716 MB
  custom copy, superseded by the catalog download in the HF cache).

---

## 2. Immediate NEXT STEPS

1. **Live GUI verification** (needs a human): `bun tauri dev` →
   (a) Streaming shortcut with the nemotron custom model → Studio Window must
   show solid committed text + a dimmer moving tail, no more freezes;
   (b) Batch shortcut (parakeet) still pastes; (c) Rolling shortcut assembles
   and pastes; (d) Settings → Speech to Text shows Streaming vs Standard lists
   correctly split; downloads via the new HF path work.
2. **Rolling window vNext:** a full analysis + research + phased plan exists in
   `docs/Rolling Window vNext Plan.md` (approved direction, NOT yet
   implemented). Phase 1 is a real regression fix: the unification dropped
   word timings, so the assembler currently runs plain text merge instead of
   its time-based dedup — transcribe-cpp exposes word/token timestamps to
   restore it. Live preview is Phase 4 of that plan.
3. **Upstream sync cadence:** when upstream moves more ONNX models to GGUF or
   bumps transcribe-cpp past 0.1.0, re-port `managers/transcription.rs` +
   `model.rs` + `catalog.json` (files are verbatim-portable; re-apply the
   `[GRAIN]`-marked hooks, which are all grep-able).
4. **Cleanup (low priority):** legacy sherpa-era `DaemonEvent::AsrPartial/
   AsrCommit/AsrSegmentFinal` variants + pill handlers still exist (harmless);
   remove once nothing constructs them. `StreamPhase*`/`is_streaming`/
   `emit_stream_working` in transcription.rs are unused-by-us upstream API
   (dead-code warnings) — keep for sync parity.

---

## 3. GOTCHAS / unwritten rules / fragile deps (READ)

- **`transcribe_cpp::init_backends_default()` MUST run once at startup** before
  any `Model::load` (both GUI and headless paths call
  `managers::transcription::init_transcribe_backend()`). Skip it → zero
  compute devices, every load fails. #1 trap.
- **Build target dir is `C:\gt` (src-tauri), NOT repo `target/`** —
  `.cargo/config.toml` MAX_PATH workaround. `handy.exe` → `C:\gt\debug\`.
  Do NOT "fix" this.
- **Tests touching transcribe-cpp need the DLLs in the deps dir**:
  `cp C:/gt/debug/transcribe.dll C:/gt/debug/ggml*.dll C:/gt/debug/deps/`.
  Access violation or "0 devices" in a test = this.
- **transcribe-cpp is v0.1.0, pinned** — expect API churn on bumps. Streaming
  API notes: `stream.feed(&[f32]) -> StreamUpdate{committed_changed,
  tentative_changed,..}`; `stream.text() -> {committed, tentative}`;
  `finalize()` commits everything. 16 kHz mono f32, any chunk size.
  **Auto-commit can stall for 30 s+ — that's WHY the pill renders tentative.**
- **`Stream<'a>` borrows the `Session`** — upstream's worker keeps both on one
  thread inside a labeled block; don't restructure.
- **Per-category selections share ONE engine slot.** Any new load path must go
  through `initiate_model_load_for`/`load_model` so the slot swap stays
  correct. Never add a second resident engine.
- **Rolling hold**: `tm.transcribe()` behaves differently under
  `set_rolling_hold(true)` (raw text, no immediate unload). Rolling MUST
  release the hold in every exit path (finish + cancel do).
- **The pill is a separate process** on the WS at `127.0.0.1:7124`. Backend →
  pill via `crate::bridge::emit(DaemonEvent)`. Frontend↔backend only via
  commands/events — don't blur.
- **Single-instance:** a 2nd `handy.exe` forwards + exits (empty log ≠ crash).
  `taskkill //F //IM handy.exe //T; taskkill //F //IM grain-pill.exe //T`
  before clean boot tests.
- **transcribe-rs (ONNX) + transcribe-cpp coexist** only because transcribe-cpp
  uses `dynamic-backends` DLLs on Windows/Linux. Never switch it to a static
  build here (ggml duplicate-symbol LNK2005).
- **Windows ships NO ort GPU feature anymore** (upstream dropped
  `ort-directml`; prebuilt ORT's /arch:AVX2 crashes pre-Haswell CPUs). ONNX
  legacy models run on CPU, exactly like upstream. GGUF models get the GPU via
  Vulkan.
- **Old `whisper_gpu_device` values are intentionally dropped** (UI-ordinal →
  registry-index semantic change); `whisper_accelerator` migrates via alias.
- **`selected_asr_model` may point at a model id that no longer exists** for
  upgrading users — the shortcut then emits a pill `ModelError` telling them to
  pick a streaming model. Not auto-cleared by design.

---

## 4. How to build / run / verify

- Backend: `cd src-tauri && cargo build` (first build compiles ggml, ~3–4 min).
- App: `bun tauri dev`, or `C:\gt\debug\handy.exe` after a build.
- Tests: `cargo test --lib` in src-tauri (96); `cargo test --workspace` at root
  (grain-core, grain-pill, rolling-window, provider-router).
- Frontend: `node node_modules/typescript/bin/tsc --noEmit`; format via
  prettier. `src/bindings.ts` re-exports on every debug app start (running
  `C:\gt\debug\handy.exe --list-devices` from `src-tauri/` is the quick way).
- Headless E2E: `handy.exe --list-devices`;
  `handy.exe --transcribe-file <16k mono s16 wav> [--model <id>] --json`.
- Streaming smoke: see §1 "Verified state".

---

## 5. TL;DR for the next you

The **whole ASR stack now mirrors Handy upstream on one unified
transcribe-cpp/ONNX engine** — batch, rolling, and live streaming share one
resident model; the 65-model catalog is in; streaming vs batch lists are split
in Settings; release DLL bundling is done; the pill freeze was a commit-stall
made invisible by committed-only rendering and is fixed by rendering the
tentative tail. Your job: run the live mic checks in §2.1, then rolling live
preview (§2.2). Mind the init/DLL/MAX_PATH traps in §3.
