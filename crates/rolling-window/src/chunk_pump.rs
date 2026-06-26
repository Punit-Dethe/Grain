//! Serialized, readiness-gated streaming chunk pump.
//!
//! Ported from `open_voice_router/services/chunk_pump.py`. A pure, framework-free
//! state machine for the on-demand-load dispatch contract — no subprocess, mic,
//! or HTTP server. Mirrors the production contract enforced in the Python
//! `AppController`:
//!
//! * **Readiness gate.** While the ASR server is not ready, finalized chunks
//!   accumulate in capture order and nothing is dispatched. Draining begins only
//!   once [`mark_ready`](ChunkPump::mark_ready) fires.
//! * **Serialization.** At most one request is in flight at a time. The next
//!   chunk dispatches only after the in-flight one [`complete`](ChunkPump::complete)s
//!   or [`fail`](ChunkPump::fail)s.
//! * **Capture order (FIFO).** Chunks dispatch strictly in enqueue order.
//! * **Failure never aborts the drain.** A failed chunk frees the slot; draining
//!   continues.
//! * **Stale-generation discard.** Each result carries the generation token it was
//!   dispatched under; a result whose generation ≠ the current generation is
//!   ignored.
//! * **No finalize-as-empty while loading.** The finalize guard refuses to
//!   finalize an empty session while the model is neither ready nor failed.
//! * **Load-failure clears buffered audio.** [`clear`](ChunkPump::clear) empties
//!   the queue and bumps the generation so stale audio cannot carry over.
//!
//! Generic over the chunk type `T` — it stores whatever opaque value is enqueued.

use std::collections::VecDeque;

/// Pure model of the serialized, readiness-gated streaming chunk pump.
///
/// Observation logs (`dispatched`/`succeeded`/`failed`) are not present in
/// production; they exist for assertions. (The Python version stored either a
/// bare chunk or a `(chunk, text)` tuple in the logs; here they are always
/// `(chunk, text)` with an empty string when there is no text/error.)
pub struct ChunkPump<T> {
    generation: u64,
    queue: VecDeque<T>,
    busy: bool,
    model_ready: bool,
    recording_done: bool,
    pub in_flight: Option<T>,

    #[cfg(test)]
    pub dispatched: Vec<T>,
    #[cfg(test)]
    pub succeeded: Vec<(T, String)>,
    #[cfg(test)]
    pub failed: Vec<(T, String)>,
}

impl<T> Default for ChunkPump<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            queue: VecDeque::new(),
            busy: false,
            model_ready: false,
            recording_done: false,
            in_flight: None,
            #[cfg(test)]
            dispatched: Vec::new(),
            #[cfg(test)]
            succeeded: Vec::new(),
            #[cfg(test)]
            failed: Vec::new(),
        }
    }
}

impl<T: Clone> ChunkPump<T> {
    pub fn new() -> Self {
        Self::default()
    }

    // -- read-only views ---------------------------------------------------

    /// The stale-result discard token.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Snapshot of the chunks still waiting in the queue, in capture order.
    pub fn queued(&self) -> Vec<T> {
        self.queue.iter().cloned().collect()
    }

    /// True while exactly one chunk request is in flight.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// True once the ASR server reported ready this session.
    pub fn model_ready(&self) -> bool {
        self.model_ready
    }

    /// True once recording has stopped.
    pub fn recording_done(&self) -> bool {
        self.recording_done
    }

    /// The chunk currently in flight, or `None` when idle.
    pub fn in_flight(&self) -> Option<&T> {
        self.in_flight.as_ref()
    }

    /// Number of requests in flight — invariant: never exceeds 1.
    pub fn in_flight_count(&self) -> usize {
        if self.in_flight.is_some() {
            1
        } else {
            0
        }
    }

    /// True when the queue is empty and nothing is in flight.
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && !self.busy
    }

    // -- mutations ---------------------------------------------------------

    /// Append a finalized chunk in capture order, then attempt a dispatch. While
    /// the model is not ready the chunk simply accumulates (the readiness gate in
    /// [`dispatch`](Self::dispatch) returns early).
    pub fn enqueue(&mut self, chunk: T) {
        self.queue.push_back(chunk);
        self.dispatch();
    }

    /// Mark the ASR server ready and begin draining the queue. Idempotent.
    pub fn mark_ready(&mut self) {
        self.model_ready = true;
        self.dispatch();
    }

    /// Record that recording has stopped, then pump (in case the queue is idle).
    pub fn mark_recording_done(&mut self) {
        self.recording_done = true;
        self.dispatch();
    }

    /// The serialization gate: dispatch the next queued chunk if allowed. Returns
    /// `true` if a chunk was dispatched. Refused when the model is not ready, a
    /// request is already in flight, or the queue is empty.
    pub fn dispatch(&mut self) -> bool {
        if !self.model_ready {
            return false; // readiness gate
        }
        if self.busy {
            return false; // at most one in flight
        }
        let Some(chunk) = self.queue.pop_front() else {
            return false;
        };
        #[cfg(test)]
        self.dispatched.push(chunk.clone());

        self.in_flight = Some(chunk);
        self.busy = true;
        true
    }

    /// Complete the in-flight request successfully, then pump the next chunk.
    /// Results carrying a stale generation token are ignored. Returns `true` if
    /// the result was applied.
    pub fn complete(&mut self, generation: u64, _transcript: &str) -> bool {
        if generation != self.generation {
            return false; // stale result from a previous/discarded session
        }
        if !self.busy {
            return false; // nothing in flight
        }
        let _finished = self.in_flight.take().expect("busy implies in_flight");
        self.busy = false;

        #[cfg(test)]
        self.succeeded.push((_finished, _transcript.to_string()));

        self.dispatch();
        true
    }

    /// Fail the in-flight request, then continue draining the queue. A single
    /// failed chunk must never abort the session. Stale-generation results are
    /// ignored. Returns `true` if the failure was applied.
    pub fn fail(&mut self, generation: u64, _error: &str) -> bool {
        if generation != self.generation {
            return false; // stale result
        }
        if !self.busy {
            return false; // nothing in flight
        }
        let _finished = self.in_flight.take().expect("busy implies in_flight");
        self.busy = false;

        #[cfg(test)]
        self.failed.push((_finished, _error.to_string()));

        self.dispatch();
        true
    }

    /// Discard all buffered audio and start a fresh generation. Empties the
    /// queue, frees the in-flight slot, and bumps the generation token so any
    /// result still in flight from the cleared session becomes stale.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.busy = false;
        self.in_flight = None;
        self.generation += 1;
    }

    /// Whether the session may finalize now: recording stopped, nothing in
    /// flight, queue drained, AND the model is either ready or the load failed.
    ///
    /// The final clause is the guard against finalize-as-empty while the model is
    /// still loading (not ready and not failed) — the session stays in PROCESSING.
    pub fn can_finalize(&self, load_failed: bool) -> bool {
        if !self.recording_done {
            return false;
        }
        if self.busy {
            return false;
        }
        if !self.queue.is_empty() {
            return false;
        }
        if !(self.model_ready || load_failed) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump() -> ChunkPump<i32> {
        ChunkPump::new()
    }

    #[test]
    fn readiness_gate_holds_until_ready() {
        let mut p = pump();
        p.enqueue(1);
        p.enqueue(2);
        assert!(p.dispatched.is_empty(), "nothing dispatches before ready");
        assert_eq!(p.queued(), vec![1, 2]);
        p.mark_ready();
        assert_eq!(p.dispatched, vec![1], "drains first in capture order");
        assert!(p.busy());
    }

    #[test]
    fn serialization_one_in_flight() {
        let mut p = pump();
        p.mark_ready();
        p.enqueue(1);
        p.enqueue(2);
        p.enqueue(3);
        assert_eq!(p.dispatched, vec![1]);
        assert_eq!(p.in_flight_count(), 1);
        assert!(p.complete(0, "a"));
        assert_eq!(p.dispatched, vec![1, 2]);
        assert!(p.complete(0, "b"));
        assert_eq!(p.dispatched, vec![1, 2, 3]);
    }

    #[test]
    fn fifo_capture_order() {
        let mut p = pump();
        p.mark_ready();
        for c in [10, 20, 30] {
            p.enqueue(c);
        }
        while p.busy() {
            p.complete(0, "");
        }
        assert_eq!(p.dispatched, vec![10, 20, 30]);
        assert_eq!(p.succeeded.iter().map(|(c, _)| *c).collect::<Vec<_>>(), vec![10, 20, 30]);
    }

    #[test]
    fn failure_never_aborts_drain() {
        let mut p = pump();
        p.mark_ready();
        p.enqueue(1);
        p.enqueue(2);
        assert!(p.fail(0, "boom"));
        assert_eq!(p.failed.iter().map(|(c, _)| *c).collect::<Vec<_>>(), vec![1]);
        assert_eq!(p.dispatched, vec![1, 2], "draining continues after failure");
        assert!(p.complete(0, "ok"));
        assert!(p.is_idle());
    }

    #[test]
    fn stale_generation_is_ignored() {
        let mut p = pump();
        p.mark_ready();
        p.enqueue(1);
        // A result from a different generation must not apply.
        assert!(!p.complete(99, "stale"));
        assert!(p.busy(), "still in flight — stale result had no effect");
        assert!(p.succeeded.is_empty());
        // The correct generation applies.
        assert!(p.complete(0, "fresh"));
        assert!(p.is_idle());
    }

    #[test]
    fn clear_bumps_generation_and_strands_in_flight() {
        let mut p = pump();
        p.mark_ready();
        p.enqueue(1);
        p.enqueue(2);
        assert_eq!(p.generation(), 0);
        p.clear();
        assert_eq!(p.generation(), 1);
        assert!(p.queued().is_empty());
        assert!(!p.busy());
        // The result still in flight under gen 0 is now stale and ignored.
        assert!(!p.complete(0, "late"));
        assert!(p.succeeded.is_empty());
    }

    #[test]
    fn can_finalize_guard() {
        let mut p = pump();
        // Recording not done → cannot finalize.
        assert!(!p.can_finalize(false));

        p.mark_recording_done();
        // Recording done, queue empty, but model still loading (not ready, not
        // failed) → must NOT finalize-as-empty.
        assert!(!p.can_finalize(false));
        // Load failed path allows finalize even without a ready model.
        assert!(p.can_finalize(true));

        // Ready + drained + recording done → finalize.
        p.mark_ready();
        assert!(p.can_finalize(false));

        // In-flight work blocks finalize.
        p.enqueue(1);
        assert!(p.busy());
        assert!(!p.can_finalize(false));
        p.complete(0, "");
        assert!(p.can_finalize(false));
    }
}
