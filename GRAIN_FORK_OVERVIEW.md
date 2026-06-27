# Grain — Fork Overview (Handy → Grain)

> **Audience: AI coding assistants.** This is the fast-path map of what Grain is,
> what it inherited from upstream **Handy**, and exactly what Grain added or
> changed on top. Read this before doing a deep tree walk — it will save you the
> full discovery pass. For identity, constraints, and operating rules, read
> [`AGENTS.md`](AGENTS.md) first; this file is the *what changed* companion to it.
>
> Convention: Grain-specific code is tagged with `[GRAIN]` comments and the
> custom Rust crates live under `crates/`. Anything not tagged and not under
> `crates/` is most likely inherited Handy code — treat it as upstream and
> prefer extending over modifying it (see the upstream-compatibility constraint).

---

## 1. What Grain is

A cross-platform desktop **speech-to-text** app (Tauri 2.x, Rust backend,
React/TypeScript frontend), forked from the open-source **Handy** project.

Core pipeline (inherited from Handy):

```
Keypress → Audio capture (cpal) → VAD filter (Silero) → Transcription → Clipboard/Paste
```

Grain keeps that pipeline but (a) decouples the frontend from the backend, (b)
replaces the React overlay with an always-on OS-native pill process, and (c) adds
cloud transcription + LLM post-processing + an in-app Agent, all behind a typed,
Tauri-free core.

---

## 2. What Handy provides (inherited baseline)

Upstream Handy is the foundation. The pieces Grain inherits and still relies on:

- **Manager pattern** — Audio, Model, Transcription (+ History) managers held in
  Tauri state. Found in `src-tauri/src/managers/`.
- **Local transcription** — `whisper-rs` / Parakeet and friends via
  `transcribe-rs`, running in-process. See `managers/transcription.rs`.
- **Audio toolkit** — device enumeration, recording, resampling, Silero VAD.
  See `src-tauri/src/audio_toolkit/`.
- **Global shortcut + paste** — `rdev`-based shortcut handling, clipboard paste
  into the foreground app. See `shortcut/`, `clipboard.rs`, `input.rs`.
- **Settings + history** — persisted app settings and transcription history.
- **Base React UI** — settings, model selector, onboarding, update checker.
  Grain keeps Handy's UI as the base and builds on it.
- **CLI / single-instance remote control** — `--toggle-transcription`,
  `--cancel`, etc., via `tauri_plugin_single_instance`. See `cli.rs`,
  `signal_handle.rs`.
- **Local LLM post-processing scaffolding** — Handy's original text
  post-processing path (Apple Intelligence / structured output) lives in
  `actions.rs`.

---

## 3. What Grain added / changed on top

### 3.1 Headless core — `crates/grain-core`

The real frontend/backend **decoupling** substrate. Tauri-free. Replaces the
four ways Handy reached through `tauri::AppHandle`:

| Handy (Tauri-coupled)      | Grain (`grain-core`)                          |
| -------------------------- | --------------------------------------------- |
| `app.emit("...")` (untyped)| `AppContext::emit` → typed `DaemonEvent` bus  |
| `app.state::<T>()`         | managers hold `Arc<AppContext>`               |
| `get_settings(&app)`       | `AppContext::settings` (owned `RwLock`)       |
| `app.path().resolve(..)`   | `AppContext::resource_dir` / `data_dir`       |

- `context.rs` — `AppContext`: owned settings (`RwLock`), broadcast event bus,
  resource/data paths. **Secrets (API keys) persist to a SEPARATE
  `grain.secrets.json`** so the main settings file never holds credentials.
- `event.rs` — `DaemonEvent` typed enum + `SessionMode` (Dictation / VoiceToAI /
  Batch). Replaces Handy's ~15 untyped `app.emit` strings.
- `settings.rs` — full `AppSettings` schema incl. STT/post-process provider
  pools, smart-rotation flags, quotas.

### 3.2 OS-native pill — `crates/grain-pill`

Replaces Handy's **React overlay** with a standalone native process (winit +
tiny-skia, Win32 layered window). The "Aura Core" dot-matrix capsule.

- **Always-on but display-only**, so the React frontend can be fully destroyed.
- Driven over a **local WebSocket** (`ws://127.0.0.1:7124`) carrying
  `DaemonEvent`s — it cannot receive Tauri webview events.
- **Lifecycle discipline ("destroy if not in use"):** the mic (cpal) is opened
  *just-in-time* when the pill becomes visible and **released on hide**; the
  event loop sleeps (no 60fps) while hidden.
- States: Idle / Recording / Processing / Fallback, plus a **prompt riser** for
  mid-speech prompt switching.
- Supervised + killed via a Windows **Job Object** (`KILL_ON_JOB_CLOSE`) and
  Linux `PR_SET_PDEATHSIG`, so it never outlives the core.

Wiring lives in `src-tauri/src/events_server.rs` (WS server + pill supervisor)
and `src-tauri/src/bridge.rs` (re-broadcast Tauri-side events onto the core bus).

### 3.3 Cloud STT — OpenAI-compatible, multi-provider

Handy transcribes locally only. Grain adds cloud STT as an OpenAI-compatible
endpoint path **alongside** the local model (local stays the default).

- `stt_router.rs` — dispatcher: rotation OFF → local in-process model; rotation
  ON → cloud-only pool (local is deliberately excluded from rotation).
- `stt_client.rs` — the cloud STT HTTP client.
- Per-provider **daily quota** gate + lazy day-rollover reset.

### 3.4 Cloud post-processing (LLM) — multi-provider

- `post_process_router.rs` — rotation pool + quota bookkeeping for LLM providers
  (a SEPARATE provider list from STT).
- `llm_client.rs` — OpenAI-compatible chat client. Shared by post-processing
  (structured output) and the Agent (free-form `send_chat`).

### 3.5 Smart rotation — `crates/provider-router` + `rotation_state.rs`

Multiple cloud providers for **both** STT and LLM, with rotation between them.

- `crates/provider-router` — pure, testable rotation logic: health-ordered
  failover, cooldowns from real 429s (Retry-After parsing), rate-limit-header
  headroom scoring, round-robin among equally-healthy providers.
- `rotation_state.rs` — thin Tauri-side glue: one `RotationTracker` per domain
  (STT vs LLM, never shared), monotonic clock, header conversion, outcome
  mapping. The tracker only **orders**; the hard daily **quota** gate stays in
  the routers (over `AppContext`).

### 3.6 Agent — `src-tauri/src/agent.rs`

A summoned, voice-first AI scratchpad in **destroyable** windows ("if it's not
in use, destroy it"). Works as a chatbot and for restructuring/modifying text.

- **PALETTE** (`agent`) — centred summon bar; records by default, type to
  override, Enter to submit. Captures the foreground selection via a synthesised
  copy + clipboard diff (then restores the clipboard), shows only a char count.
- **PANEL** (`agent-panel`) — right-side conversation, reply auto-copied.
- Reuses the STT dispatcher (`stt_router`) and the LLM rotation infra
  (`post_process_router` + `rotation_state`). Both windows are destroyed on close.

### 3.7 Rolling-window transcription — `crates/rolling-window` + `rolling.rs`

Streaming/rolling transcription (chunk pump, cursor, assembler, merge) so a
session can finalize incrementally. Lets a recording **start with any shortcut**
(batch or rolling window) and **end with an AI shortcut** to route to the LLM.
`SessionMode` drives the "what you end with wins" logic.

### 3.8 Prompt switcher

The active post-processing prompt can be switched mid-sentence; the selected
prompt change surfaces on the pill's riser (`DaemonEvent::PromptChanged`).

### 3.9 History for processed text

Handy stored only transcribed text. Grain also records **post-processed** output
in history. See `managers/history.rs` and `commands/history.rs`.

### 3.10 Other notable backend additions

- `transcription_coordinator.rs` — orchestrates the session/transcription flow.
- `apple_intelligence.rs` — local Apple Intelligence backend (macOS aarch64).
- `helpers/clamshell.rs`, `portable.rs` — platform/portable-mode helpers.

---

## 4. Where to look (quick index)

| Concern | Start here |
| --- | --- |
| Headless core / decoupling | `crates/grain-core/{context,event,settings}.rs` |
| Native pill | `crates/grain-pill/src/lib.rs` + `src-tauri/src/events_server.rs` |
| Tauri→core event bridge | `src-tauri/src/bridge.rs` |
| Cloud STT routing | `src-tauri/src/stt_router.rs`, `stt_client.rs` |
| LLM post-processing routing | `src-tauri/src/post_process_router.rs`, `llm_client.rs` |
| Smart rotation | `crates/provider-router/`, `src-tauri/src/rotation_state.rs` |
| Agent | `src-tauri/src/agent.rs` |
| Rolling window | `crates/rolling-window/`, `src-tauri/src/rolling.rs` |
| Local transcription (inherited) | `src-tauri/src/managers/transcription.rs` |

---

## 5. Constraints that shape every change (summary)

From `AGENTS.md` §2 — consult it for the authoritative version:

1. **Upstream compatibility** — stay close to Handy; extend, don't fork shared code.
2. **Frontend/backend decoupling** — never blur the command/event boundary.
3. **Destroy if not in use** — no resources/listeners/state/services beyond their lifetime.
4. **Low RAM / low overhead** — reject memory-for-convenience trades.
5. **Unusual code is probably intentional** — infer why before changing.

Optimization priority: **Correctness → Efficiency → Maintainability.**
