//! Settings-only file import policy. Decoding/chunking remain Handy-owned.
use crate::managers::{
    audio::AudioRecordingManager,
    transcription_jobs::{TranscriptionJob, TranscriptionJobManager},
};
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
use tauri::{AppHandle, Manager, State};

static DECODING: Mutex<()> = Mutex::new(());
pub(crate) const MAX_AUDIO_SECONDS: usize = 60 * 60;

pub(crate) fn decode_guard() -> anyhow::Result<MutexGuard<'static, ()>> {
    DECODING.try_lock().map_err(|_| {
        anyhow::anyhow!("Another file is being imported; wait for it to finish reading")
    })
}

pub(crate) fn check_decode_length(frames: usize, rate: u32) -> anyhow::Result<()> {
    if rate == 0 || frames as u64 > MAX_AUDIO_SECONDS as u64 * rate as u64 {
        anyhow::bail!("Audio files must be no longer than 60 minutes");
    }
    Ok(())
}

pub(crate) fn validate_import(app: &AppHandle, path: &Path) -> anyhow::Result<()> {
    if !path.is_file() {
        anyhow::bail!("Choose a regular audio file");
    }
    let model = crate::settings::get_settings(app).selected_model;
    crate::grain_capture::require_transcribable_model(app, &model).map_err(anyhow::Error::msg)
}

pub(crate) fn dictation_in_progress(app: &AppHandle) -> bool {
    app.try_state::<crate::tray::CurrentTrayIconState>()
        .is_some_and(|state| state.get() != crate::tray::TrayIconState::Idle)
        || app
            .try_state::<Arc<AudioRecordingManager>>()
            .is_some_and(|audio| audio.is_recording())
}

#[tauri::command]
#[specta::specta]
pub fn get_transcription_jobs(
    jobs: State<'_, Arc<TranscriptionJobManager>>,
) -> Vec<TranscriptionJob> {
    jobs.grain_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_limit_is_sample_rate_independent() {
        for rate in [16_000, 44_100, 48_000] {
            let limit = MAX_AUDIO_SECONDS * rate as usize;
            assert!(check_decode_length(limit, rate).is_ok());
            assert!(check_decode_length(limit + 1, rate).is_err());
        }
        assert!(check_decode_length(0, 0).is_err());
    }
}
