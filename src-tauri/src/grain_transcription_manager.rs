// Included into Handy's manager module to access its existing loading and
// worker state. Grain policy remains in this owned source file.
impl TranscriptionManager {
    pub(crate) fn grain_has_resident_model(&self) -> bool {
        self.engine.grain_has_resident_model()
    }
    pub(crate) fn grain_is_loading(&self) -> bool {
        *self.is_loading.lock().unwrap()
    }

    pub(crate) fn initiate_model_load_for(&self, model_id: String) {
        if !self.job_active() // A background chunk can still replace this model before it yields.
            && self.is_model_loaded()
            && self.get_current_model().as_deref() == Some(&model_id)
            && !self.reload_model_on_next_use.load(Ordering::Acquire)
        {
            return;
        }
        let Some(loading) = self.try_start_loading() else {
            return;
        };
        let manager = self.clone();
        thread::spawn(move || {
            let _loading = loading;
            manager
                .reload_model_on_next_use
                .store(false, Ordering::Release);
            if let Err(error) = manager.load_model(&model_id) {
                error!("Failed to load selected model: {error}");
            }
        });
    }

    pub(crate) fn grain_transcribe_cpp_language_for_model(
        &self,
        intent: &str,
        model_id: &str,
    ) -> Option<String> {
        let info = self.model_manager.get_model_info(model_id)?;
        let language = crate::managers::model::effective_language(
            intent,
            &[],
            &info.supported_languages,
            info.supports_language_detection,
        );
        transcribe_cpp_run_plan(false, &language, &info.supported_languages, false).language
    }

    pub(crate) fn with_grain_flow_session<T>(
        &self,
        expected_model: &str,
        session_id: u64,
        cancelled: &AtomicBool,
        f: impl FnOnce(&mut dyn crate::tdt_flow::Inference) -> Result<T>,
    ) -> Result<T> {
        self.touch_activity();
        {
            let mut loading = self.is_loading.lock().unwrap();
            while *loading {
                if cancelled.load(Ordering::Acquire) {
                    anyhow::bail!("Flow session cancelled");
                }
                loading = self
                    .loading_condvar
                    .wait_timeout(loading, Duration::from_millis(50))
                    .unwrap()
                    .0;
            }
        }
        let _lease = self
            .grain_claim_foreground(crate::grain_transcription::Use::Flow(session_id))
            .map_err(anyhow::Error::msg)?;
        if !self.is_model_loaded() || self.get_current_model().as_deref() != Some(expected_model) {
            anyhow::bail!("The selected Flow model is not loaded");
        }
        let mark = self.engine.cancel_mark();
        let mut session = crate::grain_transcription::FlowSession {
            engine: &self.engine,
            mark,
            cancelled,
        };
        f(&mut session)
    }

    pub(crate) fn cancel_grain_flow(&self, session_id: u64) {
        self.grain.cancel_flow(session_id, || self.engine.cancel());
    }

    /// Run one meeting processing attempt with exclusive ownership of the
    /// shared engine. The dictation model is released first so only one model
    /// is ever resident; `f` loads each role it needs through the
    /// [`MeetingEngine`](crate::grain_transcription::MeetingEngine). On every
    /// exit, success, error or panic, the last role is unloaded and the lease
    /// released before normal readiness is restored from current settings.
    #[allow(dead_code)] // Wired by the saved-audio pipeline (Step 3).
    pub(crate) fn with_grain_meeting_engine<T>(
        &self,
        attempt_id: u64,
        cancelled: &AtomicBool,
        f: impl FnOnce(&crate::grain_transcription::MeetingEngine<'_>) -> Result<T>,
    ) -> Result<T> {
        {
            let mut loading = self.is_loading.lock().unwrap();
            while *loading {
                if cancelled.load(Ordering::Acquire) {
                    anyhow::bail!("Meeting processing cancelled");
                }
                loading = self
                    .loading_condvar
                    .wait_timeout(loading, Duration::from_millis(50))
                    .unwrap()
                    .0;
            }
        }
        let lease = self
            .grain
            // A file transcription chunk in flight finishes first, then yields.
            .claim_foreground(crate::grain_transcription::Use::Meeting(attempt_id))
            .map_err(anyhow::Error::msg)?;
        // Freeze the device policy exactly as a dictation load resolves it.
        let settings = get_settings(&self.app_handle);
        let accelerator = settings.transcribe_accelerator;
        let (backend, device) =
            match resolve_gpu_device(accelerator, settings.transcribe_gpu_device.as_deref()) {
                Some(key) => (Backend::Auto, DeviceSelector::Key(key)),
                None => (select_transcribe_backend(accelerator), DeviceSelector::Auto),
            };
        let restore = self.is_model_loaded() && self.get_current_model().is_some();
        // Inside the lease: no dictation load can start until it is released.
        self.begin_unload().wait();
        let _cleanup = MeetingCleanup {
            manager: self,
            lease: Some(lease),
            restore,
        };
        let engine = crate::grain_transcription::MeetingEngine {
            engine: &self.engine,
            mark: self.engine.cancel_mark(),
            cancelled,
            backend,
            device,
            role: std::cell::Cell::new(None),
        };
        f(&engine)
    }

    /// Stop the named attempt's in-flight native work. A stale id is a no-op.
    #[allow(dead_code)] // Wired by the saved-audio pipeline (Step 3).
    pub(crate) fn cancel_grain_meeting(&self, attempt_id: u64) {
        self.grain
            .cancel_owner(crate::grain_transcription::Use::Meeting(attempt_id), || {
                self.engine.cancel()
            });
    }

    pub(crate) fn discard_recording_live(&self, samples: usize) -> Result<()> {
        self.finalize_live(samples).map(|_| ())
    }

    pub(crate) fn finalize_stream(&self) -> Result<Option<String>> {
        self.finalize_live(0).map(|outcome| {
            outcome.map(|outcome| finish_text(outcome, &get_settings(&self.app_handle)))
        })
    }
}

/// Releases a meeting attempt's resources in order: the resident role first,
/// then the lease, then the dictation model the user had loaded (re-read from
/// current settings, so a model deselected meanwhile is never resurrected).
#[allow(dead_code)] // Constructed only by the meeting engine scope above.
struct MeetingCleanup<'a> {
    manager: &'a TranscriptionManager,
    lease: Option<crate::grain_transcription::Lease>,
    restore: bool,
}

impl Drop for MeetingCleanup<'_> {
    fn drop(&mut self) {
        self.manager.engine.unload().wait();
        drop(self.lease.take());
        if self.restore {
            self.manager.initiate_model_load();
        }
    }
}

impl TranscriptionManager {
    fn grain_claim_foreground(
        &self,
        usage: crate::grain_transcription::Use,
    ) -> std::result::Result<crate::grain_transcription::Lease, String> {
        self.grain.claim_foreground(usage)
    }

    fn grain_background_lease(
        &self,
        control: &dyn ChunkControl,
    ) -> Option<crate::grain_transcription::Lease> {
        loop {
            if !control.proceed() {
                return None;
            }
            {
                // A capture cannot begin between the idle check and admission.
                let _start = crate::grain_actions::capture_start_guard();
                if !crate::grain_file_transcription::dictation_in_progress(&self.app_handle) {
                    if let Ok(lease) = self.grain.claim(crate::grain_transcription::Use::File) {
                        return Some(lease);
                    }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn grain_background_context(&self, control: &dyn ChunkControl) -> Result<Option<BatchContext>> {
        let Some(_lease) = self.grain_background_lease(control) else {
            return Ok(None);
        };
        let selected = get_settings(&self.app_handle).selected_model;
        crate::grain_capture::require_transcribable_model(&self.app_handle, &selected)
            .map_err(anyhow::Error::msg)?;
        self.grain_ensure_background_model(&selected)?;
        // No asynchronous load can replace the model while this lease is held.
        self.loaded_batch_context().map(Some)
    }

    fn grain_ensure_background_model(&self, model: &str) -> Result<()> {
        if self.get_current_model().as_deref() != Some(model)
            || !self.is_model_loaded()
            || self.reload_model_on_next_use.load(Ordering::Acquire)
        {
            self.load_model_with_device_held(model, None)?;
            self.reload_model_on_next_use
                .store(false, Ordering::Release);
        }
        Ok(())
    }

    fn grain_background_chunk(
        &self,
        control: &dyn ChunkControl,
        pcm: Vec<f32>,
        options: &RunOptions,
        model: &str,
    ) -> Result<Option<Transcript>> {
        let Some(_lease) = self.grain_background_lease(control) else {
            return Ok(None);
        };
        self.grain_ensure_background_model(model)?;
        let mark = self.engine.cancel_mark();
        self.run_batch(mark, pcm, options, model)
            .map(Some)
            .map_err(engine_error)
    }
}
