//! Native ASR audio input policy: pre-roll look-back + a bounded, drop-on-overflow
//! frame queue.
//!
//! These are the pure data structures that sit between the host's audio fan-out
//! (the producer, running on the capture thread) and the Native ASR worker (the
//! consumer). They encode two non-negotiable rules from the plan:
//!
//! * **Never clip speech onset.** A [`PreRollRing`] keeps the last few hundred
//!   milliseconds of audio so a session that opens mid-utterance (or after a VAD
//!   gate) can prepend the look-back.
//! * **Never block the capture thread.** A [`BoundedFrameQueue`] drops the
//!   *oldest* frame on overflow instead of blocking the producer, and counts the
//!   loss so the worker can raise a degraded-session signal. Losing the
//!   least-recent audio is strictly better than stalling the microphone.
//!
//! The host owns the wiring (real `AudioRecorder` → fan-out → these → worker);
//! this module owns only the policy, so it is unit-testable with no audio
//! backend, thread, or Tauri.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::session::AudioFrame;

/// A fixed-duration look-back of the most recent audio, bounded by a total
/// sample budget. Pushing past the budget evicts the oldest frames.
///
/// Single-threaded by design: the audio fan-out owns it and drains it into a
/// fresh session at open time. Keeping it lock-free keeps the capture path cheap.
pub struct PreRollRing {
    frames: VecDeque<AudioFrame>,
    /// Total samples currently buffered across all frames.
    samples: usize,
    /// Sample budget (derived from the look-back duration + sample rate).
    max_samples: usize,
}

impl PreRollRing {
    /// A ring sized to hold roughly `millis` of audio at `sample_rate_hz`.
    pub fn with_duration(sample_rate_hz: u32, millis: u32) -> Self {
        let max_samples = (sample_rate_hz as u64 * millis as u64 / 1000) as usize;
        Self {
            frames: VecDeque::new(),
            samples: 0,
            max_samples,
        }
    }

    /// Append a frame, evicting the oldest frames until back within budget. At
    /// least one frame is always retained, so a single oversized frame is kept
    /// rather than silently dropped.
    pub fn push(&mut self, frame: AudioFrame) {
        self.samples += frame.samples.len();
        self.frames.push_back(frame);
        while self.samples > self.max_samples && self.frames.len() > 1 {
            if let Some(f) = self.frames.pop_front() {
                self.samples -= f.samples.len();
            }
        }
    }

    /// Samples currently buffered.
    pub fn len_samples(&self) -> usize {
        self.samples
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Take all buffered frames (oldest → newest), clearing the ring. Call this
    /// when a session opens to prepend the captured onset.
    pub fn drain(&mut self) -> Vec<AudioFrame> {
        self.samples = 0;
        self.frames.drain(..).collect()
    }

    /// Drop everything (e.g. on a long silence, so stale audio is never
    /// prepended to a later utterance).
    pub fn clear(&mut self) {
        self.frames.clear();
        self.samples = 0;
    }
}

/// What happened when a frame was enqueued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// The queue had room; the frame was enqueued without loss.
    Queued,
    /// The queue was full: the OLDEST frame was dropped to make room. The
    /// producer did not block. Sustained `DroppedOldest` means the consumer is
    /// not keeping up and the session should be flagged degraded.
    DroppedOldest,
}

/// A bounded FIFO of audio frames between the capture-thread producer and the
/// Native ASR worker. `push` never blocks; on overflow it drops the oldest frame
/// and increments a counter.
///
/// A `Mutex<VecDeque>` (not a lock-free ring) is deliberate: the critical
/// section is a single `push_back`/`pop_front`, it is never held across work,
/// and Grain's existing audio callback already takes a comparable short lock
/// (`RollingTranscriber::feed`). Correctness and simplicity beat a hand-rolled
/// SPSC ring here.
pub struct BoundedFrameQueue {
    inner: Mutex<VecDeque<AudioFrame>>,
    capacity: usize,
    dropped: AtomicU64,
}

impl BoundedFrameQueue {
    /// Create a queue holding at most `capacity` frames.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "BoundedFrameQueue capacity must be > 0");
        Self {
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            dropped: AtomicU64::new(0),
        }
    }

    /// Producer side. Enqueue a frame, dropping the oldest if full. Never blocks
    /// on backpressure (only the brief internal lock). Returns the outcome so the
    /// caller can update metrics / raise a degraded signal.
    pub fn push(&self, frame: AudioFrame) -> PushOutcome {
        let mut q = self.inner.lock().unwrap();
        let outcome = if q.len() >= self.capacity {
            q.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
            PushOutcome::DroppedOldest
        } else {
            PushOutcome::Queued
        };
        q.push_back(frame);
        outcome
    }

    /// Consumer side. Take the next frame, or `None` if the queue is empty.
    pub fn pop(&self) -> Option<AudioFrame> {
        self.inner.lock().unwrap().pop_front()
    }

    /// Current queue depth (for metrics / overflow watch).
    pub fn depth(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Total frames dropped to overflow since creation.
    pub fn dropped_total(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn frame(n: usize) -> AudioFrame {
        AudioFrame::new(Arc::from(vec![0.0f32; n].into_boxed_slice()), 16_000)
    }

    #[test]
    fn preroll_evicts_oldest_past_budget() {
        // 16 kHz, 100 ms → 1600 sample budget.
        let mut ring = PreRollRing::with_duration(16_000, 100);
        for _ in 0..20 {
            ring.push(frame(160)); // 10 ms frames
        }
        // 20 * 160 = 3200 samples pushed, budget 1600 → keep ~the last 1600.
        assert!(ring.len_samples() <= 1600);
        assert!(ring.len_samples() >= 1600 - 160);
    }

    #[test]
    fn preroll_drain_is_oldest_to_newest_and_clears() {
        let mut ring = PreRollRing::with_duration(16_000, 1000);
        ring.push(AudioFrame::new(Arc::from(vec![1.0f32].into_boxed_slice()), 16_000));
        ring.push(AudioFrame::new(Arc::from(vec![2.0f32].into_boxed_slice()), 16_000));
        ring.push(AudioFrame::new(Arc::from(vec![3.0f32].into_boxed_slice()), 16_000));
        let drained = ring.drain();
        let firsts: Vec<f32> = drained.iter().map(|f| f.samples[0]).collect();
        assert_eq!(firsts, vec![1.0, 2.0, 3.0]);
        assert!(ring.is_empty());
        assert_eq!(ring.len_samples(), 0);
    }

    #[test]
    fn preroll_retains_single_oversized_frame() {
        let mut ring = PreRollRing::with_duration(16_000, 10); // 160 sample budget
        ring.push(frame(5000)); // one frame far over budget
        assert!(!ring.is_empty(), "must keep at least one frame");
        assert_eq!(ring.len_samples(), 5000);
    }

    #[test]
    fn queue_is_fifo_until_full() {
        let q = BoundedFrameQueue::new(4);
        for i in 1..=3 {
            assert_eq!(q.push(frame(i)), PushOutcome::Queued);
        }
        assert_eq!(q.depth(), 3);
        assert_eq!(q.pop().unwrap().samples.len(), 1);
        assert_eq!(q.pop().unwrap().samples.len(), 2);
        assert_eq!(q.pop().unwrap().samples.len(), 3);
        assert!(q.pop().is_none());
    }

    #[test]
    fn queue_drops_oldest_on_overflow_and_counts() {
        let q = BoundedFrameQueue::new(2);
        assert_eq!(q.push(frame(10)), PushOutcome::Queued);
        assert_eq!(q.push(frame(20)), PushOutcome::Queued);
        // Full → next push drops the oldest (the 10-sample frame).
        assert_eq!(q.push(frame(30)), PushOutcome::DroppedOldest);
        assert_eq!(q.depth(), 2);
        assert_eq!(q.dropped_total(), 1);
        // Remaining frames are the two newest, in order.
        assert_eq!(q.pop().unwrap().samples.len(), 20);
        assert_eq!(q.pop().unwrap().samples.len(), 30);
    }

    #[test]
    fn queue_never_exceeds_capacity_under_sustained_overflow() {
        let q = BoundedFrameQueue::new(3);
        for i in 0..100 {
            q.push(frame(i % 7 + 1));
            assert!(q.depth() <= 3, "depth must never exceed capacity");
        }
        assert_eq!(q.dropped_total(), 100 - 3);
    }
}
