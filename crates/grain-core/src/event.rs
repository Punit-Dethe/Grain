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
}

/// One event broadcast by the daemon. `Clone` so every subscriber gets a copy;
/// `Serialize`/`Deserialize` so it can cross the local WebSocket to the pill.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DaemonEvent {
    // -- Recording lifecycle --
    RecordingStarted { session_id: u64, mode: SessionMode },
    RecordingStopped { session_id: u64 },
    SessionCancelled { session_id: u64 },

    // -- Rolling-window progress --
    /// Intermediate assembled text after a chunk finalized.
    ChunkComplete { session_id: u64, chunk_idx: u32, text: String },
    /// Final assembled transcript for the session.
    TranscriptionComplete { session_id: u64, text: String },
    /// LLM post-processing finished.
    ProcessingComplete { session_id: u64, text: String },

    // -- Model lifecycle (replaces `model-state-changed`) --
    ModelLoading { model_id: String },
    ModelLoaded { model_id: String },
    ModelUnloaded,
    ModelError { error: String },
    /// Download/verify/extract progress (replaces `model-download-progress` etc.).
    ModelDownloadProgress { model_id: String, progress: f32 },

    // -- Pill UI feed --
    /// Per-bucket audio energy driving the Aura Core dots (replaces `mic-level`).
    AudioLevel { levels: Vec<f32> },
    /// Active prompt changed mid-speech → pill riser (← name →).
    PromptChanged { name: String },

    // -- Misc UI signals --
    ShowOverlay,
    HideOverlay,
    PasteError { error: String },

    /// Where the single pill should anchor — and whether to show at all
    /// (`OverlayPosition::None` = never show). Emitted on session start and when
    /// the user changes the position setting, so the pill can place/hide itself.
    OverlayConfig {
        position: crate::settings::OverlayPosition,
    },
}
