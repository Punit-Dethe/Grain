// Included at the existing audio-manager seam; no extra delivery thread.
impl RecordingReadiness {
    pub fn wait_timeout(&self, timeout: Duration) -> Result<(), mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}
impl AudioRecordingManager {
    fn grain_prepare_capture_geometry(&self) -> Result<(), anyhow::Error> {
        let expected = self.vad_frame_samples()?;
        let mismatch = self
            .recorder
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|mic| mic.recorder.info().frames_per_chunk != expected);
        if mismatch {
            self.stop_microphone_stream();
        }
        Ok(())
    }

    pub fn try_start_recording_low_ram(
        &self,
        binding_id: &str,
        vad_policy: VadPolicy,
    ) -> Result<RecordingReadiness, anyhow::Error> {
        if vad_policy != VadPolicy::Disabled {
            anyhow::bail!("Journal-only capture requires continuous audio");
        }
        self.grain_start_recording(binding_id, vad_policy, false)
    }
    pub fn arm_prompt_record(&self) -> bool {
        let Ok(state) = self.state.try_lock() else {
            return false;
        };
        if !matches!(&*state, RecordingState::Recording { binding_id }
            if grain_core::capture::supports_prompt_record(binding_id))
        {
            return false;
        }
        self.grain.arm()
    }
    pub fn take_prompt_mark(&self) -> Option<usize> {
        self.grain.take_mark()
    }
    pub fn has_prompt_mark(&self) -> bool {
        self.grain.has_mark()
    }
}
