//! Capture bookkeeping on the recorder's existing delivery thread.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::Manager;

pub(crate) struct State {
    retain: AtomicBool,
    frames: AtomicU64,
    mark: Mutex<Option<usize>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            retain: AtomicBool::new(true),
            frames: AtomicU64::new(0),
            mark: Mutex::new(None),
        }
    }
}
impl State {
    pub(crate) fn begin(&self, retain: bool) {
        self.retain.store(retain, Ordering::Release);
        self.frames.store(0, Ordering::Release);
        *self.mark.lock().unwrap() = None;
    }
    pub(crate) fn flow(&self) -> bool {
        !self.retain.load(Ordering::Acquire)
    }
    pub(crate) fn feed(&self, app: &tauri::AppHandle, frame: &[f32]) {
        self.frames.fetch_add(frame.len() as u64, Ordering::AcqRel);
        if self.flow() {
            if let Some(rolling) =
                app.try_state::<std::sync::Arc<crate::rolling::RollingTranscriber>>()
            {
                rolling.feed(frame, None);
            }
        }
    }
    pub(crate) fn arm(&self) -> bool {
        let mut mark = self.mark.lock().unwrap();
        if mark.is_some() {
            return false;
        }
        let Ok(frames) = usize::try_from(self.frames.load(Ordering::Acquire)) else {
            return false;
        };
        if frames == 0 {
            return false;
        }
        *mark = Some(frames);
        true
    }
    pub(crate) fn take_mark(&self) -> Option<usize> {
        self.mark.lock().unwrap().take()
    }
    pub(crate) fn has_mark(&self) -> bool {
        self.mark.lock().unwrap().is_some()
    }
}

/// Disabled VAD needs chunk geometry, with no model or scorer loaded.
struct Bypass;
impl crate::audio_toolkit::VoiceActivityDetector for Bypass {
    fn push_frame<'a>(
        &'a mut self,
        frame: &'a [f32],
    ) -> anyhow::Result<crate::audio_toolkit::vad::VadFrame<'a>> {
        Ok(crate::audio_toolkit::vad::VadFrame::Speech(frame))
    }
    fn frame_samples(&self) -> usize {
        512
    }
}
pub(crate) fn bypass_vad() -> crate::audio_toolkit::CaptureVad {
    crate::audio_toolkit::CaptureVad::new(Box::new(Bypass), 0, 0)
}
pub(crate) fn retain_frame(samples: &mut Vec<f32>, frame: &[f32], retain: bool) {
    if retain {
        samples.extend_from_slice(frame);
    }
}

#[cfg(test)]
mod recorder_tests {
    use super::*;
    use crate::audio_toolkit::CaptureSink;
    use crate::audio_toolkit::VadPolicy;
    use handy_recorder::{
        testing::{FakeError, FakeMic},
        RecorderConfig,
    };
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn recorder_disconnect_drains_resampler_tail_into_journal_only_sink() {
        let mic = FakeMic::new(48000, 1);
        let (failed, failure) = mpsc::channel();
        let recorder = mic
            .open::<CaptureSink>(
                RecorderConfig {
                    frames_per_chunk: Some(512),
                    ..RecorderConfig::speech()
                },
                Some(Box::new(move |error| {
                    let _ = failed.send(error);
                })),
                Duration::from_secs(2),
            )
            .unwrap();
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let audio = Arc::clone(&delivered);
        let (ready_tx, ready) = mpsc::channel();
        recorder
            .start(
                CaptureSink::new(
                    bypass_vad(),
                    VadPolicy::Disabled,
                    Arc::new(|_| {}),
                    Arc::new(move |frame| audio.lock().unwrap().extend_from_slice(frame)),
                    ready_tx,
                )
                .with_grain_retention(false),
            )
            .unwrap_or_else(|error| panic!("{error:?}"));
        // Last 17 input samples carry a distinct tail; push irregular callback sizes.
        let input: Vec<f32> = (0..48003)
            .map(|i| {
                if i >= 47986 {
                    0.8
                } else {
                    (i as f32 * 0.01).sin() * 0.2
                }
            })
            .collect();
        for block in input.chunks(733) {
            assert!(mic.push(block));
        }
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(mic.report_error(FakeError::DeviceNotAvailable));
        failure.recv_timeout(Duration::from_secs(2)).unwrap();
        let stopped = recorder.stop().unwrap();
        assert!(matches!(
            stopped.end_reason,
            handy_recorder::EndReason::RecorderFailed(_)
        ));
        let captured = stopped.sink.finish();
        assert_eq!(captured.samples.capacity(), 0);
        let delivered = delivered.lock().unwrap();
        // handy-recorder preserves the FFT filter's 171-frame latency at 48->16 kHz.
        // It drains through input*ratio + delay; terminal padding is excluded.
        assert_eq!(
            delivered.len(),
            16001 + 171,
            "resampling must drain the complete filter delay"
        );
        assert!(
            delivered[delivered.len() - 6..]
                .iter()
                .any(|sample| sample.abs() > 0.3),
            "distinct tail must survive"
        );
        drop(delivered);
        recorder.close().unwrap();
        assert!(!mic.is_streaming());
        // A fresh recorder can reopen the same device after disconnect cleanup.
        let reopened = mic
            .open::<CaptureSink>(RecorderConfig::speech(), None, Duration::from_secs(2))
            .unwrap();
        reopened.close().unwrap();
        assert!(!mic.is_streaming());
    }
}
