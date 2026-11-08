//! The typed event stream the headless core broadcasts to every subscriber
//! (the pill, the settings window, the local server's `/events`).
//!
//! Replaces Handy's ~15 raw untyped `app.emit("...")` strings with one typed
//! enum carried over a `tokio::sync::broadcast` channel — multiple subscribers,
//! and no Tauri dependency.

use serde::{Deserialize, Serialize};

/// Edge length of the icon the core hands the pill, in pixels. Fixed so the
/// wire payload is always exactly `PILL_ICON_PX² × 4` bytes and neither side has
/// to negotiate a size; the pill scales it down to its fixed application-icon slot.
pub const PILL_ICON_PX: usize = 64;

/// Where the recording pill anchors on screen. Visibility is OverlayStyle. Lives in the
/// SDK because it crosses the wire inside [`DaemonEvent::OverlayConfig`]; it is
/// also the persisted `overlay_position` setting (grain-core re-exports it).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Top,
    #[serde(alias = "none", alias = "center")]
    Bottom,
}

/// [GRAIN] What to actually paint — the user's `theme` preference already
/// resolved against the OS. Lives in the SDK because it crosses the wire inside
/// [`DaemonEvent::ThemeConfig`]; grain-core re-exports it next to the
/// `ThemeMode` preference that produces it.
///
/// Only the resolved answer travels. A surface that received `System` would
/// have to ask the OS itself. The settings window, pill and Agent panel must
/// paint the same resolved answer.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedTheme {
    Light,
    Dark,
}

/// What a recording session is for. Drives the "what you end with wins" logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum SessionMode {
    /// Grain Flow capture — model-selected incremental dictation.
    Dictation,
    /// Voice-to-AI — stop sends the transcript to the LLM with the active prompt.
    VoiceToAI,
    /// Handy dictation — the selected model chooses batch or live streaming.
    Batch,
}

/// One event broadcast by the daemon. `Clone` so every subscriber gets a copy;
/// `Serialize`/`Deserialize` so it can cross the local WebSocket to the pill.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub enum DaemonEvent {
    // -- Recording lifecycle --
    RecordingStarted {
        session_id: u64,
        mode: SessionMode,
    },
    RecordingStopped {
        session_id: u64,
    },
    SessionCancelled {
        session_id: u64,
    },

    /// [GRAIN] Prompt Record: the user clicked the compact pill mid-recording to
    /// begin dictating an AI *instruction* (everything spoken after this point is
    /// a prompt for post-processing, not content). Drives the pill's blue dot
    /// tint. One-way for now — `active` goes true and stays true until the session
    /// ends (no toggle-off, by design, to keep the interaction dead simple).
    PromptRecordingChanged {
        session_id: u64,
        active: bool,
    },

    // -- Rolling-window progress --
    /// Intermediate assembled text after a chunk finalized.
    ChunkComplete {
        session_id: u64,
        chunk_idx: u32,
        text: String,
    },
    /// Final assembled transcript for the session.
    TranscriptionComplete {
        session_id: u64,
        text: String,
    },
    /// LLM post-processing finished.
    ProcessingComplete {
        session_id: u64,
        text: String,
    },

    // -- Model lifecycle (replaces `model-state-changed`) --
    ModelLoading {
        model_id: String,
    },
    ModelLoaded {
        model_id: String,
    },
    ModelUnloaded,
    ModelError {
        error: String,
    },
    /// Download/verify/extract progress (replaces `model-download-progress` etc.).
    ModelDownloadProgress {
        model_id: String,
        progress: f32,
    },

    // -- Pill UI feed --
    /// Per-bucket audio energy driving the Aura Core dots (replaces `mic-level`).
    AudioLevel {
        levels: Vec<f32>,
    },
    /// [GRAIN] A routed action needs the user to decide
    /// (`docs/Action Routing/PLAN.md` §7). The pill reveals a capsule listing
    /// the numbered options; digits pick, Escape cancels, and it releases on
    /// its own after a moment.
    ///
    /// `heard` is what the recogniser produced, shown verbatim. It is the whole
    /// reason a chooser is not a dead end: when the wrong options appear, the
    /// answer is almost always visible in that line.
    ActionChoice {
        /// `action`, `provider`, `entity`, or `confirm` — the four things this
        /// one capsule is used for.
        kind: String,
        heard: String,
        /// One line per option, already ordered. Never more than a handful:
        /// past that there is no honest question to ask.
        options: Vec<String>,
    },
    /// [GRAIN] The choice was answered, cancelled, or timed out — hide the
    /// capsule. Emitted exactly once per [`DaemonEvent::ActionChoice`].
    ActionChoiceClosed,
    /// [GRAIN] What became of a routed action, for the pill's brief
    /// confirmation. `ok` false means it did not run.
    ActionResult {
        ok: bool,
        message: String,
    },
    /// [GRAIN] Quick Agent: a reply was just auto-pasted at the cursor. The pill
    /// briefly reveals with an "ASK FOLLOW-UP · <shortcut>" affordance; clicking
    /// it (or pressing the shortcut) reopens the Agent expanded with the
    /// conversation restored. `shortcut` is the human-readable binding label.
    AgentFollowupOffer {
        shortcut: String,
    },
    /// [GRAIN] Withdraw the follow-up offer (panel opened, offer expired, or a
    /// new session started).
    AgentFollowupClear,

    /// [GRAIN] Show the NATIVE agent input (the summon surface): a bottom-center
    /// card that records by default and expands into a typing field the moment
    /// the user types. `selection_chars` feeds the selection chip;
    /// `type_to_expand` mirrors the setting (when false, typing while listening
    /// is ignored until the user expands explicitly).
    AgentInputShow {
        selection_chars: u32,
        #[serde(default)]
        type_to_expand: bool,
    },
    /// [GRAIN] Hide the native agent input (submitted / cancelled / superseded).
    AgentInputHide,
    /// [GRAIN] The core's transient global Enter fired while the agent input is
    /// up. The pill owns the typed text, so it answers with
    /// `AgentInputSubmitText` (typing) or `AgentInputSubmitVoice` (recording).
    AgentInputSubmitRequest,

    // -- Misc UI signals --
    ShowOverlay,
    HideOverlay,
    PasteError {
        error: String,
    },

    /// [GRAIN] Paste Catch: a transcript's paste provably missed the text field,
    /// so Grain is holding it on the clipboard instead of letting the restore
    /// destroy it. The pill shows an offer for the hold's lifetime; pressing
    /// `shortcut` (or plain Ctrl+V — the transcript really is on the clipboard)
    /// delivers it. `chars` is the transcript length, for a "42 words held"
    /// style hint without shipping the text itself to the surface.
    PasteMissed {
        shortcut: String,
        chars: u32,
    },
    /// [GRAIN] Withdraw the Paste Catch offer: delivered, superseded by a new
    /// dictation, or the hold expired and the clipboard was handed back.
    PasteMissedClear,
    /// [GRAIN] Paste Catch was turned off in settings. Unlike the ordinary hold
    /// clear above, this also withdraws the pill's independent three-second
    /// clipboard confirmation immediately.
    PasteCatchDisabled,

    /// Where the single pill should anchor — and whether to show at all
    /// (`OverlayPosition::None` = never show). Emitted on session start and when
    /// the user changes the position setting, so the pill can place/hide itself.
    OverlayConfig {
        position: OverlayPosition,
    },

    /// [GRAIN] Which colour scheme every surface should paint. Emitted when the
    /// user changes the preference and when the OS flips under `System`, so the
    /// settings window, pill and Agent panel restyle without polling.
    ThemeConfig {
        theme: ResolvedTheme,
    },

    // -- Native ASR (real-time streaming dictation) --
    /// [GRAIN] transcribe-cpp streaming: the cumulative committed transcript so
    /// far (flicker-free, growing) plus the volatile `tentative` tail the model
    /// may still rewrite. The pill renders BOTH (Handy parity: its overlay shows
    /// committed + tentative) — the engine's auto-commit can stall for long
    /// stretches (often right after a sentence boundary), and without the tail
    /// the live preview visibly freezes even though decoding continues.
    AsrStreamText {
        session_id: u64,
        committed: String,
        #[serde(default)]
        tentative: String,
    },

    // The stabilized stream from the (legacy sherpa) Native ASR path. `AsrCommit`
    // text is immutable (safe to keep); `AsrPartial` text is volatile.
    /// Volatile tail for a segment. `stable` = the stabilizer is confident it
    /// won't change (held only by commit lag), else it may still be rewritten.
    AsrPartial {
        session_id: u64,
        segment_id: u64,
        text: String,
        stable: bool,
    },
    /// Newly committed (immutable) words appended to a segment's prefix.
    AsrCommit {
        session_id: u64,
        segment_id: u64,
        text: String,
    },
    /// A segment closed; `text` is its full final transcript.
    AsrSegmentFinal {
        session_id: u64,
        segment_id: u64,
        text: String,
    },
    /// The whole session closed; `text` is every segment joined (paste/history).
    AsrSessionFinal {
        session_id: u64,
        text: String,
    },
    /// A surfaced Native ASR error.
    AsrError {
        session_id: u64,
        recoverable: bool,
        message: String,
    },

    /// [GRAIN] Extension platform (SPEC §3.3): a scripted extension was
    /// disabled after violating the tool worker resource policy.
    /// The Overview UI shows the reason; the user re-enables explicitly.
    ExtensionDisabled {
        id: String,
        reason: String,
    },

    /// [GRAIN] The icon of whatever the user is dictating into, so the pill can
    /// show that it understands the surface rather than merely that it is on.
    ///
    /// `PILL_ICON_PX`² **premultiplied** RGBA, base64 — finished pixels, because
    /// resolving them means COM and the shell on Windows and entirely different
    /// machinery elsewhere, and none of that belongs in the pill process. `None`
    /// means "no icon, draw the plain state dot", which is also what the pill
    /// shows until a cold resolve lands (it never blocks a recording).
    ///
    /// Sent at most twice per session, never per frame.
    PillIcon {
        #[serde(default)]
        rgba: Option<String>,
    },
}
