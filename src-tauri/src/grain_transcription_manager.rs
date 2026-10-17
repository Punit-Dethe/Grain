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
        if self.is_model_loaded()
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
            .grain
            .claim(crate::grain_transcription::Use::Flow(session_id))
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

    pub(crate) fn discard_recording_live(&self, samples: usize) -> Result<()> {
        self.finalize_live(samples).map(|_| ())
    }

    pub(crate) fn finalize_stream(&self) -> Result<Option<String>> {
        self.finalize_live(0).map(|outcome| {
            outcome.map(|outcome| finish_text(outcome, &get_settings(&self.app_handle)))
        })
    }
}
