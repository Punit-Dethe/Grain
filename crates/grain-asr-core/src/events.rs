//! The two Native ASR event layers.
//!
//! A streaming backend is messy: it revises its hypothesis many times a second,
//! may or may not detect its own endpoints, and may or may not promise that a
//! "final" is immutable. We keep that messiness in [`AsrRawEvent`] and expose a
//! clean, UI-safe contract in [`AsrEvent`] after the [`crate::stabilizer`] has
//! decided what is committed (stable forever) versus volatile (may still change).
//!
//! The host then maps [`AsrEvent`] onto its own transport (Grain's
//! `DaemonEvent`). Keeping that mapping OUTSIDE this crate is what lets the crate
//! stay Tauri-free.

use serde::{Deserialize, Serialize};

/// One recognized word with optional timing/confidence.
///
/// Timings are milliseconds relative to the start of the *session* (not the
/// segment), so the host can place words on a single timeline. Backends that do
/// not emit word timing leave `start_ms`/`end_ms` at `0` and `confidence` at
/// `None`; consumers must treat absent timing as "unknown", never as "t=0".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsrWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: Option<f32>,
}

/// Why a segment ended. Drives whether the stabilizer trusts the tail as final.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointReason {
    /// The backend's own endpoint detector fired (e.g. trailing silence).
    Backend,
    /// The host's VAD/policy decided the utterance ended.
    Vad,
    /// The host is flushing/finishing the session (stop pressed, cancel, etc.).
    Flush,
}

/// Raw, pre-stabilization output of a backend session — exactly what
/// [`crate::session::AsrSession`] returns from `push_audio`/`flush`/`finish`.
///
/// `segment_id` groups events that belong to one utterance/endpoint span. A
/// backend that never endpoints uses a single segment for the whole session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AsrRawEvent {
    /// A revised hypothesis for `segment_id`. `text` is the FULL current best
    /// guess for the segment (not a delta); `revision` increases monotonically.
    /// `words` may be empty when the backend has no per-revision timing.
    Partial {
        segment_id: u64,
        revision: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    /// The backend's own "final" for `segment_id`. Whether this is truly
    /// immutable is told by [`crate::model::AsrCapabilities::immutable_final`];
    /// the stabilizer trusts it verbatim only when that flag is set.
    BackendFinal {
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    /// The segment ended. Some backends emit this instead of a final; the
    /// stabilizer then finalizes the segment from the last hypothesis.
    Endpoint {
        segment_id: u64,
        reason: EndpointReason,
        audio_end_ms: Option<u64>,
    },
    /// A backend error. `recoverable` means the session may keep going.
    Error { recoverable: bool, message: String },
}

/// How much the stabilizer trusts a partial's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stability {
    /// The whole partial agrees with the previous hypothesis — it is being held
    /// back only by the commit lag, and is unlikely to change.
    Stable,
    /// The tail is still in flux and may be rewritten on the next revision.
    Volatile,
}

/// UI-safe, stabilized events. This is the contract the pill renders against.
///
/// The cardinal rule (locked in the plan): **a `Commit` is immutable.** Once a
/// word is committed it never changes; only the volatile `Partial` tail may be
/// rewritten. There is no post-commit correction in the MVP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AsrEvent {
    /// The current volatile tail (everything after the committed prefix) for
    /// `segment_id`. Safe to display, NOT safe to paste — it can still change.
    Partial {
        session_id: u64,
        segment_id: u64,
        revision: u64,
        text: String,
        stability: Stability,
    },
    /// Newly committed words for `segment_id`, appended to the immutable prefix.
    /// `text` is just the new words (a delta, unlike the raw `Partial`).
    Commit {
        session_id: u64,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    /// The segment is closed. `text` is the segment's full final transcript.
    SegmentFinal {
        session_id: u64,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    },
    /// The whole session is closed. `text` is every segment joined in order —
    /// the string the host finalizes/pastes/saves to history.
    SessionFinal { session_id: u64, text: String },
    /// Surfaced backend error.
    Error {
        session_id: u64,
        recoverable: bool,
        message: String,
    },
}
