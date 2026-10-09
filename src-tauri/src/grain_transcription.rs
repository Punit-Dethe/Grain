//! Grain batch callers use the shared local model and final-text cleanup.
use crate::managers::transcription::TranscriptionManager;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Run local inference off the async runtime. Loading is idempotent; the shared
/// manager waits for an in-flight load and owns engine checkout and cleanup.
pub async fn transcribe(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    if !tm.is_model_loaded() {
        tm.initiate_model_load();
    }
    tokio::task::spawn_blocking(move || tm.transcribe(samples))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| e.to_string())
}
