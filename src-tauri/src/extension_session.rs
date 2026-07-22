//! [GRAIN] Host-owned recording sessions contributed by extensions (Phase 4).
//!
//! The microphone remains Grain's hard singleton. An extension may name a
//! mode and own the bounded slow stage, but it never receives audio or controls
//! capture directly. One mutex serializes the claim; the audio manager is the
//! second line of defence against racing a core recording.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{AppHandle, Emitter, Manager};

use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::tray::{change_tray_icon, TrayIconState};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static ACTIVE: OnceLock<Mutex<Option<ActiveSession>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Recording,
    Processing,
}

#[derive(Clone, Debug)]
struct ActiveSession {
    generation: u64,
    session_id: u64,
    ext_id: String,
    mode_id: String,
    binding_id: String,
    phase: Phase,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    Busy,
    InvalidMode,
    Unavailable(String),
}

fn active() -> &'static Mutex<Option<ActiveSession>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

fn declared_session(
    app: &AppHandle,
    ext_id: &str,
    mode_id: &str,
) -> Result<(String, String), StartError> {
    let registry = app
        .try_state::<Arc<grain_core::extensions::ExtensionsRegistry>>()
        .ok_or_else(|| StartError::Unavailable("extensions registry unavailable".into()))?;
    if !registry.record(ext_id).is_some_and(|record| record.enabled) {
        return Err(StartError::Unavailable("extension is not enabled".into()));
    }
    let pack = crate::extension_host::load_manifest_result(app, ext_id)
        .map_err(StartError::Unavailable)?;
    let mode = pack
        .manifest
        .contributes
        .session_mode
        .filter(|mode| mode.id == mode_id)
        .ok_or(StartError::InvalidMode)?;
    Ok((pack.manifest.name, mode.id))
}

/// Enter the one recording-session code path used by both API and shortcut.
pub fn start(app: &AppHandle, ext_id: &str, mode_id: &str) -> Result<u64, StartError> {
    let (ext_name, mode_id) = declared_session(app, ext_id, mode_id)?;
    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let transcription = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    let mut slot = active().lock().unwrap();
    if slot.is_some() || recording.is_recording() {
        return Err(StartError::Busy);
    }

    if !crate::stt_router::will_route_to_cloud(app) {
        transcription.initiate_model_load();
    }
    {
        let recording = Arc::clone(&recording);
        std::thread::spawn(move || {
            let _ = recording.preload_vad();
        });
    }

    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    let binding_id = format!("ext-session:{ext_id}:{mode_id}:{generation}");
    let always_on = crate::settings::get_settings(app).always_on_microphone;
    if always_on {
        let recording_for_sound = Arc::clone(&recording);
        let app_for_sound = app.clone();
        std::thread::spawn(move || {
            play_feedback_sound_blocking(&app_for_sound, SoundType::Start);
            recording_for_sound.apply_mute();
        });
    }
    recording
        .try_start_recording(&binding_id, VadPolicy::Offline)
        .map_err(|error| {
            if error == "Already recording" {
                StartError::Busy
            } else {
                StartError::Unavailable(error)
            }
        })?;
    if !always_on {
        let recording_for_sound = Arc::clone(&recording);
        let app_for_sound = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            play_feedback_sound_blocking(&app_for_sound, SoundType::Start);
            recording_for_sound.apply_mute();
        });
    }

    let session_id = crate::grain_actions::extension_session_started(app, &ext_name);
    *slot = Some(ActiveSession {
        generation,
        session_id,
        ext_id: ext_id.to_string(),
        mode_id: mode_id.clone(),
        binding_id,
        phase: Phase::Recording,
    });
    drop(slot);

    change_tray_icon(app, TrayIconState::Recording);
    crate::shortcut::register_cancel_shortcut(app);
    crate::extension_host::wake_for_session(app, ext_id, &mode_id);
    log::info!("[ext:{ext_id}] life session started mode:{mode_id}");
    Ok(session_id)
}

/// The host-contributed mode binding toggles capture. A programmatic second
/// `session.start` never reaches this function and correctly returns busy.
pub fn toggle_from_shortcut(app: &AppHandle, ext_id: &str, mode_id: &str) {
    let should_stop = active().lock().unwrap().as_ref().is_some_and(|session| {
        session.ext_id == ext_id && session.mode_id == mode_id && session.phase == Phase::Recording
    });
    if should_stop {
        stop(app, ext_id, mode_id);
    } else if let Err(error) = start(app, ext_id, mode_id) {
        log::warn!("[ext:{ext_id}] session shortcut failed: {error:?}");
    }
}

fn stop(app: &AppHandle, ext_id: &str, mode_id: &str) {
    let snapshot = {
        let mut slot = active().lock().unwrap();
        let Some(session) = slot.as_mut() else {
            return;
        };
        if session.ext_id != ext_id
            || session.mode_id != mode_id
            || session.phase != Phase::Recording
        {
            return;
        }
        session.phase = Phase::Processing;
        session.clone()
    };

    crate::shortcut::unregister_cancel_shortcut(app);
    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let cancel_generation = recording.cancel_generation();
    recording.remove_mute();
    play_feedback_sound(app, SoundType::Stop);
    change_tray_icon(app, TrayIconState::Transcribing);
    crate::grain_actions::emit_recording_stopped(app);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        finish(app, recording, snapshot, cancel_generation).await;
    });
}

async fn finish(
    app: AppHandle,
    recording: Arc<AudioRecordingManager>,
    session: ActiveSession,
    cancel_generation: u64,
) {
    let Some(samples) = recording.stop_recording(&session.binding_id, cancel_generation) else {
        complete(&app, session.generation, session.session_id);
        return;
    };
    if samples.is_empty() || recording.was_cancelled_since(cancel_generation) {
        complete(&app, session.generation, session.session_id);
        return;
    }

    let history = Arc::clone(&app.state::<Arc<HistoryManager>>());
    let file_name = format!("grain-ext-{}.wav", chrono::Utc::now().timestamp());
    let wav_path = history.recordings_dir().join(&file_name);
    let wav_samples = samples.clone();
    let wav_saved = tauri::async_runtime::spawn_blocking(move || {
        crate::audio_toolkit::save_wav_file(&wav_path, &wav_samples)
    })
    .await
    .is_ok_and(|result| result.is_ok());

    let (transcription, _) = crate::prompt_record::transcribe_split(&app, samples, None).await;
    let transcription = match transcription {
        Ok(text) => text,
        Err(error) => {
            log::error!(
                "[ext:{}] session transcription failed: {error}",
                session.ext_id
            );
            let _ = app.emit("transcription-error", error.to_string());
            complete(&app, session.generation, session.session_id);
            return;
        }
    };
    if recording.was_cancelled_since(cancel_generation) {
        return;
    }

    let transformed = crate::extension_host::run_transforms(&app, transcription.clone()).await;
    let stage = crate::grain_post_process::run_extension_session_stage(
        &session.ext_id,
        &session.mode_id,
        &transformed,
    )
    .await;
    if recording.was_cancelled_since(cancel_generation) {
        return;
    }

    if wav_saved {
        let processed = (!stage.handled && stage.text != transcription).then(|| stage.text.clone());
        if let Err(error) = history.save_entry(file_name, transcription, true, processed, None) {
            log::error!(
                "[ext:{}] failed to save session history: {error}",
                session.ext_id
            );
        }
    }

    if stage.handled || stage.text.trim().is_empty() {
        complete(&app, session.generation, session.session_id);
        return;
    }
    let output = stage.text;
    let app_for_paste = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Err(error) = crate::utils::paste(output, app_for_paste.clone()) {
            log::error!("Failed to paste extension session: {error}");
            let _ = app_for_paste.emit("paste-error", ());
        }
        complete(&app_for_paste, session.generation, session.session_id);
    }) {
        log::error!("Failed to schedule extension-session paste: {error:?}");
        complete(&app, session.generation, session.session_id);
    }
}

fn complete(app: &AppHandle, generation: u64, session_id: u64) {
    let mut slot = active().lock().unwrap();
    if !slot
        .as_ref()
        .is_some_and(|session| session.generation == generation)
    {
        return;
    }
    *slot = None;
    drop(slot);
    crate::grain_actions::emit_processing_complete(app, session_id);
    change_tray_icon(app, TrayIconState::Idle);
}

/// Cancel recording or processing immediately. The regular cancel path emits
/// the shared SessionCancelled event after this cleanup returns.
pub fn cancel(app: &AppHandle) -> bool {
    let session = active().lock().unwrap().take();
    let Some(session) = session else {
        return false;
    };
    app.state::<Arc<AudioRecordingManager>>().cancel_recording();
    crate::extension_host::cancel_session_stage(&session.ext_id, "session cancelled");
    crate::shortcut::unregister_cancel_shortcut(app);
    change_tray_icon(app, TrayIconState::Idle);
    log::info!("[ext:{}] life session cancelled", session.ext_id);
    true
}

pub fn is_active() -> bool {
    active().lock().unwrap().is_some()
}

pub fn is_owned_by(ext_id: &str) -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|session| session.ext_id == ext_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_guard_does_not_clear_a_newer_session() {
        let mut slot = active().lock().unwrap();
        *slot = Some(ActiveSession {
            generation: 9,
            session_id: 3,
            ext_id: "com.example.notes".into(),
            mode_id: "note".into(),
            binding_id: "binding".into(),
            phase: Phase::Processing,
        });
        assert!(!slot.as_ref().is_some_and(|session| session.generation == 8));
        *slot = None;
    }
}
