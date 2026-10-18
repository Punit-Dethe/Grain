//! Grain capture callers share Handy's first-sample feedback contract.
use crate::managers::audio::{AudioRecordingManager, RecordingReadiness};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// A resident selected model remains usable after its file is removed. Otherwise
/// check the actual file, rather than the registry's cached download badge,
/// before acquiring microphone or presentation resources.
pub fn require_transcribable_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let availability = app
        .state::<Arc<crate::managers::model::ModelManager>>()
        .get_model_path(model_id);
    if availability.is_ok() {
        return Ok(());
    }
    let manager = app.state::<Arc<crate::managers::transcription::TranscriptionManager>>();
    if manager.get_current_model().as_deref() == Some(model_id)
        && manager.grain_has_resident_model()
    {
        return Ok(());
    }
    availability
        .map(|_| ())
        .map_err(|error| format!("No model can transcribe this recording: {error}"))
}

/// Call after queuing the session's overlay show. Stop/cancel invalidates every
/// stage, including the mute after a blocking cue.
pub fn announce_ready(
    app: &AppHandle,
    manager: &Arc<AudioRecordingManager>,
    readiness: RecordingReadiness,
    cue: bool,
) {
    let app = app.clone();
    let manager = Arc::clone(manager);
    std::thread::spawn(move || {
        let generation = readiness.generation();
        loop {
            if !manager.is_recording_readiness_current(generation) {
                return;
            }
            match readiness.wait_timeout(std::time::Duration::from_millis(50)) {
                Ok(()) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        let handle = app.clone();
        let ready_manager = Arc::clone(&manager);
        let _ = app.run_on_main_thread(move || {
            if ready_manager.is_recording_readiness_current(generation) {
                crate::grain_overlay::mark_ready(&handle);
                crate::overlay::emit_recording_ready(&handle);
            }
        });
        if cue && manager.is_recording_readiness_current(generation) {
            crate::audio_feedback::play_feedback_sound_blocking(
                &app,
                crate::audio_feedback::SoundType::Start,
            );
        }
        if manager.is_recording_readiness_current(generation) {
            manager.apply_mute();
        }
    });
}
