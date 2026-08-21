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
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use grain_core::DaemonEvent;
use rolling_window::{
    seam_overlap_len, AudioChunk, RollingWindowConfig, SessionCursor, TimingQuality, WordTiming,
};
use tauri::AppHandle;
use transcribe_cpp::{CancelToken, Transcript};

use crate::grain_audio_journal::{PcmJournal, PcmJournalReader};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::tdt_flow::{TdtRunConfig, TdtWorkerResult};

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

fn timing_quality(transcript: &Transcript, words: &[WordTiming]) -> TimingQuality {
    if !words.is_empty() {
        TimingQuality::NativeWord
    } else if !transcript.segments.is_empty() {
        TimingQuality::SegmentApproximate
    } else {
        TimingQuality::Unavailable
    }
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

#[derive(Clone)]
pub(crate) struct RollingSessionOutput {
    pub(crate) text: String,
    pub(crate) error: Option<String>,
    pub(crate) preserves_native_text: bool,
    journal: Arc<PcmJournal>,
}

impl RollingSessionOutput {
    pub(crate) fn frame_count(&self) -> usize {
        self.journal.frame_count().min(usize::MAX as u64) as usize
    }

    pub(crate) fn materialize_audio(&self) -> std::io::Result<Vec<f32>> {
        self.journal.read_all_f32()
    }

    pub(crate) fn save_wav(&self, path: &std::path::Path) -> anyhow::Result<()> {
        self.journal.save_wav(path)
    }
}

struct ModelLoadBarrier;

fn establish_model_load_barrier(
    active_generation: &AtomicU64,
    session_id: u64,
    initiate_load: impl FnOnce() -> Result<(), String>,
) -> Result<ModelLoadBarrier, String> {
    // This store must precede load initiation: delayed cleanup from an older
    // generation uses it to decide whether it may unload the shared model.
    active_generation.store(session_id, Ordering::Release);
    initiate_load()?;
    Ok(ModelLoadBarrier)
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
    pub fn start_session(self: &Arc<Self>, app: AppHandle, session_id: u64, preview: bool) -> bool {
        // Register the generation before initiating the load so a cancelled
        // predecessor cannot unload underneath this recording. `ensure_loaded`
        // sets the manager's loading predicate synchronously, then starts the
        // expensive load on its own thread. The rolling/TDT worker must only be
        // spawned after that predicate is visible or it can mistake "not started
        // loading yet" for "model is not loaded".
        let _load_barrier =
            match establish_model_load_barrier(&self.active_generation, session_id, || {
                self.ensure_loaded(&app)
            }) {
                Ok(barrier) => barrier,
                Err(error) => {
                    log::error!("[GRAIN] rolling model load could not start: {error}");
                    let previous = self.active.lock().unwrap().take();
                    if let Some(previous) = previous {
                        self.retire_cancelled_session(previous, false);
                    }
                    return false;
                }
            };

        let settings = get_settings(&app);
        let tdt_language = self.tm.grain_transcribe_cpp_language_for_model(
            &settings.selected_language,
            &settings.selected_model,
        );
        let tdt_config = TdtRunConfig {
            model_id: settings.selected_model.clone(),
            language: tdt_language,
            translate_to_english: settings.translate_to_english,
            conditioning: settings.audio_conditioning,
        };
        let sink = preview.then(|| PreviewSink {
            app,
            session_id,
            scrap_that: self.scrap_that.load(Ordering::Relaxed),
        });
        let session = match RollingSession::start(self.clone(), sink, session_id, tdt_config) {
            Ok(session) => Arc::new(session),
            Err(error) => {
                log::error!("[GRAIN] rolling session journal creation failed: {error}");
                let previous = {
                    let mut active = self.active.lock().unwrap();
                    active.take()
                };
                if let Some(previous) = previous {
                    log::warn!(
                        "[GRAIN] retiring unfinished rolling session after journal creation failure"
                    );
                    self.retire_cancelled_session(previous, false);
                }
                return false;
            }
        };
        let previous = {
            let mut active = self.active.lock().unwrap();
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
        true
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
    pub fn finish_session(&self) -> Option<RollingSessionOutput> {
        let session = self.active.lock().unwrap().take()?;
        let worker_output = session.finish();
        let output = RollingSessionOutput {
            text: worker_output.text,
            error: worker_output.error,
            preserves_native_text: worker_output.preserves_native_text,
            journal: session.journal.clone(),
        };
        self.tm.maybe_unload_immediately("rolling session");
        Some(output)
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
/// transcript. The opt-in preview may surface tentative text; the normal path
/// returns only the final string from [`finish`](RollingSession::finish).
struct RollingSession {
    session_id: u64,
    // Shared with the worker so the live preview can peek the unsent tail
    // without stealing it from the feed path.
    cursor: Arc<Mutex<SessionCursor>>,
    journal: Arc<PcmJournal>,
    tx: SyncSender<Job>,
    worker: Mutex<Option<JoinHandle<WorkerOutput>>>,
    cancelled: Arc<AtomicBool>,
    cancel_token: CancelToken,
    journal_failed: Arc<AtomicBool>,
    metrics: Arc<RollingMetrics>,
    frames_fed: AtomicUsize,
    chunks_emitted: AtomicUsize,
}

#[derive(Default)]
pub(crate) struct RollingMetrics {
    queued_fresh_frames: AtomicU64,
    debt_frames: AtomicU64,
    queued_descriptors: AtomicUsize,
    peak_cursor_frames: AtomicUsize,
    peak_worker_frames: AtomicUsize,
    peak_debt_frames: AtomicU64,
    peak_queued_descriptors: AtomicUsize,
}

#[derive(Clone, Copy)]
struct QueueObservation {
    debt_frames: u64,
    queued_descriptors: usize,
}

impl RollingMetrics {
    fn observe_cursor_candidate(&self, frames: usize) {
        self.peak_cursor_frames.fetch_max(frames, Ordering::AcqRel);
    }

    fn reserve_descriptor(&self, fresh_frames: u64) -> QueueObservation {
        let queued_descriptors = self
            .queued_descriptors
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.queued_fresh_frames
            .fetch_add(fresh_frames, Ordering::AcqRel);
        let debt_frames = self
            .debt_frames
            .fetch_add(fresh_frames, Ordering::AcqRel)
            .saturating_add(fresh_frames);
        QueueObservation {
            debt_frames,
            queued_descriptors,
        }
    }

    fn observe_published(&self, observation: QueueObservation) {
        self.peak_debt_frames
            .fetch_max(observation.debt_frames, Ordering::AcqRel);
        self.peak_queued_descriptors.fetch_max(
            observation.queued_descriptors.min(MAX_PENDING_CHUNKS),
            Ordering::AcqRel,
        );
    }

    fn rollback_descriptor(&self, fresh_frames: u64) {
        self.queued_descriptors.fetch_sub(1, Ordering::AcqRel);
        self.queued_fresh_frames
            .fetch_sub(fresh_frames, Ordering::AcqRel);
        self.debt_frames.fetch_sub(fresh_frames, Ordering::AcqRel);
    }

    pub(crate) fn dequeue_descriptor(&self, fresh_frames: u64) -> u64 {
        self.queued_descriptors.fetch_sub(1, Ordering::AcqRel);
        self.queued_fresh_frames
            .fetch_sub(fresh_frames, Ordering::AcqRel)
            .saturating_sub(fresh_frames)
    }

    pub(crate) fn begin_decode(&self, chunk: ChunkJob) {
        let worker_frames = chunk
            .end_frame
            .saturating_sub(chunk.start_frame)
            .min(usize::MAX as u64) as usize;
        self.peak_worker_frames
            .fetch_max(worker_frames, Ordering::AcqRel);
    }

    pub(crate) fn end_decode(&self, fresh_frames: u64) {
        self.debt_frames.fetch_sub(fresh_frames, Ordering::AcqRel);
    }

    fn current_debt_frames(&self) -> u64 {
        self.debt_frames.load(Ordering::Acquire)
    }
}

pub(crate) enum Job {
    Chunk(ChunkJob),
    Finish,
}

#[derive(Clone, Copy)]
pub(crate) struct ChunkJob {
    sequence: u64,
    start_frame: u64,
    fresh_start_frame: u64,
    pub(crate) commit_end_frame: u64,
    end_frame: u64,
    start_sec: f64,
    fresh_start_sec: f64,
    end_sec: f64,
    boundary: rolling_window::CutKind,
}

impl ChunkJob {
    fn from_chunk(sequence: u64, chunk: &AudioChunk) -> Self {
        Self {
            sequence,
            start_frame: chunk.start_frame as u64,
            fresh_start_frame: chunk.fresh_start_frame as u64,
            commit_end_frame: chunk.end_frame as u64,
            end_frame: chunk.end_frame as u64,
            start_sec: chunk.start_sec,
            fresh_start_sec: chunk.fresh_start_sec,
            end_sec: chunk.end_sec,
            boundary: chunk.boundary,
        }
    }

    fn fresh_duration_sec(self) -> f64 {
        (self.end_sec - self.fresh_start_sec).max(0.0)
    }

    pub(crate) fn fresh_frames(self) -> u64 {
        self.end_frame.saturating_sub(self.fresh_start_frame)
    }

    fn input_duration_sec(self) -> f64 {
        (self.end_sec - self.start_sec).max(0.0)
    }
}

struct DecodedChunk {
    text: String,
    words: Vec<WordTiming>,
    timing_quality: TimingQuality,
}

enum ChunkStatus {
    Succeeded(DecodedChunk),
    Failed(String),
}

struct ChunkRecord {
    descriptor: ChunkJob,
    status: ChunkStatus,
    decode_duration: Duration,
}

struct WorkerOutput {
    text: String,
    error: Option<String>,
    recovered_chunks: usize,
    decode_rtf_ewma: Option<f64>,
    preserves_native_text: bool,
}

impl WorkerOutput {
    fn success(text: String, recovered_chunks: usize, decode_rtf_ewma: Option<f64>) -> Self {
        Self {
            text,
            error: None,
            recovered_chunks,
            decode_rtf_ewma,
            preserves_native_text: false,
        }
    }

    fn tdt_success(text: String, _descriptors: usize, decode_rtf_ewma: Option<f64>) -> Self {
        Self {
            text,
            error: None,
            recovered_chunks: 0,
            decode_rtf_ewma,
            preserves_native_text: true,
        }
    }

    fn failure(error: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            error: Some(error.into()),
            recovered_chunks: 0,
            decode_rtf_ewma: None,
            preserves_native_text: false,
        }
    }
}

/// How often the live preview re-decodes the unsent tail (only when preview is
/// ON). Short enough to feel live, long enough to bound the extra compute.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(2000);
/// Cap the preview tail decode to the most-recent audio; the earlier unsent
/// span becomes committed via the normal chunk path anyway.
const PREVIEW_MAX_TAIL_SEC: f64 = 20.0;
/// Don't bother decoding a tail shorter than this (nothing useful to preview).
const PREVIEW_MIN_TAIL_SEC: f64 = 0.8;
/// Fixed descriptor capacity. Audio itself is already durable in the journal;
/// overflow fails rolling closed and surfaces an explicit session error.
const MAX_PENDING_CHUNKS: usize = 8;
const RTF_EWMA_ALPHA: f64 = 0.2;

fn update_rtf_ewma(previous: Option<f64>, sample: f64) -> f64 {
    previous
        .map(|value| value * (1.0 - RTF_EWMA_ALPHA) + sample * RTF_EWMA_ALPHA)
        .unwrap_or(sample)
}

enum ChunkDecodeError {
    Journal(String),
    Transcription(String),
}

fn decode_descriptor(
    transcriber: &RollingTranscriber,
    reader: &mut PcmJournalReader,
    audio: &mut Vec<f32>,
    chunk: ChunkJob,
) -> Result<DecodedChunk, ChunkDecodeError> {
    reader
        .read_f32_range(chunk.start_frame, chunk.end_frame, audio)
        .map_err(|error| ChunkDecodeError::Journal(error.to_string()))?;
    if transcriber.conditioning.load(Ordering::Relaxed) {
        crate::audio_toolkit::audio::normalize_gain(audio);
    }
    let transcript = transcriber
        .tm
        .transcribe_rolling_chunk(audio)
        .map_err(|error| ChunkDecodeError::Transcription(error.to_string()))?;
    let text = transcript.text.trim().to_string();
    let words = map_word_timings(&transcript);
    let timing_quality = timing_quality(&transcript, &words);
    Ok(DecodedChunk {
        text,
        words,
        timing_quality,
    })
}

fn add_decoded_chunk(
    assembler: &mut rolling_window::TimelineAssembler,
    descriptor: ChunkJob,
    decoded: &DecodedChunk,
) {
    assembler.add_chunk_with_quality(
        descriptor.start_sec,
        descriptor.fresh_start_sec,
        &decoded.text,
        (!decoded.words.is_empty()).then_some(decoded.words.as_slice()),
        decoded.timing_quality,
        descriptor.boundary,
    );
}

fn recover_failed_chunks<F>(
    records: &mut [ChunkRecord],
    mut decode: F,
) -> Result<usize, Vec<String>>
where
    F: FnMut(ChunkJob) -> Result<DecodedChunk, String>,
{
    let mut recovered = 0usize;
    let mut failures = Vec::new();
    for record in records {
        let initial_error = match &record.status {
            ChunkStatus::Failed(error) => error.clone(),
            ChunkStatus::Succeeded(_) => continue,
        };
        let started = Instant::now();
        match decode(record.descriptor) {
            Ok(decoded) => {
                record.decode_duration += started.elapsed();
                record.status = ChunkStatus::Succeeded(decoded);
                recovered += 1;
            }
            Err(error) => {
                record.decode_duration += started.elapsed();
                failures.push(format!(
                    "chunk {} [{:.1}..{:.1}]s: initial decode: {}; recovery: {}",
                    record.descriptor.sequence,
                    record.descriptor.fresh_start_sec,
                    record.descriptor.end_sec,
                    initial_error,
                    error
                ));
                record.status = ChunkStatus::Failed(error);
            }
        }
    }
    if failures.is_empty() {
        Ok(recovered)
    } else {
        Err(failures)
    }
}

fn assemble_records(records: &[ChunkRecord], overlap: f64) -> Result<String, String> {
    let mut assembler = rolling_window::TimelineAssembler::new().with_fuzzy_seam(overlap);
    for record in records {
        let ChunkStatus::Succeeded(decoded) = &record.status else {
            return Err(format!(
                "chunk {} remained unresolved",
                record.descriptor.sequence
            ));
        };
        add_decoded_chunk(&mut assembler, record.descriptor, decoded);
    }
    Ok(assembler.finish().to_string())
}

impl RollingSession {
    fn start(
        transcriber: Arc<RollingTranscriber>,
        preview: Option<PreviewSink>,
        session_id: u64,
        tdt_config: TdtRunConfig,
    ) -> Result<Self, String> {
        // [GRAIN] The rolling-window geometry is fixed by the research-tuned,
        // model-agnostic defaults in `RollingWindowConfig::default()` (see
        // crates/rolling-window/src/cursor.rs). There is deliberately NO user
        // override — those dialed-in numbers are always the ones in force.
        let cfg = RollingWindowConfig::default();
        let overlap = cfg.overlap_seconds;
        let cursor = Arc::new(Mutex::new(SessionCursor::new(cfg)));
        let worker_cursor = cursor.clone();
        let journal = Arc::new(PcmJournal::create().map_err(|error| error.to_string())?);
        let mut journal_reader = journal.reader().map_err(|error| error.to_string())?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let cancel_token = CancelToken::new();
        let worker_cancel_token = cancel_token.clone();
        let journal_failed = Arc::new(AtomicBool::new(false));
        let worker_journal_failed = journal_failed.clone();
        let metrics = Arc::new(RollingMetrics::default());
        let worker_metrics = metrics.clone();
        let worker_journal = journal.clone();
        let (tx, rx) = mpsc::sync_channel::<Job>(MAX_PENDING_CHUNKS);
        let worker = std::thread::spawn(move || {
            let mut emit_tdt_preview = |text: &str| {
                if let Some(sink) = preview.as_ref() {
                    sink.emit(text, "");
                }
            };
            match crate::tdt_flow::run_worker(
                &transcriber.tm,
                &worker_journal,
                &mut journal_reader,
                &rx,
                &worker_cancelled,
                &worker_cancel_token,
                &worker_metrics,
                tdt_config,
                &mut emit_tdt_preview,
            ) {
                TdtWorkerResult::Unsupported => {}
                TdtWorkerResult::Success {
                    text,
                    descriptors,
                    decode_rtf_ewma,
                } => return WorkerOutput::tdt_success(text, descriptors, decode_rtf_ewma),
                TdtWorkerResult::Failure(error) => return WorkerOutput::failure(error),
            }

            // Capability-disabled/incompatible models enter the pre-existing
            // generic rolling path below without changed descriptors, reads,
            // assembler behavior, retry timing, or output canonicalization.
            // Time-based assembler with the fuzzy seam pass enabled (see
            // merge.rs). Native word timings use positional overlap dedup;
            // segment-only and timestamp-free models use bounded lexical escrow.
            let mut assembler = rolling_window::TimelineAssembler::new().with_fuzzy_seam(overlap);
            let mut records = Vec::<ChunkRecord>::new();
            let mut has_failure_barrier = false;
            // LocalAgreement-2 state for the live preview: the previous tail
            // hypothesis, so only text two consecutive decodes agree on is shown.
            let mut prev_tail_hyp: Vec<String> = Vec::new();
            let mut audio = Vec::<f32>::new();
            let mut decode_rtf_ewma: Option<f64> = None;
            loop {
                if worker_cancelled.load(Ordering::Acquire) {
                    return WorkerOutput::failure("rolling session cancelled");
                }
                // Preview ON polls so it can decode the tail between chunks;
                // preview OFF blocks forever (zero overhead — no wakeups).
                let job = if let Some(preview_sink) = preview.as_ref() {
                    match rx.recv_timeout(PREVIEW_INTERVAL) {
                        Ok(j) => j,
                        Err(RecvTimeoutError::Timeout) => {
                            Self::preview_tail(
                                &transcriber,
                                &worker_cursor,
                                &assembler,
                                preview_sink,
                                &mut prev_tail_hyp,
                                &worker_cancelled,
                            );
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            return WorkerOutput::failure("rolling worker channel disconnected");
                        }
                    }
                } else {
                    match rx.recv() {
                        Ok(j) => j,
                        Err(_) => {
                            return WorkerOutput::failure("rolling worker channel disconnected");
                        }
                    }
                };
                if worker_cancelled.load(Ordering::Acquire) {
                    return WorkerOutput::failure("rolling session cancelled");
                }
                match job {
                    Job::Chunk(chunk) => {
                        let queued_after = worker_metrics.dequeue_descriptor(chunk.fresh_frames());
                        // A chunk with no fresh audio past the cursor carries only
                        // overlap the previous chunk already covered (e.g. the
                        // stop-flush when nothing is unsent). Decoding it wastes
                        // compute and risks duplicating the tail — skip it.
                        if chunk.fresh_duration_sec() <= 0.0 {
                            continue;
                        }
                        // [GRAIN] boost-only AGC lifts quiet/laptop-mic speech to a
                        // good level for the model. Per-chunk is safe here — chunks
                        // are transcribed independently. The high-pass already ran
                        // on the shared frame.
                        // The shared manager waits out an in-flight model load
                        // internally, so a chunk arriving mid-load is transcribed
                        // once weights are ready — never dropped.
                        let started = Instant::now();
                        worker_metrics.begin_decode(chunk);
                        let decoded_result =
                            decode_descriptor(&transcriber, &mut journal_reader, &mut audio, chunk);
                        worker_metrics.end_decode(chunk.fresh_frames());
                        match decoded_result {
                            Ok(decoded) => {
                                let elapsed = started.elapsed();
                                let input_sec = chunk.input_duration_sec();
                                let sample_rtf = if input_sec > 0.0 {
                                    let rtf = elapsed.as_secs_f64() / input_sec;
                                    decode_rtf_ewma = Some(update_rtf_ewma(decode_rtf_ewma, rtf));
                                    rtf
                                } else {
                                    0.0
                                };
                                if worker_cancelled.load(Ordering::Acquire) {
                                    return WorkerOutput::failure("rolling session cancelled");
                                }
                                log::info!(
                                    "[GRAIN] chunk {} [{:.1}..{:.1}]s (audio={:.1}s, decode={:.2}s, rtf={:.3}, rtf_ewma={:.3}, {} words, queued={:.1}s) -> {:?}",
                                    chunk.sequence,
                                    chunk.fresh_start_sec,
                                    chunk.end_sec,
                                    input_sec,
                                    elapsed.as_secs_f64(),
                                    sample_rtf,
                                    decode_rtf_ewma.unwrap_or(0.0),
                                    decoded.words.len(),
                                    queued_after as f64 / 16_000.0,
                                    decoded.text
                                );
                                if !has_failure_barrier {
                                    add_decoded_chunk(&mut assembler, chunk, &decoded);
                                    if let Some(sink) = &preview {
                                        sink.emit(assembler.text(), "");
                                        prev_tail_hyp.clear();
                                    }
                                }
                                records.push(ChunkRecord {
                                    descriptor: chunk,
                                    status: ChunkStatus::Succeeded(decoded),
                                    decode_duration: elapsed,
                                });
                            }
                            Err(ChunkDecodeError::Transcription(error)) => {
                                log::warn!(
                                    "[GRAIN] rolling chunk {} transcribe failed; deferring one journal retry: {}",
                                    chunk.sequence,
                                    error
                                );
                                has_failure_barrier = true;
                                records.push(ChunkRecord {
                                    descriptor: chunk,
                                    status: ChunkStatus::Failed(error),
                                    decode_duration: started.elapsed(),
                                });
                            }
                            Err(ChunkDecodeError::Journal(error)) => {
                                log::error!("[GRAIN] rolling journal read failed: {error}");
                                worker_journal_failed.store(true, Ordering::Release);
                                return WorkerOutput::failure(format!(
                                    "Rolling transcription could not read audio chunk {}: {}",
                                    chunk.sequence, error
                                ));
                            }
                        }
                    }
                    Job::Finish => {
                        let recovered = if has_failure_barrier {
                            match recover_failed_chunks(&mut records, |chunk| {
                                match decode_descriptor(
                                    &transcriber,
                                    &mut journal_reader,
                                    &mut audio,
                                    chunk,
                                ) {
                                    Ok(decoded) => Ok(decoded),
                                    Err(ChunkDecodeError::Transcription(error)) => Err(error),
                                    Err(ChunkDecodeError::Journal(error)) => {
                                        worker_journal_failed.store(true, Ordering::Release);
                                        Err(format!("journal read failed: {error}"))
                                    }
                                }
                            }) {
                                Ok(count) => count,
                                Err(failures) => {
                                    return WorkerOutput::failure(format!(
                                        "Rolling transcription could not recover {} audio range(s): {}",
                                        failures.len(),
                                        failures.join("; ")
                                    ));
                                }
                            }
                        } else {
                            0
                        };
                        let text = if recovered > 0 {
                            match assemble_records(&records, overlap) {
                                Ok(text) => text,
                                Err(error) => return WorkerOutput::failure(error),
                            }
                        } else {
                            assembler.finish().to_string()
                        };
                        let decode_time: Duration =
                            records.iter().map(|record| record.decode_duration).sum();
                        log::info!(
                            "[GRAIN] rolling ledger finalized {} chunks in {:.2}s ({} recovered)",
                            records.len(),
                            decode_time.as_secs_f64(),
                            recovered
                        );
                        return WorkerOutput::success(text, recovered, decode_rtf_ewma);
                    }
                }
            }
        });
        Ok(Self {
            session_id,
            cursor,
            journal,
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled,
            cancel_token,
            journal_failed,
            metrics,
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        })
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
        if let Err(error) = self.journal.append(&buf) {
            log::error!("[GRAIN] rolling journal write failed: {error}");
            self.journal_failed.store(true, Ordering::Release);
            self.request_cancel();
            return;
        }
        self.frames_fed.fetch_add(buf.len(), Ordering::Relaxed);
        if self.journal_failed.load(Ordering::Acquire) {
            return;
        }
        // Prefer the VAD decision for silence gating (segments far better in
        // noisy rooms); fall back to raw RMS when VAD is disabled.
        let chunk = {
            let mut cursor = self.cursor.lock().unwrap();
            self.metrics
                .observe_cursor_candidate(cursor.retained_frames().saturating_add(buf.len()));
            match speech {
                Some(is_speech) => cursor.push_block_vad(&buf, is_speech),
                None => cursor.push_block(&buf, block_rms(&buf)),
            }
        };
        if let Some(chunk) = chunk {
            if let Err(error) = self.journal.flush() {
                log::error!("[GRAIN] rolling journal flush failed: {error}");
                self.journal_failed.store(true, Ordering::Release);
                self.request_cancel();
                return;
            }
            let sequence = self.chunks_emitted.fetch_add(1, Ordering::Relaxed) as u64 + 1;
            let job = ChunkJob::from_chunk(sequence, &chunk);
            let observation = self.metrics.reserve_descriptor(job.fresh_frames());
            match self.tx.try_send(Job::Chunk(job)) {
                Ok(()) => self.metrics.observe_published(observation),
                Err(TrySendError::Full(_)) => {
                    self.metrics.rollback_descriptor(job.fresh_frames());
                    log::error!(
                        "[GRAIN] rolling descriptor queue reached its {}-chunk bound; failing session explicitly",
                        MAX_PENDING_CHUNKS
                    );
                    self.journal_failed.store(true, Ordering::Release);
                    self.request_cancel();
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.metrics.rollback_descriptor(job.fresh_frames());
                    log::error!("[GRAIN] rolling descriptor worker disconnected");
                    self.journal_failed.store(true, Ordering::Release);
                    self.request_cancel();
                }
            }
        }
    }

    fn finish(&self) -> WorkerOutput {
        let finish_started = Instant::now();
        if self.journal_failed.load(Ordering::Acquire) {
            self.request_cancel();
            self.join_cancelled();
            return WorkerOutput::failure(
                "Rolling transcription stopped because its bounded audio pipeline failed",
            );
        }
        let final_tail = self.cursor.lock().unwrap().stop();
        if let Err(error) = self.journal.close() {
            log::error!("[GRAIN] rolling journal final close failed: {error}");
            self.request_cancel();
            self.join_cancelled();
            return WorkerOutput::failure(format!(
                "Rolling transcription could not close its audio journal: {error}"
            ));
        }
        if let Some(tail) = final_tail {
            let sequence = self.chunks_emitted.fetch_add(1, Ordering::Relaxed) as u64 + 1;
            let job = ChunkJob::from_chunk(sequence, &tail);
            let stop_debt_frames = self
                .metrics
                .current_debt_frames()
                .saturating_add(job.fresh_frames());
            log::info!(
                "[GRAIN] rolling stop debt: {:.1}s fresh audio",
                stop_debt_frames as f64 / 16_000.0
            );
            let observation = self.metrics.reserve_descriptor(job.fresh_frames());
            if self.tx.send(Job::Chunk(job)).is_ok() {
                self.metrics.observe_published(observation);
            } else {
                self.metrics.rollback_descriptor(job.fresh_frames());
            }
        } else {
            let stop_debt_frames = self.metrics.current_debt_frames();
            log::info!(
                "[GRAIN] rolling stop debt: {:.1}s fresh audio",
                stop_debt_frames as f64 / 16_000.0
            );
        }
        let _ = self.tx.send(Job::Finish);
        let output = self.join_worker();
        let frames = self.frames_fed.load(Ordering::Relaxed);
        log::info!(
            "[GRAIN] rolling session finished: {} frames ({:.1}s), {} chunks, {} recovered, rtf_ewma={:.3}, stop_to_final={:.2}s, peak_cursor={} frames, peak_worker={} frames, peak_descriptors={}, peak_debt={:.1}s, journal={} bytes, final={:?}, error={:?}",
            frames,
            frames as f64 / 16_000.0,
            self.chunks_emitted.load(Ordering::Relaxed),
            output.recovered_chunks,
            output.decode_rtf_ewma.unwrap_or(0.0),
            finish_started.elapsed().as_secs_f64(),
            self.metrics.peak_cursor_frames.load(Ordering::Acquire),
            self.metrics.peak_worker_frames.load(Ordering::Acquire),
            self.metrics
                .peak_queued_descriptors
                .load(Ordering::Acquire),
            self.metrics.peak_debt_frames.load(Ordering::Acquire) as f64 / 16_000.0,
            self.journal.byte_len(),
            output.text.trim(),
            output.error
        );
        output
    }

    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cancel_token.cancel();
        self.journal.wake_waiters();
        // Wake a preview-off worker blocked on `recv`. Queued chunks do not run:
        // the cancellation flag is checked before and after every receive.
        let _ = self.tx.try_send(Job::Finish);
    }

    fn join_cancelled(&self) {
        let _ = self.join_worker();
        log::info!(
            "[GRAIN] rolling session {} cancelled after {} frames; worker joined",
            self.session_id,
            self.frames_fed.load(Ordering::Relaxed)
        );
    }

    fn join_worker(&self) -> WorkerOutput {
        match self.worker.lock().unwrap().take() {
            Some(worker) => match worker.join() {
                Ok(output) => output,
                Err(_) => {
                    log::error!("[GRAIN] rolling worker panicked");
                    WorkerOutput::failure("Rolling transcription worker panicked")
                }
            },
            None => WorkerOutput::failure("Rolling transcription worker was unavailable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(sequence: u64, start_sec: f64, end_sec: f64) -> ChunkJob {
        ChunkJob {
            sequence,
            start_frame: (start_sec * 16_000.0) as u64,
            fresh_start_frame: (start_sec * 16_000.0) as u64,
            commit_end_frame: (end_sec * 16_000.0) as u64,
            end_frame: (end_sec * 16_000.0) as u64,
            start_sec,
            fresh_start_sec: start_sec,
            end_sec,
            boundary: rolling_window::CutKind::HardCut,
        }
    }

    fn succeeded(sequence: u64, text: &str) -> ChunkRecord {
        let descriptor = job(sequence, (sequence - 1) as f64, sequence as f64);
        ChunkRecord {
            descriptor,
            status: ChunkStatus::Succeeded(DecodedChunk {
                text: text.to_string(),
                words: Vec::new(),
                timing_quality: TimingQuality::Unavailable,
            }),
            decode_duration: Duration::ZERO,
        }
    }

    fn failed(sequence: u64, error: &str) -> ChunkRecord {
        let descriptor = job(sequence, (sequence - 1) as f64, sequence as f64);
        ChunkRecord {
            descriptor,
            status: ChunkStatus::Failed(error.to_string()),
            decode_duration: Duration::ZERO,
        }
    }

    #[test]
    fn model_load_barrier_precedes_rolling_worker_start() {
        let generation = AtomicU64::new(9);
        let loading_predicate = AtomicBool::new(false);

        let barrier = establish_model_load_barrier(&generation, 10, || {
            // A cancelled predecessor must already see the new generation when
            // model-load initiation establishes the predicate.
            assert_eq!(generation.load(Ordering::Acquire), 10);
            loading_predicate.store(true, Ordering::Release);
            Ok(())
        })
        .unwrap();

        // RollingSession::start is called only after this barrier returns, so
        // the TDT worker's first checkout cannot observe the old false state.
        assert!(loading_predicate.load(Ordering::Acquire));
        assert_eq!(generation.load(Ordering::Acquire), 10);
        drop(barrier);
    }

    #[test]
    fn timing_quality_never_treats_segment_rows_as_word_evidence() {
        let unavailable = Transcript::default();
        assert_eq!(
            timing_quality(&unavailable, &[]),
            TimingQuality::Unavailable
        );

        let mut segment_only = Transcript::default();
        segment_only
            .segments
            .push(transcribe_cpp::Segment::default());
        assert_eq!(
            timing_quality(&segment_only, &[]),
            TimingQuality::SegmentApproximate
        );

        assert_eq!(
            timing_quality(&segment_only, &[WordTiming::new("word", 0.0, 0.2)]),
            TimingQuality::NativeWord
        );
    }

    #[test]
    fn decode_rtf_ewma_is_constant_space_and_recent_weighted() {
        let first = update_rtf_ewma(None, 0.5);
        let second = update_rtf_ewma(Some(first), 1.0);
        assert_eq!(first, 0.5);
        assert!((second - 0.6).abs() < 1e-9);
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
        let (tx, rx) = mpsc::sync_channel::<Job>(1);
        let worker = std::thread::spawn(move || {
            let _ = rx.recv();
            assert!(worker_cancelled.load(Ordering::Acquire));
            exited.store(true, Ordering::Release);
            WorkerOutput::failure("cancelled")
        });
        let session = RollingSession {
            session_id: 42,
            cursor,
            journal: Arc::new(PcmJournal::create().unwrap()),
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled,
            cancel_token: CancelToken::new(),
            journal_failed: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(RollingMetrics::default()),
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        };

        session.request_cancel();
        session.join_cancelled();

        assert!(worker_exited.load(Ordering::Acquire));
        assert!(session.worker.lock().unwrap().is_none());
    }

    #[test]
    fn descriptor_queue_overflow_fails_closed_with_journal_intact() {
        let cursor = Arc::new(Mutex::new(SessionCursor::new(
            RollingWindowConfig::default(),
        )));
        let journal = Arc::new(PcmJournal::create().unwrap());
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::sync_channel::<Job>(1);
        let worker = std::thread::spawn(|| WorkerOutput::success(String::new(), 0, None));
        let session = RollingSession {
            session_id: 43,
            cursor,
            journal: journal.clone(),
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled: cancelled.clone(),
            cancel_token: CancelToken::new(),
            journal_failed: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(RollingMetrics::default()),
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        };
        let full_window = vec![0.1f32; 25 * 16_000];

        session.feed(&full_window, Some(true));
        session.feed(&full_window, Some(true));

        assert!(session.journal_failed.load(Ordering::Acquire));
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(journal.frame_count(), (50 * 16_000) as u64);
        assert_eq!(
            session.metrics.queued_fresh_frames.load(Ordering::Acquire),
            (25 * 16_000) as u64
        );
        let _ = session.join_worker();
    }

    #[test]
    fn stalled_fake_decoder_keeps_one_inference_and_bounded_descriptors() {
        let cursor = Arc::new(Mutex::new(SessionCursor::new(
            RollingWindowConfig::default(),
        )));
        let journal = Arc::new(PcmJournal::create().unwrap());
        let cancelled = Arc::new(AtomicBool::new(false));
        let metrics = Arc::new(RollingMetrics::default());
        let worker_metrics = metrics.clone();
        let active_decodes = Arc::new(AtomicUsize::new(0));
        let peak_decodes = Arc::new(AtomicUsize::new(0));
        let worker_active = active_decodes.clone();
        let worker_peak = peak_decodes.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (tx, rx) = mpsc::sync_channel::<Job>(MAX_PENDING_CHUNKS);
        let worker = std::thread::spawn(move || {
            let Job::Chunk(chunk) = rx.recv().expect("first descriptor") else {
                panic!("expected chunk descriptor");
            };
            worker_metrics.dequeue_descriptor(chunk.fresh_frames());
            worker_metrics.begin_decode(chunk);
            let active = worker_active.fetch_add(1, Ordering::AcqRel) + 1;
            worker_peak.fetch_max(active, Ordering::AcqRel);
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            worker_active.fetch_sub(1, Ordering::AcqRel);
            worker_metrics.end_decode(chunk.fresh_frames());
            WorkerOutput::success(String::new(), 0, Some(1.0))
        });
        let session = RollingSession {
            session_id: 44,
            cursor,
            journal: journal.clone(),
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled: cancelled.clone(),
            cancel_token: CancelToken::new(),
            journal_failed: Arc::new(AtomicBool::new(false)),
            metrics: metrics.clone(),
            frames_fed: AtomicUsize::new(0),
            chunks_emitted: AtomicUsize::new(0),
        };
        let full_window = vec![0.1f32; 25 * 16_000];

        session.feed(&full_window, Some(true));
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("fake decode should start");
        for _ in 0..MAX_PENDING_CHUNKS {
            session.feed(&full_window, Some(true));
        }

        assert!(!session.journal_failed.load(Ordering::Acquire));
        assert_eq!(
            metrics.peak_queued_descriptors.load(Ordering::Acquire),
            MAX_PENDING_CHUNKS
        );
        assert_eq!(peak_decodes.load(Ordering::Acquire), 1);
        assert_eq!(
            metrics.peak_debt_frames.load(Ordering::Acquire),
            ((MAX_PENDING_CHUNKS + 1) * 25 * 16_000) as u64
        );

        // One more descriptor cannot fit. Its audio is still durable, and the
        // session fails closed instead of retaining PCM or starting a decoder.
        session.feed(&full_window, Some(true));
        assert!(session.journal_failed.load(Ordering::Acquire));
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(
            journal.frame_count(),
            ((MAX_PENDING_CHUNKS + 2) * 25 * 16_000) as u64
        );
        assert!(std::mem::size_of::<ChunkJob>() <= 80);

        release_tx.send(()).unwrap();
        let _ = session.join_worker();
        assert_eq!(active_decodes.load(Ordering::Acquire), 0);
    }

    #[test]
    fn successful_ledger_performs_no_recovery_decode() {
        let mut records = vec![succeeded(1, "alpha"), succeeded(2, "beta")];
        let mut calls = 0usize;

        let recovered = recover_failed_chunks(&mut records, |_| {
            calls += 1;
            Err("must not run".to_string())
        })
        .unwrap();

        assert_eq!(recovered, 0);
        assert_eq!(calls, 0);
        assert_eq!(assemble_records(&records, 2.0).unwrap(), "alpha beta");
    }

    #[test]
    fn failed_middle_range_is_retried_once_and_reassembled_in_order() {
        let mut records = vec![
            succeeded(1, "alpha"),
            failed(2, "temporary model error"),
            succeeded(3, "gamma"),
        ];
        let mut retried = Vec::new();

        let recovered = recover_failed_chunks(&mut records, |descriptor| {
            retried.push((
                descriptor.sequence,
                descriptor.start_frame,
                descriptor.end_frame,
            ));
            Ok(DecodedChunk {
                text: "beta".to_string(),
                words: Vec::new(),
                timing_quality: TimingQuality::Unavailable,
            })
        })
        .unwrap();

        assert_eq!(recovered, 1);
        assert_eq!(retried, vec![(2, 16_000, 32_000)]);
        assert_eq!(assemble_records(&records, 2.0).unwrap(), "alpha beta gamma");
    }

    #[test]
    fn unrecovered_final_range_returns_explicit_failure() {
        let mut records = vec![succeeded(1, "alpha"), failed(2, "first failure")];
        let mut attempts = 0usize;

        let failures = recover_failed_chunks(&mut records, |descriptor| {
            attempts += 1;
            assert_eq!(descriptor.sequence, 2);
            Err("retry failure".to_string())
        })
        .unwrap_err();

        assert_eq!(attempts, 1);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("chunk 2"));
        assert!(failures[0].contains("first failure"));
        assert!(failures[0].contains("retry failure"));
        assert!(assemble_records(&records, 2.0).is_err());
    }
}
