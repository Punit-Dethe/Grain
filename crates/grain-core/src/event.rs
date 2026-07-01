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
    /// far (flicker-free, growing). transcribe-cpp does its own commit
    /// stabilization, so this replaces the sherpa-era `AsrCommit`/`AsrPartial`
    /// split — the pill renders `committed` directly (committed-only, like Handy).
    AsrStreamText {
        session_id: u64,
        committed: String,
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
