//! [GRAIN] Native ASR (streaming) commands — GGUF models + transcribe-cpp.
//!
//! list / download / cancel / delete / select target the separate streaming
//! registry and the `selected_asr_model` setting (`selected_model` is never
//! touched). start/stop drive the live streaming session on the selected GGUF
//! model through the transcribe-cpp worker.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::audio_toolkit::text::finalize_transcript;
use crate::managers::asr_model::{AsrModelInfo, AsrModelManager};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::native_asr::NativeAsrManager;
use crate::settings::{get_settings, write_settings};

/// Recording binding id for the Native ASR route (distinct from Batch/Rolling so
/// they never alias in the recorder's active-binding state).
const NATIVE_ASR_BINDING: &str = "native_asr";

#[tauri::command]
#[specta::specta]
pub async fn list_asr_models(
    manager: State<'_, Arc<AsrModelManager>>,
) -> Result<Vec<AsrModelInfo>, String> {
    Ok(manager.list())
}

#[tauri::command]
#[specta::specta]
pub async fn download_asr_model(
    manager: State<'_, Arc<AsrModelManager>>,
    model_id: String,
) -> Result<(), String> {
    manager.download(&model_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_asr_model_download(
    manager: State<'_, Arc<AsrModelManager>>,
    model_id: String,
) -> Result<(), String> {
    manager.cancel_download(&model_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_asr_model(
    app_handle: AppHandle,
    manager: State<'_, Arc<AsrModelManager>>,
    model_id: String,
) -> Result<(), String> {
    // Clear the selection if we're deleting the active model.
    let settings = get_settings(&app_handle);
    if settings.selected_asr_model == model_id {
        let mut settings = settings;
        settings.selected_asr_model = String::new();
        write_settings(&app_handle, settings);
    }
    manager.delete(&model_id).map_err(|e| e.to_string())
}

/// Persist the selected streaming model. Accepts any catalog id (download may
/// happen afterward); the start path checks the GGUF is actually present.
#[tauri::command]
#[specta::specta]
pub async fn select_asr_model(
    app_handle: AppHandle,
    _manager: State<'_, Arc<AsrModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let mut settings = get_settings(&app_handle);
    settings.selected_asr_model = model_id;
    write_settings(&app_handle, settings);
    Ok(())
}

/// Source language hint for the engine: `None` = autodetect, else the ISO code.
pub(crate) fn language_hint(app: &AppHandle) -> Option<String> {
    let lang = get_settings(app).selected_language;
    if lang.is_empty() || lang == "auto" {
        None
    } else {
        Some(lang)
    }
}

/// Start a streaming Native ASR session on the selected GGUF model. Frees
/// Batch/Rolling via the lifecycle arbiter, opens the mic, and starts the worker.
#[tauri::command]
#[specta::specta]
pub async fn start_native_asr(
    app: AppHandle,
    recording: State<'_, Arc<AudioRecordingManager>>,
    manager: State<'_, Arc<NativeAsrManager>>,
    asr_models: State<'_, Arc<AsrModelManager>>,
) -> Result<u64, String> {
    if manager.is_running() {
        return Err("a Native ASR session is already running".into());
    }
    let selected = get_settings(&app).selected_asr_model;
    let Some(gguf_path) = asr_models.get_gguf_path(&selected) else {
        return Err("Install and select a streaming model in Settings → Speech to Text".into());
    };

    // Mutual exclusion: free the inactive Batch/Rolling engine before starting.
    if let Some(lifecycle) = app.try_state::<Arc<engine_lifecycle_core::LifecycleManager>>() {
        lifecycle
            .prepare_load(
                engine_lifecycle_core::EngineSlot::NativeAsr,
                std::time::Instant::now(),
            )
            .map_err(|e| e.to_string())?;
    }

    // Open the mic so frames fan out to the (about-to-be-armed) Native ASR sink.
    recording.try_start_recording(NATIVE_ASR_BINDING)?;

    let session_id = manager.start(gguf_path, language_hint(&app));
    Ok(session_id)
}

/// Stop the streaming session: stop the mic, finalize, paste, save to history.
#[tauri::command]
#[specta::specta]
pub async fn stop_native_asr(
    app: AppHandle,
    recording: State<'_, Arc<AudioRecordingManager>>,
    manager: State<'_, Arc<NativeAsrManager>>,
    history: State<'_, Arc<HistoryManager>>,
) -> Result<Option<String>, String> {
    let _ = recording.stop_recording(NATIVE_ASR_BINDING);

    let Some(raw) = manager.stop() else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(Some(raw));
    }

    let settings = get_settings(&app);
    let finalized = finalize_transcript(
        &raw,
        &settings.custom_words,
        settings.word_correction_threshold,
        &settings.app_language,
        &settings.custom_filler_words,
        false,
    );
    let _ = crate::clipboard::paste(finalized.clone(), app.clone());
    if let Err(e) = history.save_entry(String::new(), finalized.clone(), false, None, None) {
        log::warn!("[GRAIN] failed to save Native ASR history entry: {e}");
    }
    Ok(Some(finalized))
}

/// Whether a Native ASR session is currently running.
#[tauri::command]
#[specta::specta]
pub async fn native_asr_running(manager: State<'_, Arc<NativeAsrManager>>) -> Result<bool, String> {
    Ok(manager.is_running())
}
