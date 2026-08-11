//! [GRAIN] Real-time rolling-window transcription driver.
//!
//! ISOLATED Grain module (Handy has no rolling mode — keep upstream files free
//! of rolling knowledge so manual upstream syncs stay clean). Since the
//! transcribe-cpp unification this module owns NO speech engine of its own:
//! chunk transcription goes through the app-wide [`TranscriptionManager`]
//! (`selected_model`, same engine slot as Batch / Native ASR), so switching
//! between the three capture modes never leaves an extra engine's RAM remnant
//! behind. What stays here is everything rolling-specific: the session cursor
//! (VAD-aware chunking at silence), the serial chunk worker, and the timeline
//! assembler.
//!
//! Per-chunk decodes go through [`TranscriptionManager::transcribe_rolling_chunk`],
//! which returns the FULL transcript (text + word timings) WITHOUT per-chunk
//! post-processing or idle-unload — the driver dedups overlaps by timeline
//! position, then applies custom-word + filler correction ONCE on the assembled
//! transcript in the shortcut action, and triggers a single idle-unload at
//! session end.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use grain_core::DaemonEvent;
use rolling_window::{
    seam_overlap_len, AudioChunk, RollingWindowConfig, SessionCursor, WordTiming,
};
use tauri::AppHandle;
use transcribe_cpp::Transcript;

use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;

/// Where the live preview streams to (the pill's Studio Window over the WS bus).
/// `None` = preview OFF, which is the zero-overhead path (the worker blocks on
/// `recv()` and never runs a tail decode). Only constructed when the user has
/// opted in via `rolling_live_preview`.
#[derive(Clone)]
struct PreviewSink {
    app: AppHandle,
    session_id: u64,
    /// [GRAIN] Mirror of `settings.scrap_that_enabled`, captured at session start.
    /// When set, the live caption is scrubbed past the last "scrap that" so the
    /// Studio pill restarts + collapses. The final assembled text is scrubbed
    /// independently in the shortcut action's `finalize_transcript`.
    scrap_that: bool,
}

impl PreviewSink {
    fn emit(&self, committed: &str, tentative: &str) {
        let (committed, tentative) = if self.scrap_that {
            crate::audio_toolkit::scrub_stream_preview(committed, tentative)
        } else {
            (committed.to_string(), tentative.to_string())
        };
        crate::bridge::emit(
            &self.app,
            DaemonEvent::AsrStreamText {
                session_id: self.session_id,
                committed,
                tentative,
            },
        );
    }
}

/// Longest common word-prefix of two hypotheses — the LocalAgreement-2 commit
/// rule: text agreed by two consecutive tail decodes is trustworthy enough to
/// surface. Returns the agreed words.
fn longest_common_prefix(a: &[String], b: &[String]) -> Vec<String> {
    a.iter()
        .zip(b.iter())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.clone())
        .collect()
}

/// Map a chunk `Transcript`'s word rows into the assembler's [`WordTiming`]s.
/// transcribe-cpp reports `t0_ms`/`t1_ms` relative to the audio we fed (i.e. the
/// chunk's `start_sec`), which is exactly what `TimelineAssembler::add_chunk`
/// expects. Empty/blank words are dropped.
fn map_word_timings(t: &Transcript) -> Vec<WordTiming> {
    t.words
        .iter()
        .filter(|w| !w.text.trim().is_empty())
        .map(|w| {
            WordTiming::new(
                w.text.trim().to_string(),
                w.t0_ms as f64 / 1000.0,
                w.t1_ms as f64 / 1000.0,
            )
        })
        .collect()
}

/// Fallback when a model returns no word rows (rare — most catalog models do
/// segment-or-finer): synthesize evenly-spaced timings across the chunk's audio
/// duration so the timeline assembler's positional dedup + fuzzy seam still run
/// (they were designed for approximate timings). `chunk_dur_sec` is the chunk's
/// full audio span (overlap context included), matching real word-time origins.
fn synthesize_word_timings(text: &str, chunk_dur_sec: f64) -> Vec<WordTiming> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || chunk_dur_sec <= 0.0 {
        return Vec::new();
    }
    let per = chunk_dur_sec / words.len() as f64;
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let start = i as f64 * per;
            WordTiming::new(w.to_string(), start, start + per)
        })
        .collect()
}

/// Convert one captured `f32` frame to the `i16` block the session cursor
/// expects (it was designed around 16-bit PCM levels for its silence tracking).
fn f32_to_i16(frame: &[f32]) -> Vec<i16> {
    frame
        .iter()
        .map(|&s| (s * 32768.0).clamp(-32768.0, 32767.0) as i16)
        .collect()
}

/// RMS of an i16 block on the 0–1 float scale — the silence signal the cursor's
/// early-finalize logic consumes. (Moved here from the retired grain-transcribe
/// crate; the scale must match `RollingWindowConfig`'s silence thresholds.)
fn block_rms(block: &[i16]) -> f64 {
    if block.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = block
        .iter()
        .map(|&s| {
            let f = s as f64 / 32768.0;
            f * f
        })
        .sum();
    (sum_sq / block.len() as f64).sqrt()
}

fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// Rolling-window driver, held in Tauri managed state. Stateless between
/// sessions apart from the shared manager handle.
pub struct RollingTranscriber {
    tm: Arc<TranscriptionManager>,
    /// The current live recording's rolling session, if any.
    active: Mutex<Option<Arc<RollingSession>>>,
    /// Monotonic session identity used to prevent a cancelled session's delayed
    /// cleanup from unloading the model underneath a newer recording.
    active_generation: AtomicU64,
    /// [GRAIN] Mirror of `settings.audio_conditioning`, refreshed on each
    /// `ensure_loaded`. When set, each rolling chunk gets boost-only AGC before
    /// transcription — the high-pass already ran upstream on the shared frame.
    conditioning: AtomicBool,
    /// [GRAIN] Mirror of `settings.scrap_that_enabled`, refreshed on each
    /// `ensure_loaded` and read at `start_session` to configure the preview sink.
    scrap_that: AtomicBool,
}

impl RollingTranscriber {
    pub fn new(tm: Arc<TranscriptionManager>) -> Self {
        Self {
            tm,
            active: Mutex::new(None),
            active_generation: AtomicU64::new(0),
            conditioning: AtomicBool::new(false),
            scrap_that: AtomicBool::new(false),
        }
    }

    /// Kick off (or confirm) the batch model load on the shared manager (on
    /// hotkey press). Non-blocking: chunk transcription waits on the manager's
    /// load condvar, so a chunk emitted during the load is never dropped.
    pub fn ensure_loaded(&self, app: &AppHandle) -> Result<(), String> {
        let settings = get_settings(app);
        // Refresh the conditioning mirror so the session worker (no AppHandle)
        // can honor the current setting.
        self.conditioning
            .store(settings.audio_conditioning, Ordering::Relaxed);
        self.scrap_that
            .store(settings.scrap_that_enabled, Ordering::Relaxed);
        let model_id = settings.selected_model;
        if model_id.is_empty() {
            return Err("no model selected".into());
        }
        self.tm.initiate_model_load_for(model_id);
        Ok(())
    }

    // -- live session control ---------------------------------------------

    /// Begin a live rolling session (on recording start). When `preview` is set
    /// the session streams a live caption to the pill's Studio Window (opt-in,
    /// extra compute); when `None` the worker takes the exact zero-overhead path
    /// it always did.
    pub fn start_session(self: &Arc<Self>, app: AppHandle, session_id: u64, preview: bool) {
        let sink = preview.then(|| PreviewSink {
            app,
            session_id,
            scrap_that: self.scrap_that.load(Ordering::Relaxed),
        });
        let session = Arc::new(RollingSession::start(self.clone(), sink, session_id));
        let previous = {
            let mut active = self.active.lock().unwrap();
            self.active_generation.store(session_id, Ordering::Release);
            active.replace(session)
        };
        if let Some(previous) = previous {
            log::warn!("[GRAIN] replacing an unfinished rolling session");
            self.retire_cancelled_session(previous, false);
        }
        log::info!(
            "[GRAIN] rolling session started (shared engine, preview={})",
            preview
        );
    }

    /// Feed one captured 16 kHz mono frame to the active session (audio thread).
    /// `speech` is the frame's voice-activity decision (`None` when VAD is off).
    /// No-op when no session is active.
    pub fn feed(&self, frame: &[f32], speech: Option<bool>) {
        if let Some(session) = self.active.lock().unwrap().as_ref() {
            session.feed(frame, speech);
        }
    }

    /// Stop the live session: flush the tail, drain the worker, return the final
    /// assembled transcript. `None` if no session was active. Honors the
    /// "Immediately" unload once, now that no more chunks will decode.
    pub fn finish_session(&self) -> Option<String> {
        let session = self.active.lock().unwrap().take()?;
        let text = session.finish();
        self.tm.maybe_unload_immediately("rolling session");
        Some(text)
    }

    /// Abort the live session without producing a transcript (cancel).
    pub fn cancel_session(self: &Arc<Self>) {
        if let Some(session) = self.active.lock().unwrap().take() {
            self.retire_cancelled_session(session, true);
        }
    }

    /// Cancel immediately from the caller's perspective, then join the single
    /// in-flight decoder on a short-lived cleanup thread. The decoder backend
    /// has no preemption API, so an already-running call must return naturally;
    /// queued chunks are skipped as soon as it does.
    fn retire_cancelled_session(
        self: &Arc<Self>,
        session: Arc<RollingSession>,
        unload_when_current: bool,
    ) {
        let session_id = session.session_id;
        session.request_cancel();
        let transcriber = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("grain-rolling-cancel".into())
            .spawn(move || {
                session.join_cancelled();
                if !unload_when_current {
                    return;
                }
                let Some(transcriber) = transcriber.upgrade() else {
                    return;
                };
                let still_current = transcriber.active_generation.load(Ordering::Acquire)
                    == session_id
                    && transcriber.active.lock().unwrap().is_none();
                if still_current {
                    transcriber
                        .tm
                        .maybe_unload_immediately("cancelled rolling session");
                }
            })
            .expect("failed to spawn rolling cancellation cleanup");
    }
}

/// One live recording's rolling-window transcription. Frames are fed from the
/// audio thread (cheap); a single worker thread transcribes finalized chunks
/// serially through the shared manager (never blocking audio) and assembles the
/// transcript. No partial text is ever surfaced — only the final string at
/// [`finish`](RollingSession::finish).
struct RollingSession {
    session_id: u64,
    // Shared with the worker so the live preview can peek the unsent tail
    // without stealing it from the feed path.
    cursor: Arc<Mutex<SessionCursor>>,
    tx: Sender<Job>,
    worker: Mutex<Option<JoinHandle<String>>>,
    cancelled: Arc<AtomicBool>,
    frames_fed: AtomicUsize,
    chunks_emitted: AtomicUsize,
}

enum Job {
    Chunk(AudioChunk),
    Finish,
}

/// How often the live preview re-decodes the unsent tail (only when preview is
/// ON). Short enough to feel live, long enough to bound the extra compute.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(2000);
/// Cap the preview tail decode to the most-recent audio; the earlier unsent
/// span becomes committed via the normal chunk path anyway.
const PREVIEW_MAX_TAIL_SEC: f64 = 20.0;
/// Don't bother decoding a tail shorter than this (nothing useful to preview).
const PREVIEW_MIN_TAIL_SEC: f64 = 0.8;

impl RollingSession {
    fn start(
        transcriber: Arc<RollingTranscriber>,
        preview: Option<PreviewSink>,
        session_id: u64,
    ) -> Self {
        // [GRAIN] The rolling-window geometry is fixed by the research-tuned,
        // model-agnostic defaults in `RollingWindowConfig::default()` (see
        // crates/rolling-window/src/cursor.rs). There is deliberately NO user
        // override — those dialed-in numbers are always the ones in force.
        let cfg = RollingWindowConfig::default();
        let overlap = cfg.overlap_seconds;
        let cursor = Arc::new(Mutex::new(SessionCursor::new(cfg)));
        let worker_cursor = cursor.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (tx, rx) = mpsc::channel::<Job>();
        let worker = std::thread::spawn(move || {
            // Time-based assembler with the fuzzy seam pass enabled (see
            // merge.rs). Chunks carry real word timings from transcribe-cpp, so
            // overlap dedup is positional; the fuzzy seam is the safety net for
            // timing jitter / re-worded overlaps.
            let mut assembler = rolling_window::TimelineAssembler::new().with_fuzzy_seam(overlap);
            // LocalAgreement-2 state for the live preview: the previous tail
            // hypothesis, so only text two consecutive decodes agree on is shown.
            let mut prev_tail_hyp: Vec<String> = Vec::new();
            loop {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                // Preview ON polls so it can decode the tail between chunks;
                // preview OFF blocks forever (zero overhead — no wakeups).
                let job = if preview.is_some() {
                    match rx.recv_timeout(PREVIEW_INTERVAL) {
                        Ok(j) => j,
                        Err(RecvTimeoutError::Timeout) => {
                            Self::preview_tail(
                                &transcriber,
                                &worker_cursor,
                                &assembler,
                                preview.as_ref().unwrap(),
                                &mut prev_tail_hyp,
                                &worker_cancelled,
                            );
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(j) => j,
                        Err(_) => break,
                    }
                };
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                match job {
                    Job::Chunk(chunk) => {
                        // A chunk with no fresh audio past the cursor carries only
                        // overlap the previous chunk already covered (e.g. the
                        // stop-flush when nothing is unsent). Decoding it wastes
                        // compute and risks duplicating the tail — skip it.
                        if chunk.fresh_duration_sec() <= 0.0 {
                            continue;
                        }
                        let mut audio = i16_to_f32(&chunk.samples);
                        // [GRAIN] boost-only AGC lifts quiet/laptop-mic speech to a
                        // good level for the model. Per-chunk is safe here — chunks
                        // are transcribed independently. The high-pass already ran
                        // on the shared frame.
                        if transcriber.conditioning.load(Ordering::Relaxed) {
                            crate::audio_toolkit::audio::normalize_gain(&mut audio);
                        }
                        let chunk_dur = chunk.end_sec - chunk.start_sec;
                        // The shared manager waits out an in-flight model load
                        // internally, so a chunk arriving mid-load is transcribed
                        // once weights are ready — never dropped.
                        match transcriber.tm.transcribe_rolling_chunk(&audio) {
                            Ok(transcript) => {
                                if worker_cancelled.load(Ordering::Acquire) {
                                    break;
                                }
                                let text = transcript.text.trim().to_string();
                                // Prefer the model's real word timings; synthesize
                                // evenly-spaced ones only if it returned none.
                                let mut words = map_word_timings(&transcript);
                                if words.is_empty() {
                                    words = synthesize_word_timings(&text, chunk_dur);
                                }
                                log::info!(
                                    "[GRAIN] chunk [{:.1}..{:.1}]s ({} words) -> {:?}",
                                    chunk.fresh_start_sec,
                                    chunk.end_sec,
                                    words.len(),
                                    text
                                );
                                assembler.add_chunk(
                                    chunk.start_sec,
                                    chunk.fresh_start_sec,
                                    &text,
                                    if words.is_empty() {
                                        None
                                    } else {
                                        Some(words.as_slice())
                                    },
                                    // Keep the acoustic cut reason attached to
                                    // the chunk without inferring formatting.
                                    chunk.boundary,
                                );
                                // Preview: the committed text just grew; show it
                                // solid and clear the tentative tail (its audio is
                                // now committed). Restart LocalAgreement for the
                                // fresh unsent region.
                                if let Some(sink) = &preview {
                                    sink.emit(assembler.text(), "");
                                    prev_tail_hyp.clear();
                                }
                            }
                            Err(e) => log::warn!("[GRAIN] rolling chunk transcribe failed: {e}"),
                        }
                    }
                    Job::Finish => break,
                }
            }
            assembler.text().to_string()
        });
        Self {
            session_id,
            cursor,
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled,
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        }
    }

    /// [GRAIN] Live-preview tail decode (only runs when preview is ON). Peeks
    /// the unsent tail (no cursor advance), decodes it, and surfaces a tentative
    /// caption using LocalAgreement-2: only the prefix that TWO consecutive
    /// decodes agree on is shown, so the tail doesn't flicker as the model
    /// revises unstable words. The committed overlap is stripped so the tentative
    /// shows only text beyond what's already committed.
    fn preview_tail(
        transcriber: &Arc<RollingTranscriber>,
        cursor: &Arc<Mutex<SessionCursor>>,
        assembler: &rolling_window::TimelineAssembler,
        sink: &PreviewSink,
        prev_tail_hyp: &mut Vec<String>,
        cancelled: &AtomicBool,
    ) {
        let (tail, _start_sec) = cursor
            .lock()
            .unwrap()
            .peek_unsent_tail(PREVIEW_MAX_TAIL_SEC);
        if (tail.len() as f64) < PREVIEW_MIN_TAIL_SEC * 16_000.0 {
            return; // too little unsent audio to preview
        }
        let mut audio = i16_to_f32(&tail);
        if transcriber.conditioning.load(Ordering::Relaxed) {
            crate::audio_toolkit::audio::normalize_gain(&mut audio);
        }
        let hyp: Vec<String> = match transcriber.tm.transcribe_rolling_chunk(&audio) {
            Ok(t) => t.text.split_whitespace().map(String::from).collect(),
            Err(_) => return,
        };
        if cancelled.load(Ordering::Acquire) {
            return;
        }

        // LocalAgreement-2: surface only the prefix two consecutive decodes agree
        // on, then remember this hypothesis for the next comparison.
        let agreed = longest_common_prefix(prev_tail_hyp, &hyp);
        *prev_tail_hyp = hyp;
        if agreed.is_empty() {
            return;
        }

        // The tail re-covers the committed overlap; drop the words already
        // committed so the tentative is only NEW text past the commit point.
        let committed = assembler.text();
        let committed_words: Vec<&str> = committed.split_whitespace().collect();
        let ctail = &committed_words[committed_words.len().saturating_sub(8)..];
        let agreed_refs: Vec<&str> = agreed.iter().map(String::as_str).collect();
        let drop = seam_overlap_len(ctail, &agreed_refs).min(agreed_refs.len());
        let tentative = agreed_refs[drop..].join(" ");
        if !tentative.is_empty() && !cancelled.load(Ordering::Acquire) {
            sink.emit(committed, &tentative);
        }
    }

    fn feed(&self, frame: &[f32], speech: Option<bool>) {
        let buf = f32_to_i16(frame);
        self.frames_fed.fetch_add(buf.len(), Ordering::Relaxed);
        // Prefer the VAD decision for silence gating (segments far better in
        // noisy rooms); fall back to raw RMS when VAD is disabled.
        let chunk = {
            let mut cursor = self.cursor.lock().unwrap();
            match speech {
                Some(is_speech) => cursor.push_block_vad(&buf, is_speech),
                None => cursor.push_block(&buf, block_rms(&buf)),
            }
        };
        if let Some(chunk) = chunk {
            self.chunks_emitted.fetch_add(1, Ordering::Relaxed);
            let _ = self.tx.send(Job::Chunk(chunk));
        }
    }

    fn finish(&self) -> String {
        if let Some(tail) = self.cursor.lock().unwrap().stop() {
            self.chunks_emitted.fetch_add(1, Ordering::Relaxed);
            let _ = self.tx.send(Job::Chunk(tail));
        }
        let _ = self.tx.send(Job::Finish);
        let text = self.join_worker();
        let frames = self.frames_fed.load(Ordering::Relaxed);
        log::info!(
            "[GRAIN] rolling session finished: {} frames ({:.1}s), {} chunks, final={:?}",
            frames,
            frames as f64 / 16_000.0,
            self.chunks_emitted.load(Ordering::Relaxed),
            text.trim()
        );
        text
    }

    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        // Wake a preview-off worker blocked on `recv`. Queued chunks do not run:
        // the cancellation flag is checked before and after every receive.
        let _ = self.tx.send(Job::Finish);
    }

    fn join_cancelled(&self) {
        let _ = self.join_worker();
        log::info!(
            "[GRAIN] rolling session {} cancelled after {} frames; worker joined",
            self.session_id,
            self.frames_fed.load(Ordering::Relaxed)
        );
    }

    fn join_worker(&self) -> String {
        match self.worker.lock().unwrap().take() {
            Some(worker) => match worker.join() {
                Ok(text) => text,
                Err(_) => {
                    log::error!("[GRAIN] rolling worker panicked");
                    String::new()
                }
            },
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_word_timings_spans_chunk_duration() {
        let w = synthesize_word_timings("a b c d", 4.0);
        assert_eq!(w.len(), 4);
        assert!((w[0].start - 0.0).abs() < 1e-9);
        assert!((w[3].end - 4.0).abs() < 1e-9);
    }

    #[test]
    fn synthesize_word_timings_empty_is_empty() {
        assert!(synthesize_word_timings("", 4.0).is_empty());
        assert!(synthesize_word_timings("a b", 0.0).is_empty());
    }

    #[test]
    fn cancellation_wakes_and_joins_an_idle_worker() {
        let cursor = Arc::new(Mutex::new(SessionCursor::new(
            RollingWindowConfig::default(),
        )));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker_exited = Arc::new(AtomicBool::new(false));
        let exited = worker_exited.clone();
        let (tx, rx) = mpsc::channel::<Job>();
        let worker = std::thread::spawn(move || {
            let _ = rx.recv();
            assert!(worker_cancelled.load(Ordering::Acquire));
            exited.store(true, Ordering::Release);
            String::new()
        });
        let session = RollingSession {
            session_id: 42,
            cursor,
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled,
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        };

        session.request_cancel();
        session.join_cancelled();

        assert!(worker_exited.load(Ordering::Acquire));
        assert!(session.worker.lock().unwrap().is_none());
    }
}
