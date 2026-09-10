//! [GRAIN] Parakeet TDT v2/v3 Flow recording service.
//!
//! Capture only appends exact Float32 samples to a temporary journal and sends
//! a coalescing wake. One serial worker owns the shared transcribe.cpp lease,
//! finalizes stable Fluid-style windows, and optionally revisits only the tail
//! for live previews. Standard and Native ASR do not use this module.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use grain_core::DaemonEvent;
use tauri::{AppHandle, Manager};
use transcribe_cpp::CancelToken;

use crate::grain_audio_journal::{PcmJournal, PcmJournalReader};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::tdt_flow::{
    validate_model_installation, TdtAccumulator, TdtRunConfig,
};

const PREVIEW_MIN_SAMPLES: u64 = grain_tdt::SAMPLE_RATE;
const PREVIEW_INTERVAL: Duration = Duration::from_millis(600);

#[derive(Clone)]
struct PreviewSink {
    app: AppHandle,
    session_id: u64,
    scrap_that: bool,
    active_generation: Arc<AtomicU64>,
}

impl PreviewSink {
    fn emit(&self, text: &str) {
        if !generation_is_current(&self.active_generation, self.session_id) {
            return;
        }
        let (committed, tentative) = if self.scrap_that {
            crate::audio_toolkit::scrub_stream_preview("", text)
        } else {
            (String::new(), text.to_string())
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

pub struct RollingTranscriber {
    tm: Arc<TranscriptionManager>,
    active: Mutex<Option<Arc<RollingSession>>>,
    active_generation: Arc<AtomicU64>,
}

fn generation_is_current(active_generation: &AtomicU64, session_id: u64) -> bool {
    active_generation.load(Ordering::Acquire) == session_id
}

#[derive(Clone)]
pub(crate) struct RollingSessionOutput {
    pub(crate) text: String,
    pub(crate) error: Option<String>,
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
    active_generation.store(session_id, Ordering::Release);
    initiate_load()?;
    Ok(ModelLoadBarrier)
}

impl RollingTranscriber {
    pub fn new(tm: Arc<TranscriptionManager>) -> Self {
        Self {
            tm,
            active: Mutex::new(None),
            active_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn start_session(
        self: &Arc<Self>,
        app: AppHandle,
        session_id: u64,
        preview: bool,
    ) -> Result<(), String> {
        let settings = get_settings(&app);
        crate::tdt_flow::validate_model(&settings.selected_model, settings.translate_to_english)?;
        let model_id = settings.selected_model.clone();
        let is_downloaded = app
            .try_state::<Arc<ModelManager>>()
            .and_then(|models| models.get_model_info(&model_id))
            .is_some_and(|model| model.is_downloaded);
        validate_model_installation(is_downloaded)?;

        // Serialize replacement through the active-session lock. The worker
        // owns the model lease for its whole lifetime, so spawning its
        // replacement before cancellation/join would make the new worker lose
        // lease admission and fail the recording immediately.
        let mut active = self.active.lock().unwrap();
        if let Some(previous) = active.take() {
            log::warn!("[GRAIN] replacing an unfinished Flow session");
            previous.request_cancel();
            previous.join_cancelled();
        }
        let _barrier = establish_model_load_barrier(&self.active_generation, session_id, || {
            self.tm.initiate_model_load_for(model_id.clone());
            Ok(())
        })?;

        let config = TdtRunConfig {
            language: self
                .tm
                .grain_transcribe_cpp_language_for_model(&settings.selected_language, &model_id),
            model_id,
        };
        let sink = preview.then(|| PreviewSink {
            app,
            session_id,
            scrap_that: settings.scrap_that_enabled,
            active_generation: Arc::clone(&self.active_generation),
        });
        let session = Arc::new(
            RollingSession::start(Arc::clone(self), session_id, config, sink)
                .map_err(|error| format!("Flow could not create its audio journal: {error}"))?,
        );
        *active = Some(session);
        log::info!("[GRAIN] Flow session started (preview={preview})");
        Ok(())
    }

    pub fn feed(&self, frame: &[f32], _speech: Option<bool>) {
        if let Some(session) = self.active.lock().unwrap().as_ref() {
            session.feed(frame);
        }
    }

    pub fn finish_session(&self) -> Option<RollingSessionOutput> {
        let session = self.active.lock().unwrap().take()?;
        let worker = session.finish();
        let output = RollingSessionOutput {
            text: worker.text,
            error: worker.error,
            journal: Arc::clone(&session.journal),
        };
        self.tm.maybe_unload_immediately("Flow session");
        Some(output)
    }

    pub fn cancel_session(self: &Arc<Self>) {
        if let Some(session) = self.active.lock().unwrap().take() {
            self.retire_cancelled_session(session);
        }
    }

    fn retire_cancelled_session(self: &Arc<Self>, session: Arc<RollingSession>) {
        let session_id = session.session_id;
        session.request_cancel();
        let transcriber = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("grain-flow-cancel".into())
            .spawn(move || {
                session.join_cancelled();
                let Some(transcriber) = transcriber.upgrade() else {
                    return;
                };
                let still_current = transcriber.active_generation.load(Ordering::Acquire)
                    == session_id
                    && transcriber.active.lock().unwrap().is_none();
                if still_current {
                    transcriber
                        .tm
                        .maybe_unload_immediately("cancelled Flow session");
                }
            })
            .expect("failed to spawn Flow cancellation cleanup");
    }
}

enum WorkerCommand {
    Wake,
    Finish,
}

struct RollingSession {
    session_id: u64,
    journal: Arc<PcmJournal>,
    tx: SyncSender<WorkerCommand>,
    worker: Mutex<Option<JoinHandle<WorkerOutput>>>,
    cancelled: Arc<AtomicBool>,
    cancel_token: CancelToken,
    journal_failed: AtomicBool,
}

impl RollingSession {
    fn start(
        transcriber: Arc<RollingTranscriber>,
        session_id: u64,
        config: TdtRunConfig,
        sink: Option<PreviewSink>,
    ) -> std::io::Result<Self> {
        let journal = Arc::new(PcmJournal::create()?);
        let reader = journal.reader()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_token = CancelToken::new();
        let (tx, rx) = mpsc::sync_channel(1);
        let worker_journal = Arc::clone(&journal);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_token = cancel_token.clone();
        let worker = std::thread::Builder::new()
            .name("grain-flow".into())
            .spawn(move || {
                run_worker(
                    &transcriber.tm,
                    &worker_journal,
                    reader,
                    &rx,
                    &worker_cancelled,
                    &worker_token,
                    &config,
                    sink.as_ref(),
                )
            })?;
        Ok(Self {
            session_id,
            journal,
            tx,
            worker: Mutex::new(Some(worker)),
            cancelled,
            cancel_token,
            journal_failed: AtomicBool::new(false),
        })
    }

    fn feed(&self, frame: &[f32]) {
        if self.cancelled.load(Ordering::Acquire) || frame.is_empty() {
            return;
        }
        if let Err(error) = self.journal.append(frame) {
            log::error!("[GRAIN] Flow journal write failed: {error}");
            self.journal_failed.store(true, Ordering::Release);
            self.request_cancel();
            return;
        }
        match self.tx.try_send(WorkerCommand::Wake) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                self.journal_failed.store(true, Ordering::Release);
                self.request_cancel();
            }
        }
    }

    fn finish(&self) -> WorkerOutput {
        if self.journal_failed.load(Ordering::Acquire) {
            self.request_cancel();
            self.join_cancelled();
            return WorkerOutput::failure("Flow stopped because its audio journal failed");
        }
        if let Err(error) = self.journal.close() {
            self.request_cancel();
            self.join_cancelled();
            return WorkerOutput::failure(format!(
                "Flow could not close its audio journal: {error}"
            ));
        }
        if self.tx.send(WorkerCommand::Finish).is_err() {
            return WorkerOutput::failure("Flow worker disconnected before finalization");
        }
        self.join_worker()
    }

    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cancel_token.cancel();
        let _ = self.tx.try_send(WorkerCommand::Wake);
    }

    fn join_cancelled(&self) {
        let _ = self.join_worker();
    }

    fn join_worker(&self) -> WorkerOutput {
        let Some(worker) = self.worker.lock().unwrap().take() else {
            return WorkerOutput::failure("Flow worker was already joined");
        };
        worker
            .join()
            .unwrap_or_else(|_| WorkerOutput::failure("Flow worker panicked"))
    }
}

struct WorkerOutput {
    text: String,
    error: Option<String>,
}

impl WorkerOutput {
    fn success(text: String) -> Self {
        Self { text, error: None }
    }

    fn failure(error: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            error: Some(error.into()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    manager: &TranscriptionManager,
    journal: &PcmJournal,
    reader: PcmJournalReader,
    rx: &Receiver<WorkerCommand>,
    cancelled: &AtomicBool,
    cancel_token: &CancelToken,
    config: &TdtRunConfig,
    sink: Option<&PreviewSink>,
) -> WorkerOutput {
    let result = manager.with_grain_flow_session(&config.model_id, |session| {
        session.set_cancel_token(cancel_token);
        let result = run_session(session, journal, reader, rx, cancelled, config, sink);
        session.clear_cancel_token();
        Ok(result)
    });
    match result {
        Ok(output) => output,
        Err(error) => WorkerOutput::failure(format!(
            "Flow could not acquire the loaded transcription session: {error}"
        )),
    }
}

fn run_session(
    session: &mut transcribe_cpp::Session,
    journal: &PcmJournal,
    mut reader: PcmJournalReader,
    rx: &Receiver<WorkerCommand>,
    cancelled: &AtomicBool,
    config: &TdtRunConfig,
    sink: Option<&PreviewSink>,
) -> WorkerOutput {
    let mut accumulator = match TdtAccumulator::new(session, config) {
        Ok(value) => value,
        Err(error) => return WorkerOutput::failure(error),
    };
    let mut audio = Vec::with_capacity(grain_tdt::MAX_MODEL_SAMPLES as usize);
    let mut pending_preview = false;
    let mut preview_deadline = Instant::now();
    let mut first_error: Option<String> = None;
    let started = Instant::now();

    loop {
        if cancelled.load(Ordering::Acquire) {
            return WorkerOutput::failure("Flow session cancelled");
        }

        let command =
            if sink.is_some() && pending_preview && journal.frame_count() >= PREVIEW_MIN_SAMPLES {
                let now = Instant::now();
                if now >= preview_deadline {
                    None
                } else {
                    match rx.recv_timeout(preview_deadline - now) {
                        Ok(command) => Some(command),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => {
                            return WorkerOutput::failure("Flow worker channel disconnected")
                        }
                    }
                }
            } else {
                match rx.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return WorkerOutput::failure("Flow worker channel disconnected"),
                }
            };

        match command {
            Some(WorkerCommand::Wake) => pending_preview = true,
            Some(WorkerCommand::Finish) => {
                let total = journal.frame_count();
                if total < PREVIEW_MIN_SAMPLES {
                    return WorkerOutput::success(String::new());
                }
                let mut final_error = first_error.take();
                if final_error.is_none() {
                    match accumulator.render(
                        session,
                        config,
                        journal,
                        &mut reader,
                        &mut audio,
                        total,
                    ) {
                        Ok(text) => {
                            log::info!(
                                "[GRAIN] Flow finalized {} samples via {} window decodes in {:.2}s",
                                total,
                                accumulator.decoded_windows(),
                                started.elapsed().as_secs_f64()
                            );
                            return WorkerOutput::success(text);
                        }
                        Err(error) => final_error = Some(error),
                    }
                }

                // One clean replay from journal with a fresh accumulator. There
                // is no Standard/generic fallback and no partial accumulation.
                let original = final_error.unwrap_or_else(|| "unknown Flow failure".into());
                let mut replay = match TdtAccumulator::new(session, config) {
                    Ok(value) => value,
                    Err(error) => {
                        return WorkerOutput::failure(format!(
                            "Flow failed ({original}); clean replay could not start: {error}"
                        ))
                    }
                };
                match replay.render(session, config, journal, &mut reader, &mut audio, total) {
                    Ok(text) => {
                        log::warn!(
                            "[GRAIN] Flow recovered through one clean journal replay: {original}"
                        );
                        return WorkerOutput::success(text);
                    }
                    Err(replay_error) => {
                        return WorkerOutput::failure(format!(
                            "Flow failed ({original}); clean replay also failed: {replay_error}"
                        ))
                    }
                }
            }
            None => {
                if first_error.is_none() {
                    let total = journal.frame_count();
                    match accumulator.render(
                        session,
                        config,
                        journal,
                        &mut reader,
                        &mut audio,
                        total,
                    ) {
                        Ok(text) => {
                            if !cancelled.load(Ordering::Acquire) {
                                if let Some(sink) = sink {
                                    sink.emit(&text);
                                }
                            }
                        }
                        Err(error) => first_error = Some(error),
                    }
                }
                pending_preview = false;
                preview_deadline = Instant::now() + PREVIEW_INTERVAL;
            }
        }

        // With previews disabled, wakes still finalize stable windows. Tail
        // inference remains exactly zero until Finish.
        if sink.is_none() && first_error.is_none() && pending_preview {
            let total = journal.frame_count();
            if let Err(error) =
                accumulator.process_stable(session, config, journal, &mut reader, &mut audio, total)
            {
                first_error = Some(error);
            }
            pending_preview = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_load_barrier_publishes_generation_before_load() {
        let generation = AtomicU64::new(0);
        let observed = AtomicU64::new(0);
        establish_model_load_barrier(&generation, 42, || {
            observed.store(generation.load(Ordering::Acquire), Ordering::Relaxed);
            Ok(())
        })
        .unwrap();
        assert_eq!(observed.load(Ordering::Relaxed), 42);
    }

    #[test]
    fn preview_contract_is_one_second_then_completion_plus_six_hundred_ms() {
        assert_eq!(PREVIEW_MIN_SAMPLES, 16_000);
        assert_eq!(PREVIEW_INTERVAL, Duration::from_millis(600));
    }

    #[test]
    fn stale_preview_generation_is_rejected() {
        let generation = AtomicU64::new(11);
        assert!(generation_is_current(&generation, 11));
        assert!(!generation_is_current(&generation, 10));
        generation.store(12, Ordering::Release);
        assert!(!generation_is_current(&generation, 11));
    }
}
