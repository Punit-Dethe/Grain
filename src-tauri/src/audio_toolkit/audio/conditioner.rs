//! [GRAIN] Acoustic signal conditioning before VAD + STT.
//!
//! Ported from the Python project's `audio_conditioner.py`, where it measurably
//! improved accuracy on quiet/laptop microphones. Two timing-exact stages — the
//! sample COUNT is never altered, so frame cursors, VAD timing, and the rolling
//! window's chunk boundaries all stay valid:
//!
//! 1. **High-pass biquad at 85 Hz** (RBJ cookbook, Q=0.707), applied per capture
//!    frame with filter state carried across frames. Strips DC offset, HVAC
//!    rumble, and mic-handling thumps — energy that sits below the human voice
//!    but can still fool RMS silence detection and waste model attention.
//!
//! 2. **Noise-gated, boost-only AGC** over a finished buffer. Quiet/distant
//!    speakers cost STT models real accuracy; this measures RMS over the ACTIVE
//!    (above-gate) frames only — so silence is never amplified — and applies one
//!    uniform gain, capped so the loudest sample stays below clipping. Uniform
//!    gain preserves the speech envelope (no pumping); loud audio is untouched.
//!
//! Deliberately NOT included: AI denoising. Modern ASR encoders are trained on
//! noisy speech and denoiser artifacts frequently RAISE WER on moderately noisy
//! input. Pure arithmetic — no extra dependencies.
//!
//! NOTE: Grain's audio is `f32` in `[-1.0, 1.0]` (cpal `to_sample::<f32>()`), so
//! these operate directly on that domain. The Python original worked on int16
//! normalized by `/32768`, which is the same scale — the tunables carry over 1:1.

/// 85 Hz — just below the lowest male fundamental (~90 Hz).
const HIGHPASS_HZ: f32 = 85.0;
/// Butterworth (maximally flat) response.
const HIGHPASS_Q: f32 = 0.707;

const AGC_TARGET_RMS: f32 = 0.06; // ≈ -24 dBFS — a comfortable level for STT
const AGC_MAX_GAIN: f32 = 8.0; // never boost more than +18 dB
const AGC_PEAK_CEILING: f32 = 0.95; // post-gain peak must stay below this
const AGC_GATE_RMS: f32 = 0.0045; // frames quieter than this aren't "speech"
const AGC_FRAME: usize = 320; // 20 ms @ 16 kHz — activity-measurement window

/// Stateful streaming high-pass biquad for mono `f32` frames.
///
/// One instance per recording session (state carries across frames; clear it
/// with [`HighPass::reset`] when a fresh session starts). Runs on the audio
/// consumer thread, so it stays allocation-free and operates in place.
pub struct HighPass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // Direct-form-II-transposed state, carried across frames.
    z1: f32,
    z2: f32,
}

impl HighPass {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_params(sample_rate, HIGHPASS_HZ, HIGHPASS_Q)
    }

    pub fn with_params(sample_rate: u32, cutoff_hz: f32, q: f32) -> Self {
        // RBJ audio-EQ-cookbook high-pass biquad coefficients.
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w0) / 2.0) / a0,
            b1: (-(1.0 + cos_w0)) / a0,
            b2: ((1.0 + cos_w0) / 2.0) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Clear filter state for a fresh recording session.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// High-pass a frame in place. A biquad's feedback can't be vectorised
    /// (each output depends on prior outputs), so this is a per-sample loop —
    /// but it's cheap and only touches the small (~30 ms) capture frames.
    pub fn process_in_place(&mut self, frame: &mut [f32]) {
        let (b0, b1, b2, a1, a2) = (self.b0, self.b1, self.b2, self.a1, self.a2);
        let (mut z1, mut z2) = (self.z1, self.z2);
        for x in frame.iter_mut() {
            let xi = *x;
            let yi = b0 * xi + z1;
            z1 = b1 * xi - a1 * yi + z2;
            z2 = b2 * xi - a2 * yi;
            *x = yi;
        }
        self.z1 = z1;
        self.z2 = z2;
    }
}

/// Noise-gated, boost-only AGC over a finished buffer, applied in place.
///
/// Measures RMS over 20 ms windows that exceed the activity gate (the speech,
/// not the silence). If that speech level is below target, the WHOLE buffer is
/// scaled by one uniform gain — capped at +18 dB and at whatever keeps the
/// loudest sample under the clipping ceiling. Already-loud audio is left as-is
/// (boost-only by design). Silence scales too, but from near-zero stays
/// near-zero, so downstream silence/VAD splitting is unaffected.
pub fn normalize_gain(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }

    // Active-frame RMS: ignore windows that are essentially silence.
    let n_windows = samples.len() / AGC_FRAME;
    let active_rms = if n_windows == 0 {
        rms(samples)
    } else {
        let mut sum_sq = 0.0f64;
        let mut active = 0usize;
        for w in samples.chunks_exact(AGC_FRAME) {
            let r = rms(w);
            if r > AGC_GATE_RMS {
                sum_sq += (r as f64) * (r as f64);
                active += 1;
            }
        }
        if active == 0 {
            return; // pure silence — nothing to normalize
        }
        (sum_sq / active as f64).sqrt() as f32
    };

    if active_rms <= 0.0 || active_rms >= AGC_TARGET_RMS {
        return; // already loud enough — boost-only
    }

    let mut gain = (AGC_TARGET_RMS / active_rms).min(AGC_MAX_GAIN);
    let peak = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    if peak > 0.0 {
        gain = gain.min(AGC_PEAK_CEILING / peak);
    }
    if gain <= 1.0 {
        return;
    }

    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highpass_removes_dc_offset() {
        // A constant (pure DC) signal should be driven toward zero.
        let mut hp = HighPass::new(16_000);
        let mut buf = vec![0.5f32; 16_000];
        hp.process_in_place(&mut buf);
        // After a second of settling the tail should be near zero.
        let tail_rms = rms(&buf[8_000..]);
        assert!(tail_rms < 0.01, "DC not removed, tail rms = {tail_rms}");
    }

    #[test]
    fn highpass_preserves_length() {
        let mut hp = HighPass::new(16_000);
        let mut buf = vec![0.1f32; 480];
        hp.process_in_place(&mut buf);
        assert_eq!(buf.len(), 480);
    }

    #[test]
    fn agc_boosts_quiet_speech() {
        // A 200 Hz tone at RMS ~0.02 (below the 0.06 target) should be boosted.
        let mut buf: Vec<f32> = (0..16_000)
            .map(|n| {
                let t = n as f32 / 16_000.0;
                0.0283 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
            })
            .collect();
        let before = rms(&buf);
        normalize_gain(&mut buf);
        let after = rms(&buf);
        assert!(
            after > before * 1.5,
            "quiet speech not boosted: {before} -> {after}"
        );
        assert!(
            buf.iter().all(|&s| s.abs() <= 1.0),
            "clipped past full scale"
        );
    }

    #[test]
    fn agc_leaves_loud_audio_untouched() {
        // RMS already above target — boost-only means no change.
        let mut buf: Vec<f32> = (0..16_000)
            .map(|n| {
                let t = n as f32 / 16_000.0;
                0.2 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
            })
            .collect();
        let before = buf.clone();
        normalize_gain(&mut buf);
        assert_eq!(buf, before);
    }

    #[test]
    fn agc_ignores_pure_silence() {
        let mut buf = vec![0.0f32; 16_000];
        normalize_gain(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
    }
}
