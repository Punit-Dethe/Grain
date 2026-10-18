//! Grain batch callers use the shared local model and final-text cleanup.
use crate::managers::transcription::TranscriptionManager;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Use {
    Load,
    Batch,
    Stream,
    Flow(u64),
}

#[derive(Default)]
pub(crate) struct State {
    owner: std::sync::Mutex<(u64, Option<Use>)>,
}

pub(crate) struct Lease {
    state: Arc<State>,
    ticket: u64,
}

impl State {
    pub(crate) fn claim(self: &Arc<Self>, usage: Use) -> Result<Lease, String> {
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.1.is_some() {
            return Err("The transcription model is already in use".into());
        }
        owner.0 = owner
            .0
            .checked_add(1)
            .ok_or("Inference ownership counter exhausted")?;
        owner.1 = Some(usage);
        Ok(Lease {
            state: Arc::clone(self),
            ticket: owner.0,
        })
    }

    pub(crate) fn cancel_flow(&self, session_id: u64, cancel: impl FnOnce()) {
        let owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.1 == Some(Use::Flow(session_id)) {
            cancel();
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut owner = self
            .state
            .owner
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if owner.0 == self.ticket {
            owner.1 = None;
        }
    }
}

pub(crate) struct FlowSession<'a> {
    pub engine: &'a crate::engine_supervisor::EngineSupervisor,
    pub mark: crate::engine_supervisor::CancelMark,
    pub cancelled: &'a std::sync::atomic::AtomicBool,
}

impl FlowSession<'_> {
    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err("Flow session cancelled".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for FlowSession<'_> {
    fn drop(&mut self) {
        // Still inside the exclusive inference lease. The epoch gate rejects
        // stale cleanup after cancellation, so it cannot reset a new owner.
        let _ = self
            .engine
            .grain_request(self.mark, crate::grain_engine::Request::ResetFlow);
    }
}

impl crate::tdt_flow::Inference for FlowSession<'_> {
    fn flow_model(&self) -> Result<crate::tdt_flow::FlowModel, String> {
        self.check_cancelled()?;
        match self
            .engine
            .grain_request(self.mark, crate::grain_engine::Request::FlowModel)
            .map_err(|error| error.to_string())?
        {
            crate::grain_engine::Response::FlowModel {
                arch,
                variant,
                supported,
            } => Ok(crate::tdt_flow::FlowModel {
                arch,
                variant,
                supported,
            }),
            _ => Err("Worker returned an unexpected Flow model response".into()),
        }
    }

    fn run(
        &mut self,
        audio: &[f32],
        options: &transcribe_cpp::RunOptions,
    ) -> Result<transcribe_cpp::Transcript, String> {
        self.check_cancelled()?;
        // Each copy is bounded by the production window ceiling, independent of
        // recording length. The worker transport owns queued audio.
        self.engine
            .transcribe_since(self.mark, audio.to_vec(), options.clone())
            .map_err(|error| error.to_string())
    }

    fn detokenize(&self, tokens: &[i32]) -> Result<String, String> {
        self.check_cancelled()?;
        match self
            .engine
            .grain_request(
                self.mark,
                crate::grain_engine::Request::Detokenize {
                    tokens: tokens.to_vec(),
                },
            )
            .map_err(|error| error.to_string())?
        {
            crate::grain_engine::Response::Text(text) => Ok(text),
            _ => Err("Worker returned an unexpected detokenization response".into()),
        }
    }
}

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

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn exclusive_lease_releases_and_stale_flow_cancel_cannot_kill_new_owner() {
        let state = Arc::new(State::default());
        let lease = state.claim(Use::Flow(41)).unwrap();
        assert!(state.claim(Use::Load).is_err());
        assert!(state.claim(Use::Batch).is_err());
        assert!(state.claim(Use::Stream).is_err());
        let cancelled = std::cell::Cell::new(false);
        state.cancel_flow(40, || cancelled.set(true));
        assert!(!cancelled.get());
        state.cancel_flow(41, || cancelled.set(true));
        assert!(cancelled.get());
        drop(lease);
        let _new = state.claim(Use::Flow(42)).unwrap();
        state.cancel_flow(41, || panic!("stale cancellation reached the replacement"));
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_chinese_script_setting(
    app: AppHandle,
    script: grain_core::settings::ChineseScript,
) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.chinese_script = script)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_vad_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.vad_enabled = enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_long_form_chunking_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.long_form_chunking = enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn change_vad_backend_setting(
    app: AppHandle,
    backend: grain_core::settings::VadBackend,
) -> Result<(), String> {
    let manager = Arc::clone(&app.state::<Arc<crate::managers::audio::AudioRecordingManager>>());
    tauri::async_runtime::spawn_blocking(move || {
        let context = app.state::<Arc<grain_core::AppContext>>();
        let previous = context.settings().vad_backend;
        manager
            .update_vad_backend(backend)
            .map_err(|error| error.to_string())?;
        if let Err(error) = context.update_settings(|settings| settings.vad_backend = backend) {
            let _ = context.update_settings(|settings| settings.vad_backend = previous);
            let _ = manager.update_vad_backend(previous);
            return Err(error.to_string());
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}
