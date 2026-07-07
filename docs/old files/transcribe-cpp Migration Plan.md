# Grain → transcribe.cpp Migration Plan (live-first)

**Status:** PLAN ONLY — nothing executed yet.
**Decision (owner):** Commit to Handy's `transcribe-cpp` as the ASR engine. Do
the **live/streaming** path first, structured so **batch + rolling** can move to
the same engine later. Keep Grain's differentiators and the **three-shortcut
UX** (Batch / Rolling / Real-time) on top. Retire the sherpa Native ASR engine.

Source of truth: upstream `cjpais/handy` commit **#1529 `31d8fc2` "introduce
transcribe.cpp"** (+ `#1541`). We adapt their code; we do not reinvent it.

---

## 1. Findings from reading their commit (the facts that shape this plan)

- **`transcribe-cpp` is a published crate** (crates.io `0.1.0`), a native
  ggml/GGUF ASR runtime. It loads ~19 architectures from GGUF (whisper,
  parakeet, **nemotron**, voxtral_realtime, moonshine_streaming, canary, cohere,
  qwen3_asr, granite, sensevoice, gigaam, funasr…).
- **Their engine is UNIFIED.** One `TranscriptionManager` owns one
  `LoadedEngine` enum; `TranscribeCpp(Session)` is a single variant holding one
  live `Session`. The **same** manager does batch (`session.run`) and streaming
  (`session.stream`); the model's `supports_streaming` capability picks the path.
  → *Answer to "unified vs diversified": unified. We move toward unified too.*
- **transcribe-cpp stabilizes internally.** `session.stream(&run, &StreamOptions::default())`
  (CommitPolicy::Auto); `stream.feed(&pcm)` returns an update with **`committed`
  (cumulative prefix) + `tentative` (tail)** and `committed_changed`/`tentative_changed`
  flags; `stream.finalize()` completes it. → **Our SAPrefix stabilizer and
  `NativeAsrBackend`/`AsrRawEvent` layer are redundant for this path.**
- **Capabilities come from GGUF metadata** (`stt.capability.streaming|translate|lang_detect`,
  `general.languages`) via `gguf_meta.rs` + `model_capabilities.rs`. → Replaces
  our hardcoded `AsrCapabilities` / `AsrTuning` table.
- **Models are GGUF, fetched via `hf-hub`** (cjpais fork). Their `catalog.json`
  already lists **`nemotron-3.5-asr-streaming-0.6b-gguf` (streaming:true)** in
  Q4/Q5/Q6/Q8/F16/F32 — the same model we run on sherpa, but quantized (Q4_K_M ≪
  our 464 MB ONNX), plus Parakeet-unified streaming, Voxtral-Realtime, etc.
- **Their streaming overlay is a React webview** (`src/overlay/RecordingOverlay.tsx`).
  **We keep our native winit pill / Studio Window instead** — it only needs
  committed/tentative text, which we feed from the transcribe-cpp worker.
- **Native runtime shipping** (already familiar from sherpa): per-platform
  features — Win x64 `dynamic-backends,vulkan`; Win arm64 default (static);
  macOS `metal` (static); Linux `dynamic-backends,vulkan` (+ `$ORIGIN/../lib`
  rpath). `build.rs` stages the shared libs + dlopen'd ggml backend modules into
  the bundle.

## 2. Their streaming pipeline ≈ our sherpa pipeline (already converged)

| Grain (sherpa Native ASR) | Handy transcribe.cpp | Migration |
| --- | --- | --- |
| `NativeAsrInput` (atomic-gated sink) | `StreamRouter` (atomic-gated router) | keep ours (equivalent) |
| `FrameCmd::Frame/Flush/Stop` | `StreamCmd::Feed/Finalize/Cancel` | keep ours; map to transcribe-cpp |
| `SherpaOnnxBackend` + `AsrRawEvent` | `Session::stream().feed()` | **replace** |
| SAPrefix stabilizer | internal `CommitPolicy::Auto` | **drop** (engine does it) |
| `DaemonEvent::AsrCommit/AsrPartial` | `StreamTextEvent{committed,tentative}` | **simplify** to one `AsrStreamText{committed,tentative}` |
| winit pill / Studio Window | React `RecordingOverlay.tsx` | **keep ours** |
| `AsrModelManager` (sherpa .tar.bz2) | `ModelManager` + GGUF + hf-hub | **adopt GGUF path** |
| `AsrModelSpec`/`AsrTuning` (hardcoded) | GGUF metadata capabilities | **adopt metadata** |

## 3. Keep / Adopt / Drop

**KEEP (our differentiators — engine-agnostic):**
- `grain-core` headless event bus + the pill process (`grain-pill`) + Studio Window.
- The **three shortcuts** (`transcribe`, `transcribe_realtime`, `transcribe_native_asr`)
  and the Quick Panel trio — a UX layer on top of whatever engine.
- Provider rotation (STT/post-process), the Agent, the whole frontend shell.
- `NativeAsrInput` (frame sink) and the `NativeAsrManager` worker-orchestration shape.
- The pill-facing `DaemonEvent` protocol (extend, don't replace).

**ADOPT (from transcribe.cpp, adapted to our tree):**
- The `transcribe-cpp` crate + per-platform feature matrix + `build.rs` staging
  (`stage_transcribe_runtime_libs`, linux rpath) + installer bundling.
- Their **streaming worker logic** (`session.stream → feed → committed/tentative
  → finalize`) — ported into our `native_asr` worker.
- Their **GGUF model layer**: `catalog/` (scoped to streaming models for now),
  `gguf_meta.rs`, `model_capabilities.rs`, and `hf-hub` GGUF download.
- Eventually their **unified engine** shape (one manager, capability-driven).

**DROP (retire):**
- `grain-asr-sherpa` crate + the `native-asr-sherpa` feature + sherpa DLL wrangling.
- The SAPrefix stabilizer, `NativeAsrBackend`/`AsrSession`/`AsrRawEvent` trait
  layer, and `AsrModelSpec`/`AsrTuning` **for this path** (transcribe-cpp
  supplies commit policy + tuning + capabilities). Keep the pure crate around
  only if a second non-stabilizing engine is ever needed; otherwise delete.
- The sherpa `.tar.bz2` catalog entries (replace with GGUF).

## 4. The three-shortcut UX on a unified engine

Owner's constraint: users keep three shortcuts (Rolling / Real-time / ASR); they
are NOT forced to pick a mode in settings. So the shortcuts stay as a UX layer;
the engine underneath unifies over time.

- **This phase (live only):** `transcribe_native_asr` → transcribe-cpp streaming
  on a selected **streaming GGUF model**. `transcribe` (batch) and
  `transcribe_realtime` (rolling) stay on the current engine (transcribe-rs /
  grain-transcribe) untouched.
- **Later (unify phase — decide then):** two clean options to resolve, e.g.
  (a) one loaded transcribe-cpp model + shortcuts = interaction modes gated by
  capability (with graceful fallback), or (b) "two models selected" (a streaming
  one + a batch one) where the shortcut chooses which to load/unload. Not decided
  now; the live path must not preclude either.

## 5. Phased execution (live-first) — NOT started

### Phase 1 — Foundation: dependency + native build
1. Add `transcribe-cpp` to `src-tauri/Cargo.toml` with the exact per-platform
   feature matrix from upstream (Win x64 `dynamic-backends,vulkan`; Win arm64
   default; macOS `metal`; Linux `dynamic-backends,vulkan`).
2. Port the `build.rs` pieces: `stage_transcribe_runtime_libs()` + linux
   `$ORIGIN/../lib` rpath; ensure the ggml backend DLLs land next to the exe
   (Windows) / in the bundle (Linux) — same class of work we did for sherpa.
3. Add `hf-hub` (cjpais fork, `cancellable-downloads`).
4. Smoke test: load a small GGUF `Session` and run one buffer (mirror our sherpa
   smoke test) to prove the crate builds + links + runs on this machine.
   **Exit:** `cargo build` clean on Windows; a GGUF model loads + transcribes a
   test wav in a unit test.

### Phase 2 — GGUF model layer (scoped to streaming models)
1. Port `gguf_meta.rs` + `model_capabilities.rs` (capability probing from GGUF).
2. Add a small catalog of **streaming** GGUF models (start: `nemotron-3.5-asr-streaming-0.6b-gguf`
   Q4_K_M/Q8; parakeet-unified streaming) — reuse their `catalog.json` entries.
3. Download via `hf-hub` into the HF cache; surface download/verify/extract
   stages to our existing ASR model UI (we already wired those events).
4. Reconcile with our `AsrModelManager`: either repoint it at GGUF/hf-hub, or
   replace it with a thin adaptation of their `ModelManager` GGUF path. Keep the
   frontend model-library UI we built.
   **Exit:** the ASR model section lists + downloads + selects a GGUF streaming model.

### Phase 3 — Streaming path on our pill
1. New `DaemonEvent::AsrStreamText { session_id, committed, tentative }` (mirrors
   their `StreamTextEvent`); keep `AsrSessionFinal`. Retire `AsrCommit`/`AsrPartial`/
   `AsrSegmentFinal` for this path.
2. Pill: `AsrDisplay` becomes `{ committed, tentative }` set directly from the
   event (committed is the full prefix — no delta bug, no stabilizer). Studio
   Window renders committed crisp + tentative blurred, exactly as now.
3. Rewrite the `native_asr` worker: on start, load the selected GGUF `Session`,
   `session.stream(...)`; the worker loop maps our `FrameCmd::Frame → stream.feed`,
   emits `AsrStreamText` on `committed_changed || tentative_changed`; `Stop →
   stream.finalize()` → final text → paste + history (unchanged tail).
4. Wire the existing `transcribe_native_asr` shortcut/action to this worker
   (drop the sherpa `resolve_backend`). Keep lifecycle/mutual-exclusion + mic
   fan-out (`NativeAsrInput`) as-is.
   **Exit:** press Real-time shortcut → speak → live committed/tentative in the
   Studio Window → release → pasted.

### Phase 4 — Retire sherpa
1. Remove `grain-asr-sherpa` usage + the `native-asr-sherpa` feature; delete the
   crate (or archive it). Drop the sherpa onnxruntime DLL staging.
2. Delete the stabilizer/`NativeAsrBackend`/`AsrRawEvent`/`AsrTuning` code paths
   now unused (keep `grain-asr-core` only for the pill event types still in use,
   or fold those into `grain-core`).
   **Exit:** default build has no sherpa; app is lighter; one engine for live.

### Phase 5 — Verify + document
- Build all platforms' feature sets compile (at least Windows here); smoke +
  end-to-end; update AGENTS/CLAUDE notes; log to SQLite.

### Later (explicitly NOT this pass)
- Move **batch** (`transcribe`) and **rolling** (`transcribe_realtime`) onto the
  same transcribe-cpp `Session` (`session.run` / chunked run), then collapse to a
  single unified `TranscriptionManager` like upstream — resolving the two-model
  vs capability-gated shortcut question. Add rolling live-preview (owner idea).

## 6. Risks / open questions (resolve during execution)
- **transcribe-cpp API stability** (v0.1.0, 1 day old). Pin the exact version;
  expect churn. Confirm the exact `RunOptions`/`StreamOptions`/`Session` surface
  against docs.rs before Phase 3.
- **GGUF streaming quality/latency** of Nemotron GGUF vs our known-good sherpa
  ONNX — validate in Phase 2/3; keep sherpa branch until validated.
- **Native build on our Windows setup** (Vulkan shader-gen, MAX_PATH, the
  `dynamic-backends` DLLs) — same hazards we already tamed for sherpa/whisper.
- **onnxruntime coexistence:** transcribe-rs (ONNX batch) + transcribe-cpp still
  coexist during the live-first phase; upstream ships a baseline onnxruntime.dll
  via `stage_onnxruntime_dll()`. Ensure no repeat of the 1.17 vs 1.24 clash.
- **Upstream-mergeability:** to reduce future pain, port their files with minimal
  diff and isolate Grain-specific glue behind our own modules where possible.

---

## 7. Owner refinements (2026-07-02) — fold into the phases above

- **Committed-only text.** Handy shows ONLY committed text (no tentative tail).
  Match it: the pill/Studio Window renders committed text (crisp, growing live)
  and **drops the blurred tentative tail** entirely. Adopt their `StreamPhase`
  states and map them onto pill states. (Reverses the earlier blur work.)
- **Shortcut → per-category model (separate selections kept).** Model selection
  stays split by category because the shortcuts are separate:
  - ASR shortcut → load the selected **ASR (streaming) model**, stream live.
  - Batch shortcut → load the selected **batch model**, record→run→paste (normal pill).
  - Rolling shortcut → load the **batch model**, but drive it through our rolling engine.
  One engine is *loaded* at a time; the shortcut decides which model + which mode.
- **Rolling is ours; Handy has none.** Unify onto the transcribe-cpp engine but
  keep the rolling driver **isolated** from ported upstream code (its own module,
  no edits inside their files) so future upstream syncs are clean manual merges,
  not git-merge conflicts. Only one engine loads.
- **Rolling live-preview (LATER differentiator):** we already do VAD/silence
  segmentation; on each commit-at-silence, surface that text as a toggleable live
  preview in rolling. Design for it now (don't build it yet).
- **Version risk accepted:** mirror Handy's setup exactly; they ship it at scale,
  so we inherit their reliability. Pin the version, port faithfully.

### Immediate implementation order (starting now)
1. **Phase 1 Foundation:** add `transcribe-cpp` (exact per-platform features) +
   `hf-hub`; port `build.rs` staging (`stage_transcribe_runtime_libs`) + bundle
   the `transcribe-libs` dir; get a clean `cargo build` + link on Windows.
2. Then Phase 2 (GGUF model layer) and Phase 3 (streaming worker → committed-only
   pill), retire sherpa (Phase 4), verify (Phase 5).
