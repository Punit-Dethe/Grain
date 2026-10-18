//! NVIDIA's accuracy-oriented long-form policy: 10s left, 10s owned, 4.96s right.
//! The v2 published operating point also implements the v3 long-form guidance;
//! this is not a measured v3 WER optimum. See docs/FLOW-V3-STATEFUL-PLAN.md.
//! The encoder is stateless. Only owned frames advance the native TDT state.
use crate::{Window, FRAME_SAMPLES};

pub const BUFFERED_LEFT_FRAMES: u64 = 125;
pub const BUFFERED_OWNED_FRAMES: u64 = 125;
pub const BUFFERED_RIGHT_FRAMES: u64 = 62;
pub const BUFFERED_MAX_SAMPLES: u64 =
    (BUFFERED_LEFT_FRAMES + BUFFERED_OWNED_FRAMES + BUFFERED_RIGHT_FRAMES) * FRAME_SAMPLES;

#[derive(Default, Debug)]
pub struct BufferedCursor {
    next_start: u64,
}

impl BufferedCursor {
    pub fn next_stable(&self, total_samples: u64) -> Option<Window> {
        let end = self
            .next_start
            .checked_add((BUFFERED_OWNED_FRAMES + BUFFERED_RIGHT_FRAMES) * FRAME_SAMPLES)?;
        // At an exact end, Stop can consume the complete real tail in one call.
        (total_samples > end).then(|| self.work(end, false))
    }

    pub fn commit(&mut self, window: Window) -> bool {
        let Some(end) = self
            .next_start
            .checked_add((BUFFERED_OWNED_FRAMES + BUFFERED_RIGHT_FRAMES) * FRAME_SAMPLES)
        else {
            return false;
        };
        if window != self.work(end, false) {
            return false;
        }
        self.next_start += BUFFERED_OWNED_FRAMES * FRAME_SAMPLES;
        true
    }

    pub fn tail(&self, total_samples: u64) -> Option<Window> {
        if total_samples <= self.next_start || self.next_stable(total_samples).is_some() {
            return None;
        }
        Some(self.work(total_samples, true))
    }

    fn work(&self, end: u64, is_last: bool) -> Window {
        Window {
            chunk_start: self.next_start,
            read_start: self
                .next_start
                .saturating_sub(BUFFERED_LEFT_FRAMES * FRAME_SAMPLES),
            read_end: end,
            is_last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_geometry_and_exact_end_tail() {
        assert_eq!(BUFFERED_MAX_SAMPLES, 399_360);
        let mut cursor = BufferedCursor::default();
        assert!(cursor.tail(0).is_none());
        assert!(cursor.next_stable(239_360).is_none());
        assert_eq!(cursor.tail(239_360).unwrap().read_end, 239_360);
        let first = cursor.next_stable(239_361).unwrap();
        assert_eq!(
            (first.read_start, first.chunk_start, first.read_end),
            (0, 0, 239_360)
        );
        assert!(cursor.tail(239_361).is_none());
        assert!(cursor.commit(first));
        assert!(!cursor.commit(first));
        let second = cursor.next_stable(399_361).unwrap();
        assert_eq!(
            (second.read_start, second.chunk_start, second.read_end),
            (0, 160_000, 399_360)
        );
        assert!(cursor.commit(second));
        let tail = cursor.tail(399_361).unwrap();
        assert_eq!(
            (tail.read_start, tail.chunk_start, tail.read_end),
            (160_000, 320_000, 399_361)
        );
        assert!(!cursor.commit(tail));
    }

    #[test]
    fn partition_independent_complete_ownership_and_bounded_input() {
        for total in [
            1,
            1279,
            1280,
            1281,
            239_359,
            239_360,
            239_361,
            399_360,
            399_361,
            1_333_333,
            16_000 * 3600,
        ] {
            let plan = |partition: u64| {
                let mut cursor = BufferedCursor::default();
                let mut work = Vec::new();
                let mut available = 0;
                while available < total {
                    available = (available + partition).min(total);
                    while let Some(w) = cursor.next_stable(available) {
                        assert!(cursor.commit(w));
                        work.push(w);
                    }
                }
                if let Some(w) = cursor.tail(total) {
                    work.push(w);
                }
                work
            };
            let work = plan(total);
            assert_eq!(plan(8001), work);
            let mut owned_end = 0;
            for w in work {
                assert_eq!(w.chunk_start, owned_end);
                assert!(w.read_start.is_multiple_of(FRAME_SAMPLES));
                assert!(w.input_samples() as u64 <= BUFFERED_MAX_SAMPLES);
                owned_end = if w.is_last {
                    w.read_end
                } else {
                    w.chunk_start + BUFFERED_OWNED_FRAMES * FRAME_SAMPLES
                };
            }
            assert_eq!(owned_end, total);
        }
    }
}
