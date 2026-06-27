//! Absolute send-cursor session engine — the heart of the rolling window.
//!
//! Ported from `open_voice_router/services/chunked_audio.py` (`ChunkedAudioService`).
//!
//! Design (cursor model — the robust fix for "the last few seconds get cut off"):
//! - The ENTIRE session's audio is kept as an ordered list of blocks
//!   ([`SessionCursor::all_blocks`]) with a running frame count (`total_frames`).
//! - A single absolute cursor, `sent_frames`, marks how many leading frames have
//!   already been dispatched in a chunk.
//! - A chunk covers frames `[cursor - overlap, current_end)`. After emitting it,
//!   the cursor advances to `current_end`.
//! - On [`stop`](SessionCursor::stop), the remaining tail is exactly
//!   `[cursor, end)` (plus overlap context) — so EVERY captured frame past the
//!   cursor is guaranteed to reach the model. There is no rolling buffer to
//!   mis-juggle, so the tail can never be dropped.
//!
//! Chunk boundaries (while recording): ~`max_chunk_seconds` of unsent audio
//! (hard cut), OR `silence_min_duration` of silence after enough unsent speech
//! (early finalize). An overlap protects boundary words; the assembler dedups it.
//!
//! **Scope:** this is the pure cursor/chunking logic only. Audio capture (cpal),
//! WAV encoding, and DSP conditioning live in the integration layer — none of
//! them change frame counts, so the absolute-cursor timeline is unaffected.

/// Chunking configuration. Defaults match the Python module constants.
#[derive(Clone, Debug)]
pub struct RollingWindowConfig {
    /// Capture sample rate in Hz (frames per second).
    pub sample_rate: usize,
    /// Hard cut if no silence found (seconds). User-settable; clamped [15, 60].
    pub max_chunk_seconds: f64,
    /// Overlap context preceding the cursor (seconds) — protects boundary words.
    pub overlap_seconds: f64,
    /// RMS below this counts as silence (0–1 scale).
    pub silence_threshold_rms: f64,
    /// Contiguous trailing silence needed to early-finalize (seconds).
    pub silence_min_duration: f64,
    /// Minimum unsent speech before an early finalize is considered (seconds).
    pub early_min_seconds: f64,
    /// Absolute floor of unsent audio for early finalize (seconds).
    pub early_guard_seconds: f64,
}

impl Default for RollingWindowConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            max_chunk_seconds: 15.0,
            overlap_seconds: 2.0,
            silence_threshold_rms: 0.008,
            silence_min_duration: 0.6,
            early_min_seconds: 10.0,
            early_guard_seconds: 3.0,
        }
    }
}

/// A finalized chunk of session audio, tagged with its ABSOLUTE position on the
/// session timeline (seconds since recording started).
///
/// The timeline is ground truth from our own frame counter — it never depends on
/// any model's timestamps. `fresh_start_sec` is the send-cursor position when the
/// chunk was cut: everything before it is overlap context a previous chunk
/// already covered; everything from it to `end_sec` is new audio only this chunk
/// carries. Port of `chunked_audio.AudioChunk` (carrying raw samples instead of
/// pre-encoded WAV bytes — encoding happens at the STT boundary).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioChunk {
    pub samples: Vec<i16>,
    pub start_sec: f64,
    pub fresh_start_sec: f64,
    pub end_sec: f64,
    pub attempts: u32,
}

impl AudioChunk {
    /// Number of mono frames carried by this chunk.
    pub fn frame_count(&self) -> usize {
        self.samples.len()
    }

    pub fn fresh_duration_sec(&self) -> f64 {
        (self.end_sec - self.fresh_start_sec).max(0.0)
    }
}

/// Continuous-recording cursor with an absolute send position.
///
/// Feed captured blocks via [`push_block`](Self::push_block); it returns a chunk
/// whenever an auto-finalize boundary is crossed. Call [`stop`](Self::stop) to
/// flush the trailing audio past the cursor as the final chunk.
pub struct SessionCursor {
    cfg: RollingWindowConfig,

    // Derived frame thresholds.
    max_frames: usize,
    overlap_frames: usize,
    silence_min_frames: usize,
    early_min_frames: usize,
    early_guard_frames: usize,

    // A trailing WINDOW of the session's audio: everything from `base_frame`
    // (its absolute frame index) to the current end. Frames before
    // `sent_frames - overlap` are dropped after each emit (see `compact`),
    // because the cursor can never slice them again. This bounds memory to
    // ~one chunk + the overlap window regardless of session length.
    all_samples: Vec<i16>,
    /// Absolute frame index of `all_samples[0]`. Advanced by `compact`.
    base_frame: usize,
    /// Absolute cursor: number of leading frames already dispatched.
    sent_frames: usize,
    /// Frames of contiguous trailing silence (raw RMS based).
    silence_frames: usize,
}

impl SessionCursor {
    pub fn new(cfg: RollingWindowConfig) -> Self {
        let sr = cfg.sample_rate as f64;
        let max_frames = (cfg.max_chunk_seconds * sr) as usize;
        let overlap_frames = (cfg.overlap_seconds * sr) as usize;
        let silence_min_frames = (cfg.silence_min_duration * sr) as usize;
        let early_min_frames = (cfg.early_min_seconds * sr) as usize;
        let early_guard_frames = (cfg.early_guard_seconds * sr) as usize;
        Self {
            cfg,
            max_frames,
            overlap_frames,
            silence_min_frames,
            early_min_frames,
            early_guard_frames,
            all_samples: Vec::new(),
            base_frame: 0,
            sent_frames: 0,
            silence_frames: 0,
        }
    }

    /// Total absolute frames the session has seen (including dropped ones).
    fn total_frames(&self) -> usize {
        self.base_frame + self.all_samples.len()
    }

    /// Drop leading frames the cursor can never reference again. The earliest
    /// frame any future chunk needs is `sent_frames - overlap_frames` (the
    /// overlap context preceding the cursor); everything before that is dead.
    /// Advances `base_frame` so absolute frame math stays correct.
    fn compact(&mut self) {
        let keep_from = self.sent_frames.saturating_sub(self.overlap_frames);
        if keep_from > self.base_frame {
            let drop = keep_from - self.base_frame;
            // `drop` is always <= all_samples.len(): sent_frames <= total_frames
            // = base_frame + all_samples.len(), so keep_from - base_frame is
            // bounded by all_samples.len().
            let drop = drop.min(self.all_samples.len());
            self.all_samples.drain(..drop);
            self.base_frame += drop;
        }
    }

    /// Set the rolling-window hard-cut length (seconds) for the NEXT session.
    /// Clamped to `[15, 60]` defensively so a bad value can never produce a
    /// degenerate chunk size.
    pub fn set_rolling_window(&mut self, seconds: f64) {
        let clamped = seconds.clamp(15.0, 60.0);
        self.max_frames = (clamped * self.cfg.sample_rate as f64) as usize;
    }

    /// Clear all state so a new session starts completely fresh.
    pub fn reset(&mut self) {
        self.all_samples.clear();
        self.base_frame = 0;
        self.sent_frames = 0;
        self.silence_frames = 0;
    }

    /// The assembled transcript text region cursor — current end of session, sec.
    pub fn total_sec(&self) -> f64 {
        self.frame_to_sec(self.total_frames())
    }

    /// Push one captured block with its raw RMS. Returns a finalized chunk if an
    /// auto-finalize boundary (hard cut or silence-gap early finalize) is crossed.
    pub fn push_block(&mut self, block: &[i16], raw_rms: f64) -> Option<AudioChunk> {
        self.all_samples.extend_from_slice(block);

        // Silence tracking uses RAW rms so quiet end-of-sentence speech is not
        // mistaken for silence.
        if raw_rms < self.cfg.silence_threshold_rms {
            self.silence_frames += block.len();
        } else {
            self.silence_frames = 0;
        }

        let unsent = self.total_frames() - self.sent_frames;
        let should_finalize = unsent >= self.max_frames
            || (unsent >= self.early_min_frames
                && self.silence_frames >= self.silence_min_frames
                && unsent >= self.early_guard_frames);

        if should_finalize {
            self.emit_chunk()
        } else {
            None
        }
    }

    /// Emit frames `[cursor - overlap, total)` as a chunk and advance the cursor.
    ///
    /// Port of `_emit_chunk_locked`. The overlap before the cursor protects
    /// boundary words; the assembler removes the duplicated region.
    pub fn emit_chunk(&mut self) -> Option<AudioChunk> {
        let end = self.total_frames();
        if end <= self.sent_frames {
            return None;
        }
        let start_frame = self.sent_frames.saturating_sub(self.overlap_frames);
        let fresh_start_frame = self.sent_frames;
        let samples = self.slice_frames(start_frame, end);
        if samples.is_empty() {
            return None;
        }
        let chunk = AudioChunk {
            samples,
            start_sec: self.frame_to_sec(start_frame),
            fresh_start_sec: self.frame_to_sec(fresh_start_frame),
            end_sec: self.frame_to_sec(end),
            attempts: 0,
        };
        self.sent_frames = end;
        self.silence_frames = 0;
        // Release frames before the new overlap window — they can't be sliced again.
        self.compact();
        Some(chunk)
    }

    /// Flush the trailing audio past the cursor (plus overlap context) as the
    /// final chunk. Port of `stop`'s flush. Returns `None` only if the range is
    /// empty (no audio at all).
    pub fn stop(&mut self) -> Option<AudioChunk> {
        let start_frame = self.sent_frames.saturating_sub(self.overlap_frames);
        let fresh_start_frame = self.sent_frames;
        let end_frame = self.total_frames();
        let samples = self.slice_frames(start_frame, end_frame);
        // Cursor now covers the whole session — nothing left unsent.
        self.sent_frames = self.total_frames();
        if samples.is_empty() {
            return None;
        }
        Some(AudioChunk {
            samples,
            start_sec: self.frame_to_sec(start_frame),
            fresh_start_sec: self.frame_to_sec(fresh_start_frame),
            end_sec: self.frame_to_sec(end_frame),
            attempts: 0,
        })
    }

    fn frame_to_sec(&self, frame: usize) -> f64 {
        frame as f64 / self.cfg.sample_rate as f64
    }

    /// Slice the session by ABSOLUTE frame range `[start_frame, end_frame)`,
    /// translating through `base_frame` into the retained buffer. A range that
    /// starts before `base_frame` (already-compacted audio) is clamped to what
    /// remains — the cursor never asks for dropped frames, so in practice the
    /// requested start is always >= base_frame.
    fn slice_frames(&self, start_frame: usize, end_frame: usize) -> Vec<i16> {
        let total = self.total_frames();
        if end_frame <= start_frame || start_frame >= total {
            return Vec::new();
        }
        let end = end_frame.min(total);
        // Translate absolute -> buffer-relative, clamping a start that predates
        // the retained window to 0 (defensive; the cursor doesn't do this).
        let rel_start = start_frame.saturating_sub(self.base_frame);
        let rel_end = end.saturating_sub(self.base_frame);
        if rel_end <= rel_start || rel_start >= self.all_samples.len() {
            return Vec::new();
        }
        let rel_end = rel_end.min(self.all_samples.len());
        self.all_samples[rel_start..rel_end].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur() -> SessionCursor {
        SessionCursor::new(RollingWindowConfig::default())
    }

    fn fps(c: &SessionCursor) -> usize {
        c.cfg.sample_rate
    }

    fn block(n: usize, value: i16) -> Vec<i16> {
        vec![value; n]
    }

    #[test]
    fn set_rolling_window_sets_max_frames() {
        let mut s = cur();
        s.set_rolling_window(30.0);
        assert_eq!(s.max_frames, 30 * fps(&s));
    }

    #[test]
    fn set_rolling_window_clamps() {
        for (given, expected_s) in [
            (5.0, 15),
            (14.0, 15),
            (15.0, 15),
            (60.0, 60),
            (120.0, 60),
            (10.0, 15),
        ] {
            let mut s = cur();
            s.set_rolling_window(given);
            assert_eq!(s.max_frames, expected_s * fps(&s), "given {given}");
        }
    }

    #[test]
    fn slice_is_frame_exact() {
        let mut s = cur();
        s.all_samples = vec![block(100, 1000), block(100, 1000), block(100, 1000)].concat();
        let frames = s.slice_frames(50, 250);
        assert_eq!(frames.len(), 200); // exactly [50, 250)
    }

    #[test]
    fn slice_within_single_block() {
        let mut s = cur();
        s.all_samples = block(100, 1000);
        let frames = s.slice_frames(20, 60);
        assert_eq!(frames.len(), 40);
    }

    #[test]
    fn emit_chunk_advances_cursor() {
        let mut s = cur();
        let f = fps(&s);
        s.all_samples = vec![block(f, 1000); 12].concat(); // 12s of audio
        let chunk = s.emit_chunk();
        assert!(chunk.is_some());
        assert_eq!(s.sent_frames, 12 * f); // cursor advanced to the end
    }

    #[test]
    fn stop_flushes_trailing_audio() {
        let mut s = cur();
        let f = fps(&s);
        // 12s already sent, then 3 more seconds captured but never chunked.
        s.all_samples = vec![block(f, 1000); 15].concat();
        s.sent_frames = 12 * f; // 3s unsent tail
        let chunk = s.stop().expect("tail flushed");
        // Tail = unsent 3s + 2s overlap context = 5s.
        assert_eq!(chunk.frame_count(), (3 + 2) * f);
        assert_eq!(s.sent_frames, s.all_samples.len());
    }

    #[test]
    fn stop_with_no_unsent_audio_still_safe() {
        let mut s = cur();
        let f = fps(&s);
        s.all_samples = vec![block(f, 1000); 10].concat();
        s.sent_frames = 10 * f; // nothing unsent
        let chunk = s.stop().expect("overlap window emitted");
        assert_eq!(chunk.frame_count(), s.overlap_frames);
    }

    /// Feed `secs` seconds of audio one 1s block at a time WITHOUT tripping an
    /// auto-finalize (loud blocks, well under max_chunk_seconds), collecting any
    /// emitted chunks. Used to drive the cursor through its real API.
    fn feed_secs(s: &mut SessionCursor, secs: usize) -> Vec<AudioChunk> {
        let f = fps(s);
        let mut out = Vec::new();
        for _ in 0..secs {
            if let Some(c) = s.push_block(&block(f, 1000), 0.5) {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn no_frames_dropped_across_chunk_and_stop() {
        let mut s = cur();
        let f = fps(&s);
        // 12s in, force a chunk; then 13s more, then stop. Total 25s.
        feed_secs(&mut s, 12);
        let first = s.emit_chunk().expect("chunk at 12s");
        assert_eq!(first.end_sec, 12.0);
        feed_secs(&mut s, 13);
        let tail = s.stop().expect("tail flushed");
        // Tail covers cursor(12s) - overlap(2s) to 25s = 15s.
        assert_eq!(tail.frame_count(), (25 - 12 + 2) * f);
        // Fresh regions tile the whole session with no gap or overlap.
        assert_eq!(tail.fresh_start_sec, first.end_sec);
        assert_eq!(tail.end_sec, 25.0);
    }

    #[test]
    fn chunks_carry_exact_timeline_metadata() {
        let mut s = cur();
        let f = fps(&s);
        feed_secs(&mut s, 12);
        let first = s.emit_chunk().unwrap();
        feed_secs(&mut s, 13);
        let tail = s.stop().unwrap();

        // First chunk: no overlap context exists yet — starts at 0.
        assert_eq!(first.start_sec, 0.0);
        assert_eq!(first.fresh_start_sec, 0.0);
        assert_eq!(first.end_sec, 12.0);
        // Tail: 2s overlap context before the cursor, fresh from 12s to 25s.
        assert_eq!(tail.start_sec, 12.0 - s.cfg.overlap_seconds);
        assert_eq!(tail.fresh_start_sec, 12.0);
        assert_eq!(tail.end_sec, 25.0);
        // Fresh regions tile the whole session exactly.
        assert_eq!(tail.fresh_start_sec, first.end_sec);
        // Payload length matches the tagged range.
        assert_eq!(
            first.frame_count(),
            ((first.end_sec - first.start_sec) * f as f64) as usize
        );
        assert_eq!(
            tail.frame_count(),
            ((tail.end_sec - tail.start_sec) * f as f64) as usize
        );
    }

    #[test]
    fn buffer_stays_bounded_over_long_session() {
        // Drive many auto-finalized chunks; the retained buffer must never grow
        // past ~one max chunk + overlap, no matter how long the session runs.
        let mut s = cur();
        let f = fps(&s);
        let bound = s.max_frames + s.overlap_frames + f; // +1s slack for the in-flight block
                                                         // 5 minutes of audio in 1s loud blocks (no silence early-finalize).
        for sec in 0..300 {
            s.push_block(&block(f, 1000), 0.5);
            assert!(
                s.all_samples.len() <= bound,
                "retained buffer {} exceeded bound {} at {}s",
                s.all_samples.len(),
                bound,
                sec
            );
        }
        // The absolute timeline still reflects the full session length.
        assert_eq!(s.total_frames(), 300 * f);
        assert!(
            s.base_frame > 0,
            "compaction should have advanced base_frame"
        );
    }

    #[test]
    fn compaction_preserves_absolute_timeline() {
        // After compaction, emitted chunks must still carry correct ABSOLUTE
        // timestamps (driven by base_frame), tiling the session with no gaps.
        let mut s = cur();
        let mut prev_end = 0.0_f64;
        let mut chunks = Vec::new();
        for _ in 0..60 {
            // 60s; max_chunk is 15s so ~4 auto chunks fire.
            if let Some(c) = s.push_block(&block(fps(&s), 1000), 0.5) {
                chunks.push(c);
            }
        }
        if let Some(c) = s.stop() {
            chunks.push(c);
        }
        for c in &chunks {
            // Each chunk's fresh region starts exactly where the previous ended.
            assert!(
                (c.fresh_start_sec - prev_end).abs() < 1e-6,
                "gap at {prev_end}"
            );
            prev_end = c.end_sec;
        }
        assert!(
            (prev_end - 60.0).abs() < 1e-6,
            "session should tile to 60s, got {prev_end}"
        );
    }

    // -- cached-cursor slicer equivalence ---------------------------------

    /// Deterministic LCG so the long-session fuzz is reproducible without a dep.
    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn range(&mut self, lo: i64, hi: i64) -> i64 {
            lo + (self.next_u64() % ((hi - lo) as u64)) as i64
        }
    }

    /// Full-scan reference slicer (the pre-optimization algorithm).
    fn reference_slice(blocks: &[Vec<i16>], start_frame: usize, end_frame: usize) -> Vec<i16> {
        if end_frame <= start_frame {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut pos = 0usize;
        for blk in blocks {
            let blk_len = blk.len();
            let blk_start = pos;
            let blk_end = pos + blk_len;
            pos = blk_end;
            if blk_end <= start_frame {
                continue;
            }
            if blk_start >= end_frame {
                break;
            }
            let lo = start_frame.saturating_sub(blk_start);
            let hi = (end_frame - blk_start).min(blk_len);
            if hi > lo {
                out.extend_from_slice(&blk[lo..hi]);
            }
        }
        out
    }

    #[test]
    fn cursor_slice_matches_full_scan_over_long_session() {
        let mut rng = Lcg(7);
        let blocks: Vec<Vec<i16>> = (0..1000)
            .map(|_| {
                let n = rng.range(700, 900) as usize;
                let v = rng.range(-5000, 5000) as i16;
                block(n, v)
            })
            .collect();

        let all_samples = blocks.concat();
        let total = all_samples.len();

        let mut s = cur();
        s.all_samples = all_samples;

        // Walk forward in overlapping windows, exactly like emit/stop do.
        let step = 800 * 20; // ~20s of unsent audio per chunk
        let overlap = s.overlap_frames;
        let mut cursor = 0usize;
        while cursor < total {
            let end = (cursor + step).min(total);
            let start = cursor.saturating_sub(overlap);
            let got = s.slice_frames(start, end);
            let reference = reference_slice(&blocks, start, end);
            assert_eq!(got, reference, "mismatch at [{start},{end})");
            cursor = end;
        }
    }
}
