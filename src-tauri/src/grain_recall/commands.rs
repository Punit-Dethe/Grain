//! Tauri commands and the status event for Grain Recall.
//!
//! One meeting may record and one attempt may process at a time. Recording
//! uses the shared microphone, so dictation and a meeting refuse each other.
//! Processing holds the exclusive inference lease, so dictation's model is
//! released for its duration and restored afterwards.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use super::processing::{self, Models, Stage};
use super::recorder::{self, Recording};
use super::storage::{self, SessionState};
use super::transcript::{self, TranscriptRevision};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;

/// The recorder owner id for meeting capture.
const RECALL_BINDING: &str = "grain_recall";

/// The first-release diarizer: Sortformer v2.1 Q8_0, pinned by revision and hash.
const DIARIZER_FILE: &str = "diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf";
const DIARIZER_URL: &str = "https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf/resolve/60ebab3ef7cae9869be0abcde13360347e96ba03/diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf";
const DIARIZER_SHA256: &str = "a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9";
const DIARIZER_BYTES: u64 = 139_310_336;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RecordingStatus {
    pub session_id: String,
    pub elapsed_ms: u64,
    pub recorded_ms: u64,
    /// False while the microphone is not delivering audio.
    pub source_active: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ProcessingStatus {
    pub session_id: String,
    pub stage: Stage,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RecallStatus {
    pub recording: Option<RecordingStatus>,
    pub processing: Option<ProcessingStatus>,
    /// The last processing failure, until the next attempt starts.
    pub last_error: Option<String>,
}

/// Grain Recall state changed; the payload is the full current status.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RecallStatusChanged(pub RecallStatus);

/// Diarizer download progress in bytes.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RecallDiarizerProgress {
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
}

struct Active {
    recording: Recording,
    live: Arc<Mutex<bool>>,
}

struct Attempt {
    session_id: String,
    attempt_id: u64,
    cancelled: Arc<AtomicBool>,
    stage: Stage,
}

#[derive(Default)]
struct State {
    active: Option<Active>,
    processing: Option<Attempt>,
    last_error: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    active: None,
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
        .join("grain-recall")
        .join(DIARIZER_FILE))
}

fn snapshot(state: &State) -> RecallStatus {
    RecallStatus {
        recording: state.active.as_ref().map(|active| {
            let progress = &active.recording.progress;
            RecordingStatus {
                session_id: active.recording.session_id.clone(),
                elapsed_ms: active.recording.started.elapsed().as_millis() as u64,
                recorded_ms: progress.frames.load(Ordering::Acquire) * 1_000
                    / u64::from(recorder::MIC_RATE),
                source_active: progress.source_active.load(Ordering::Acquire),
                error: progress
                    .error
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            }
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
    let _ = RecallStatusChanged(status).emit(app);
}

/// The shared microphone, seen from the meeting writer. `live` is held while
/// starting or stopping, so a restart can never revive a stopped meeting.
struct MicSource {
    app: AppHandle,
    live: Arc<Mutex<bool>>,
}

impl recorder::Source for MicSource {
    fn restart(&self) -> Result<(), String> {
        let live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        if !*live {
            return Ok(());
        }
        let audio = self.app.state::<Arc<AudioRecordingManager>>();
        if audio.grain_recording_owner_is(RECALL_BINDING) {
            audio.stop_recording(RECALL_BINDING, audio.cancel_generation());
        }
        audio
            .try_start_meeting_recording(RECALL_BINDING)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn stop(&self) {
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        *live = false;
        stop_microphone(&self.app);
    }
}

fn stop_microphone(app: &AppHandle) {
    let audio = app.state::<Arc<AudioRecordingManager>>();
    if audio.grain_recording_owner_is(RECALL_BINDING) {
        audio.stop_recording(RECALL_BINDING, audio.cancel_generation());
    }
}

#[tauri::command]
#[specta::specta]
pub fn recall_status() -> RecallStatus {
    snapshot(&state())
}

/// List meetings, newest first, recovering any interrupted by a crash.
#[tauri::command]
#[specta::specta]
pub fn recall_list(app: AppHandle) -> Result<Vec<MeetingSummary>, String> {
    let root = meetings_root(&app)?;
    let active = state()
        .active
        .as_ref()
        .map(|a| a.recording.session_id.clone());
    let mut out = Vec::new();
    for summary in storage::list_sessions(&root)? {
        let dir = storage::session_dir(&root, &summary.id)?;
        let summary = if summary.state == SessionState::Recording
            && active.as_deref() != Some(summary.id.as_str())
        {
            match storage::recover(&dir) {
                Ok((manifest, report)) => {
                    log::info!("Recovered interrupted meeting {}: {report:?}", manifest.id);
                    storage::SessionSummary {
                        duration_ms: manifest.duration_ns() / 1_000_000,
                        state: manifest.state,
                        ..summary
                    }
                }
                Err(error) => {
                    log::warn!("Could not recover meeting {}: {error}", summary.id);
                    summary
                }
            }
        } else {
            summary
        };
        out.push(MeetingSummary {
            transcript_revision: transcript::latest_revision(&dir)?,
            id: summary.id,
            title: summary.title,
            created_unix_ms: summary.created_unix_ms,
            state: summary.state,
            duration_ms: summary.duration_ms,
        });
    }
    Ok(out)
}

/// Start recording the microphone into a new meeting. Returns its id.
#[tauri::command]
#[specta::specta]
pub fn recall_start_recording(app: AppHandle, title: Option<String>) -> Result<String, String> {
    // Serialized with dictation startup, which checks the same recorder.
    let _start = crate::grain_actions::capture_start_guard();
    if crate::grain_actions::dictation_start_blocked(&app) {
        return Err("The microphone is in use by dictation or Agent; finish it first".into());
    }
    let mut guard = state();
    if guard.active.is_some() {
        return Err("A meeting is already recording".into());
    }
    let root = meetings_root(&app)?;
    let settings = crate::settings::get_settings(&app);
    let device = settings
        .selected_microphone
        .clone()
        .unwrap_or_else(|| "Default microphone".to_string());
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Meeting".to_string());
    let live = Arc::new(Mutex::new(true));
    let recording = Recording::begin(
        &root,
        &title,
        device,
        MicSource {
            app: app.clone(),
            live: Arc::clone(&live),
        },
    )?;
    let started = {
        let _live = live.lock().unwrap_or_else(|e| e.into_inner());
        app.state::<Arc<AudioRecordingManager>>()
            .try_start_meeting_recording(RECALL_BINDING)
    };
    if let Err(error) = started {
        *live.lock().unwrap_or_else(|e| e.into_inner()) = false;
        let id = recording.session_id.clone();
        drop(recording);
        let _ = std::fs::remove_dir_all(storage::session_dir(&root, &id)?);
        return Err(format!("Could not start the microphone: {error}"));
    }
    let id = recording.session_id.clone();
    guard.active = Some(Active { recording, live });
    drop(guard);
    emit_status(&app);
    Ok(id)
}

/// Stop the current recording and seal its session.
#[tauri::command]
#[specta::specta]
pub fn recall_stop_recording(app: AppHandle) -> Result<MeetingSummary, String> {
    let active = state().active.take().ok_or("No meeting is recording")?;
    {
        let mut live = active.live.lock().unwrap_or_else(|e| e.into_inner());
        *live = false;
        stop_microphone(&app);
    }
    let result = active.recording.finish();
    emit_status(&app);
    let manifest = result?;
    let root = meetings_root(&app)?;
    Ok(MeetingSummary {
        transcript_revision: transcript::latest_revision(&storage::session_dir(
            &root,
            &manifest.id,
        )?)?,
        duration_ms: manifest.duration_ns() / 1_000_000,
        id: manifest.id,
        title: manifest.title,
        created_unix_ms: manifest.created_unix_ms,
        state: manifest.state,
    })
}

/// Start processing a recorded meeting in the background.
#[tauri::command]
#[specta::specta]
pub fn recall_process(app: AppHandle, session_id: String) -> Result<(), String> {
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
        .name("grain-recall-process".into())
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
            let result = manager.with_grain_recalling_engine(attempt_id, &cancelled, |engine| {
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
pub fn recall_cancel_processing(app: AppHandle) -> Result<(), String> {
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
        .cancel_grain_recalling(attempt);
    Ok(())
}

/// The newest published transcript of a meeting, if any.
#[tauri::command]
#[specta::specta]
pub fn recall_transcript(
    app: AppHandle,
    session_id: String,
) -> Result<Option<TranscriptRevision>, String> {
    transcript::latest(&storage::session_dir(&meetings_root(&app)?, &session_id)?)
}

/// Delete a meeting's audio and transcripts. Refused while it is in use.
#[tauri::command]
#[specta::specta]
pub fn recall_delete(app: AppHandle, session_id: String) -> Result<(), String> {
    let dir = storage::session_dir(&meetings_root(&app)?, &session_id)?;
    {
        let guard = state();
        if guard
            .active
            .as_ref()
            .is_some_and(|a| a.recording.session_id == session_id)
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

/// Whether the pinned speaker model is installed.
#[tauri::command]
#[specta::specta]
pub fn recall_diarizer_installed(app: AppHandle) -> Result<bool, String> {
    Ok(diarizer_path(&app)?.is_file())
}

/// Download the pinned speaker model, verifying its size and SHA-256 before
/// it is installed.
#[tauri::command]
#[specta::specta]
pub async fn recall_download_diarizer(app: AppHandle) -> Result<(), String> {
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
            let _ = RecallDiarizerProgress {
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
        assert_eq!(RecallStatusChanged::NAME, "recall-status-changed");
        assert_eq!(RecallDiarizerProgress::NAME, "recall-diarizer-progress");
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
