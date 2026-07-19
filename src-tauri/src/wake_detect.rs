//! [GRAIN] Acoustic wake-phrase spotter — the LOW-LATENCY front end for voice
//! commands.
//!
//! ## Why this exists
//!
//! The text-based detector in [`crate::voice_command`] reads the transcript, so in
//! Rolling mode it only sees a phrase once its VAD chunk has been decoded — up to
//! ~15s late. That is fatal for a *live* affordance (the yellow listening state,
//! the prompt switcher). This module closes the gap: it spots the wake phrase
//! directly in the audio ~100ms after it is spoken, and never touches the ASR
//! engine (which Rolling keeps busy decoding chunks anyway).
//!
//! ## How: MFCC + DTW against the user's own recordings
//!
//! Classic speaker-dependent keyword spotting, and a deliberate choice over a
//! neural wake-word model:
//!
//! - **No training, no GPU, no cloud, no model download.** A neural spotter
//!   (openWakeWord et al.) needs ~90 min of GPU training per phrase, which would
//!   lock the phrase and add an ONNX runtime we don't ship.
//! - **The user picks ANY phrase** — they just say it a few times.
//! - **Speaker-adapted**, so it is more robust for that user than a generic model.
//! - **Immune to ASR mistranscription** (the "grain" → green / grin problem that
//!   forced the phonetic matcher in `voice_command`): no ASR is involved at all.
//! - **Zero new dependencies** — `rustfft` and `hound` are already in the tree.
//!
//! (The `rustpotter` crate implements this same approach, but its last release is
//! from 2023 and hard-pins candle 0.2.2, which collides with our candle 0.9.)
//!
//! ## Pipeline
//!
//! 16kHz mono f32 → 25ms Hamming frames every 10ms → FFT power spectrum → mel
//! filterbank → log → DCT-II → 13 MFCCs, then cepstral mean normalization (which
//! cancels per-recording level/channel differences). Live MFCC frames land in a
//! ring buffer; every 50ms the most recent span is compared to each enrolled
//! template with band-limited DTW. The best normalized distance under the
//! threshold fires a detection.
//!
//! ## Cost
//!
//! Templates are ~30KB total; DTW over a ~1s phrase is ~15k cell updates per
//! template, run 20x/second — well under 1% CPU, and gated on speech energy so
//! silence costs nothing. Constructed at recording start ONLY when voice commands
//! are enabled and an enrolled phrase exists, and dropped at stop: no idle RAM.

use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use serde::{Deserialize, Serialize};

// ── DSP geometry ────────────────────────────────────────────────────────────

/// Capture/analysis rate. The whole pipeline is fixed at 16kHz mono, matching
/// what the recorder already produces.
const SAMPLE_RATE: usize = 16_000;
/// Analysis window (25ms) and hop (10ms) — the standard MFCC framing.
const FRAME_LEN: usize = 400;
const HOP_LEN: usize = 160;
/// FFT size (next power of two above FRAME_LEN).
const N_FFT: usize = 512;
/// Mel filterbank size and the speech band it covers.
const N_MELS: usize = 26;
const MEL_LOW_HZ: f32 = 300.0;
const MEL_HIGH_HZ: f32 = 8000.0;
/// Retained cepstral coefficients per frame.
const N_MFCC: usize = 13;

/// Run the (relatively) expensive DTW pass every N feature frames — 5 frames =
/// 50ms, which is far below human reaction time but 5x cheaper than per-frame.
const DTW_EVERY: usize = 5;
/// Sakoe-Chiba band as a fraction of the template length. Bounds DTW cost and
/// forbids pathological warps that would match unrelated audio.
const DTW_BAND_FRAC: f32 = 0.2;
/// How much longer than the template the compared span may be, absorbing a
/// slower delivery than enrollment. Also sizes the ring buffer.
const SPAN_STRETCH: f32 = 1.35;
/// Span lengths tried per template, as multiples of the template length. A single
/// fixed span would drag whatever precedes the phrase (silence, the tail of the
/// previous word) into the comparison and inflate the distance; trying a few
/// alignments lets the tightest-fitting window win. DTW absorbs rate variation
/// *within* a span, but cannot undo a badly-chosen span boundary.
const SPAN_SCALES: [f32; 4] = [0.85, 1.0, 1.15, 1.35];
/// After a detection, ignore audio for this long so one utterance fires once.
const REFRACTORY_FRAMES: usize = 100; // 100 * 10ms = 1s

/// Frames whose log-energy sits below the running noise floor by less than this
/// are treated as silence — DTW is skipped when the span is mostly silent.
const SPEECH_MARGIN: f32 = 1.5;

// ── Feature extraction ──────────────────────────────────────────────────────

/// Streaming MFCC extractor: push raw samples, pull fixed-size feature frames.
struct MfccExtractor {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    filters: Vec<Vec<f32>>,
    dct: Vec<Vec<f32>>,
    /// Unconsumed samples carried between pushes.
    carry: Vec<f32>,
}

impl MfccExtractor {
    fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        // Hamming window over the analysis frame.
        let window = (0..FRAME_LEN)
            .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f32 / (FRAME_LEN - 1) as f32).cos())
            .collect();
        MfccExtractor {
            fft,
            window,
            filters: mel_filterbank(),
            dct: dct_matrix(),
            carry: Vec::with_capacity(FRAME_LEN * 2),
        }
    }

    /// Append `samples` and emit every complete feature frame they produce.
    fn push(&mut self, samples: &[f32], out: &mut Vec<[f32; N_MFCC]>) {
        self.carry.extend_from_slice(samples);
        let mut offset = 0;
        while offset + FRAME_LEN <= self.carry.len() {
            out.push(self.frame(&self.carry[offset..offset + FRAME_LEN].to_vec()));
            offset += HOP_LEN;
        }
        if offset > 0 {
            self.carry.drain(..offset);
        }
    }

    /// One analysis window → one MFCC vector.
    fn frame(&self, samples: &[f32]) -> [f32; N_MFCC] {
        let mut buf: Vec<Complex32> = (0..N_FFT)
            .map(|i| {
                let v = if i < FRAME_LEN {
                    samples[i] * self.window[i]
                } else {
                    0.0
                };
                Complex32::new(v, 0.0)
            })
            .collect();
        self.fft.process(&mut buf);

        // Power spectrum over the non-redundant half.
        let bins = N_FFT / 2 + 1;
        let power: Vec<f32> = buf[..bins].iter().map(|c| c.norm_sqr()).collect();

        // Mel energies → log.
        let mut mel = [0.0f32; N_MELS];
        for (m, filter) in self.filters.iter().enumerate() {
            let e: f32 = filter.iter().zip(power.iter()).map(|(w, p)| w * p).sum();
            // Floor before log so silence is finite rather than -inf.
            mel[m] = (e + 1e-10).ln();
        }

        // DCT-II → cepstrum.
        let mut out = [0.0f32; N_MFCC];
        for (k, row) in self.dct.iter().enumerate() {
            out[k] = row.iter().zip(mel.iter()).map(|(c, m)| c * m).sum();
        }
        out
    }
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

/// Triangular mel filters over the FFT bins.
fn mel_filterbank() -> Vec<Vec<f32>> {
    let bins = N_FFT / 2 + 1;
    let low = hz_to_mel(MEL_LOW_HZ);
    let high = hz_to_mel(MEL_HIGH_HZ);
    // N_MELS filters need N_MELS + 2 band edges.
    let points: Vec<f32> = (0..N_MELS + 2)
        .map(|i| {
            let mel = low + (high - low) * i as f32 / (N_MELS + 1) as f32;
            mel_to_hz(mel) * N_FFT as f32 / SAMPLE_RATE as f32
        })
        .collect();

    (0..N_MELS)
        .map(|m| {
            let (l, c, r) = (points[m], points[m + 1], points[m + 2]);
            (0..bins)
                .map(|b| {
                    let f = b as f32;
                    if f < l || f > r {
                        0.0
                    } else if f <= c {
                        if (c - l).abs() < 1e-6 {
                            0.0
                        } else {
                            (f - l) / (c - l)
                        }
                    } else if (r - c).abs() < 1e-6 {
                        0.0
                    } else {
                        (r - f) / (r - c)
                    }
                })
                .collect()
        })
        .collect()
}

/// Orthonormal-ish DCT-II basis, N_MFCC rows over N_MELS inputs.
fn dct_matrix() -> Vec<Vec<f32>> {
    (0..N_MFCC)
        .map(|k| {
            (0..N_MELS)
                .map(|n| {
                    (PI * k as f32 * (n as f32 + 0.5) / N_MELS as f32).cos()
                        * (2.0 / N_MELS as f32).sqrt()
                })
                .collect()
        })
        .collect()
}

/// Cepstral mean normalization: subtract the per-coefficient mean across the
/// sequence. Cancels constant channel/level offsets so a template recorded at one
/// volume still matches speech at another.
fn cmn(frames: &mut [[f32; N_MFCC]]) {
    if frames.is_empty() {
        return;
    }
    let mut mean = [0.0f32; N_MFCC];
    for f in frames.iter() {
        for k in 0..N_MFCC {
            mean[k] += f[k];
        }
    }
    let n = frames.len() as f32;
    for m in mean.iter_mut() {
        *m /= n;
    }
    for f in frames.iter_mut() {
        for k in 0..N_MFCC {
            f[k] -= mean[k];
        }
    }
}

// ── DTW ─────────────────────────────────────────────────────────────────────

fn dist(a: &[f32; N_MFCC], b: &[f32; N_MFCC]) -> f32 {
    let mut s = 0.0;
    for k in 0..N_MFCC {
        let d = a[k] - b[k];
        s += d * d;
    }
    s.sqrt()
}

/// Band-limited DTW distance, normalized by path length so sequences of
/// different lengths stay comparable. `f32::MAX` when the band admits no path.
fn dtw(a: &[[f32; N_MFCC]], b: &[[f32; N_MFCC]]) -> f32 {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return f32::MAX;
    }
    let band = ((n.max(m) as f32 * DTW_BAND_FRAC) as usize).max(n.abs_diff(m) + 1);

    // Two rolling rows of (cost, path length).
    let mut prev = vec![(f32::MAX, 0usize); m + 1];
    let mut cur = vec![(f32::MAX, 0usize); m + 1];
    prev[0] = (0.0, 0);

    for i in 1..=n {
        cur[0] = (f32::MAX, 0);
        // Only cells inside the Sakoe-Chiba band are considered.
        let lo = i.saturating_sub(band).max(1);
        let hi = (i + band).min(m);
        for j in 0..=m {
            if j < lo || j > hi {
                cur[j] = (f32::MAX, 0);
            }
        }
        for j in lo..=hi {
            let d = dist(&a[i - 1], &b[j - 1]);
            // Best predecessor: diagonal, up, or left.
            let mut best = prev[j - 1];
            if prev[j].0 < best.0 {
                best = prev[j];
            }
            if cur[j - 1].0 < best.0 {
                best = cur[j - 1];
            }
            cur[j] = if best.0 == f32::MAX {
                (f32::MAX, 0)
            } else {
                (best.0 + d, best.1 + 1)
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    let (cost, len) = prev[m];
    if cost == f32::MAX || len == 0 {
        f32::MAX
    } else {
        cost / len as f32
    }
}

// ── Enrolled phrase ─────────────────────────────────────────────────────────

/// One enrollment recording reduced to its (CMN'd) MFCC sequence.
#[derive(Serialize, Deserialize, Clone)]
struct Template {
    frames: Vec<[f32; N_MFCC]>,
}

/// The user's enrolled wake phrase: a handful of templates plus the calibration
/// derived from them.
#[derive(Serialize, Deserialize, Clone)]
pub struct WakeReference {
    /// Display label (the phrase the user recorded).
    pub name: String,
    templates: Vec<Template>,
    /// Mean pairwise DTW distance among the templates — the natural spread of
    /// this speaker saying this phrase. Detection thresholds scale off it, so a
    /// crisply-repeated phrase gets a tight threshold and a variable one a looser
    /// threshold, without the user tuning anything.
    spread: f32,
}

impl WakeReference {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse wake reference: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("write {}: {e}", path.display()))
    }

    fn max_len(&self) -> usize {
        self.templates.iter().map(|t| t.frames.len()).max().unwrap_or(0)
    }
}

/// Reduce raw samples to a normalized MFCC sequence (shared by enrollment and
/// the live path so both sides of the comparison are built identically).
fn features(samples: &[f32]) -> Vec<[f32; N_MFCC]> {
    let mut ex = MfccExtractor::new();
    let mut frames = Vec::new();
    ex.push(samples, &mut frames);
    cmn(&mut frames);
    frames
}

/// Build a reference from enrollment recordings (raw 16kHz mono f32 clips).
///
/// Requires at least 3 usable recordings. The calibration `spread` is the mean
/// pairwise DTW distance between templates.
pub fn build_reference(name: &str, clips: &[Vec<f32>]) -> Result<WakeReference, String> {
    let templates: Vec<Template> = clips
        .iter()
        .map(|c| Template {
            frames: features(c),
        })
        .filter(|t| t.frames.len() >= 10)
        .collect();

    if templates.len() < 3 {
        return Err(format!(
            "need at least 3 clear recordings of the phrase (got {} usable)",
            templates.len()
        ));
    }

    // Mean pairwise distance = how consistently this speaker says the phrase.
    let mut sum = 0.0;
    let mut pairs = 0;
    for i in 0..templates.len() {
        for j in (i + 1)..templates.len() {
            let d = dtw(&templates[i].frames, &templates[j].frames);
            if d < f32::MAX {
                sum += d;
                pairs += 1;
            }
        }
    }
    let spread = if pairs > 0 { sum / pairs as f32 } else { 1.0 };

    Ok(WakeReference {
        name: name.to_string(),
        templates,
        spread,
    })
}

// ── Live spotter ────────────────────────────────────────────────────────────

/// Live acoustic spotter for one recording session.
pub struct WakeSpotter {
    extractor: MfccExtractor,
    reference: WakeReference,
    /// Rolling window of recent feature frames (bounded by the longest template).
    ring: Vec<[f32; N_MFCC]>,
    capacity: usize,
    /// Shortest template length — the minimum evidence needed before a DTW pass
    /// is worth running. Gating on the FULL ring instead would blind the spotter
    /// for the first ~1s of every session.
    min_len: usize,
    /// Frames seen since the last DTW pass.
    since_dtw: usize,
    /// Frames remaining in the post-detection refractory period.
    refractory: usize,
    /// Distance below which a span counts as the phrase.
    threshold: f32,
    /// Running estimate of the quiet-floor log energy, for the speech gate.
    noise_floor: f32,
    scratch: Vec<[f32; N_MFCC]>,
}

impl WakeSpotter {
    /// Build a spotter for `reference`. `sensitivity` is 0.0–1.0; higher fires
    /// more eagerly by widening the accepted distance around the reference's own
    /// measured spread.
    pub fn new(reference: WakeReference, sensitivity: f32) -> Self {
        let s = sensitivity.clamp(0.0, 1.0);
        // Accept anything within 1.0x..2.0x the speaker's own natural spread.
        let threshold = reference.spread * (1.0 + s);
        let capacity = ((reference.max_len() as f32 * SPAN_STRETCH) as usize).max(16);
        let min_len = reference
            .templates
            .iter()
            .map(|t| t.frames.len())
            .min()
            .unwrap_or(16);
        log::info!(
            "[GRAIN] wake spotter ready: phrase={:?}, {} templates, spread={:.3}, threshold={:.3}",
            reference.name,
            reference.templates.len(),
            reference.spread,
            threshold
        );
        WakeSpotter {
            extractor: MfccExtractor::new(),
            reference,
            ring: Vec::with_capacity(capacity + 8),
            capacity,
            min_len,
            since_dtw: 0,
            refractory: 0,
            threshold,
            noise_floor: f32::MAX,
            scratch: Vec::new(),
        }
    }

    /// Load a reference from disk and build a spotter. `None` (not an error the
    /// caller must handle) when there is no usable enrollment — the feature stays
    /// inert rather than breaking dictation.
    pub fn from_file(path: &Path, sensitivity: f32) -> Option<Self> {
        if !path.is_file() {
            return None;
        }
        match WakeReference::load(path) {
            Ok(r) => Some(Self::new(r, sensitivity)),
            Err(e) => {
                log::warn!("[GRAIN] wake spotter: {e}");
                None
            }
        }
    }

    /// Feed captured samples (f32 mono @16kHz). Returns `true` on the call where
    /// the wake phrase fired.
    pub fn process(&mut self, samples: &[f32]) -> bool {
        self.scratch.clear();
        let mut frames = std::mem::take(&mut self.scratch);
        self.extractor.push(samples, &mut frames);

        let mut fired = false;
        for f in frames.drain(..) {
            // c0 tracks frame loudness; use it to maintain a quiet floor so the
            // speech gate adapts to the room instead of a fixed level.
            let energy = f[0];
            self.noise_floor = if self.noise_floor == f32::MAX {
                energy
            } else if energy < self.noise_floor {
                // Fall fast to a new quiet floor…
                self.noise_floor * 0.9 + energy * 0.1
            } else {
                // …and rise very slowly, so speech doesn't drag the floor up.
                self.noise_floor * 0.999 + energy * 0.001
            };

            self.ring.push(f);
            if self.ring.len() > self.capacity {
                let excess = self.ring.len() - self.capacity;
                self.ring.drain(..excess);
            }

            if self.refractory > 0 {
                self.refractory -= 1;
                continue;
            }

            self.since_dtw += 1;
            if self.since_dtw < DTW_EVERY || self.ring.len() < self.min_len {
                continue;
            }
            self.since_dtw = 0;

            if self.match_span() {
                fired = true;
                self.refractory = REFRACTORY_FRAMES;
                self.ring.clear();
            }
        }
        self.scratch = frames;
        fired
    }

    /// Compare the current window against every template.
    fn match_span(&self) -> bool {
        // Skip silence: if the window never rises meaningfully above the floor,
        // there is nothing to match and DTW would just burn CPU.
        let peak = self
            .ring
            .iter()
            .map(|f| f[0])
            .fold(f32::MIN, |a, b| a.max(b));
        if peak - self.noise_floor < SPEECH_MARGIN {
            return false;
        }

        for t in &self.reference.templates {
            for scale in SPAN_SCALES {
                let span_len = ((t.frames.len() as f32 * scale) as usize).min(self.ring.len());
                if span_len < 8 {
                    continue;
                }
                let span = &self.ring[self.ring.len() - span_len..];
                // The live span carries its own channel offset — re-normalize it
                // so the comparison matches how templates were built.
                let mut norm = span.to_vec();
                cmn(&mut norm);
                if dtw(&t.frames, &norm) <= self.threshold {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crude voiced-speech-like signal: a pitched buzz whose formant shifts over
    /// time, so different `seed`s produce genuinely different spectral tracks.
    fn utterance(seed: f32, len_ms: usize) -> Vec<f32> {
        let n = SAMPLE_RATE * len_ms / 1000;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let f0 = 110.0 + 20.0 * seed;
                let formant = 500.0 + 400.0 * seed + 300.0 * (t * 3.0).sin();
                0.5 * (2.0 * PI * f0 * t).sin() + 0.4 * (2.0 * PI * formant * t).sin()
            })
            .collect()
    }

    fn silence(len_ms: usize) -> Vec<f32> {
        vec![0.0; SAMPLE_RATE * len_ms / 1000]
    }

    #[test]
    fn mfcc_produces_expected_frame_count() {
        // 1s of audio at 10ms hop ≈ 98 frames (last partial window dropped).
        let f = features(&utterance(1.0, 1000));
        assert!(f.len() > 90 && f.len() < 101, "got {} frames", f.len());
    }

    #[test]
    fn dtw_identical_sequences_is_zero() {
        let a = features(&utterance(1.0, 600));
        assert!(dtw(&a, &a) < 1e-3, "self distance {}", dtw(&a, &a));
    }

    #[test]
    fn dtw_differs_for_different_utterances() {
        let a = features(&utterance(1.0, 600));
        let b = features(&utterance(5.0, 600));
        let self_d = dtw(&a, &a);
        let cross_d = dtw(&a, &b);
        assert!(
            cross_d > self_d,
            "cross {cross_d} should exceed self {self_d}"
        );
    }

    #[test]
    fn dtw_absorbs_rate_variation() {
        // The same "phrase" said slower should still be much closer than a
        // different phrase — that is the whole point of DTW.
        let base = features(&utterance(1.0, 600));
        let slower = features(&utterance(1.0, 780));
        let other = features(&utterance(6.0, 600));
        assert!(
            dtw(&base, &slower) < dtw(&base, &other),
            "rate-varied {} should beat different-phrase {}",
            dtw(&base, &slower),
            dtw(&base, &other)
        );
    }

    #[test]
    fn build_reference_rejects_too_few_clips() {
        let clips = vec![utterance(1.0, 600), utterance(1.0, 600)];
        assert!(build_reference("hey grain", &clips).is_err());
    }

    #[test]
    fn build_reference_computes_spread() {
        let clips: Vec<Vec<f32>> = (0..4).map(|_| utterance(1.0, 600)).collect();
        let r = build_reference("hey grain", &clips).expect("reference");
        assert_eq!(r.templates.len(), 4);
        // Identical clips → near-zero spread.
        assert!(r.spread < 1e-2, "spread {}", r.spread);
    }

    #[test]
    fn spotter_fires_on_the_enrolled_phrase() {
        let clips: Vec<Vec<f32>> = (0..3).map(|_| utterance(2.0, 700)).collect();
        let reference = build_reference("hey grain", &clips).expect("reference");
        // Identical clips give ~0 spread, so widen the gate for the test.
        let mut spotter = WakeSpotter::new(reference, 1.0);
        spotter.threshold = spotter.threshold.max(0.5);

        spotter.process(&silence(300));
        let fired = spotter.process(&utterance(2.0, 700));
        assert!(fired, "spotter should fire on the enrolled phrase");
    }

    #[test]
    fn spotter_ignores_a_different_phrase() {
        // Enroll with slight natural variation so `spread` (and therefore the
        // threshold) is realistic rather than degenerately zero.
        let clips: Vec<Vec<f32>> = [2.0, 2.06, 1.94]
            .iter()
            .map(|s| utterance(*s, 700))
            .collect();
        let reference = build_reference("hey grain", &clips).expect("reference");
        let mut spotter = WakeSpotter::new(reference, 0.5);

        spotter.process(&silence(300));
        assert!(
            !spotter.process(&utterance(9.0, 700)),
            "a clearly different utterance must not fire the wake phrase"
        );
    }

    #[test]
    fn spotter_ignores_silence() {
        let clips: Vec<Vec<f32>> = (0..3).map(|_| utterance(2.0, 700)).collect();
        let reference = build_reference("hey grain", &clips).expect("reference");
        let mut spotter = WakeSpotter::new(reference, 1.0);
        spotter.threshold = spotter.threshold.max(0.5);
        assert!(!spotter.process(&silence(2000)), "silence must not fire");
    }

    #[test]
    fn reference_roundtrips_through_disk() {
        let clips: Vec<Vec<f32>> = (0..3).map(|_| utterance(3.0, 500)).collect();
        let r = build_reference("hey grain", &clips).expect("reference");
        let path = std::env::temp_dir().join("grain_wake_ref_test.json");
        r.save(&path).expect("save");
        let back = WakeReference::load(&path).expect("load");
        assert_eq!(back.name, "hey grain");
        assert_eq!(back.templates.len(), r.templates.len());
        let _ = std::fs::remove_file(&path);
    }
}
