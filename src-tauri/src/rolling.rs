//! [GRAIN] Real-time rolling-window transcription engine for the core.
//!
//! Uses grain-transcribe's `GrainModel` — loading the EXACT model files Handy
//! already downloads/manages (via `ModelManager`) — with its OWN on-demand
//! lifecycle. Mutual-exclusion with Handy's `TranscriptionManager` keeps ≤1
//! model resident in RAM (task A5). Engine is loaded on hotkey press and unloaded
//! when idle / when switching to batch.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use grain_transcribe::{block_rms, i16_to_f32, Asr, EngineKind, GrainModel};
use rolling_window::{
    AudioChunk, RollingWindowConfig, SessionCursor, TimelineAssembler, WordTiming,
};
use tauri::{AppHandle, Manager};

use crate::managers::model::{EngineType, ModelManager};
use crate::settings::get_settings;

/// Map Handy's model engine family to grain-transcribe's (1:1).
fn map_engine(e: EngineType) -> EngineKind {
    match e {
        EngineType::Whisper => EngineKind::Whisper,
        EngineType::Parakeet => EngineKind::Parakeet,
        EngineType::Moonshine => EngineKind::Moonshine,
        EngineType::MoonshineStreaming => EngineKind::MoonshineStreaming,
        EngineType::SenseVoice => EngineKind::SenseVoice,
        EngineType::GigaAM => EngineKind::GigaAM,
        EngineType::Canary => EngineKind::Canary,
        EngineType::Cohere => EngineKind::Cohere,
    }
}

struct Loaded {
    id: String,
    model: GrainModel,
}

// Model load lifecycle, surfaced as a lock-free signal so the session worker can
// WAIT for an in-flight load instead of dropping early chunks (the load race).
const LS_IDLE: u8 = 0;
const LS_LOADING: u8 = 1;
const LS_LOADED: u8 = 2;
const LS_FAILED: u8 = 3;

/// On-demand rolling transcription model, held in Tauri managed state.
#[derive(Default)]
pub struct RollingTranscriber {
    loaded: Mutex<Option<Loaded>>,
    /// Lock-free mirror of the load lifecycle (`LS_*`) for the worker to poll.
    load_state: AtomicU8,
    /// The current live recording's rolling session, if any.
    active: Mutex<Option<Arc<RollingSession>>>,
    /// Last-use timestamp (ms) for the idle-unload watcher.
    last_activity: AtomicU64,
    /// [GRAIN] Mirror of `settings.audio_conditioning`, refreshed on each load.
    /// When set, each rolling chunk gets boost-only AGC before transcription —
    /// the high-pass already ran upstream on the shared 16 kHz frame.
    conditioning: AtomicBool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl RollingTranscriber {
    /// Load the currently-selected model if not already loaded (on hotkey press).
    /// Reuses it if the selection is unchanged. Same files as Handy's batch path.
    pub fn ensure_loaded(&self, app: &AppHandle) -> Result<(), String> {
        let settings = get_settings(app);
        let model_id = settings.selected_model;
        // Refresh the conditioning mirror so the session worker (no AppHandle)
        // can honor the current setting.
        self.conditioning
            .store(settings.audio_conditioning, Ordering::Relaxed);
        if model_id.is_empty() {
            return Err("no model selected".into());
        }
        if self
            .loaded
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|l| l.id == model_id)
        {
            self.load_state.store(LS_LOADED, Ordering::Release);
            self.touch();
            return Ok(());
        }
        // Mark loading up-front so the worker waits rather than seeing IDLE and
        // giving up while we fetch info / load weights.
        self.load_state.store(LS_LOADING, Ordering::Release);
        let load = (|| {
            let mm = app.state::<Arc<ModelManager>>();
            let info = mm
                .get_model_info(&model_id)
                .ok_or_else(|| format!("model not found: {model_id}"))?;
            if !info.is_downloaded {
                return Err(format!("model not downloaded: {model_id}"));
            }
            let path = mm.get_model_path(&model_id).map_err(|e| e.to_string())?;
            let kind = map_engine(info.engine_type);
            // [GRAIN] A5 mutual exclusion: free Handy's batch model before loading
            // the rolling one, so ≤1 model's weights are ever resident.
            if let Some(tm) =
                app.try_state::<Arc<crate::managers::transcription::TranscriptionManager>>()
            {
                let _ = tm.unload_model();
            }
            log::info!(
                "[GRAIN] loading rolling model '{model_id}' ({kind:?}) from {}",
                path.display()
            );
            let started = Instant::now();
            // [GRAIN] Prefer the GPU for the rolling model. transcribe-rs's `Auto`
            // deliberately excludes DirectML, so on Windows Parakeet runs on the
            // CPU even with an idle GPU (the every-chunk CPU spike). When the user
            // is on `Auto` AND DirectML is compiled in, load this model on
            // DirectML; ORT's execution-provider chain falls back to CPU on its
            // own if there's no DirectML device or an op isn't supported. We
            // restore the global immediately after load so Handy's batch path
            // keeps whatever accelerator the user chose. (No-op on Mac/Linux,
            // whose `Auto` already selects CoreML/CUDA/ROCm.)
            use transcribe_rs::accel::{get_ort_accelerator, set_ort_accelerator, OrtAccelerator};
            let accel_prev = get_ort_accelerator();
            let prefer_gpu = accel_prev == OrtAccelerator::Auto
                && OrtAccelerator::available().contains(&OrtAccelerator::DirectMl);
            if prefer_gpu {
                log::info!("[GRAIN] rolling: preferring DirectML (GPU); CPU fallback is automatic");
                set_ort_accelerator(OrtAccelerator::DirectMl);
            }
            let load_result = GrainModel::load(kind, &path);
            if prefer_gpu {
                set_ort_accelerator(accel_prev); // restore for the batch path
            }
            let model = load_result.map_err(|e| e.to_string())?;
            log::info!("[GRAIN] rolling model loaded in {:?}", started.elapsed());
            Ok::<_, String>(Loaded {
                id: model_id,
                model,
            })
        })();
        match load {
            Ok(loaded) => {
                *self.loaded.lock().unwrap() = Some(loaded);
                self.load_state.store(LS_LOADED, Ordering::Release);
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.load_state.store(LS_FAILED, Ordering::Release);
                Err(e)
            }
        }
    }

    /// Free the model weights (idle / switching to batch).
    pub fn unload(&self) {
        self.load_state.store(LS_IDLE, Ordering::Release);
        if self.loaded.lock().unwrap().take().is_some() {
            log::info!("[GRAIN] rolling model unloaded");
        }
    }

    /// Block until the model finishes loading (`true`) or the load failed / a
    /// deadline passed (`false`). Used by the session worker so a chunk emitted
    /// during the load is transcribed once weights are ready — never dropped.
    fn wait_for_model(&self, max: Duration) -> bool {
        let deadline = Instant::now() + max;
        loop {
            match self.load_state.load(Ordering::Acquire) {
                LS_LOADED => return true,
                LS_FAILED | LS_IDLE => return false,
                _ => {} // LS_LOADING — keep waiting
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.lock().unwrap().is_some()
    }

    /// Transcribe one chunk of 16 kHz mono `f32` → (text, optional chunk-relative
    /// word timings). Errs if no model is loaded.
    pub fn transcribe_chunk(
        &self,
        samples: &[f32],
    ) -> Result<(String, Option<Vec<WordTiming>>), String> {
        let mut guard = self.loaded.lock().unwrap();
        let loaded = guard.as_mut().ok_or("rolling model not loaded")?;
        loaded.model.transcribe(samples).map_err(|e| e.to_string())
    }

    // -- live session control ---------------------------------------------

    /// Begin a live rolling session (on recording start). The model must already
    /// be loaded via [`ensure_loaded`].
    pub fn start_session(self: &Arc<Self>, max_chunk_seconds: f64) {
        // If a load is about to be (or is being) kicked off, advertise LOADING so
        // the worker waits for it rather than racing to IDLE and dropping audio.
        let _ = self.load_state.compare_exchange(
            LS_IDLE,
            LS_LOADING,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let session = Arc::new(RollingSession::start(self.clone(), max_chunk_seconds));
        *self.active.lock().unwrap() = Some(session);
        self.touch();
        log::info!("[GRAIN] rolling session started");
    }

    /// Feed one captured 16 kHz mono frame to the active session (audio thread).
    /// No-op when no session is active.
    pub fn feed(&self, frame: &[f32]) {
        if let Some(session) = self.active.lock().unwrap().as_ref() {
            session.feed(frame);
        }
    }

    /// Stop the live session: flush the tail, drain the worker, return the final
    /// assembled transcript. `None` if no session was active.
    pub fn finish_session(&self) -> Option<String> {
        self.touch();
        let session = self.active.lock().unwrap().take()?;
        Some(session.finish())
    }

    /// Abort the live session without producing a transcript (cancel).
    pub fn cancel_session(&self) {
        self.active.lock().unwrap().take();
    }

    pub fn has_active_session(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    fn touch(&self) {
        self.last_activity.store(now_ms(), Ordering::Relaxed);
    }

    /// Spawn the idle-unload watcher: frees the rolling model after the user's
    /// configured `model_unload_timeout` of inactivity (mirrors Handy's TM).
    pub fn start_idle_watcher(self: &Arc<Self>, app: AppHandle) {
        let this = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            if this.has_active_session() || !this.is_loaded() {
                continue;
            }
            let timeout = get_settings(&app).model_unload_timeout;
            if let Some(limit_s) = timeout.to_seconds() {
                let idle_ms = now_ms().saturating_sub(this.last_activity.load(Ordering::Relaxed));
                if idle_ms > limit_s.saturating_mul(1000) {
                    log::info!("[GRAIN] rolling model idle {}s — unloading", idle_ms / 1000);
                    this.unload();
                }
            }
        });
    }
}

/// One live recording's rolling-window transcription. Frames are fed from the
/// audio thread (cheap); a single worker thread transcribes finalized chunks
/// serially (never blocking audio) and assembles the transcript. No partial
/// text is ever surfaced — only the final string at [`finish`](RollingSession::finish).
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
            // Time-based assembler with the fuzzy seam pass enabled (see merge.rs).
            let mut assembler = TimelineAssembler::new().with_fuzzy_seam(overlap);
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Chunk(chunk) => {
                        // Chunks own their samples, so we can wait out an in-flight
                        // model load instead of dropping the audio (the load race).
                        if !transcriber.wait_for_model(Duration::from_secs(20)) {
                            log::warn!(
                                "[GRAIN] model not ready — dropping chunk [{:.1}..{:.1}]s",
                                chunk.start_sec,
                                chunk.end_sec
                            );
                            continue;
                        }
                        let mut audio = i16_to_f32(&chunk.samples);
                        // [GRAIN] boost-only AGC lifts quiet/laptop-mic speech to a
                        // good level for the model. Per-chunk is safe here — chunks
                        // are transcribed independently (text only, no audio output
                        // to pump). The high-pass already ran on the shared frame.
                        if transcriber.conditioning.load(Ordering::Relaxed) {
                            crate::audio_toolkit::audio::normalize_gain(&mut audio);
                        }
                        match transcriber.transcribe_chunk(&audio) {
                            Ok((text, words)) => {
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
                                    words.as_deref(),
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
        let buf: Vec<i16> = frame
            .iter()
            .map(|&s| (s * 32768.0).clamp(-32768.0, 32767.0) as i16)
            .collect();
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
