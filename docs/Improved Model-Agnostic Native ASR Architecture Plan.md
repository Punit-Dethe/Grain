# Final Model-Agnostic Native ASR Implementation Plan for Grain

Date: 2026-06-29

## Purpose

This is the final implementation plan for adding model-agnostic Native ASR to Grain. It updates the prior plan using the verification/reflection file `ASR Integration Plan Verification.md`.

The plan is intentionally selective. It accepts improvements that reduce real risk in Grain's current architecture and rejects or defers proposals that would add large dependency, memory, packaging, or product scope before the core Native ASR path is proven.

## Final Consensus

Native ASR should be a third engine path beside the existing Batch and Rolling paths.

Grain should build:

- a small pure-Rust ASR protocol crate
- a model-agnostic transcript stabilizer
- a shared local-engine lifecycle manager
- a separate Native ASR model registry
- a Sherpa-ONNX adapter as the first real backend
- a Tauri integration layer that streams events to the existing `grain-core` event bus

Grain should not build the first implementation around:

- dual local ASR engines running concurrently
- a new WebSocket ASR server
- a full DSP stack
- neural denoising
- diarization
- WhisperX
- Indic-specific model pipelines
- punctuation restoration side models
- replacing the existing audio recorder

Those can be future capability work once the model-agnostic Native ASR foundation is reliable.

## Model-Agnosticism Reflection (2026-07-01, post-M8)

After the full build (M1–M8 + Sherpa), a dedicated pass looked for places where
the implementation or the plan bakes in a single-model assumption. The audio
pipeline was the prompt; the audit went wider. Findings and decisions:

### Real bug (fixed)

1. **Native ASR was invisible to the lifecycle arbiter.** The `NativeAsrEngine`
   adapter was a stub (`is_loaded`/`has_active_session` hardcoded false). Once a
   real Sherpa model is resident, the arbiter couldn't see it, so Batch/Rolling
   could load alongside it — two heavyweight models resident, breaking the ≤1-RAM
   guarantee, and Batch could load mid-Native-session. FIX: the adapter now
   delegates to `NativeAsrManager::is_running()` (the model is resident only for
   the duration of a session in the current load-per-session design), so
   mutual-exclusion and active-session protection cover Native ASR correctly.

### Contracts settled (documented; non-breaking to implement later)

2. **Capability-driven behavior must be explicit.** Capabilities are advertised
   but the consuming logic must honor them:
   - `endpointing == false`: the BACKEND will not segment, so the HOST must drive
     segment endpoints from its VAD. The mechanism is a future
     `AsrSession::endpoint()` (default impl = `flush`) invoked by the worker on a
     host VAD-silence signal. Sherpa (`endpointing == true`) self-segments; no
     host action. (Adding a default trait method later is non-breaking, so this
     stays a documented contract rather than speculative code.)
   - `immutable_final`: SAPrefix-always is the SAFE model-agnostic default and is
     kept for all backends. The capability is an OPTIMIZATION hint (a trusted
     backend could commit partials faster), not a correctness switch.
   - `word_timestamps == true`: the adapter must populate `AsrWord` timing on
     `Commit`/`SegmentFinal`. Sherpa advertises `false` today because mapping its
     subword tokens (+`timestamps`) to whole words is non-trivial and dictation
     does not need it.

3. **Backend-specific recognition tuning lives with the backend, not the agnostic
   core.** Sherpa knobs (decoding method, endpoint trailing-silence rules
   `rule1/2/3`, execution provider) are model/use-case specific. They belong in
   the backend-shaped `AsrModelFiles::SherpaTransducer` variant (or the registry
   entry), NOT as fields on the agnostic `AsrModelSpec`/`AsrCapabilities`.
   Defaults are sane (greedy, endpoint on, CPU EP); per-model overrides are
   additive and do not change the agnostic surface.

4. **One fixed host delivery format; backends adapt.** The host captures and
   resamples to exactly `AudioFormat::HOST_DEFAULT` (16 kHz mono f32 — the
   recorder's `WHISPER_SAMPLE_RATE`) and stamps every `AudioFrame` with it. Each
   backend adapts that to its model (Sherpa resamples from the frame's true rate
   to the model's rate internally). The two 16 kHz constants
   (`AudioFormat::HOST_DEFAULT` and `WHISPER_SAMPLE_RATE`) are a coupling that
   must stay in sync. We deliberately do NOT reconfigure the host resampler
   per-model — that would destabilize a pipeline shared with Batch/Rolling.

5. **`ContextHints` (custom-words / preceding-text) are declared but unwired.**
   Sherpa supports hotword biasing (`hotwords_buf` + `hotwords_score`); wire
   `settings.custom_words` → session hints → Sherpa hotwords when the
   load/session design is revisited (hotwords are a recognizer-config concern, so
   this pairs with deciding warm-keep vs load-per-session).

### Rejected (not real problems)

6. **Per-model sample FORMAT (f32 vs i16).** f32 is the correct universal
   interchange. No per-model format negotiation; backends convert internally if a
   future model ever needs i16.

7. **Per-model host VAD thresholds.** For streaming Native ASR the host VAD does
   not gate the stream (the recorder's sample-callback fires for every frame,
   un-gated); the model's own endpointer does segmentation. "VAD thresholds" for
   Native ASR are endpoint rules — see (3).

8. **Warm-keep vs load-per-session.** Current design loads the model at session
   start and unloads at session end (low-RAM, but adds start latency for a real
   model). Documented as a future UX optimization; not a correctness issue, and
   it is exactly what makes (1)'s `is_loaded == is_running` accurate.

### Verdict on the proposed `AudioProfile`

The proposal correctly identifies the gap but conflates host-pipeline concerns
(rate/format/VAD/pre-roll) with backend-recognition concerns. The production-grade
decomposition is (a) ONE fixed host delivery format, (b) backend adapts to its
model, (c) backend-specific tuning lives with the model/backend, (d) host
VAD/pre-roll is largely model-independent for streaming. So: accept the principle,
implement the honest version (done: `AudioFormat` + backend-adapts + the
`AsrModelFiles` variant as the home for backend tuning), reject the monolithic
per-model host `AudioProfile`.

## Architecture Refinements (2026-06-30, post-Milestone-1 review)

After Milestone 1 (`grain-asr-core`) was implemented, an independent top-tier
architecture pass was run against this plan. The plan held up well; the
following refinements were folded in. They are deltas, not redirections.

1. Zero-copy audio frames. `AudioFrame.samples` is `Arc<[f32]>`, not `Vec<f32>`,
   so the frame fan-out hands ONE buffer to Rolling and Native ASR with no PCM
   copy. (Implemented in `grain-asr-core`.)

2. The stabilizer is a policy, not the session's text store. It retains only the
   current segment's committed words plus finalized segment strings; the host is
   the source of truth and accumulates final text from the `Commit`/`SegmentFinal`
   stream. This keeps multi-hour sessions bounded.

3. Capability-branched stabilization. A true streaming transducer with
   `immutable_final` trusts backend finals verbatim; a partial-only or
   pseudo-streaming backend re-runs SAPrefix over the final. The stabilizer
   branches on the RESOLVED capabilities from `load()`, never the static spec.

4. Word timing only where trustworthy. Per-revision `Partial.words` is per-frame
   allocation churn against the low-RAM goal; authoritative timed words ride on
   `Commit`/`SegmentFinal` only.

5. Drop policy must never corrupt silently. The bounded Native ASR channel drops
   under overflow WITHOUT blocking the capture thread, and sustained overflow
   surfaces a recoverable "degraded" signal to the UI — dropped mid-utterance
   audio otherwise corrupts the transcript invisibly.

6. Lifecycle = admission control, not just mutual exclusion. Before a load the
   manager checks projected resident memory against a configured ceiling and
   refuses/evicts to stay under it, in addition to the heavyweight-exclusion rule.

7. Crash-safe finalization. A non-recoverable backend error still emits
   `SessionFinal` with the committed-so-far text; the user never loses dictation
   to a crash.

8. De-risk model-agnosticism early and cheaply. Before the protocol freezes, add
   shape-only mapping tests that fold a cloud-realtime event sequence
   (Deepgram/AssemblyAI interim/final/endpoint) onto `AsrRawEvent`/`AsrEvent` —
   no network — so the abstraction is proven against a non-Sherpa backend shape.

9. The lifecycle arbiter is pure and Tauri-free. Mutual-exclusion/TTL/admission
   logic lives in `crates/engine-lifecycle` over the `ManagedEngine` trait;
   `src-tauri/src/engine_lifecycle.rs` shrinks to thin Batch/Rolling/Native
   adapters. Mirrors `rolling-window`/`grain-core`/`grain-asr-core`.

## Current Grain Baseline

Current Batch path:

- `src-tauri/src/managers/transcription.rs`
- uses `transcribe-rs` engines
- returns final text after recording stops
- owns existing local model load/unload behavior

Current Rolling path:

- `src-tauri/src/rolling.rs`
- uses the same selected local model as Batch
- receives live 16 kHz frames from `AudioRecorder`
- chunks audio and assembles final text with `rolling-window`
- does not expose stable live partial text today

Current audio path:

- `src-tauri/src/managers/audio.rs`
- `src-tauri/src/audio_toolkit/audio/recorder.rs`
- already captures with `cpal`
- already downmixes and resamples to 16 kHz mono
- already supports high-pass conditioning and boost-only AGC
- already uses smoothed Silero VAD
- already emits levels and supports a live sample callback

Current event path:

- `crates/grain-core/src/event.rs`
- `src-tauri/src/bridge.rs`
- already broadcasts typed `DaemonEvent`s to the native pill and local event transport

Current cloud STT path:

- `src-tauri/src/stt_router.rs`
- `src-tauri/src/stt_client.rs`
- batch upload and provider rotation only
- not realtime streaming ASR

## Reflection Assessment

### Accepted Now

These improvements should change the implementation plan immediately.

1. Blocking inference must be isolated from async/runtime/audio threads.
   - Adopt the `rust-asr-server` style of separating I/O from inference.
   - In Grain, this means a bounded Native ASR worker, not a server.

2. Add pre-roll ring buffering for speech onset.
   - Keep 300-500 ms of 16 kHz mono frames.
   - When a Native ASR session opens or speech gate activates, prepend the look-back.
   - This prevents clipped first phonemes.

3. Add explicit Sherpa-ONNX packaging and Windows validation.
   - Use the official `sherpa-onnx` Rust crate.
   - Verify Windows linking/runtime behavior during the Sherpa milestone.
   - Do not depend on the deprecated `sherpa-rs` crate.

4. Keep Rust as the orchestration layer.
   - No Python runtime in the production desktop path.
   - Future Python/JAX/server-only model experiments must remain outside the desktop critical path.

5. Treat audio processing as a replaceable front-end policy.
   - Grain should not hardwire today's VAD/conditioning forever.
   - Native ASR should receive frames through an explicit audio input policy that can later add Sonora, DPDFNet, or backend-native enhancement.

### Partially Accepted

These are useful ideas, but must be scoped down.

1. Sonora/WebRTC DSP.
   - Accept as a future optional audio-front-end module.
   - Do not add to MVP.
   - Grain already has high-pass conditioning, boost-only AGC, mute-while-recording, and feedback sounds. AEC matters most if Grain later speaks while listening or plays long audio while the mic is open.

2. Sherpa DPDFNet speech enhancement.
   - Accept as future noisy-environment enhancement.
   - Do not put denoising before first Native ASR.
   - It adds another model, more CPU, more memory, more settings, and more failure modes.

3. Dolphin CTC / SenseVoice / multilingual Sherpa models.
   - Accept as model candidates after the first Sherpa streaming model works.
   - Do not make the first milestone a model zoo.
   - Native ASR v1 should prove one known-good online streaming model end to end.

4. VAD gating.
   - Accept VAD for no-speech suppression, endpoint hints, and resource protection.
   - Do not starve a true streaming recognizer of cadence unless the backend explicitly supports sparse ingestion.

5. Punctuation restoration.
   - Accept as backend capability or later post-processing option.
   - Do not add a separate punctuation model in MVP.

6. SimulStreaming / LocalAgreement.
   - Accept as a research reference for partial-vs-committed transcript policy.
   - Do not adopt it as a dependency or as the default algorithm.
   - Grain should implement its own pure-Rust stabilizer using SAPrefix, which is a softer LocalAgreement-style policy that tolerates punctuation, casing, and small spelling drift before committing text.

### Deferred Or Rejected For MVP

These proposals are logged but should not shape the first implementation.

1. `sherpa-rs`.
   - Rejected for the plan.
   - It is deprecated in favor of the official `sherpa-onnx` Rust API.

2. Custom WebSocket ASR networking.
   - Rejected for local Native ASR.
   - Grain already has local event transport for the pill. Native ASR runs in-process.
   - `rust-asr-server` remains a reference for worker isolation and Sherpa config.

3. WhisperX.
   - Deferred.
   - Useful for subtitle-grade offline alignment, not first-pass desktop dictation.
   - Also brings Python-like ecosystem complexity if adopted directly.

4. Speaker diarization.
   - Deferred.
   - Grain's main workflow is single-speaker dictation and paste.
   - Diarization is a meeting/interview feature, not Native ASR foundation.

5. IndicWhisper JAX / IndicConformer pipelines.
   - Deferred.
   - Important for future localized model packs, especially Indian languages.
   - Not appropriate for the first Rust desktop streaming path unless an ONNX/Sherpa-compatible package exists.

6. Dual local engine concurrency.
   - Deferred.
   - It conflicts with Grain's low-RAM and "destroy if not in use" constraints.
   - Can become an explicit high-resource mode later.

7. Full acoustic front-end replacement.
   - Rejected for MVP.
   - Existing Grain audio code is already functional and integrated.
   - Native ASR should extend it with fan-out and pre-roll, not rewrite it.

## Architecture

Native ASR flow:

```text
AudioRecorder 16 kHz mono frames
  -> frame fan-out
  -> NativeAsrManager bounded input channel
  -> Native ASR worker thread
  -> NativeAsrBackend / AsrSession
  -> AsrRawEvent
  -> TranscriptStabilizer
  -> AsrEvent
  -> DaemonEvent
  -> pill partial/commit/final UI
  -> finalize/history/paste on SessionFinal
```

Three engine slots:

```rust
pub enum EngineSlot {
    Batch,
    Rolling,
    NativeAsr,
}
```

Native ASR must not be implemented by mutating Rolling. Rolling remains a chunked batch-model live path. Native ASR is for true streaming sessions that own incremental recognizer state.

## Crates And Modules

### `crates/grain-asr-core`

Pure Rust. No Tauri. No Sherpa. No network.

Owns:

- `AudioFrame`
- `AsrRawEvent`
- `AsrEvent`
- `AsrSession`
- `NativeAsrBackend`
- `AsrModelSpec`
- `AsrCapabilities`
- `AsrSessionConfig`
- `ContextHints`
- `TranscriptStabilizer`

Core traits:

```rust
pub trait AsrSession: Send {
    fn push_audio(&mut self, frame: AudioFrame) -> anyhow::Result<Vec<AsrRawEvent>>;
    fn flush(&mut self) -> anyhow::Result<Vec<AsrRawEvent>>;
    fn finish(&mut self) -> anyhow::Result<Vec<AsrRawEvent>>;
}

pub trait NativeAsrBackend: Send {
    fn backend_id(&self) -> &'static str;
    fn static_capabilities(&self) -> AsrCapabilities;
    fn load(&mut self, model: &AsrModelSpec) -> anyhow::Result<AsrCapabilities>;
    fn unload(&mut self);
    fn start_session(&mut self, config: AsrSessionConfig) -> anyhow::Result<Box<dyn AsrSession>>;
}
```

`load()` returns resolved runtime capabilities. Static model metadata is not enough because actual support can vary by backend, model package, or execution provider.

### `crates/grain-asr-sherpa`

Sherpa adapter using the official `sherpa-onnx` Rust crate.

Owns:

- `SherpaOnnxBackend`
- `SherpaOnnxSession`
- model file validation
- Sherpa config mapping
- partial/final/endpoint mapping into `AsrRawEvent`
- fixture WAV smoke tests

Do not leak Sherpa types into `grain-asr-core`.

### `src-tauri/src/native_asr`

Tauri integration.

Owns:

- `NativeAsrManager`
- session worker lifecycle
- bounded audio input channel
- pre-roll ring buffer policy
- bridge from `AsrEvent` to `DaemonEvent`
- finalization/history/paste integration

### `crates/engine-lifecycle` + `src-tauri/src/engine_lifecycle.rs`

Shared lifecycle arbiter for local heavyweight engines. The PURE arbiter
(`EngineSlot`, `ManagedEngine` trait, mutual exclusion, TTL, admission control)
lives in the Tauri-free `crates/engine-lifecycle` so it is unit-testable without
the Tauri build. `src-tauri/src/engine_lifecycle.rs` is the thin wiring layer:
the Batch/Rolling/Native adapters that implement `ManagedEngine` over the real
managers, plus engine-state event emission.

Owns:

- mutual exclusion between Batch, Rolling, Native ASR
- admission control against a resident-memory ceiling
- active-session protection
- warm TTL using existing unload settings
- manual unload
- engine state events

## Event Protocol

Use two event layers.

Raw backend events:

```rust
pub enum AsrRawEvent {
    Partial {
        segment_id: u64,
        revision: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    BackendFinal {
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    Endpoint {
        segment_id: u64,
        reason: EndpointReason,
        audio_end_ms: Option<u64>,
    },
    Error {
        recoverable: bool,
        message: String,
    },
}
```

UI-safe events:

```rust
pub enum AsrEvent {
    Partial {
        session_id: u64,
        segment_id: u64,
        revision: u64,
        text: String,
        stability: Stability,
    },
    Commit {
        session_id: u64,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    SegmentFinal {
        session_id: u64,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    SessionFinal {
        session_id: u64,
        text: String,
    },
    Error {
        session_id: u64,
        recoverable: bool,
        message: String,
    },
}
```

Do not include default post-commit correction. Committed text must mean stable text. Add correction later only behind a capability flag and an explicit user-facing mode.

## Transcript Stabilizer

The stabilizer is a policy layer, not a backend.

Responsibilities:

- separate committed text from volatile partial text
- keep committed text immutable
- trust backend final when capability says final is immutable
- use SAPrefix for partial-only or unstable backends
- commit only at word boundaries
- bound revision history
- be a policy, not the session text store: retain only the current segment's committed words plus finalized segment strings; the host accumulates whole-session text from the commit stream so long sessions stay bounded

SAPrefix MVP:

- whitespace tokenization first
- case and trailing punctuation normalization for comparisons
- normalized Levenshtein similarity per word
- stable prefix comparison across consecutive hypotheses
- configurable thresholds
- tests for punctuation, case, substitutions, repeated words, and endpoint behavior

Use `strsim` if it keeps the implementation simple. Otherwise implement the small needed similarity function in `grain-asr-core`.

## Audio Input Policy

Do not replace the current audio recorder in MVP.

Build on:

- `AudioRecorder`
- `FrameResampler`
- current high-pass conditioning
- current boost-only AGC
- current smoothed Silero VAD
- current live frame callback

Add:

- frame fan-out (zero-copy `Arc<[f32]>` frames) that can feed Rolling and Native ASR without either path blocking audio capture
- pre-roll ring buffer for Native ASR
- explicit backpressure/drop policy for Native ASR channel overflow that drops WITHOUT blocking the capture thread, and surfaces sustained overflow as a recoverable "degraded" signal rather than silently corrupting the transcript
- metrics/logs for dropped frames, queue depth, endpoint decisions, and commit latency

Important rule:

The audio callback may enqueue frames and update tiny atomics only. It must not perform model load, inference, Tauri emit, JSON serialization, history writes, or blocking locks.

Future audio-front-end extension points:

- Sonora AEC/NS/AGC for feedback or assistant voice scenarios
- Sherpa DPDFNet for noisy environments
- backend-native VAD when it clearly improves a specific backend

These are not MVP dependencies.

## Stabilization References

SimulStreaming is the research family behind LocalAgreement-style streaming policies for models that are not truly streaming, especially Whisper-like models. It repeatedly decodes overlapping audio, compares consecutive hypotheses, commits only the stable prefix, and leaves the unstable tail as a volatile partial.

Grain should use this as conceptual grounding, not as a direct dependency. The Native ASR stabilizer should implement SAPrefix in `grain-asr-core`: the same partial/commit separation, but with similarity-aware prefix matching instead of strict exact agreement.

## Native ASR Model Registry

Create a separate Native ASR registry. Do not overload `selected_model`.

New settings:

- `selected_asr_model`
- `native_asr_enabled` or a route/mode selector once UX is decided

Model spec:

```rust
pub struct AsrModelSpec {
    pub id: String,
    pub name: String,
    pub backend: AsrBackendKind,
    pub files: AsrModelFiles,
    pub sample_rate_hz: u32,
    pub languages: Vec<String>,
    pub capabilities: AsrCapabilities,
    pub memory: MemoryProfile,
}
```

Sherpa streaming transducer layout:

```rust
pub enum AsrModelFiles {
    SherpaTransducer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
        tokens: PathBuf,
        config: Option<PathBuf>,
    },
}
```

Reuse existing download/extract/verify mechanics from `ModelManager` where possible. The registry is separate because Native ASR model topology, endpointing, runtime state, and capabilities differ from Batch/Rolling models.

## Lifecycle Policy

Current mutual exclusion is pairwise. Replace it with one shared policy.

```rust
pub trait ManagedEngine: Send + Sync {
    fn slot(&self) -> EngineSlot;
    fn is_loaded(&self) -> bool;
    fn has_active_session(&self) -> bool;
    fn touch(&self);
    fn unload(&self) -> anyhow::Result<()>;
    fn memory_class(&self) -> EngineMemoryClass;
}
```

Rules:

- active sessions cannot be unloaded
- local heavyweight engines are mutually exclusive by default
- before a load, the manager performs admission control: it checks projected resident memory against a configured ceiling and refuses/evicts to stay under it (not just pairwise exclusion)
- Batch, Rolling, and Native ASR register with the lifecycle manager
- completed local sessions keep a warm TTL using the existing `model_unload_timeout`
- manual unload unloads all inactive local engines
- cloud batch STT remains separate
- cloud realtime adapters, if added later, do not define local lifecycle policy
- the arbiter is pure/Tauri-free (`crates/engine-lifecycle`); only the Batch/Rolling/Native adapters live in `src-tauri`

## Backend Choice

First backend:

- official `sherpa-onnx` Rust crate
- one known-good streaming Sherpa model
- streaming transducer preferred for MVP if packaging is manageable

Candidate follow-ups:

- Dolphin CTC if the online API and partial behavior are good
- SenseVoice for fast multilingual/batch-like use cases if it fits the event protocol
- Moonshine Streaming only if a true incremental Rust-callable session is available
- cloud realtime adapters only after local Native ASR is stable

Avoid first:

- model zoo expansion
- Python/JAX model pipelines
- WhisperX
- separate punctuation models

## Reference And Adoption Map

Keep this section even if implementation details change. Its purpose is to prevent future sessions from rebuilding complex ASR pieces from scratch when there are already useful references.

### Current Grain / Handy-Derived Code

Use as the implementation baseline, not as an external dependency.

- `src-tauri/src/managers/audio.rs`: app-integrated audio lifecycle, VAD preload, mic mode, lazy close, live frame callback.
- `src-tauri/src/audio_toolkit/audio/recorder.rs`: `cpal` capture, downmixing, resampling, high-pass conditioning, VAD filtering, sample callback.
- `src-tauri/src/managers/model.rs`: download, SHA validation, archive extraction, cancellation, directory model handling.
- `src-tauri/src/managers/transcription.rs`: current Batch engine lifecycle and panic-safe model handling.
- `src-tauri/src/rolling.rs`: Rolling model lifecycle, frame ingestion, chunk worker, mutual exclusion with Batch.
- `crates/rolling-window`: chunk cursor, timeline assembler, seam dedup, bounded rolling-session tests.
- `crates/grain-transcribe`: existing `transcribe-rs` integration layer and ASR trait for batch-style models.
- `crates/grain-core`: typed daemon event bus, settings schema, headless core direction.

Why this matters:

- The app shell, shortcut flow, tray behavior, audio recorder, model management, history, paste, and event bus already exist.
- Do not re-import Handy. Grain already carries the useful Handy structure plus Grain-specific rolling/event work.

### Official `sherpa-onnx`

Primary adopted Native ASR backend library.

Look at:

- official Rust crate docs: `https://docs.rs/sherpa-onnx`
- project docs: `https://k2-fsa.github.io/sherpa/onnx/index.html`
- Rust examples in the `k2-fsa/sherpa-onnx` repo
- online recognizer / online stream examples
- endpointing examples
- speech enhancement examples only for future DPDFNet work
- diarization examples only for future multi-speaker work

Borrow:

- `OnlineRecognizer`
- `OnlineStream`
- safe RAII wrapper patterns from the official crate
- config structs for transducer/CTC/SenseVoice-style models
- model file layout expectations
- endpointing behavior

Do not borrow into MVP:

- TTS, speaker diarization, keyword spotting, audio tagging, speech enhancement, or broad audio intelligence features.

Important:

- Use official `sherpa-onnx`, not deprecated `sherpa-rs`.
- Validate Windows packaging/linking in the Sherpa milestone.

### `rust-asr-server` (`aivo0/rust-asr-server`)

Reference for worker isolation and Sherpa session orchestration.

Look at:

- worker/thread-pool separation
- bounded message/channel flow
- Sherpa config mapping
- conversion from synchronous recognizer calls to async-style transcript events
- WebSocket protocol only as an example of event shape, not as local architecture

Borrow:

- async I/O vs blocking inference separation
- compute worker boundaries
- backpressure and session message patterns
- configuration approach for Sherpa model families

Do not borrow:

- the server as a runtime dependency
- a local WebSocket ASR server for Grain's in-process dictation

### `yamabiko-whisper`

Reference for streaming UX and LocalAgreement-style state handling.

Look at:

- partial vs committed text separation
- LocalAgreement-2 flow
- low-latency UI hypothesis handling
- test cases around unstable hypotheses
- audio pipeline only as comparison; Grain already has an audio pipeline

Borrow:

- state-machine ideas
- tests and edge cases for hypothesis drift
- terminology around tentative/committed words

Do not borrow:

- Whisper-specific inference stack
- its full audio pipeline
- exact LocalAgreement as Grain's default stabilizer

### SimulStreaming / WhisperStreaming / LocalAgreement

Research reference for transcript commitment policy.

Look at:

- how overlapping hypotheses are compared
- how confirmed/committed words are separated from unconfirmed partial text
- LocalAgreement failure cases around punctuation/casing/tokenization drift

Borrow:

- the concept of stable prefix commitment
- the UI contract: committed text is stable, partial text is volatile

Do not borrow:

- strict exact-agreement as the final policy

Grain's implementation:

- implement SAPrefix in `grain-asr-core`, inspired by LocalAgreement but similarity-aware.

### WhisperPipe / SAPrefix

Primary research reference for Grain's stabilizer.

Look at:

- similarity-aware prefix commitment
- normalized Levenshtein similarity across consecutive hypotheses
- memory-bounded streaming policy
- latency/flicker tradeoffs

Borrow:

- SAPrefix algorithm shape
- threshold-driven stable-prefix commitment
- punctuation/case tolerant comparison

Do not borrow blindly:

- any model-specific buffer-trimming rule that assumes Whisper-style stateless re-decode.
- true streaming Sherpa sessions should use backend endpoint/reset semantics first.

### Silero VAD

Already used in Grain. Keep as the MVP VAD foundation.

Look at:

- `src-tauri/src/audio_toolkit/vad/silero.rs`
- current smoothed VAD wrapper
- official Silero VAD behavior only if tuning thresholds

Borrow/keep:

- no-speech suppression
- endpoint hints
- hallucination prevention

Add:

- pre-roll ring buffer so VAD does not clip speech onset.

Do not:

- replace it globally with Sherpa VAD in MVP.

### Sonora / WebRTC Audio Processing

Future optional audio-front-end reference.

Look at:

- Sonora pure-Rust WebRTC audio processing
- AEC, noise suppression, AGC

Borrow later:

- echo cancellation if Grain starts speaking while listening
- stronger noise suppression if current conditioning is insufficient

Do not add to MVP:

- full DSP chain before Native ASR works.

### DeepFilterNet / Sherpa DPDFNet

Future optional neural denoising references.

Look at:

- DeepFilterNet for neural speech enhancement concepts
- Sherpa-ONNX DPDFNet examples for streaming speech enhancement

Borrow later:

- optional noisy-room enhancement mode
- backend-specific speech enhancement adapter

Do not add to MVP:

- another model in the critical path before the recognizer path is stable.

### Whisper.cpp / `transcribe-rs`

Existing Batch/Rolling model ecosystem reference.

Look at:

- current `transcribe-rs` integration in `TranscriptionManager`
- current `grain-transcribe` wrapper
- whisper.cpp acceleration only for Batch/Rolling, not Native ASR MVP

Borrow:

- model lifecycle lessons
- accelerator settings integration lessons
- finalization path reuse

Do not:

- make Native ASR another Whisper pseudo-streaming wrapper first.

### Moonshine Streaming

Future protocol-validation backend.

Look at:

- whether a true incremental Rust-callable API exists
- how streaming state, sliding windows, and decoder cache are represented

Borrow later:

- second backend to prove the event protocol is not overfit to Sherpa transducers

Do not:

- route finite-buffer `transcribe(samples)` through Native ASR and call it streaming.
- keep Moonshine in Batch/Rolling until true streaming state is accessible.

### RealtimeSTT / Dual-Lane ASR

Reference only for the dual-engine idea.

Look at:

- preview/final lane tradeoffs
- UX patterns for fast preview plus accurate final

Do not add to MVP:

- dual local model concurrency. It conflicts with Grain's low-RAM/default-lightweight policy.

### WhisperX

Future offline/subtitle-grade alignment reference.

Look at:

- VAD segmentation
- forced alignment
- speaker diarization pipeline
- timestamp correction

Borrow later:

- offline archive/subtitle workflows if Grain ever adds them

Do not add to MVP:

- Python-heavy forced alignment/diarization path.

### Cloud Realtime Providers

Future protocol-hardening references.

Look at:

- OpenAI realtime transcription events
- Deepgram interim/final/endpoint events
- AssemblyAI Universal Streaming message sequence

Borrow later:

- mapping tests for provider event semantics
- stress tests for `AsrRawEvent` / `AsrEvent`

Do not add to MVP:

- cloud realtime adapters before local Native ASR is stable.

Current cloud batch STT remains in:

- `src-tauri/src/stt_router.rs`
- `src-tauri/src/stt_client.rs`

### AI4Bharat / Indic ASR

Future localization reference.

Look at:

- IndicWhisper for batch/localized accuracy research
- IndicConformerASR for streaming-oriented Indian language work
- whether ONNX/Sherpa-compatible packages exist

Borrow later:

- specialized model registry entries for Indian languages
- capability metadata for language coverage

Do not add to MVP:

- Python/JAX server-side pipelines in the desktop app.

### ONNX Runtime Optimizations

Future optimization reference.

Look at:

- `.onnx` vs `.ort` packaging
- QDQ/int8 quantization
- execution provider configuration
- memory arena behavior

Borrow opportunistically:

- model package metadata fields
- runtime hints
- quantized model preferences

Do not require for MVP:

- `.ort` flatbuffers or memory mapping as mandatory model format.

## Implementation Roadmap

### Milestone 0: Finalize Guardrails

Deliver:

- keep this plan as the implementation reference
- confirm Native ASR remains separate from Batch and Rolling
- confirm official `sherpa-onnx`, not `sherpa-rs`
- confirm no full DSP/denoising/diarization/model-zoo work in MVP

Exit:

- implementation scope is narrow enough to build safely

### Milestone 1: `grain-asr-core`

Deliver:

- new crate under `crates/grain-asr-core`
- core events, traits, capabilities, model spec, context hints
- SAPrefix stabilizer
- fake backend/session test harness

Exit:

- pure Rust tests pass without Tauri, model files, microphone, or network

### Milestone 2: Engine Lifecycle Manager

Deliver:

- `crates/engine-lifecycle` (pure): `EngineSlot`, `EngineMemoryClass`, `ManagedEngine` trait, `LifecycleManager` (mutual exclusion + TTL + admission control), fake-engine test harness
- `src-tauri/src/engine_lifecycle.rs` (wiring): Batch adapter, Rolling adapter, Native ASR placeholder adapter
- shared unload/touch/TTL policy

Exit:

- pure `LifecycleManager` tests pass without the Tauri build
- Batch and Rolling behavior remains intact
- loading any local engine unloads inactive incompatible engines
- active sessions cannot be unloaded

### Milestone 3: Audio Fan-Out And Native Input Policy

Deliver:

- extend `AudioRecorder` callback path so Native ASR can receive frames
- bounded Native ASR input channel
- 300-500 ms pre-roll ring buffer
- overflow policy and logs
- no blocking in audio callback

Exit:

- a fake Native ASR session can receive live frames safely
- Rolling still receives frames as before
- Batch recording still returns final samples as before

### Milestone 4: ASR Model Registry

Deliver:

- `AsrModelManager`
- separate ASR model metadata
- `selected_asr_model` setting
- commands for list/download/delete/select ASR model
- validation for Sherpa multi-file bundles

Exit:

- ASR model lifecycle works without changing `selected_model`
- current model UI and Batch/Rolling model selection remain intact

### Milestone 5: Sherpa-ONNX Adapter

Deliver:

- `crates/grain-asr-sherpa`
- official `sherpa-onnx` dependency
- model config mapping
- adapter from Sherpa recognizer/session output to `AsrRawEvent`
- fixture WAV smoke test
- Windows build/link validation note or test script

Exit:

- fixture audio produces partial/final or final/endpoint events
- model unload drops Sherpa resources cleanly
- Windows build risk is explicitly tested or documented

### Milestone 6: Native ASR Manager And Events

Deliver:

- `src-tauri/src/native_asr/mod.rs`
- worker thread/session control
- event bridge to `DaemonEvent`
- new `DaemonEvent` variants for Native ASR partial, commit, segment final, session final, and error
- cancellation and stop handling

Exit:

- fake backend can drive pill-facing partial/commit/final event replay
- Sherpa backend can run through the same manager

### Milestone 7: End-To-End Native Dictation

Deliver:

- Native ASR shortcut/action or controlled experimental route
- live mic to Native ASR session
- pill partial and committed text display
- final text through `finalize_transcript`
- optional post-processing using existing post-process path
- history save
- paste

Exit:

- user can dictate through Native ASR end to end
- Batch still works
- Rolling still works
- cloud batch STT rotation still works
- only one heavyweight local engine is resident by default

### Milestone 8: Hardening

Deliver:

- long-session bounded-memory test
- no-speech test
- clipped-onset/pre-roll test
- stop-while-decoding test
- unload-during-active-session test
- queue overflow test
- commit latency and dropped-frame logs

Exit:

- committed text does not flicker
- partial text remains responsive
- long sessions do not grow unbounded memory
- cancellation and unload are predictable

## Likely File Changes

New files:

- `crates/grain-asr-core/Cargo.toml`
- `crates/grain-asr-core/src/lib.rs`
- `crates/grain-asr-core/src/events.rs`
- `crates/grain-asr-core/src/stabilizer.rs`
- `crates/grain-asr-core/src/model.rs`
- `crates/grain-asr-core/src/session.rs`
- `crates/grain-asr-sherpa/Cargo.toml`
- `crates/grain-asr-sherpa/src/lib.rs`
- `src-tauri/src/native_asr/mod.rs`
- `src-tauri/src/commands/native_asr.rs`
- `src-tauri/src/engine_lifecycle.rs`

Modified files:

- `Cargo.toml`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`
- `src-tauri/src/actions.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/managers/audio.rs`
- `src-tauri/src/audio_toolkit/audio/recorder.rs`
- `src-tauri/src/managers/transcription.rs`
- `src-tauri/src/rolling.rs`
- `crates/grain-core/src/event.rs`
- `crates/grain-core/src/settings.rs`
- `src-tauri/src/settings.rs`
- `src/bindings.ts` through Specta generation only

Frontend UI should wait until backend replay and fake-backend events work. Avoid tying the first core implementation to settings UI churn.

## Verification Plan

Core:

- `cargo test -p grain-asr-core`
- SAPrefix tests for punctuation/case drift, unstable substitutions, repeated words, endpoint with no final, backend final after partials

Lifecycle:

- test Batch/Rolling/Native placeholder mutual exclusion
- test unload denied during active session
- test idle unload after TTL

Audio:

- unit or harness test for pre-roll ring buffer
- fake Native ASR receiver confirms frame ordering and no callback blocking
- overflow policy test

Sherpa:

- fixture WAV smoke test
- invalid bundle validation test
- unload/reload smoke test
- Windows build/link validation before declaring Sherpa milestone complete

End to end:

- Native ASR dictation produces partial, commit, final, history entry, and paste
- Batch dictation still works
- Rolling dictation still works
- cloud batch STT still works

## Sources Checked

Local:

- `src-tauri/src/managers/transcription.rs`
- `src-tauri/src/rolling.rs`
- `src-tauri/src/managers/audio.rs`
- `src-tauri/src/audio_toolkit/audio/recorder.rs`
- `src-tauri/src/stt_router.rs`
- `src-tauri/src/stt_client.rs`
- `src-tauri/src/managers/model.rs`
- `crates/grain-core/src/event.rs`
- `crates/grain-core/src/settings.rs`
- `crates/rolling-window/src/assembler.rs`

Reflection:

- `C:/Users/watrm/Downloads/ASR Integration Plan Verification.md`

External:

- official `sherpa-onnx` Rust docs: https://docs.rs/sherpa-onnx
- `sherpa-onnx` project docs: https://k2-fsa.github.io/sherpa/onnx/index.html
- `sherpa-rs` deprecation notice: https://github.com/thewh1teagle/sherpa-rs
- Sonora: https://github.com/dignifiedquire/sonora
- Sonora crate: https://crates.io/crates/sonora
- DPDFNet in Sherpa-ONNX examples/docs: https://github.com/k2-fsa/sherpa-onnx/blob/master/c-api-examples/speech-enhancement-dpdfnet-c-api.c
- `rust-asr-server`: https://github.com/aivo0/rust-asr-server
- `yamabiko-whisper`: https://crates.io/crates/yamabiko-whisper

---

## Per-Model Runtime Profile (2026-07-02, `AsrTuning`)

Every catalog model now declares an `AsrTuning` profile (`grain-asr-core::model`)
— the standardized, model-agnostic way to tune a model. Missing/unspecified →
`AsrTuning::default()` (single thread, greedy, sherpa stock endpoint timings), so
no model is ever left un-tuned. The sherpa backend maps it onto
`OnlineRecognizerConfig` in `load()`.

Configurable now (all per-model, applied by the backend):
- `num_threads` — heavier encoders need more to stay real-time (a model that
  can't process each streaming chunk within its budget drops frames → worse
  accuracy). Nemotron 0.6B → 4, fast-conformer → 2, zipformer → 1.
- `decoding` — `Greedy` vs `ModifiedBeamSearch { num_active_paths }`. Beam search
  is cheap on these models (tiny decoder/joiner vs the encoder) so the accurate
  models use it (paths=4); the compact zipformer stays greedy.
- `endpoint_trailing_silence_secs` (sherpa `rule2`) + `endpoint_max_utterance_secs`
  (`rule3`). Snappier endpoint (Nemotron 0.8s) finalizes segments sooner, so
  committed text flows instead of landing in one big block at a long pause.
- Feature sample rate follows `AsrModelSpec::sample_rate_hz`; NeMo feature
  *normalization* is auto-detected by sherpa from onnx metadata (not a manual knob).

Known model-specific audio knobs NOT yet expressible in `AsrTuning` (they live on
the GLOBAL recorder / native input today; tracked to migrate into the per-model
profile so this stays fully model-agnostic):
- **Voice conditioning (85 Hz high-pass + boost-only AGC)** — a global recorder
  setting. NeMo/Nemotron models normalize features internally, so external AGC can
  be redundant or mildly harmful; these models likely prefer raw 16 kHz. Wants a
  per-model `prefers_raw_audio`/conditioning override.
- **Pre-roll duration** — the `NativeAsrInput` pre-roll ring is a fixed 400 ms set
  at construction. A higher-lookahead model (Nemotron 560 ms) may want more so the
  first words aren't clipped. Wants to be per-model.

When we make those per-model, extend `AsrTuning` (or a broader `ModelProfile`
wrapping it) and have the Native ASR session reconfigure the recorder/input at
session start from the active model's profile.
