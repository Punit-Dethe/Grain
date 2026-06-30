//! [GRAIN] M4: Native ASR model commands (list / download / cancel / delete /
//! select). Mirrors `commands::models` but targets the separate ASR registry and
//! the `selected_asr_model` setting — `selected_model` is never touched.

use std::sync::Arc;

use grain_asr_core::model::{
    AsrBackendKind, AsrCapabilities, AsrModelFiles, AsrModelSpec, MemoryProfile,
};
use grain_asr_core::session::{ContextHints, NativeAsrBackend};
use grain_asr_core::testing::ScriptedBackend;
use grain_asr_core::AsrRawEvent;
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
    // Clear the selection if we're deleting the active ASR model.
    let settings = get_settings(&app_handle);
    if settings.selected_asr_model == model_id {
        let mut settings = settings;
        settings.selected_asr_model = String::new();
        write_settings(&app_handle, settings);
    }
    manager.delete(&model_id).map_err(|e| e.to_string())
}

/// Persist the selected Native ASR model. Validates it is installed first so the
/// Native ASR path never points at a missing bundle.
#[tauri::command]
#[specta::specta]
pub async fn select_asr_model(
    app_handle: AppHandle,
    manager: State<'_, Arc<AsrModelManager>>,
    model_id: String,
) -> Result<(), String> {
    if !model_id.is_empty() && manager.get_spec(&model_id).is_none() {
        return Err(format!("ASR model '{model_id}' is not installed"));
    }
    let mut settings = get_settings(&app_handle);
    settings.selected_asr_model = model_id;
    write_settings(&app_handle, settings);
    Ok(())
}

// ============================================================================
// M7: experimental end-to-end Native ASR route (behind `experimental_enabled`).
//
// This drives the FULL pipeline — mic → fan-out → worker → stabilizer → pill
// events → paste/history — with the SCRIPTED fake backend, so the UX path exists
// and is testable before the real Sherpa backend (Milestone 5) lands. Once a
// real backend is built and `selected_asr_model` is installed, `demo_backend`
// is replaced by resolving the selected model's `AsrModelSpec` + Sherpa backend.
// ============================================================================

/// The scripted backend + a dummy spec used for the M7 demo route. Emits an
/// incremental dictation as the live mic feeds frames, then a final.
fn demo_backend() -> (Box<dyn NativeAsrBackend>, AsrModelSpec) {
    let caps = AsrCapabilities {
        partials: true,
        immutable_final: true,
        endpointing: false,
        word_timestamps: false,
    };
    let p = |rev: u64, text: &str| AsrRawEvent::Partial {
        segment_id: 0,
        revision: rev,
        text: text.to_string(),
        words: vec![],
    };
    let script = vec![
        vec![p(0, "this")],
        vec![p(1, "this is")],
        vec![p(2, "this is a")],
        vec![p(3, "this is a native")],
        vec![p(4, "this is a native asr")],
        vec![p(5, "this is a native asr demo")],
        vec![AsrRawEvent::BackendFinal {
            segment_id: 0,
            text: "this is a native asr demo".into(),
            words: vec![],
        }],
    ];
    let spec = AsrModelSpec {
        id: "demo-fake".into(),
        name: "Demo (fake backend)".into(),
        backend: AsrBackendKind::SherpaOnnx,
        files: AsrModelFiles::SherpaTransducer {
            encoder: "e".into(),
            decoder: "d".into(),
            joiner: "j".into(),
            tokens: "t".into(),
            config: None,
        },
        sample_rate_hz: 16_000,
        languages: vec!["en".into()],
        capabilities: caps,
        memory: MemoryProfile { approx_mb: 0 },
    };
    (Box::new(ScriptedBackend::new(caps, script)), spec)
}

/// Resolve which backend + spec to run. With the `native-asr-sherpa` feature on
/// AND an installed `selected_asr_model`, use the real Sherpa-ONNX backend;
/// otherwise fall back to the scripted demo so the route always works.
#[cfg(feature = "native-asr-sherpa")]
pub(crate) fn resolve_backend(app: &AppHandle, asr: &AsrModelManager) -> (Box<dyn NativeAsrBackend>, AsrModelSpec) {
    let id = get_settings(app).selected_asr_model;
    if !id.is_empty() {
        if let Some(spec) = asr.get_spec(&id) {
            // Dispatch on the model's declared backend kind — exhaustive, so a
            // new AsrBackendKind forces a matching backend here.
            let backend: Box<dyn NativeAsrBackend> = match spec.backend {
                AsrBackendKind::SherpaOnnx => {
                    log::info!("[GRAIN] native ASR using sherpa-onnx model '{id}'");
                    Box::new(grain_asr_sherpa::SherpaOnnxBackend::new())
                }
            };
            return (backend, spec);
        }
        log::warn!("[GRAIN] selected ASR model '{id}' not installed; using demo backend");
    }
    demo_backend()
}

#[cfg(not(feature = "native-asr-sherpa"))]
pub(crate) fn resolve_backend(_app: &AppHandle, _asr: &AsrModelManager) -> (Box<dyn NativeAsrBackend>, AsrModelSpec) {
    demo_backend()
}

/// Start an experimental Native ASR session. Frees Batch/Rolling via the
/// lifecycle arbiter, opens the mic, and starts the worker (real Sherpa backend
/// when enabled + a model is installed, else the scripted demo).
#[tauri::command]
#[specta::specta]
pub async fn start_native_asr(
    app: AppHandle,
    recording: State<'_, Arc<AudioRecordingManager>>,
    manager: State<'_, Arc<NativeAsrManager>>,
    asr_models: State<'_, Arc<AsrModelManager>>,
) -> Result<u64, String> {
    if !get_settings(&app).experimental_enabled {
        return Err("Native ASR is experimental; enable Experimental features in settings".into());
    }
    if manager.is_running() {
        return Err("a Native ASR session is already running".into());
    }

    // Mutual exclusion: free the inactive Batch/Rolling engine before starting.
    if let Some(lifecycle) =
        app.try_state::<Arc<engine_lifecycle_core::LifecycleManager>>()
    {
        lifecycle
            .prepare_load(engine_lifecycle_core::EngineSlot::NativeAsr, std::time::Instant::now())
            .map_err(|e| e.to_string())?;
    }

    // Open the mic so frames fan out to the (about-to-be-armed) Native ASR sink.
    recording.try_start_recording(NATIVE_ASR_BINDING)?;

    let (backend, spec) = resolve_backend(&app, &asr_models);
    let session_id = manager.start(backend, spec, None, ContextHints::default());
    Ok(session_id)
}

/// Stop the experimental Native ASR session: stop the mic, finalize the
/// transcript, paste it, and save it to history. Returns the final text.
#[tauri::command]
#[specta::specta]
pub async fn stop_native_asr(
    app: AppHandle,
    recording: State<'_, Arc<AudioRecordingManager>>,
    manager: State<'_, Arc<NativeAsrManager>>,
    history: State<'_, Arc<HistoryManager>>,
) -> Result<Option<String>, String> {
    // Stop the mic (samples are discarded — Native ASR consumed frames live).
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
pub async fn native_asr_running(
    manager: State<'_, Arc<NativeAsrManager>>,
) -> Result<bool, String> {
    Ok(manager.is_running())
}
