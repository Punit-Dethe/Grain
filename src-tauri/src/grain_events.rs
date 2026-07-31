//! [GRAIN] The typed webview event surface.
//!
//! # Why there are two event buses, and why that is correct
//!
//! Grain emits events on two paths, and it is easy to mistake them for a
//! duplication that wants merging. They are not.
//!
//! | Bus | Transport | Audience |
//! | --- | --- | --- |
//! | Tauri events (this file) | in-process event bus | Grain's own webviews |
//! | [`grain_core::DaemonEvent`] | authenticated local WebSocket | the native pill, extensions |
//!
//! [`crate::bridge`] states the reason: the winit pill is a *separate native
//! surface* and cannot receive Tauri webview events at all, so it subscribes to
//! `DaemonEvent` on the core's broadcast bus instead. That bus is
//! capability-filtered per client (`events_auth::allows_event`) because
//! **untrusted extension code is on it**. Grain's own window is not untrusted
//! and is not out of process, so routing the app's UI notifications through a
//! socket + handshake + capability map would buy nothing and cost a connection.
//!
//! `DaemonEvent`'s "replaces `model-state-changed`" comments mean *the pill
//! learns model state this way instead of the way a webview would* — not that
//! the webview surface is going away.
//!
//! # What this module is
//!
//! Payload types for the events Grain's main window consumes, registered in
//! `collect_events!` so they appear in `src/bindings.ts` and the UI can write
//! `events.modelStateChanged.listen(...)` instead of `listen("model-state-changed")`.
//! A renamed event then fails to compile rather than failing silently in a
//! screen nobody happened to open.
//!
//! **Nothing here changes what is emitted.** Every name below is the name
//! already on the wire — `tauri_specta` derives it as kebab-case of the type
//! name, and `event_names` asserts each one, so a rename cannot slip through.
//! The emit sites are untouched, and most of them live in `handy/` where they
//! must stay byte-identical to upstream anyway.
//!
//! That is also why these are *mirrors* of the payload structs rather than
//! derives added to them: `ModelStateEvent`, `DownloadProgress` and
//! `RecordingErrorEvent` are Handy's, inside the frozen tree. `shapes_match`
//! guards the copy — it round-trips each upstream struct through JSON into its
//! mirror, so if upstream adds or renames a field, the test fails instead of
//! the TypeScript quietly lying about the payload.
//!
//! Deliberately NOT typed here: `log://log`, `grain-space://*` and
//! `ext-host://*`. Their names cannot be expressed as Rust type names, and they
//! are internal plumbing for the log viewer, the notes workspace and the hidden
//! extension supervisor rather than part of the app's UI surface. The
//! `grain-space://` ones already have named constants in
//! `grain_space::{mod,embed}`, which is the same protection by other means.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A model was selected, started loading, finished loading, or failed.
/// Mirrors `managers::transcription::ModelStateEvent`.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct ModelStateChanged {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

/// Byte progress for a model download. Mirrors `managers::model::DownloadProgress`.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

/// A recording could not start or could not finish. Mirrors
/// `actions::RecordingErrorEvent`.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RecordingError {
    pub error_type: String,
    pub detail: Option<String>,
}

/// The transcript could not be pasted. Carries no payload — the technical
/// detail is logged on the Rust side and the window shows a localized message.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct PasteError;

/// Model-lifecycle events whose entire payload is the model id.
macro_rules! model_id_event {
    ($(#[$doc:meta] $name:ident),* $(,)?) => {
        $(
            #[$doc]
            #[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
            pub struct $name(pub String);
        )*
    };
}

model_id_event! {
    /// The model's bytes are on disk.
    ModelDownloadComplete,
    /// The user cancelled an in-flight download.
    ModelDownloadCancelled,
    /// The model's files were removed.
    ModelDeleted,
    /// Checksum verification began.
    ModelVerificationStarted,
    /// Checksum verification passed.
    ModelVerificationCompleted,
    /// Archive extraction began (directory-based models).
    ModelExtractionStarted,
    /// Archive extraction finished.
    ModelExtractionCompleted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_specta::Event;

    /// The wire names must be exactly what is already emitted. `tauri_specta`
    /// derives them from the type name, so a rename here is a silent protocol
    /// break — the UI would subscribe to an event nothing sends.
    #[test]
    fn event_names() {
        assert_eq!(ModelStateChanged::NAME, "model-state-changed");
        assert_eq!(ModelDownloadProgress::NAME, "model-download-progress");
        assert_eq!(ModelDownloadComplete::NAME, "model-download-complete");
        assert_eq!(ModelDownloadCancelled::NAME, "model-download-cancelled");
        assert_eq!(ModelDeleted::NAME, "model-deleted");
        assert_eq!(ModelVerificationStarted::NAME, "model-verification-started");
        assert_eq!(
            ModelVerificationCompleted::NAME,
            "model-verification-completed"
        );
        assert_eq!(ModelExtractionStarted::NAME, "model-extraction-started");
        assert_eq!(ModelExtractionCompleted::NAME, "model-extraction-completed");
        assert_eq!(RecordingError::NAME, "recording-error");
        assert_eq!(PasteError::NAME, "paste-error");
    }

    /// Each mirror must still match the upstream struct actually emitted. Round
    /// -tripping through JSON is the check that matters: it is the same
    /// serialization the webview receives, so a field upstream adds, renames or
    /// retypes fails here rather than in a user's window.
    #[test]
    fn shapes_match() {
        let upstream = crate::managers::transcription::ModelStateEvent {
            event_type: "loading_failed".into(),
            model_id: Some("parakeet".into()),
            model_name: Some("Parakeet".into()),
            error: Some("boom".into()),
        };
        let mirrored: ModelStateChanged =
            serde_json::from_str(&serde_json::to_string(&upstream).unwrap()).unwrap();
        assert_eq!(mirrored.event_type, upstream.event_type);
        assert_eq!(mirrored.model_id, upstream.model_id);
        assert_eq!(mirrored.model_name, upstream.model_name);
        assert_eq!(mirrored.error, upstream.error);

        let upstream = crate::managers::model::DownloadProgress {
            model_id: "parakeet".into(),
            downloaded: 12,
            total: 100,
            percentage: 12.0,
        };
        let mirrored: ModelDownloadProgress =
            serde_json::from_str(&serde_json::to_string(&upstream).unwrap()).unwrap();
        assert_eq!(mirrored.model_id, upstream.model_id);
        assert_eq!(mirrored.downloaded, upstream.downloaded);
        assert_eq!(mirrored.total, upstream.total);
        assert_eq!(mirrored.percentage, upstream.percentage);

        let upstream = crate::actions::RecordingErrorEvent {
            error_type: "no_input_device".into(),
            detail: None,
        };
        let mirrored: RecordingError =
            serde_json::from_str(&serde_json::to_string(&upstream).unwrap()).unwrap();
        assert_eq!(mirrored.error_type, upstream.error_type);
        assert_eq!(mirrored.detail, upstream.detail);
    }
}
