//! Integration layer: drive real `transcribe-rs` models through the pure
//! [`rolling_window`] engine.
//!
//! The pure crate knows nothing about ASR; this crate is the seam where a model
//! meets the cursor + assembler. Each model is wrapped behind the [`Asr`] trait
//! (one impl per `transcribe-rs` engine → the model matrix), and
//! [`RollingWindowSession`] ties `SessionCursor` → model → `TimelineAssembler`.
//! The daemon uses this exact session type; the `asr-harness` binary uses it to
//! validate every model against the engine.

use anyhow::Result;
use rolling_window::{AudioChunk, RollingWindowConfig, SessionCursor, TimelineAssembler, WordTiming};

pub mod engines;
pub mod timing;

pub use engines::{EngineKind, GrainModel};

/// Convert i16 PCM frames to the `f32` in `[-1, 1]` that `transcribe-rs` expects.
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// RMS of an i16 block on the 0–1 float scale — the silence signal the cursor's
/// early-finalize logic consumes. Mirrors the Python audio callback.
pub fn block_rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64 / 32768.0;
            f * f
        })
        .sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// A speech-to-text model that can transcribe a buffer of 16 kHz mono `f32`
/// samples, optionally returning word-level timings. One impl per `transcribe-rs`
/// engine; the timing-less models naturally drive the assembler's text-fallback
/// path (`words = None`).
pub trait Asr {
    /// A short identifier for logs / the matrix report (e.g. `"parakeet-v3"`).
    fn id(&self) -> &str;

    /// Transcribe one chunk. Returns `(text, words)`; `words` is `None` when the
    /// model does not emit word timestamps.
    fn transcribe(&mut self, samples: &[f32]) -> Result<(String, Option<Vec<WordTiming>>)>;
}

/// Drives one recording session: feed captured blocks, get assembled text.
///
/// Owns the absolute-frame cursor, the timeline assembler, and a model. Each
/// time the cursor finalizes a chunk, it is transcribed and folded into the
/// assembler — exactly the wiring the daemon performs, here behind a single type
/// so the harness and the daemon share one code path.
pub struct RollingWindowSession<A: Asr> {
    cursor: SessionCursor,
    assembler: TimelineAssembler,
    asr: A,
    /// Per-chunk `(fresh_start_sec, assembled_text_after_chunk)` for inspection.
    pub chunk_log: Vec<(f64, String)>,
}

impl<A: Asr> RollingWindowSession<A> {
    pub fn new(asr: A, cfg: RollingWindowConfig) -> Self {
        // Fuzzy seam dedup zone = the chunk overlap, so any re-transcribed overlap
        // word (which lands within ~overlap of the fresh boundary) is in scope,
        // while genuine repeats further into the fresh region are left untouched.
        let seam_window = cfg.overlap_seconds;
        Self {
            cursor: SessionCursor::new(cfg),
            assembler: TimelineAssembler::new().with_fuzzy_seam(seam_window),
            asr,
            chunk_log: Vec::new(),
        }
    }

    /// Set the rolling-window hard-cut length (seconds) before the session starts.
    pub fn set_rolling_window(&mut self, seconds: f64) {
        self.cursor.set_rolling_window(seconds);
    }

    /// Push one captured block. Transcribes + folds in a chunk if the cursor
    /// finalized one. Returns the latest assembled text.
    pub fn push_block(&mut self, block: &[i16], rms: f64) -> Result<&str> {
        if let Some(chunk) = self.cursor.push_block(block, rms) {
            self.process_chunk(chunk)?;
        }
        Ok(self.assembler.text())
    }

    /// Flush the trailing audio past the cursor as the final chunk, then return
    /// the complete assembled transcript.
    pub fn finish(&mut self) -> Result<String> {
        if let Some(chunk) = self.cursor.stop() {
            self.process_chunk(chunk)?;
        }
        Ok(self.assembler.text().to_string())
    }

    fn process_chunk(&mut self, chunk: AudioChunk) -> Result<()> {
        let audio = i16_to_f32(&chunk.samples);
        let (text, words) = self.asr.transcribe(&audio)?;
        self.assembler
            .add_chunk(chunk.start_sec, chunk.fresh_start_sec, &text, words.as_deref());
        self.chunk_log
            .push((chunk.fresh_start_sec, self.assembler.text().to_string()));
        Ok(())
    }
}
