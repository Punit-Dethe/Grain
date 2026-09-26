//! Grain-owned bounded windows for ordinary Parakeet v2 inference.
//! Every start/cut stays on the encoder's subsampling phase. Quiet detection
//! only chooses a cut near the ceiling; it never removes samples or skips ASR.
use crate::{Window, FRAME_SAMPLES, OVERLAP_SAMPLES};

pub const NATIVE_MAX_SAMPLES: u64 = 384_000; // 24 s / 300 encoder frames
const SEARCH_FRAMES: usize = 38; // 3.04 s before the hard ceiling
const QUIET_FRAMES: usize = 9; // 720 ms of consecutive quiet
const QUIET_MEAN_SQUARE: f64 = 0.008 * 0.008;
const CUT_MARGIN_SAMPLES: u64 = (SEARCH_FRAMES - QUIET_FRAMES) as u64 * FRAME_SAMPLES;

#[derive(Debug)]
pub struct NativeWindowCursor {
    next_start: u64,
    max_samples: u64,
}

impl Default for NativeWindowCursor {
    fn default() -> Self {
        Self {
            next_start: 0,
            max_samples: NATIVE_MAX_SAMPLES,
        }
    }
}

impl NativeWindowCursor {
    /// Bounded alternative ceilings for controlled accuracy/latency evaluation.
    pub fn with_max_samples(max_samples: u64) -> Option<Self> {
        ((240_640..=480_000).contains(&max_samples) && max_samples.is_multiple_of(FRAME_SAMPLES))
            .then_some(Self {
                next_start: 0,
                max_samples,
            })
    }

    /// Read this bounded range once. Stop uses a complete prefix up to the ceiling.
    pub fn next_stable(&self, total: u64) -> Option<Window> {
        let end = self.next_start.checked_add(self.max_samples)?;
        (total > end).then(|| self.work(end, false))
    }

    /// Prefer the first complete quiet interval in the last 3.04 s. The caller
    /// truncates its already-read buffer to the returned range before decoding.
    /// Never consult samples beyond the bounded input, even when stopping.
    pub fn prefer_pause(&self, input: Window, pcm: &[f32]) -> Option<Window> {
        let end = self.next_start.checked_add(self.max_samples)?;
        if input != self.work(end, false) || pcm.len() as u64 != self.max_samples {
            return None;
        }
        let search = pcm.len() - SEARCH_FRAMES * FRAME_SAMPLES as usize;
        let mut quiet = 0;
        for (index, frame) in pcm[search..]
            .chunks_exact(FRAME_SAMPLES as usize)
            .enumerate()
        {
            let energy =
                frame.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / FRAME_SAMPLES as f64;
            quiet = if energy.is_finite() && energy <= QUIET_MEAN_SQUARE {
                quiet + 1
            } else {
                0
            };
            if quiet >= QUIET_FRAMES {
                let count = search as u64 + (index as u64 + 1) * FRAME_SAMPLES;
                return Some(self.work(self.next_start.checked_add(count)?, false));
            }
        }
        Some(input)
    }

    /// Only successful inference/merging advances the cursor. All samples
    /// remain covered, with exactly two seconds repeated across adjacent calls.
    pub fn commit(&mut self, window: Window) -> bool {
        let Some(count) = window.read_end.checked_sub(self.next_start) else {
            return false;
        };
        if window != self.work(window.read_end, false)
            || !(self.max_samples - CUT_MARGIN_SAMPLES..=self.max_samples).contains(&count)
            || count % FRAME_SAMPLES != 0
        {
            return false;
        }
        self.next_start = window.read_end - OVERLAP_SAMPLES;
        true
    }

    pub fn tail(&self, total: u64) -> Option<Window> {
        if total <= self.next_start || self.next_stable(total).is_some() {
            return None;
        }
        Some(self.work(total, true))
    }

    fn work(&self, end: u64, is_last: bool) -> Window {
        Window {
            chunk_start: self.next_start,
            read_start: self.next_start,
            read_end: end,
            is_last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternative_ceilings_are_bounded_aligned_and_keep_exact_overlap() {
        for limit in [
            240_640, 320_000, 384_000, 416_000, 448_000, 464_640, 480_000,
        ] {
            let mut cursor = NativeWindowCursor::with_max_samples(limit).unwrap();
            assert!(cursor.next_stable(limit).is_none());
            let input = cursor.next_stable(limit + 1).unwrap();
            let cut = cursor
                .prefer_pause(input, &vec![0.0; limit as usize])
                .unwrap();
            assert_eq!(cut.read_end, limit - CUT_MARGIN_SAMPLES);
            assert!(cursor.commit(cut));
            let tail = cursor.tail(limit + 1).unwrap();
            assert_eq!(cut.read_end - tail.read_start, OVERLAP_SAMPLES);
        }
        for limit in [0, 32_000, 240_000, 320_001, 480_001, u64::MAX] {
            assert!(NativeWindowCursor::with_max_samples(limit).is_none());
        }
    }

    #[test]
    fn short_recordings_keep_exact_batch_input_without_padding() {
        let cursor = NativeWindowCursor::default();
        assert_eq!(cursor.tail(0), None);
        for total in [1, 16_000, 240_001, NATIVE_MAX_SAMPLES] {
            assert_eq!(cursor.next_stable(total), None);
            assert_eq!(cursor.tail(total).unwrap().input_samples(), total as usize);
        }
    }

    #[test]
    fn quiet_cut_preserves_phase_overlap_and_exact_tail() {
        let mut cursor = NativeWindowCursor::default();
        let input = cursor.next_stable(NATIVE_MAX_SAMPLES + 1).unwrap();
        let cut = cursor
            .prefer_pause(input, &vec![0.0; input.input_samples()])
            .unwrap();
        assert_eq!(cut.read_end, NATIVE_MAX_SAMPLES - CUT_MARGIN_SAMPLES);
        assert!(cursor.commit(cut));
        assert!(!cursor.commit(cut));
        let tail = cursor.tail(NATIVE_MAX_SAMPLES + 1).unwrap();
        assert_eq!(tail.read_start, cut.read_end - OVERLAP_SAMPLES);
        assert_eq!(tail.read_start % FRAME_SAMPLES, 0);
        assert_eq!(tail.read_end, NATIVE_MAX_SAMPLES + 1);
        assert!(!cursor.commit(tail));
    }

    #[test]
    fn continuous_and_nonfinite_audio_use_hard_bound() {
        let cursor = NativeWindowCursor::default();
        let input = cursor.next_stable(NATIVE_MAX_SAMPLES + 1).unwrap();
        for level in [0.1, f32::NAN, f32::INFINITY] {
            assert_eq!(
                cursor.prefer_pause(input, &vec![level; input.input_samples()]),
                Some(input)
            );
        }
        assert_eq!(cursor.prefer_pause(input, &[0.0; 1]), None);
    }

    #[test]
    fn brief_pause_does_not_cut_and_invalid_commits_do_not_advance() {
        let mut cursor = NativeWindowCursor::default();
        let input = cursor.next_stable(NATIVE_MAX_SAMPLES + 1).unwrap();
        let mut pcm = vec![0.1; input.input_samples()];
        let first = pcm.len() - SEARCH_FRAMES * FRAME_SAMPLES as usize;
        pcm[first..first + (QUIET_FRAMES - 1) * FRAME_SAMPLES as usize].fill(0.0);
        assert_eq!(cursor.prefer_pause(input, &pcm), Some(input));
        assert!(!cursor.commit(Window {
            read_end: input.read_end - 1,
            ..input
        }));
        assert_eq!(cursor.next_stable(NATIVE_MAX_SAMPLES + 1), Some(input));
    }

    #[test]
    fn append_partitions_do_not_change_cuts_or_coverage() {
        let total = NATIVE_MAX_SAMPLES * 7 + 777;
        let pcm = vec![0.0; total as usize];
        let plan = |step: u64| {
            let mut cursor = NativeWindowCursor::default();
            let mut work = Vec::new();
            let mut available = 0;
            while available < total {
                available = (available + step).min(total);
                while let Some(input) = cursor.next_stable(available) {
                    let window = cursor
                        .prefer_pause(
                            input,
                            &pcm[input.read_start as usize..input.read_end as usize],
                        )
                        .unwrap();
                    assert!(cursor.commit(window));
                    work.push(window);
                }
            }
            work.push(cursor.tail(total).unwrap());
            work
        };
        let expected = plan(total);
        for step in [1_279, 8_001, 480_001] {
            let actual = plan(step);
            assert_eq!(actual, expected);
            assert_eq!(actual.first().unwrap().read_start, 0);
            assert_eq!(actual.last().unwrap().read_end, total);
            for pair in actual.windows(2) {
                assert_eq!(pair[0].read_end - pair[1].read_start, OVERLAP_SAMPLES);
            }
            assert!(actual
                .iter()
                .all(|w| w.input_samples() <= NATIVE_MAX_SAMPLES as usize));
        }
    }
}
