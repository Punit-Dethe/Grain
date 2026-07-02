//! [GRAIN] Native ASR (live streaming) commands.
//!
//! Since the transcribe-cpp unification the streaming path shares the ONE
//! `TranscriptionManager` + `ModelManager` with Batch/Rolling — there is no
//! separate streaming registry anymore. These commands only expose the
//! streaming *view* of the unified catalog and the `selected_asr_model`
//! setting (`selected_model` is never touched). Download / cancel / delete go
//! through the unified `commands::models::*` commands.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::managers::model::{ModelInfo, ModelManager};
use crate::settings::{get_settings, write_settings};

/// The streaming slice of the unified model catalog (`supports_streaming`),
/// for the "Streaming model" section of Settings → Speech to Text.
#[tauri::command]
#[specta::specta]
pub async fn list_asr_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager
        .get_available_models()
        .into_iter()
        .filter(|m| m.supports_streaming)
        .collect())
}

/// Persist the selected streaming model. Accepts any catalog id (download may
/// happen afterward); the shortcut's start path checks it is actually on disk.
#[tauri::command]
#[specta::specta]
pub async fn select_asr_model(app_handle: AppHandle, model_id: String) -> Result<(), String> {
    let mut settings = get_settings(&app_handle);
    settings.selected_asr_model = model_id;
    write_settings(&app_handle, settings);
    Ok(())
}
