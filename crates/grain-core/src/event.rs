//! The typed event stream the headless core broadcasts to every subscriber
//! (the pill, the settings window, the local server's `/events`).
//!
//! Replaces Handy's ~15 raw untyped `app.emit("...")` strings with one typed
//! enum carried over a `tokio::sync::broadcast` channel — multiple subscribers,
//! and no Tauri dependency.

use serde::{Deserialize, Serialize};

/// What a recording session is for. Drives the "what you end with wins" logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMode {
    /// Plain dictation — stop pastes the raw transcript.
    Dictation,
    /// Voice-to-AI — stop sends the transcript to the LLM with the active prompt.
    VoiceToAI,
    /// Batch — record fully, then transcribe once (no rolling window).
    Batch,
    /// [GRAIN] Native ASR — live streaming dictation. Tells the pill to switch
    /// from the small collapsed capsule to the expanded Studio Window, since
    /// only this mode has a stabilized live-text stream (`Asr*` events) worth
    /// displaying.
    NativeAsr,
}

/// One event broadcast by the daemon. `Clone` so every subscriber gets a copy;
/// `Serialize`/`Deserialize` so it can cross the local WebSocket to the pill.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// Active prompt changed mid-speech → pill riser (← name →).
    PromptChanged {
        name: String,
    },
    /// [GRAIN] Auto-dictionary: offer to learn a corrected word. The pill shows a
    /// clickable `Add "word"?` prompt; a click sends a reverse WS action back to
    /// the core (see `events_server`).
    DictionarySuggestion {
        word: String,
    },
    /// [GRAIN] Dismiss any active dictionary suggestion (accepted, timed out, or
    /// superseded).
    DictionarySuggestionClear,

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

    // -- Misc UI signals --
    ShowOverlay,
    HideOverlay,
    PasteError {
        error: String,
    },

    /// Where the single pill should anchor — and whether to show at all
    /// (`OverlayPosition::None` = never show). Emitted on session start and when
    /// the user changes the position setting, so the pill can place/hide itself.
    OverlayConfig {
        position: crate::settings::OverlayPosition,
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
}

/// [GRAIN] The reverse channel: actions the pill sends BACK to the core over the
/// same local WebSocket (the core's events server reads these). Kept tiny and
/// self-describing so the transport stays a single duplex JSON stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PillAction {
    /// User clicked the pill's dictionary suggestion — learn `word`.
    DictionaryAccept { word: String },
    /// User dismissed the suggestion without accepting.
    DictionaryDismiss,
    /// [GRAIN] User clicked the compact pill mid-recording to enter Prompt Record
    /// mode. The core marks the current audio position as the content→instruction
    /// split point and echoes back `PromptRecordingChanged { active: true }`.
    PromptRecord,
    /// [GRAIN] User clicked the pill's Quick-Agent follow-up offer — reopen the
    /// Agent expanded with the retained conversation.
    AgentFollowup,
}
