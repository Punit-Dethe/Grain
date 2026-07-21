//! The typed event stream the headless core broadcasts to every subscriber
//! (the pill, the settings window, the local server's `/events`).
//!
//! Replaces Handy's ~15 raw untyped `app.emit("...")` strings with one typed
//! enum carried over a `tokio::sync::broadcast` channel — multiple subscribers,
//! and no Tauri dependency.

use serde::{Deserialize, Serialize};

/// [GRAIN] Which brain the NATIVE agent input card serves — purely
/// presentational (the core routes submits by its own `AgentState.mode`). It
/// lets the ONE pill surface render the right variant without a second window:
/// `Assist` keeps the original card; the Grain Space kinds (`Capture`,
/// `Recall`) anchor to the TOP and relabel the card ("Noting…"/"Save Note" vs
/// "Listening…"/"Confirm"). No extra RAM — same window, same pixmap, just
/// different strings/anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentInputKind {
    /// The generic assistant (operates on selection/field). Original card.
    #[default]
    Assist,
    /// Grain Space capture (note authoring) — "Noting…", title+body, "Save Note".
    Capture,
    /// Grain Space recall (memory question) — "Listening…", single ask field.
    Recall,
}

/// Where the single pill anchors on screen (`None` = never show). Lives in the
/// SDK because it crosses the wire inside [`DaemonEvent::OverlayConfig`]; it is
/// also the persisted `overlay_position` setting (grain-core re-exports it).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
    /// [GRAIN] Vertically centered — the Native ASR Studio Window's natural home
    /// (a tall content box reads poorly hugging an edge); also selectable for
    /// the small pill.
    Center,
}

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

    /// [GRAIN] Show the NATIVE agent input (the summon surface): a bottom-center
    /// card that records by default and expands into a typing field the moment
    /// the user types. `selection_chars` feeds the selection chip;
    /// `type_to_expand` mirrors the setting (when false, typing while listening
    /// is ignored until the user expands explicitly).
    AgentInputShow {
        selection_chars: u32,
        #[serde(default)]
        type_to_expand: bool,
        /// Which brain this summon serves — drives the card variant (anchor +
        /// labels). Defaults to `Assist` for back-compat with older cores.
        #[serde(default)]
        kind: AgentInputKind,
    },
    /// [GRAIN] Hide the native agent input (submitted / cancelled / superseded).
    AgentInputHide,
    /// [GRAIN] Grain Space capture succeeded HEADLESSLY: tell the card to play a
    /// brief in-place "Saved" confirmation (green dot sweep + "Saved") before the
    /// core hides it. No new pill/surface — the same summon card confirms itself.
    AgentInputSaved,
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

    /// Where the single pill should anchor — and whether to show at all
    /// (`OverlayPosition::None` = never show). Emitted on session start and when
    /// the user changes the position setting, so the pill can place/hide itself.
    OverlayConfig {
        position: OverlayPosition,
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
    /// [GRAIN] Agent input: the user submitted TYPED text (expanded card).
    /// `title` is the optional Grain Space note title (Capture only; empty
    /// otherwise). `quick` = the user held Shift → Quick Agent (paste in place)
    /// instead of opening the panel (Assist only).
    AgentInputSubmitText {
        text: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        quick: bool,
    },
    /// [GRAIN] Agent input: submit the in-progress VOICE capture (compact card) —
    /// the core stops dictation, transcribes, and runs the instruction. `quick`
    /// as above (Shift held → Quick Agent).
    AgentInputSubmitVoice {
        #[serde(default)]
        quick: bool,
    },
    /// [GRAIN] Agent input: the user cancelled (Esc) — the core cancels dictation
    /// and destroys the pre-created panel.
    AgentInputCancel,
    /// [GRAIN] Agent input mode switch: `active: true` = the user started typing
    /// (core cancels the voice capture); `false` = the user tabbed back to voice
    /// (core restarts dictation).
    AgentInputTyping { active: bool },
}
