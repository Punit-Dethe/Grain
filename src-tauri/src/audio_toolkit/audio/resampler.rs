use rubato::{FftFixedIn, Resampler};
use std::time::Duration;

// Make this a constant you can tweak
const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<FftFixedIn<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameResampler {
    pub fn new(in_hz: usize, out_hz: usize, frame_dur: Duration) -> Self {
        let frame_samples = ((out_hz as f64 * frame_dur.as_secs_f64()).round()) as usize;
        assert!(frame_samples > 0, "frame duration too short");

        // Use fixed chunk size instead of GCD-based
        let chunk_in = RESAMPLER_CHUNK_SIZE;

        let resampler = (in_hz != out_hz).then(|| {
            FftFixedIn::<f32>::new(in_hz, out_hz, chunk_in, 1, 1)
                .expect("Failed to create resampler")
        });

        Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }

        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];

            if self.in_buf.len() == self.chunk_in {
                // let start = std::time::Instant::now();
                if let Ok(out) = self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process(&[&self.in_buf[..]], None)
                {
                    // let duration = start.elapsed();
                    // log::debug!("Resampler took: {:?}", duration);
                    self.emit_frames(&out[0], &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        // Flush the resampler. `FftFixedIn` has an internal delay
        // (`output_delay()` output frames): the first `process()` calls emit
        // fewer output frames than the steady-state rate while its FFT delay
        // line fills, so at end-of-stream that many output frames are still
        // trapped inside. If we don't pull them out, we silently drop the last
        // ~`output_delay()` frames of audio — i.e. the final fraction of a
        // second of the recording. Local ASR tolerates that; cloud STT
        // transcribes exactly the bytes we send, so the tail word gets clipped.
        if let Some(ref mut resampler) = self.resampler {
            // 1. Process any remaining partial input, zero-padded to a full
            //    chunk. This emits the output corresponding to the real leftover
            //    samples (still latency-delayed by the FFT).
            if !self.in_buf.is_empty() {
                self.in_buf.resize(self.chunk_in, 0.0);
                if let Ok(out) = resampler.process(&[&self.in_buf[..]], None) {
                    Self::emit_frames_into(
                        self.frame_samples,
                        &mut self.pending,
                        &out[0],
                        &mut emit,
                    );
                }
                self.in_buf.clear();
            }

            // 2. Drain the delay line: feed silent full-size chunks until we've
            //    pulled out at least `output_delay()` output frames — the audio
            //    held back by the FFT latency. Each `process()` consumes exactly
            //    `chunk_in` input frames, so we always feed a full silent chunk.
            let mut remaining = resampler.output_delay();
            let silence = vec![0.0f32; self.chunk_in];
            // Bound the loop defensively: one extra silent chunk yields ~`ratio *
            // chunk_in` output frames, so the delay clears in a handful of
            // iterations. The guard prevents an infinite loop if the backend
            // ever reports a pathological delay.
            let mut guard = 0usize;
            while remaining > 0 && guard < 64 {
                guard += 1;
                match resampler.process(&[&silence[..]], None) {
                    Ok(out) => {
                        let produced = out[0].len();
                        let take = produced.min(remaining);
                        Self::emit_frames_into(
                            self.frame_samples,
                            &mut self.pending,
                            &out[0][..take],
                            &mut emit,
                        );
                        remaining = remaining.saturating_sub(produced);
                    }
                    Err(_) => break,
                }
            }
        }

        // Emit any remaining pending sub-frame (zero-padded to a full frame),
        // so no captured tail samples are left buffered.
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    /// Clear all internal buffers so the next `push()` starts from a clean state.
    ///
    /// Call this between recordings to prevent stale audio from the previous
    /// session leaking into the start of the next one via the FFT overlap buffers.
    pub fn reset(&mut self) {
        self.in_buf.clear();
        self.pending.clear();
        if let Some(ref mut resampler) = self.resampler {
            resampler.reset();
        }
    }

    fn emit_frames(&mut self, data: &[f32], emit: &mut impl FnMut(&[f32])) {
        Self::emit_frames_into(self.frame_samples, &mut self.pending, data, emit);
    }

    /// Accumulate `data` into `pending`, emitting whole `frame_samples`-sized
    /// frames as they fill. Free function form so `finish()` can call it while
    /// holding a mutable borrow of `self.resampler`.
    fn emit_frames_into(
        frame_samples: usize,
        pending: &mut Vec<f32>,
        mut data: &[f32],
        emit: &mut impl FnMut(&[f32]),
    ) {
        while !data.is_empty() {
            let space = frame_samples - pending.len();
            let take = space.min(data.len());
            pending.extend_from_slice(&data[..take]);
            data = &data[take..];

            if pending.len() == frame_samples {
                emit(pending);
                pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a 1kHz sine wave at the given sample rate and duration.
    fn sine_wave(sample_rate: usize, freq: f64, duration_secs: f64) -> Vec<f32> {
        let n = (sample_rate as f64 * duration_secs) as usize;
        (0..n)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate as f64).sin() as f32
            })
            .collect()
    }

    fn collect_output(resampler: &mut FrameResampler, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        resampler.push(input, |frame| out.extend_from_slice(frame));
        out
    }

    #[test]
    fn reset_clears_in_buf_and_pending() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Push less than one chunk (1024 samples) to leave data in in_buf
        let partial = vec![0.5f32; 500];
        let _ = collect_output(&mut r, &partial);

        r.reset();

        // Now push silence — should get only silence out, no remnants of 0.5
        let silence = vec![0.0f32; 4096];
        let out = collect_output(&mut r, &silence);

        let max_abs = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 0.01,
            "After reset, silence input should produce near-silence output, got max_abs={}",
            max_abs
        );
    }

    #[test]
    fn reset_clears_fft_overlap_buffers() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Push a loud 1kHz sine wave through the resampler (simulates recording 1)
        let sine = sine_wave(48000, 1000.0, 0.5); // 500ms of audio
        let _ = collect_output(&mut r, &sine);
        r.finish(|_| {});

        // Reset (simulates new recording starting)
        r.reset();

        // Push silence (simulates recording 2 starting with no speech)
        let silence = vec![0.0f32; 4096];
        let out = collect_output(&mut r, &silence);

        // The output should be near-zero. If the FFT overlap buffers weren't
        // cleared, the sine wave's tail would leak into this output.
        let max_abs = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 0.01,
            "FFT overlap should not leak after reset; got max_abs={} (expected near-zero)",
            max_abs
        );
    }

    #[test]
    fn reset_between_recordings_no_crosstalk() {
        let mut r = FrameResampler::new(48000, 16000, Duration::from_millis(30));

        // Recording 1: ascending ramp (distinctive pattern)
        let ramp: Vec<f32> = (0..48000).map(|i| i as f32 / 48000.0).collect(); // 1 second
        let out1 = collect_output(&mut r, &ramp);
        r.finish(|_| {});
        assert!(!out1.is_empty(), "Recording 1 should produce output");

        // Reset between recordings
        r.reset();

        // Recording 2: constant DC signal of -0.5
        let dc = vec![-0.5f32; 48000]; // 1 second
        let out2 = collect_output(&mut r, &dc);

        // After the resampler settles (skip first frame which may have transient),
        // all samples should be near -0.5, not contaminated by the ascending ramp.
        if out2.len() > 480 {
            // Skip first frame (480 samples at 16kHz/30ms), check the rest
            let tail = &out2[480..];
            for (i, &s) in tail.iter().enumerate() {
                assert!(
                    (s - (-0.5)).abs() < 0.05,
                    "Recording 2 sample {} = {} (expected ~-0.5); ramp leaked through",
                    i + 480,
                    s
                );
            }
        }
    }

    #[test]
    fn reset_passthrough_mode_clears_pending() {
        // When in_hz == out_hz, no rubato resampler is created (passthrough mode).
        // Reset should still clear the pending frame buffer.
        let mut r = FrameResampler::new(16000, 16000, Duration::from_millis(30));

        // Push partial frame (less than 480 samples) to leave data in pending
        let partial = vec![1.0f32; 200];
        let _ = collect_output(&mut r, &partial);

        r.reset();

        // Push silence
        let silence = vec![0.0f32; 960];
        let out = collect_output(&mut r, &silence);

        // First complete frame should be all zeros, not contain the 1.0 values
        if !out.is_empty() {
            let max_abs = out.iter().take(480).map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(
                max_abs < 0.001,
                "Passthrough mode: pending buffer should be cleared after reset, got max_abs={}",
                max_abs
            );
        }
    }

    /// Collect every emitted sample from a push/finish run into one buffer.
    fn run(in_hz: usize, out_hz: usize, input: &[f32]) -> Vec<f32> {
        let mut r = FrameResampler::new(in_hz, out_hz, Duration::from_millis(30));
        let mut out = Vec::new();
        r.push(input, |frame| out.extend_from_slice(frame));
        r.finish(|frame| out.extend_from_slice(frame));
        out
    }

    #[test]
    fn passthrough_when_rates_match() {
        // No resampler is created when in_hz == out_hz; every sample passes
        // through, padded only up to a frame boundary at finish.
        let input: Vec<f32> = (0..5000).map(|i| (i as f32 * 0.001).sin()).collect();
        let out = run(16_000, 16_000, &input);
        assert!(
            out.len() >= input.len(),
            "passthrough must not drop samples (got {}, in {})",
            out.len(),
            input.len()
        );
        // The leading samples are exactly the input (tail is zero-padded frame).
        assert_eq!(&out[..input.len()], &input[..]);
    }

    #[test]
    fn downsample_preserves_total_duration_within_one_frame() {
        // 48 kHz -> 16 kHz over 1 second. Expected ~16000 output samples. The
        // old finish() dropped the FFT delay line, losing the trailing frames;
        // this asserts the total length is correct within a single frame.
        let in_hz = 48_000;
        let out_hz = 16_000;
        let secs = 1.0_f64;
        let n_in = (in_hz as f64 * secs) as usize;
        let input: Vec<f32> = (0..n_in)
            .map(|i| (i as f32 / in_hz as f32 * 440.0 * std::f32::consts::TAU).sin())
            .collect();

        let out = run(in_hz, out_hz, &input);
        let expected = (n_in as f64 * out_hz as f64 / in_hz as f64).round() as usize;

        let frame = ((out_hz as f64) * 0.030).round() as usize;
        let diff = out.len().abs_diff(expected);
        assert!(
            diff <= frame,
            "resampled length {} differs from expected {} by more than one frame ({})",
            out.len(),
            expected,
            frame
        );
    }

    #[test]
    fn upsample_preserves_total_duration_within_one_frame() {
        let in_hz = 16_000;
        let out_hz = 44_100;
        let n_in = 16_000; // 1 second
        let input: Vec<f32> = (0..n_in)
            .map(|i| (i as f32 / in_hz as f32 * 220.0 * std::f32::consts::TAU).sin())
            .collect();

        let out = run(in_hz, out_hz, &input);
        let expected = (n_in as f64 * out_hz as f64 / in_hz as f64).round() as usize;

        let frame = ((out_hz as f64) * 0.030).round() as usize;
        let diff = out.len().abs_diff(expected);
        assert!(
            diff <= frame,
            "resampled length {} differs from expected {} by more than one frame ({})",
            out.len(),
            expected,
            frame
        );
    }

    #[test]
    fn tail_audio_survives_resampling() {
        // Put a loud marker in the LAST 100 ms of input and assert energy
        // survives into the LAST 100 ms of output. The pre-fix finish() dropped
        // the delay line, so the marker (the final word) went missing on the
        // cloud path.
        let in_hz = 48_000;
        let out_hz = 16_000;
        let n_in = in_hz; // 1 second
        let marker_start = n_in - in_hz / 10; // last 100 ms
        let input: Vec<f32> = (0..n_in)
            .map(|i| if i >= marker_start { 0.9 } else { 0.0 })
            .collect();

        let out = run(in_hz, out_hz, &input);
        let tail_len = out_hz / 10; // last 100 ms of output
        assert!(out.len() >= tail_len);
        let tail = &out[out.len() - tail_len..];
        let energy: f32 = tail.iter().map(|s| s * s).sum();
        assert!(
            energy > 0.1,
            "trailing marker did not survive resampling (tail energy {energy})"
        );
    }
}
