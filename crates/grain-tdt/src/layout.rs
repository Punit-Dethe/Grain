// Adapted from FluidAudio ParakeetChunkLayout/ParakeetIncrementalState.
// See ../NOTICE. Rust/file-backed planning changes are Grain-owned.

pub const SAMPLE_RATE: u64 = 16_000;
pub const FRAME_SAMPLES: u64 = 1_280;
pub const MAX_MODEL_SAMPLES: u64 = 240_000;
pub const CONTENT_SAMPLES: u64 = 238_080;
pub const OVERLAP_SAMPLES: u64 = 32_000;
pub const STRIDE_SAMPLES: u64 = CONTENT_SAMPLES - OVERLAP_SAMPLES;

/// One bounded input request. All sample coordinates refer to the journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Window {
    pub chunk_start: u64,
    pub read_start: u64,
    pub read_end: u64,
    pub is_last: bool,
}

impl Window {
    pub fn input_samples(self) -> usize {
        (self.read_end - self.read_start) as usize
    }

    /// Short-prefix inference aligns to an encoder frame when it fits the
    /// 15-second limit. The caller appends zeros after reading real samples;
    /// journal counts and audio duration must continue to use the real length.
    /// Long-path windows keep their exact input length including mel context.
    pub fn encoder_input_samples(self) -> usize {
        let input = self.input_samples() as u64;
        if self.chunk_start == 0 && self.is_last {
            let aligned = input.div_ceil(FRAME_SAMPLES) * FRAME_SAMPLES;
            if aligned <= MAX_MODEL_SAMPLES {
                return aligned as usize;
            }
        }
        input as usize
    }

    /// Fluid's short-prefix call leaves isLastChunk at its decoder default
    /// (false). Extra boundary decoding applies only to a long-path final tail.
    pub fn finalize_decoder(self) -> bool {
        self.is_last && self.chunk_start > 0
    }

    /// Skip this many encoder frames before the first decoder step.
    pub fn context_frames(self) -> usize {
        ((self.chunk_start - self.read_start) / FRAME_SAMPLES) as usize
    }

    /// Fluid clamps the decoder end to this content-frame count, *without*
    /// adding the context frame back. Preserve that contract in the adapter.
    pub fn content_frames(self) -> usize {
        (self.read_end - self.chunk_start).div_ceil(FRAME_SAMPLES) as usize
    }

    /// Fluid adds the chunk origin to local decoder-frame timestamps. The
    /// context frame is intentionally not subtracted from this origin.
    pub fn global_frame_offset(self) -> u64 {
        self.chunk_start / FRAME_SAMPLES
    }
}

/// Only the next stable-window position; recording state belongs to the caller.
/// Sample counts must be monotonic. Commit only after successful inference and
/// merging, so a failed decode cannot move the audio cursor forward.
#[derive(Default, Debug)]
pub struct WindowCursor {
    next_start: u64,
}

impl WindowCursor {
    pub fn next_stable(&self, total_samples: u64) -> Option<Window> {
        // Short audio uses the entire prefix, even after CONTENT_SAMPLES.
        if total_samples <= MAX_MODEL_SAMPLES {
            return None;
        }
        let end = self.next_start.checked_add(CONTENT_SAMPLES)?;
        (total_samples > end).then(|| self.work(end, false))
    }

    /// Advance only the stable window returned for this cursor.
    /// A stale descriptor or a final tail cannot be committed.
    pub fn commit(&mut self, window: Window) -> bool {
        let Some(end) = self.next_start.checked_add(CONTENT_SAMPLES) else {
            return false;
        };
        if window != self.work(end, false) {
            return false;
        }
        self.next_start += STRIDE_SAMPLES;
        true
    }

    /// Call after draining next_stable. Empty audio has no work. For <=15s,
    /// preserve the full prefix; longer audio uses the unfinished overlap tail.
    pub fn tail(&self, total_samples: u64) -> Option<Window> {
        if total_samples <= self.next_start || self.next_stable(total_samples).is_some() {
            return None;
        }
        Some(self.work(total_samples, true))
    }

    fn work(&self, end: u64, is_last: bool) -> Window {
        Window {
            chunk_start: self.next_start,
            read_start: self.next_start.saturating_sub(FRAME_SAMPLES),
            read_end: end,
            is_last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(cursor: &mut WindowCursor, total: u64, output: &mut Vec<Window>) {
        while let Some(work) = cursor.next_stable(total) {
            assert!(cursor.commit(work));
            output.push(work);
        }
    }

    #[test]
    fn short_audio_keeps_full_prefix_through_exactly_fifteen_seconds() {
        let cursor = WindowCursor::default();
        assert_eq!(cursor.tail(0), None);
        for total in [
            1,
            15_999,
            16_000,
            CONTENT_SAMPLES,
            CONTENT_SAMPLES + 1,
            MAX_MODEL_SAMPLES,
        ] {
            assert_eq!(cursor.next_stable(total), None);
            assert_eq!(
                cursor.tail(total),
                Some(Window {
                    chunk_start: 0,
                    read_start: 0,
                    read_end: total,
                    is_last: true,
                })
            );
        }
    }

    #[test]
    fn transition_has_one_stable_window_and_exact_overlap_tail() {
        let mut cursor = WindowCursor::default();
        let work = cursor.next_stable(240_001).unwrap();
        assert_eq!(
            work,
            Window {
                chunk_start: 0,
                read_start: 0,
                read_end: 238_080,
                is_last: false
            }
        );
        assert_eq!(cursor.tail(240_001), None); // stable work must drain first
        assert!(cursor.commit(work));
        assert!(!cursor.commit(work)); // no duplicate/stale commits
        let tail = cursor.tail(240_001).unwrap();
        assert_eq!(
            tail,
            Window {
                chunk_start: 206_080,
                read_start: 204_800,
                read_end: 240_001,
                is_last: true
            }
        );
        assert_eq!(tail.context_frames(), 1);
        assert_eq!(tail.global_frame_offset(), 161);
        assert_eq!(tail.content_frames(), 27);
        assert!(tail.finalize_decoder());
        assert!(!cursor.commit(tail));
    }

    #[test]
    fn short_prefix_alignment_does_not_change_recorded_length_or_enable_tail_loop() {
        let cursor = WindowCursor::default();
        for (samples, padded) in [
            (16_000, 16_640),
            (238_080, 238_080),
            (238_081, 239_360),
            (239_361, 239_361),
            (240_000, 240_000),
        ] {
            let work = cursor.tail(samples).unwrap();
            assert_eq!(work.input_samples(), samples as usize);
            assert_eq!(work.encoder_input_samples(), padded as usize);
            assert!(!work.finalize_decoder());
        }
        let stable = cursor.next_stable(240_001).unwrap();
        assert_eq!(stable.encoder_input_samples(), stable.input_samples());
        assert!(!stable.finalize_decoder());
    }

    #[test]
    fn later_boundary_is_strict_and_context_stays_inside_model_limit() {
        let mut cursor = WindowCursor::default();
        let first = cursor.next_stable(240_001).unwrap();
        assert!(cursor.commit(first));
        let end = STRIDE_SAMPLES + CONTENT_SAMPLES;
        assert_eq!(cursor.next_stable(end), None);
        assert_eq!(cursor.tail(end).unwrap().input_samples(), 239_360);
        let second = cursor.next_stable(end + 1).unwrap();
        assert_eq!(second.input_samples(), 239_360);
        assert_eq!(second.content_frames(), 186);
        assert_eq!(second.context_frames(), 1);
        assert!(cursor.commit(second));
        assert_eq!(cursor.tail(end + 1).unwrap().read_end, end + 1);
    }

    #[test]
    fn random_append_partitions_produce_identical_work() {
        let total = SAMPLE_RATE * 7_201 + 777;
        let mut batch = WindowCursor::default();
        let mut expected = Vec::new();
        drain(&mut batch, total, &mut expected);
        expected.push(batch.tail(total).unwrap());
        for seed in 1..=32_u64 {
            let mut cursor = WindowCursor::default();
            let mut actual = Vec::new();
            let mut count = 0;
            let mut random = seed;
            while count < total {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                count = (count + 1 + (random >> 32) % 500_000).min(total);
                drain(&mut cursor, count, &mut actual);
            }
            actual.push(cursor.tail(total).unwrap());
            assert_eq!(actual, expected, "seed {seed}");
            assert!(actual
                .iter()
                .all(|w| w.input_samples() <= MAX_MODEL_SAMPLES as usize));
        }
    }

    #[test]
    fn near_maximum_sample_count_does_not_overflow() {
        let cursor = WindowCursor {
            next_start: u64::MAX - 10,
        };
        assert_eq!(cursor.next_stable(u64::MAX), None);
        assert_eq!(cursor.tail(u64::MAX).unwrap().input_samples(), 1_290);
    }
}
