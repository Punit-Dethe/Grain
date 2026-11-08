//! Tauri commands and the status event for Grain Meet.
//!
//! One meeting may record and one attempt may process at a time. A meeting
//! records its own sources (see `sources`), so dictation and its microphone
//! setting are untouched while it records. Processing holds the exclusive
//! inference lease, so dictation's model is released for its duration and
//! restored afterwards.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use super::analysis;
use super::processing::{self, Models, Stage};
use super::recorder::{DeviceOpener, Recording, SourceKind};
use super::sources::{self, SourceDevice, SourceSettings};
use super::storage::{self, SessionState};
use super::transcript::{self, TranscriptRevision};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;

/// The diarizer: Nemotron 3 Diarization Q8_0 (Streaming Sortformer v3, 8
/// speakers, OpenMDW-1.1), the model family FluidMeet and Glimpse use. Pinned
/// by revision, size and SHA-256.
const DIARIZER_FILE: &str = "nemotron-3-diarization-Q8_0.gguf";
const DIARIZER_URL: &str = "https://huggingface.co/Glimpse-Dictation/Nemotron-3-Diarization-gguf/resolve/273ebb0e65377f74576768257cd11f00c248c39d/nemotron-3-diarization-Q8_0.gguf";
const DIARIZER_SHA256: &str = "877ff9e77e829e30158528cfaabf56188fca11d05349e09821b3033a97d31688";
const DIARIZER_BYTES: u64 = 105_936_416;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SourceStatus {
    pub kind: SourceKind,
    /// The device being recorded.
    pub name: String,
    pub recorded_ms: u64,
    /// Peak audio level since the previous status read, 0 to 1.
    pub level: f32,
    /// False while this source is not delivering audio.
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RecordingStatus {
    pub session_id: String,
    pub elapsed_ms: u64,
    pub sources: Vec<SourceStatus>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ProcessingStatus {
    pub session_id: String,
    pub stage: Stage,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MeetStatus {
    pub recording: Option<RecordingStatus>,
    pub processing: Option<ProcessingStatus>,
    /// The last processing failure, until the next attempt starts.
    pub last_error: Option<String>,
}

/// Grain Meet state changed; the payload is the full current status.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MeetStatusChanged(pub MeetStatus);

/// Diarizer download progress in bytes.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MeetDiarizerProgress {
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub created_unix_ms: u64,
    pub state: SessionState,
    pub duration_ms: u64,
    pub transcript_revision: Option<u32>,
    /// The opening words of the newest transcript, for the list.
    pub preview: Option<String>,
    /// Speakers in the newest transcript (0 before transcription).
    pub speaker_count: u32,
    /// What each track recorded, in track order.
    pub sources: Vec<SourceKind>,
}

/// Recall's audio sources and what this device can offer.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MeetSources {
    pub settings: SourceSettings,
    pub microphones: Vec<SourceDevice>,
    pub outputs: Vec<SourceDevice>,
    pub system_audio_supported: bool,
}

struct Attempt {
    session_id: String,
    attempt_id: u64,
    cancelled: Arc<AtomicBool>,
    stage: Stage,
}

#[derive(Default)]
struct State {
    active: Option<Recording>,
    /// A start is opening its devices; a second start is refused meanwhile.
    starting: bool,
    processing: Option<Attempt>,
    last_error: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    active: None,
    starting: false,
    processing: None,
    last_error: None,
});
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(1);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn meetings_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("meetings"))
}

fn diarizer_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("models")
        .join("grain-meet")
        .join(DIARIZER_FILE))
}

fn snapshot(state: &State) -> MeetStatus {
    MeetStatus {
        recording: state.active.as_ref().map(|recording| RecordingStatus {
            session_id: recording.session_id.clone(),
            elapsed_ms: recording.started.elapsed().as_millis() as u64,
            sources: recording
                .progress
                .sources
                .iter()
                .map(|source| SourceStatus {
                    kind: source.kind,
                    name: source.name.clone(),
                    recorded_ms: source.frames_ns.load(Ordering::Acquire) / 1_000_000,
                    level: f32::from_bits(source.level.swap(0, Ordering::AcqRel)),
                    active: source.active.load(Ordering::Acquire),
                })
                .collect(),
            error: recording
                .progress
                .error
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }),
        processing: state.processing.as_ref().map(|attempt| ProcessingStatus {
            session_id: attempt.session_id.clone(),
            stage: attempt.stage,
        }),
        last_error: state.last_error.clone(),
    }
}

fn emit_status(app: &AppHandle) {
    let status = snapshot(&state());
    let _ = MeetStatusChanged(status).emit(app);
}

fn summary(dir: &std::path::Path, manifest: &storage::Manifest) -> Result<MeetingSummary, String> {
    // A broken sidecar never hides a meeting from the list.
    let preview = transcript::preview(dir).ok().flatten();
    Ok(MeetingSummary {
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        created_unix_ms: manifest.created_unix_ms,
        state: manifest.state,
        duration_ms: manifest.duration_ns() / 1_000_000,
        transcript_revision: transcript::latest_revision(dir)?,
        speaker_count: preview.as_ref().map_or(0, |p| p.speakers),
        preview: preview.map(|p| p.text).filter(|text| !text.is_empty()),
        sources: manifest
            .tracks
            .iter()
            .map(|track| match track.source {
                storage::TrackSource::Microphone { .. } => SourceKind::Microphone,
                _ => SourceKind::SystemAudio,
            })
            .collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn meet_status() -> MeetStatus {
    snapshot(&state())
}

/// Recall's source settings with the devices available right now. Listing
/// devices can stall on some systems, so it runs off the async runtime.
#[tauri::command]
#[specta::specta]
pub async fn meet_sources(app: AppHandle) -> Result<MeetSources, String> {
    let root = meetings_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(MeetSources {
            settings: sources::load_settings(&root),
            microphones: sources::list_microphones().unwrap_or_default(),
            outputs: sources::list_outputs().unwrap_or_default(),
            system_audio_supported: sources::system_audio_supported(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save Recall's source settings. Dictation settings are never touched.
#[tauri::command]
#[specta::specta]
pub fn meet_set_sources(app: AppHandle, settings: SourceSettings) -> Result<(), String> {
    settings.validate()?;
    sources::save_settings(&meetings_root(&app)?, &settings)
}

/// List meetings, newest first, recovering any interrupted by a crash.
#[tauri::command]
#[specta::specta]
pub fn meet_list(app: AppHandle) -> Result<Vec<MeetingSummary>, String> {
    let root = meetings_root(&app)?;
    let active = state().active.as_ref().map(|r| r.session_id.clone());
    let mut out = Vec::new();
    for listed in storage::list_sessions(&root)? {
        let dir = storage::session_dir(&root, &listed.id)?;
        let manifest = if listed.state == SessionState::Recording
            && active.as_deref() != Some(listed.id.as_str())
        {
            match storage::recover(&dir) {
                Ok((manifest, report)) => {
                    log::info!("Recovered interrupted meeting {}: {report:?}", manifest.id);
                    manifest
                }
                Err(error) => {
                    log::warn!("Could not recover meeting {}: {error}", listed.id);
                    continue;
                }
            }
        } else {
            match storage::load_manifest(&dir) {
                Ok(manifest) => manifest,
                Err(_) => continue,
            }
        };
        out.push(summary(&dir, &manifest)?);
    }
    Ok(out)
}

/// Start recording the enabled sources into a new meeting. Returns its id.
#[tauri::command]
#[specta::specta]
pub async fn meet_start_recording(app: AppHandle, title: Option<String>) -> Result<String, String> {
    {
        let mut guard = state();
        if guard.active.is_some() || guard.starting {
            return Err("A meeting is already recording".into());
        }
        guard.starting = true;
    }
    struct Starting;
    impl Drop for Starting {
        fn drop(&mut self) {
            state().starting = false;
        }
    }
    let _starting = Starting;
    let root = meetings_root(&app)?;
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Meeting".to_string());
    // Opening devices can block for seconds (Bluetooth); not on the runtime.
    let recording = tauri::async_runtime::spawn_blocking(move || {
        let settings = sources::load_settings(&root);
        settings.validate()?;
        let mut kinds = Vec::new();
        if settings.microphone.enabled {
            kinds.push((
                SourceKind::Microphone,
                sources::microphone_name(&settings.microphone),
            ));
        }
        if settings.system_audio.enabled {
            kinds.push((
                SourceKind::SystemAudio,
                sources::output_name(settings.system_audio.device_id.as_deref()),
            ));
        }
        Recording::begin(&root, &title, &kinds, DeviceOpener(settings))
    })
    .await
    .map_err(|e| e.to_string())??;
    let id = recording.session_id.clone();
    {
        let mut guard = state();
        guard.active = Some(recording);
        guard.starting = false;
    }
    emit_status(&app);
    Ok(id)
}

/// Stop the current recording and seal its session.
#[tauri::command]
#[specta::specta]
pub async fn meet_stop_recording(app: AppHandle) -> Result<MeetingSummary, String> {
    let recording = state().active.take().ok_or("No meeting is recording")?;
    let result = tauri::async_runtime::spawn_blocking(move || recording.finish())
        .await
        .map_err(|e| e.to_string())?;
    emit_status(&app);
    let manifest = result?;
    let dir = storage::session_dir(&meetings_root(&app)?, &manifest.id)?;
    summary(&dir, &manifest)
}

/// Start processing a recorded meeting in the background.
#[tauri::command]
#[specta::specta]
pub fn meet_process(app: AppHandle, session_id: String) -> Result<(), String> {
    let root = meetings_root(&app)?;
    let dir = storage::session_dir(&root, &session_id)?;
    if storage::load_manifest(&dir)?.state != SessionState::Recorded {
        return Err("Finish the recording before processing it".into());
    }
    let diarizer = diarizer_path(&app)?;
    if !diarizer.is_file() {
        return Err("Download the speaker model first".into());
    }
    let model_id = crate::settings::get_settings(&app).selected_model;
    crate::tdt_flow::validate_model(&model_id, false).map_err(|_| {
        "Meeting transcription needs a Parakeet TDT 0.6B v2 or v3 model selected".to_string()
    })?;
    let model_manager = app.state::<Arc<ModelManager>>();
    if !model_manager
        .get_model_info(&model_id)
        .is_some_and(|info| info.is_downloaded)
    {
        return Err("Download the selected transcription model first".into());
    }
    let asr = model_manager
        .get_model_path(&model_id)
        .map_err(|e| e.to_string())?;
    let attempt_id = NEXT_ATTEMPT.fetch_add(1, Ordering::AcqRel);
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state();
        if guard.processing.is_some() {
            return Err("Another meeting is being processed".into());
        }
        guard.processing = Some(Attempt {
            session_id: session_id.clone(),
            attempt_id,
            cancelled: Arc::clone(&cancelled),
            stage: Stage::Preparing,
        });
        guard.last_error = None;
    }
    emit_status(&app);
    let thread_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("grain-meet-process".into())
        .spawn(move || {
            let app = thread_app;
            let models = Models { diarizer, asr };
            let manager = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
            let on_stage = |stage: Stage| {
                if let Some(attempt) = state()
                    .processing
                    .as_mut()
                    .filter(|a| a.attempt_id == attempt_id)
                {
                    attempt.stage = stage;
                }
                emit_status(&app);
            };
            let result = manager.with_grain_meeting_engine(attempt_id, &cancelled, |engine| {
                processing::process(&dir, engine, &models, &attempt_id.to_string(), &on_stage)
                    .map_err(anyhow::Error::msg)
            });
            {
                let mut guard = state();
                guard.processing = None;
                guard.last_error = match result {
                    Ok(revision) => {
                        log::info!(
                            "Meeting {} transcript revision {} published",
                            revision.session_id,
                            revision.revision
                        );
                        None
                    }
                    Err(error) if cancelled.load(Ordering::Acquire) => {
                        log::info!("Meeting processing cancelled: {error}");
                        None
                    }
                    Err(error) => Some(error.to_string()),
                };
            }
            emit_status(&app);
        });
    if let Err(error) = spawned {
        state().processing = None;
        emit_status(&app);
        return Err(format!("Could not start processing: {error}"));
    }
    Ok(())
}

/// Stop the current processing attempt. Its recording is kept.
#[tauri::command]
#[specta::specta]
pub fn meet_cancel_processing(app: AppHandle) -> Result<(), String> {
    let attempt = {
        let guard = state();
        let attempt = guard
            .processing
            .as_ref()
            .ok_or("No meeting is being processed")?;
        attempt.cancelled.store(true, Ordering::Release);
        attempt.attempt_id
    };
    app.state::<Arc<TranscriptionManager>>()
        .cancel_grain_meeting(attempt);
    Ok(())
}

/// The newest published transcript of a meeting, if any.
#[tauri::command]
#[specta::specta]
pub fn meet_transcript(
    app: AppHandle,
    session_id: String,
) -> Result<Option<TranscriptRevision>, String> {
    transcript::latest(&storage::session_dir(&meetings_root(&app)?, &session_id)?)
}

/// Delete a meeting's audio and transcripts. Refused while it is in use.
#[tauri::command]
#[specta::specta]
pub fn meet_delete(app: AppHandle, session_id: String) -> Result<(), String> {
    let dir = storage::session_dir(&meetings_root(&app)?, &session_id)?;
    {
        let guard = state();
        if guard
            .active
            .as_ref()
            .is_some_and(|r| r.session_id == session_id)
        {
            return Err("Stop the recording before deleting it".into());
        }
        if guard
            .processing
            .as_ref()
            .is_some_and(|a| a.session_id == session_id)
        {
            return Err("Cancel processing before deleting the meeting".into());
        }
    }
    // Only a real session directory is ever removed.
    storage::load_manifest(&dir).map(|_| ()).or_else(|error| {
        if dir.join("manifest.json").exists() {
            Ok(())
        } else {
            Err(error)
        }
    })?;
    std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the meeting: {e}"))
}

/// The meeting's playback file (created on first request): every track mixed
/// on the meeting clock. Only that one file is opened to the webview.
#[tauri::command]
#[specta::specta]
pub async fn meet_playback_audio(app: AppHandle, session_id: String) -> Result<String, String> {
    let dir = storage::session_dir(&meetings_root(&app)?, &session_id)?;
    let path = tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, String> {
        let manifest = storage::load_manifest(&dir)?;
        if manifest.state != SessionState::Recorded {
            return Err("Finish the recording before playing it".into());
        }
        let folder = dir.join("playback");
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not prepare playback audio: {e}"))?;
        let path = folder.join("meeting.wav");
        if !path.is_file() {
            analysis::write_meeting_playback(&dir, &manifest.tracks, &path)?;
        }
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|e| e.to_string())?;
    {
        use tauri_plugin_fs::FsExt;
        let _ = app.fs_scope().allow_file(&path);
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Whether the pinned speaker model is installed.
#[tauri::command]
#[specta::specta]
pub fn meet_diarizer_installed(app: AppHandle) -> Result<bool, String> {
    Ok(diarizer_path(&app)?.is_file())
}

/// Download the pinned speaker model, verifying its size and SHA-256 before
/// it is installed.
#[tauri::command]
#[specta::specta]
pub async fn meet_download_diarizer(app: AppHandle) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let path = diarizer_path(&app)?;
    if path.is_file() {
        return Ok(());
    }
    if DOWNLOADING.swap(true, Ordering::AcqRel) {
        return Err("The speaker model is already downloading".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            DOWNLOADING.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    let dir = path
        .parent()
        .expect("model path has a folder")
        .to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create the model folder: {e}"))?;
    let partial = dir.join(format!("{DIARIZER_FILE}.partial"));
    let client = app.state::<reqwest::Client>().inner().clone();
    let response = client
        .get(DIARIZER_URL)
        .timeout(std::time::Duration::from_secs(30 * 60))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Could not download the speaker model: {e}"))?;
    let mut file = std::fs::File::create(&partial)
        .map_err(|e| format!("Could not save the speaker model: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    let result: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("The speaker model download failed: {e}"))?;
            downloaded += chunk.len() as u64;
            if downloaded > DIARIZER_BYTES {
                return Err("The speaker model is larger than expected".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .map_err(|e| format!("Could not save the speaker model: {e}"))?;
            let _ = MeetDiarizerProgress {
                downloaded,
                total: DIARIZER_BYTES,
            }
            .emit(&app);
        }
        file.sync_all()
            .map_err(|e| format!("Could not save the speaker model: {e}"))?;
        let digest = format!("{:x}", hasher.finalize());
        if downloaded != DIARIZER_BYTES || digest != DIARIZER_SHA256 {
            return Err("The downloaded speaker model failed verification".into());
        }
        Ok(())
    }
    .await;
    drop(file);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    std::fs::rename(&partial, &path)
        .map_err(|e| format!("Could not install the speaker model: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_are_stable() {
        assert_eq!(MeetStatusChanged::NAME, "meet-status-changed");
        assert_eq!(MeetDiarizerProgress::NAME, "meet-diarizer-progress");
    }

    #[test]
    fn idle_status_is_empty() {
        let status = snapshot(&State::default());
        assert!(
            status.recording.is_none()
                && status.processing.is_none()
                && status.last_error.is_none()
        );
    }
}
