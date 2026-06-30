//! [GRAIN] Native ASR audio input (Milestone 3): the capture-thread fan-out sink.
//!
//! Owns the pre-roll look-back ring + the bounded drop-on-overflow queue from
//! `grain-asr-core`. The audio callback ([`crate::managers::audio`]) pushes
//! frames here via [`NativeAsrInput::feed`] without ever blocking; the Native ASR
//! worker ([`super::worker`]) drains them via [`NativeAsrInput::next_frame`].
//!
//! A cheap `armed` atomic gates ALL work, so when Native ASR is not in use the
//! fan-out costs a single relaxed atomic load per frame and nothing else —
//! honoring the "no overhead / destroy if not in use" rule. Frames are only
//! buffered between [`arm`](NativeAsrInput::arm) and
//! [`disarm`](NativeAsrInput::disarm).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use grain_asr_core::audio_input::{BoundedFrameQueue, PreRollRing};
use grain_asr_core::session::AudioFrame;

/// Frame fan-out sink for Native ASR, held in Tauri managed state.
///
/// Lifecycle: `arm()` (start look-back) → `open_session()` (commit pre-roll,
/// route live frames to the queue) → worker drains `next_frame()` → `disarm()`
/// (stop + clear). Closed/disarmed is the resting state.
pub struct NativeAsrInput {
    sample_rate_hz: u32,
    /// Gate: when false, `feed` is a no-op (one atomic load on the audio thread).
    armed: AtomicBool,
    /// Once true, live frames route to the queue instead of the pre-roll ring.
    session_active: AtomicBool,
    /// Look-back captured while armed but before the session opens.
    preroll: Mutex<PreRollRing>,
    /// Live frames awaiting the worker. Drops oldest on overflow (never blocks).
    queue: BoundedFrameQueue,
}

// The session-control + consumer methods (`arm`/`disarm`/`open_session`/
// `next_frame`/metrics) are driven by the Native ASR worker/manager. `new`/`feed`
// are already live (managed state + audio fan-out); the rest are wired by the
// Native ASR action in Milestone 7.
#[allow(dead_code)]
impl NativeAsrInput {
    /// `preroll_ms` of look-back, `queue_capacity` frames of in-flight buffer.
    pub fn new(sample_rate_hz: u32, preroll_ms: u32, queue_capacity: usize) -> Self {
        Self {
            sample_rate_hz,
            armed: AtomicBool::new(false),
            session_active: AtomicBool::new(false),
            preroll: Mutex::new(PreRollRing::with_duration(sample_rate_hz, preroll_ms)),
            queue: BoundedFrameQueue::new(queue_capacity),
        }
    }

    /// Start capturing look-back audio (before the session opens).
    pub fn arm(&self) {
        self.preroll.lock().unwrap().clear();
        self.armed.store(true, Ordering::Release);
    }

    /// Stop all capture and clear every buffer (session ended / cancelled).
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
        self.session_active.store(false, Ordering::Release);
        self.preroll.lock().unwrap().clear();
        while self.queue.pop().is_some() {}
    }

    /// Open the live session: move the captured pre-roll into the queue (so the
    /// onset is not clipped) and route subsequent frames to the queue. Returns
    /// the number of pre-roll frames carried over.
    pub fn open_session(&self) -> usize {
        let preroll = self.preroll.lock().unwrap().drain();
        let n = preroll.len();
        for f in preroll {
            self.queue.push(f);
        }
        self.session_active.store(true, Ordering::Release);
        n
    }

    /// Capture-thread fan-out. Non-blocking; no-op unless armed. Copies the
    /// borrowed frame once into a shared `Arc<[f32]>` for zero-copy downstream.
    pub fn feed(&self, frame: &[f32]) {
        if !self.armed.load(Ordering::Acquire) {
            return;
        }
        let af = AudioFrame::new(frame, self.sample_rate_hz);
        if self.session_active.load(Ordering::Acquire) {
            self.queue.push(af);
        } else {
            self.preroll.lock().unwrap().push(af);
        }
    }

    /// Consumer side (Native ASR worker): the next captured frame, if any.
    pub fn next_frame(&self) -> Option<AudioFrame> {
        self.queue.pop()
    }

    /// Frames dropped to queue overflow since creation (degraded-session metric).
    pub fn dropped_total(&self) -> u64 {
        self.queue.dropped_total()
    }

    /// Current queue depth (overflow watch / metrics).
    pub fn queue_depth(&self) -> usize {
        self.queue.depth()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> NativeAsrInput {
        NativeAsrInput::new(16_000, 400, 64)
    }

    #[test]
    fn feed_is_noop_until_armed() {
        let i = input();
        i.feed(&[0.1; 160]);
        assert_eq!(i.queue_depth(), 0);
        assert!(i.next_frame().is_none());
    }

    #[test]
    fn preroll_is_carried_into_session() {
        let i = input();
        i.arm();
        i.feed(&[0.1; 160]);
        i.feed(&[0.2; 160]);
        let carried = i.open_session();
        assert_eq!(carried, 2, "pre-roll frames must carry into the session");
        i.feed(&[0.3; 160]);
        assert_eq!(i.queue_depth(), 3);
        assert_eq!(i.next_frame().unwrap().samples[0], 0.1);
        assert_eq!(i.next_frame().unwrap().samples[0], 0.2);
        assert_eq!(i.next_frame().unwrap().samples[0], 0.3);
    }

    #[test]
    fn disarm_clears_everything() {
        let i = input();
        i.arm();
        i.open_session();
        i.feed(&[0.5; 160]);
        i.disarm();
        assert_eq!(i.queue_depth(), 0);
        assert!(i.next_frame().is_none());
        i.feed(&[0.9; 160]);
        assert_eq!(i.queue_depth(), 0);
    }
}
