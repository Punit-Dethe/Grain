//! [GRAIN] Real-time rolling-window transcription driver.
//!
//! ISOLATED Grain module (Handy has no rolling mode — keep upstream files free
//! of rolling knowledge so manual upstream syncs stay clean). Since the
//! transcribe-cpp unification this module owns NO speech engine of its own:
//! chunk transcription goes through the app-wide [`TranscriptionManager`]
//! (`selected_model`, same engine slot as Batch / Native ASR), so switching
//! between the three capture modes never leaves an extra engine's RAM remnant
//! behind. What stays here is everything rolling-specific: the session cursor
//! (chunking at silence), the serial chunk worker, and the timeline assembler.
//!
//! While a session is live the manager is put under a *rolling hold*
//! ([`TranscriptionManager::set_rolling_hold`]): per-chunk custom-word/filler
//! post-processing is skipped (the assembled transcript is finalized ONCE in
//! the action) and the "Immediately" unload policy is deferred to session end.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rolling_window::{AudioChunk, RollingWindowConfig, SessionCursor};
use tauri::AppHandle;

use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;

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
    /// [GRAIN] Mirror of `settings.audio_conditioning`, refreshed on each
    /// `ensure_loaded`. When set, each rolling chunk gets boost-only AGC before
    /// transcription — the high-pass already ran upstream on the shared frame.
    conditioning: AtomicBool,
}

impl RollingTranscriber {
    pub fn new(tm: Arc<TranscriptionManager>) -> Self {
        Self {
            tm,
            active: Mutex::new(None),
            conditioning: AtomicBool::new(false),
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
        let model_id = settings.selected_model;
        if model_id.is_empty() {
            return Err("no model selected".into());
        }
        self.tm.initiate_model_load_for(model_id);
        Ok(())
    }

    // -- live session control ---------------------------------------------

    /// Begin a live rolling session (on recording start). Puts the shared
    /// manager under the rolling hold for the duration of the session.
    pub fn start_session(self: &Arc<Self>, max_chunk_seconds: f64) {
        self.tm.set_rolling_hold(true);
        let session = Arc::new(RollingSession::start(self.clone(), max_chunk_seconds));
        *self.active.lock().unwrap() = Some(session);
        log::info!("[GRAIN] rolling session started (shared engine)");
    }

    /// Feed one captured 16 kHz mono frame to the active session (audio thread).
    /// No-op when no session is active.
    pub fn feed(&self, frame: &[f32]) {
        if let Some(session) = self.active.lock().unwrap().as_ref() {
            session.feed(frame);
        }
    }

    /// Stop the live session: flush the tail, drain the worker, return the final
    /// assembled transcript. `None` if no session was active. Releases the
    /// rolling hold and honors a deferred "Immediately" unload.
    pub fn finish_session(&self) -> Option<String> {
        let session = self.active.lock().unwrap().take()?;
        let text = session.finish();
        self.tm.set_rolling_hold(false);
        self.tm.maybe_unload_immediately("rolling session");
        Some(text)
    }

    /// Abort the live session without producing a transcript (cancel).
    pub fn cancel_session(&self) {
        if self.active.lock().unwrap().take().is_some() {
            self.tm.set_rolling_hold(false);
            self.tm
                .maybe_unload_immediately("cancelled rolling session");
        }
    }
}

/// One live recording's rolling-window transcription. Frames are fed from the
/// audio thread (cheap); a single worker thread transcribes finalized chunks
/// serially through the shared manager (never blocking audio) and assembles the
/// transcript. No partial text is ever surfaced — only the final string at
/// [`finish`](RollingSession::finish).
struct RollingSession {
    cursor: Mutex<SessionCursor>,
    tx: Sender<Job>,
    worker: Mutex<Option<JoinHandle<String>>>,
    frames_fed: AtomicUsize,
    chunks_emitted: AtomicUsize,
}

enum Job {
    Chunk(AudioChunk),
    Finish,
}

impl RollingSession {
    fn start(transcriber: Arc<RollingTranscriber>, max_chunk_seconds: f64) -> Self {
        let mut cfg = RollingWindowConfig::default();
        // [GRAIN] Honor the user's configured rolling-window length. The setting
        // is already clamped to [15, 60] by its setter; clamp again here so a
        // hand-edited settings file can never push the engine out of range.
        cfg.max_chunk_seconds = (max_chunk_seconds).clamp(15.0, 60.0);
        let overlap = cfg.overlap_seconds;
        let cursor = SessionCursor::new(cfg);
        let (tx, rx) = mpsc::channel::<Job>();
        let worker = std::thread::spawn(move || {
            // Time-based assembler with the fuzzy seam pass enabled (see
            // merge.rs). Word timings are no longer available from the shared
            // batch path, so seams are reconciled by fuzzy text overlap alone.
            let mut assembler = rolling_window::TimelineAssembler::new().with_fuzzy_seam(overlap);
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Chunk(chunk) => {
                        let mut audio = i16_to_f32(&chunk.samples);
                        // [GRAIN] boost-only AGC lifts quiet/laptop-mic speech to a
                        // good level for the model. Per-chunk is safe here — chunks
                        // are transcribed independently (text only, no audio output
                        // to pump). The high-pass already ran on the shared frame.
                        if transcriber.conditioning.load(Ordering::Relaxed) {
                            crate::audio_toolkit::audio::normalize_gain(&mut audio);
                        }
                        // The shared manager waits out an in-flight model load
                        // internally, so a chunk arriving mid-load is transcribed
                        // once weights are ready — never dropped.
                        match transcriber.tm.transcribe(audio) {
                            Ok(text) => {
                                log::info!(
                                    "[GRAIN] chunk [{:.1}..{:.1}]s -> {:?}",
                                    chunk.fresh_start_sec,
                                    chunk.end_sec,
                                    text.trim()
                                );
                                assembler.add_chunk(
                                    chunk.start_sec,
                                    chunk.fresh_start_sec,
                                    &text,
                                    None,
                                );
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
            cursor: Mutex::new(cursor),
            tx,
            worker: Mutex::new(Some(worker)),
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        }
    }

    fn feed(&self, frame: &[f32]) {
        let buf = f32_to_i16(frame);
        self.frames_fed.fetch_add(buf.len(), Ordering::Relaxed);
        let rms = block_rms(&buf);
        let chunk = self.cursor.lock().unwrap().push_block(&buf, rms);
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
        let text = match self.worker.lock().unwrap().take() {
            Some(worker) => worker.join().unwrap_or_default(),
            None => String::new(),
        };
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
}
