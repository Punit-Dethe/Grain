// [GRAIN] transcribe-rs (ONNX) was removed entirely: every legacy ONNX family
// (parakeet, moonshine, sensevoice, gigaam, canary, cohere) ships as GGUF in
// the bundled catalog, so transcribe-cpp is the ONLY inference engine. The
// `EngineType` ONNX variants remain as inert enum tags for upstream-diff
// parity; loading one yields a clear error pointing at the GGUF equivalent.
use crate::audio_toolkit::{apply_custom_words, filter_transcription_output};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::{EngineType, ModelManager};
use crate::settings::{
    get_settings, AppSettings, ModelUnloadTimeout, TranscribeAcceleratorSetting,
};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tauri_specta::Event;
use transcribe_cpp::{
    Backend, Feature, Model, ModelOptions, RunExtension, RunOptions, Session, StreamOptions, Task,
    TimestampKind, Transcript, WhisperRunOptions,
};

const STREAM_PERF_LOG_INTERVAL: Duration = Duration::from_secs(5);
const STREAM_FINALIZE_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

/// Live transcription snapshot emitted to the overlay during a streaming run.
/// `committed` is the append-only, flicker-free prefix; `tentative` is the
/// volatile suffix the model may still rewrite.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct StreamTextEvent {
    pub committed: String,
    pub tentative: String,
}

/// Commands sent to the streaming worker thread. Audio frames and the finalize
/// request travel the same channel so FIFO ordering guarantees every fed frame
/// is processed before finalize runs.
enum StreamCmd {
    Feed(Vec<f32>),
    /// Flush the stream and reply with the final text, or `None` if no stream
    /// was ever active (caller should fall back to batch transcription).
    Finalize(mpsc::Sender<Option<String>>),
    Cancel,
}

/// Routes real-time audio frames to the active streaming worker. Shared between
/// the [`TranscriptionManager`] (opens/closes the route) and the audio recorder's
/// per-frame callback (feeds frames). The recorder holds an `Arc<StreamRouter>`
/// directly, so a frame with no stream pending costs a single relaxed atomic
/// load — no Tauri state lookup, no mutex lock.
pub struct StreamRouter {
    /// Command channel to the active streaming worker, present from
    /// `start_stream` until `finalize_stream`/`cancel_stream`.
    tx: Mutex<Option<mpsc::Sender<StreamCmd>>>,
    /// True while a stream is pending or active (channel is open). The audio
    /// callback checks this first to avoid the mutex lock when no stream runs.
    open: Arc<AtomicBool>,
}

impl StreamRouter {
    fn new() -> Self {
        Self {
            tx: Mutex::new(None),
            open: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Open a fresh command channel for a new streaming session, returning the
    /// receiver the worker should drain. Caller must ensure no prior channel is
    /// still open.
    fn open(&self) -> mpsc::Receiver<StreamCmd> {
        let (tx, rx) = mpsc::channel::<StreamCmd>();
        *self.tx.lock().unwrap() = Some(tx);
        self.open.store(true, Ordering::Relaxed);
        rx
    }

    /// Take the sender out (closing the channel to new feeds). Returns the
    /// sender so the caller can send the final `Finalize`/`Cancel` command.
    fn take(&self) -> Option<mpsc::Sender<StreamCmd>> {
        self.open.store(false, Ordering::Relaxed);
        self.tx.lock().unwrap().take()
    }

    /// Drop the channel and mark closed without sending a final command (used
    /// when the worker exits without a finalize/cancel handshake).
    fn clear(&self) {
        self.open.store(false, Ordering::Relaxed);
        *self.tx.lock().unwrap() = None;
    }

    /// Forward a 16 kHz frame to the active streaming worker. Cheap no-op (a
    /// single relaxed atomic load) when no stream is pending.
    pub fn feed(&self, frame: &[f32]) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(StreamCmd::Feed(frame.to_vec()));
        }
    }

    /// Whether a stream is pending or active.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }
}

enum LoadedEngine {
    /// Any GGUF/GGML model via transcribe-cpp — the only engine since the
    /// [GRAIN] transcribe-rs removal. Holds the live `Session`, which keeps its
    /// `Model` alive internally, so repeated dictation reuses the session
    /// without reloading.
    TranscribeCpp(Session),
}

/// RAII guard that clears the `is_loading` flag and notifies waiters on drop.
/// Ensures the loading flag is always reset, even on early returns or panics.
pub struct LoadingGuard {
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        // Recover from a poisoned mutex instead of panicking —
        // a panic inside Drop calls abort().
        let mut is_loading = match self.is_loading.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("Recovered poisoned is_loading mutex during LoadingGuard drop — a panic occurred earlier this session");
                e.into_inner()
            }
        };
        *is_loading = false;
        self.loading_condvar.notify_all();
    }
}

/// RAII guard that clears the streaming worker/lease flags on any worker exit -
/// normal return, early return, or a panic in an engine call that unwinds the
/// detached worker thread. Tokens prevent an older worker from clearing a newer
/// worker's state if a start/finalize race ever slips through.
struct StreamWorkerGuard {
    worker_id: u64,
    active_stream_worker: Arc<AtomicU64>,
    active_engine_lease: Arc<AtomicU64>,
}

impl Drop for StreamWorkerGuard {
    fn drop(&mut self) {
        let _ = self.active_engine_lease.compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let _ = self.active_stream_worker.compare_exchange(
            self.worker_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<Option<LoadedEngine>>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    current_model_id: Arc<Mutex<Option<String>>>,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
    reload_model_on_next_use: Arc<AtomicBool>,
    /// Routes real-time audio frames to the active streaming worker; see
    /// [`StreamRouter`]. Shared with the audio recorder so per-frame feeds skip
    /// Tauri state and the manager lock.
    router: Arc<StreamRouter>,
    /// Streaming uses three independent flags: router open = frames should route,
    /// worker active = no second worker may start, engine lease = engine is out
    /// of the mutex.
    ///
    /// Monotonic id source for stream workers; zero means "no worker".
    next_stream_worker_id: Arc<AtomicU64>,
    /// Nonzero while a stream worker exists, even if it has not leased the engine
    /// yet. This prevents a second worker from starting after finalize/cancel
    /// closes the router but before the first worker has fully exited.
    active_stream_worker: Arc<AtomicU64>,
    /// Nonzero while the streaming worker has taken the engine out of `engine`.
    /// `is_model_loaded()` consults this so the model still reports "loaded"
    /// while the worker holds it.
    active_engine_lease: Arc<AtomicU64>,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(Mutex::new(None)),
            model_manager,
            app_handle: app_handle.clone(),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(AtomicU64::new(Self::now_ms())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            reload_model_on_next_use: Arc::new(AtomicBool::new(false)),
            router: Arc::new(StreamRouter::new()),
            next_stream_worker_id: Arc::new(AtomicU64::new(1)),
            active_stream_worker: Arc::new(AtomicU64::new(0)),
            active_engine_lease: Arc::new(AtomicU64::new(0)),
        };

        // Start the idle watcher
        {
            let app_handle_cloned = app_handle.clone();
            let manager_cloned = manager.clone();
            let shutdown_signal = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                debug!("Idle watcher thread started");
                while !shutdown_signal.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10)); // Check every 10 seconds

                    // Check shutdown signal again after sleep
                    if shutdown_signal.load(Ordering::Relaxed) {
                        break;
                    }

                    let settings = get_settings(&app_handle_cloned);
                    let timeout = settings.model_unload_timeout;

                    // Skip Immediately — that variant is handled by
                    // maybe_unload_immediately() after each transcription.
                    // Treating it as 0s here would unload the model mid-recording.
                    if timeout == ModelUnloadTimeout::Immediately {
                        continue;
                    }

                    // While recording, keep the idle timer fresh so the
                    // model is never unloaded mid-session.
                    let is_recording = app_handle_cloned
                        .try_state::<Arc<AudioRecordingManager>>()
                        .is_some_and(|a| a.is_recording());
                    if is_recording {
                        manager_cloned.touch_activity();
                        continue;
                    }

                    if let Some(limit_seconds) = timeout.to_seconds() {
                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        let now_ms = TranscriptionManager::now_ms();
                        let idle_ms = now_ms.saturating_sub(last);
                        let limit_ms = limit_seconds * 1000;

                        if idle_ms > limit_ms {
                            // idle -> unload
                            if manager_cloned.is_model_loaded() {
                                let unload_start = std::time::Instant::now();
                                info!(
                                    "Model idle for {}s (limit: {}s), unloading",
                                    idle_ms / 1000,
                                    limit_seconds
                                );
                                match manager_cloned.unload_model() {
                                    Ok(()) => {
                                        let unload_duration = unload_start.elapsed();
                                        info!(
                                            "Model unloaded due to inactivity (took {}ms)",
                                            unload_duration.as_millis()
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to unload idle model: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                debug!("Idle watcher thread shutting down gracefully");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    /// Lock the engine mutex, recovering from poison if a previous transcription panicked.
    fn lock_engine(&self) -> MutexGuard<'_, Option<LoadedEngine>> {
        self.engine.lock().unwrap_or_else(|poisoned| {
            warn!("Engine mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        // The engine may be leased out to the streaming worker (taken out of
        // the mutex). It's still loaded, just in use, so report true.
        self.lock_engine().is_some() || self.active_engine_lease.load(Ordering::Acquire) != 0
    }

    /// Accelerator changes should not disturb the current transcription. Mark
    /// the cached engine stale; the next model-use path reloads it with the
    /// latest settings.
    pub fn reload_model_on_next_use(&self) {
        self.reload_model_on_next_use.store(true, Ordering::Release);
    }

    /// Atomically check whether a model load is in progress and, if not, mark
    /// one as starting. Returns a [`LoadingGuard`] whose [`Drop`] impl will
    /// clear the flag and wake waiters. Returns `None` if a load is already in
    /// progress.
    pub fn try_start_loading(&self) -> Option<LoadingGuard> {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return None;
        }
        *is_loading = true;
        Some(LoadingGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        })
    }

    pub fn unload_model(&self) -> Result<()> {
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        {
            let mut engine = self.lock_engine();
            // Dropping the engine frees all resources
            *engine = None;
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        // Emit unloaded event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );

        let unload_duration = unload_start.elapsed();
        debug!(
            "Model unloaded manually (took {}ms)",
            unload_duration.as_millis()
        );
        Ok(())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Reset the idle timer to now.
    fn touch_activity(&self) {
        self.last_activity.store(Self::now_ms(), Ordering::Relaxed);
    }

    /// Unloads the model immediately if the setting is enabled and the model is
    /// loaded. [GRAIN] The rolling driver calls this ONCE at session end (its
    /// per-chunk decodes go through `transcribe_rolling_chunk`, which never
    /// unloads), so the "Immediately" policy still fires once per dictation.
    pub fn maybe_unload_immediately(&self, context: &str) {
        let settings = get_settings(&self.app_handle);
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            info!("Immediately unloading model after {}", context);
            if let Err(e) = self.unload_model() {
                warn!("Failed to immediately unload model: {}", e);
            }
        }
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        self.load_model_with_device(model_id, None)
    }

    /// Like [`load_model`](Self::load_model), but lets a caller hard-select the
    /// compute device for this one load by its `transcribe_cpp::devices()`
    /// registry index (the index shown by `--list-devices`). `None` keeps the
    /// persisted accelerator setting (which may be Auto). Only affects
    /// transcribe-cpp (whisper-family) models; the selection is not persisted.
    pub fn load_model_with_device(
        &self,
        model_id: &str,
        device_index: Option<usize>,
    ) -> Result<()> {
        apply_accelerator_settings(&self.app_handle);

        let load_start = std::time::Instant::now();
        debug!("Starting to load model: {}", model_id);

        // Emit loading started event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_started".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            let error_msg = "Model not downloaded";
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
            return Err(anyhow::anyhow!(error_msg));
        }

        let model_path = self.model_manager.get_model_path(model_id)?;

        // Drop the current engine BEFORE building the new one so transcribe-cpp
        // frees the previous native context first — avoids holding two models at
        // once (peak memory on large GGUFs). Clear the id too: if the new load
        // fails, status should read "no loaded model", not the dropped engine.
        {
            let mut engine = self.lock_engine();
            *engine = None;
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        // Create appropriate engine based on model type
        let emit_loading_failed = |error_msg: &str| {
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
        };

        let loaded_engine = match model_info.engine_type {
            EngineType::TranscribeCpp => {
                // The whisper backend is chosen at load time (transcribe-cpp has
                // no runtime global). With an explicit `device_index` (the
                // --device-index flag) hard-select that registered device;
                // otherwise re-read the persisted accelerator preference (so an
                // accelerator change marked for reload takes effect here).
                let (backend, gpu_device) = match device_index {
                    Some(index) => resolve_device_index(index).inspect_err(|e| {
                        emit_loading_failed(&e.to_string());
                    })?,
                    None => {
                        let settings = get_settings(&self.app_handle);
                        let accelerator = settings.transcribe_accelerator;
                        (
                            select_transcribe_backend(accelerator),
                            resolve_gpu_device(accelerator, settings.transcribe_gpu_device),
                        )
                    }
                };
                let model_options = ModelOptions {
                    backend,
                    gpu_device,
                };
                let model = Model::load_with(&model_path, &model_options).map_err(|e| {
                    let error_msg = format!("Failed to load whisper model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                // The bound backend may differ from the request (e.g. CPU
                // fallback under Auto); log what actually loaded.
                let bound_backend = model.backend();
                let session = model.session().map_err(|e| {
                    let error_msg = format!(
                        "Failed to create session for whisper model {}: {}",
                        model_id, e
                    );
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                // Reconcile the registry's advertised capabilities with the
                // loaded model's real ones (GGUF metadata) so badges/gating
                // reflect runtime truth, not the pre-download probe. The
                // load-completed event below triggers the frontend refresh.
                let caps = session.model().capabilities();
                self.model_manager.set_runtime_capabilities(
                    model_id,
                    caps.supports_streaming,
                    caps.supports_translate,
                    caps.supports_language_detect,
                    caps.languages.clone(),
                );
                info!(
                    "Loaded whisper model '{}' (requested {:?}, gpu_device {}, bound backend '{}', \
                     supports_streaming={}, supports_translate={}, supports_language_detect={})",
                    model_id,
                    backend,
                    gpu_device,
                    bound_backend,
                    caps.supports_streaming,
                    caps.supports_translate,
                    caps.supports_language_detect
                );
                LoadedEngine::TranscribeCpp(session)
            }
            // [GRAIN] transcribe-rs (ONNX) was removed; these engine tags can
            // only appear for a leftover on-disk ONNX model from an old build.
            // Every family ships as GGUF in the catalog — point the user there.
            other => {
                let error_msg = format!(
                    "Model '{}' uses the retired ONNX engine ({:?}); download its GGUF \
                     version from the model list instead",
                    model_id, other
                );
                emit_loading_failed(&error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        // Update the current engine and model ID
        {
            let mut engine = self.lock_engine();
            *engine = Some(loaded_engine);
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = Some(model_id.to_string());
        }

        // Reset idle timer so the watcher doesn't immediately unload a just-loaded model
        self.touch_activity();

        // Emit loading completed event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_completed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );

        let load_duration = load_start.elapsed();
        debug!(
            "Successfully loaded transcription model: {} (took {}ms)",
            model_id,
            load_duration.as_millis()
        );
        Ok(())
    }

    /// Kicks off the model loading in a background thread if it's not already loaded
    pub fn initiate_model_load(&self) {
        // [GRAIN] Delegates to the per-category variant with the persisted batch
        // selection. Grain keeps separate batch (`selected_model`) and streaming
        // (`selected_asr_model`) selections sharing this ONE engine slot, so the
        // load must also swap when a *different* category's model is resident.
        let settings = get_settings(&self.app_handle);
        self.initiate_model_load_for(settings.selected_model);
    }

    /// [GRAIN] Like [`initiate_model_load`](Self::initiate_model_load) but for an
    /// explicit model id (per-category selections: Batch/Rolling load
    /// `selected_model`, Native ASR loads `selected_asr_model`). No-op when that
    /// exact model is already resident and no reload is pending; otherwise the
    /// background load replaces whatever engine currently holds the slot.
    pub fn initiate_model_load_for(&self, model_id: String) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return;
        }

        let reload_pending = self.reload_model_on_next_use.load(Ordering::Acquire);
        if !reload_pending
            && self.is_model_loaded()
            && self.get_current_model().as_deref() == Some(model_id.as_str())
        {
            return;
        }

        *is_loading = true;
        let self_clone = self.clone();
        thread::spawn(move || {
            if reload_pending {
                self_clone
                    .reload_model_on_next_use
                    .store(false, Ordering::Release);
            }
            if let Err(e) = self_clone.load_model(&model_id) {
                error!("Failed to load model: {}", e);
            }
            let mut is_loading = self_clone.is_loading.lock().unwrap();
            *is_loading = false;
            self_clone.loading_condvar.notify_all();
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        let current_model = self.current_model_id.lock().unwrap();
        current_model.clone()
    }

    /// The compute backend the currently-loaded engine is bound to, for
    /// diagnostics (e.g. confirming `--device-index` actually bound a GPU rather
    /// than falling back to CPU/auto). Reports transcribe-cpp's real backend
    /// string; `None` when no model is loaded.
    pub fn current_backend(&self) -> Option<String> {
        match self.lock_engine().as_ref() {
            Some(LoadedEngine::TranscribeCpp(session)) => {
                Some(session.model().backend().to_string())
            }
            None => None,
        }
    }

    /// Shared handle to the stream router, used by the audio recorder to feed
    /// real-time frames without going through Tauri state on every frame.
    pub fn stream_router(&self) -> Arc<StreamRouter> {
        Arc::clone(&self.router)
    }

    /// Begin a live streaming transcription on the held engine's session.
    /// Audio frames pushed via [`StreamRouter::feed`] (captured directly by the
    /// audio recorder) are decoded incrementally and emitted to the overlay as
    /// [`StreamTextEvent`].
    ///
    /// Non-blocking: spawns a worker that waits for any in-progress model load,
    /// verifies the model supports streaming, then begins the stream. If the
    /// model can't stream, the worker idles until finalize/cancel and reports
    /// `None` so the caller falls back to batch transcription. Frames sent
    /// before the stream begins queue on the channel and are not lost.
    pub fn start_stream(&self) {
        if self.router.is_open() || self.active_stream_worker.load(Ordering::Acquire) != 0 {
            warn!("start_stream called while a stream worker is already active");
            return;
        }
        let worker_id = self.next_stream_worker_id.fetch_add(1, Ordering::Relaxed);
        if self
            .active_stream_worker
            .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("start_stream lost a race with another stream worker");
            return;
        }
        let rx = self.router.open();

        let manager = self.clone();
        thread::spawn(move || manager.run_stream_worker(rx, worker_id));
    }

    fn run_stream_worker(&self, rx: mpsc::Receiver<StreamCmd>, worker_id: u64) {
        let _worker = StreamWorkerGuard {
            worker_id,
            active_stream_worker: Arc::clone(&self.active_stream_worker),
            active_engine_lease: Arc::clone(&self.active_engine_lease),
        };

        // Wait for any in-progress model load to finish (start_stream races the
        // background load kicked off when recording starts).
        {
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }
        }

        let model_id = self.get_current_model().unwrap_or_default();

        // Take the engine out of the mutex so we own it during streaming,
        // structurally excluding any concurrent batch transcription (which
        // transcribe-cpp's compute_lock would refuse anyway). Returned when the
        // worker exits, or dropped if the model was switched/unloaded mid-stream.
        if self
            .active_engine_lease
            .compare_exchange(0, worker_id, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            warn!("Live preview: another worker already holds the transcription engine");
            self.router.clear();
            drain_until_finalize(rx);
            return;
        }
        let mut engine = match self.lock_engine().take() {
            Some(e) => e,
            None => {
                info!(
                    "Live preview: model '{}' was unloaded before streaming could begin; \
                     falling back to batch transcription",
                    model_id
                );
                let _ = self.active_engine_lease.compare_exchange(
                    worker_id,
                    0,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                self.router.clear();
                drain_until_finalize(rx);
                return;
            }
        };

        // The loaded session (not the ModelManager copy) is the source of
        // truth for run-path capabilities.
        let (supports_streaming, supports_translate, languages) = {
            let LoadedEngine::TranscribeCpp(session) = &engine;
            let model = session.model();
            let caps = model.capabilities();
            info!(
                "Live preview: model '{}' arch='{}' variant='{}' supports_streaming={} \
                 supports_translate={} languages={:?}",
                model_id,
                model.arch(),
                model.variant(),
                caps.supports_streaming,
                caps.supports_translate,
                caps.languages,
            );
            (
                caps.supports_streaming,
                caps.supports_translate,
                caps.languages,
            )
        };

        if !supports_streaming {
            self.return_engine(engine, &model_id);
            self.router.clear();
            drain_until_finalize(rx);
            return;
        }

        // Build run options mirroring the offline transcribe-cpp path: task +
        // language gated against what the model actually advertises.
        let settings = get_settings(&self.app_handle);
        let effective_language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &model_id);
        let run_plan = transcribe_cpp_run_plan(
            settings.translate_to_english,
            &effective_language,
            &languages,
            supports_translate,
        );
        let run_options = RunOptions {
            task: run_plan.task,
            language: run_plan.language,
            target_language: run_plan.target_language,
            ..Default::default()
        };

        // Run the stream on the held session. The Stream borrows the session
        // (and thus the engine) for its lifetime, so the feed/finalize loop
        // lives in a labeled block — when it exits, the borrow is released and
        // the engine can be moved into return_engine().
        let mut finalize_reply: Option<mpsc::Sender<Option<String>>> = None;
        let mut finalize_result: Option<Option<String>> = None;
        let stream_started = 'stream: {
            let LoadedEngine::TranscribeCpp(session) = &mut engine;

            // Read the backend string before beginning the stream — the
            // `Stream` borrows `session` mutably for its lifetime, so we can't
            // call `session.model()` once it exists.
            let backend = session.model().backend();

            // StreamOptions::default() uses CommitPolicy::Auto and lets the
            // family pick its own streaming strategy (no family-specific ext).
            let mut stream = match session.stream(&run_options, &StreamOptions::default()) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to begin stream: {}", e);
                    break 'stream false;
                }
            };

            self.touch_activity();
            info!(
                "Live streaming transcription started (model '{}', backend '{}')",
                model_id, backend
            );

            let mut perf = StreamPerf::new();
            // [GRAIN] Read the "scrap that" toggle once per stream (not per emit).
            // When on, the live committed/tentative preview is scrubbed past the
            // last reset phrase so the Studio pill restarts + collapses; the final
            // text is scrubbed again independently in `finalize_stream`.
            let scrap_that = get_settings(&self.app_handle).scrap_that_enabled;
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    StreamCmd::Feed(pcm) => {
                        self.touch_activity();
                        perf.record_feed(pcm.len());
                        let feed_start = Instant::now();
                        match stream.feed(&pcm) {
                            Ok(update) => {
                                perf.record_compute(feed_start.elapsed());
                                perf.record_update(
                                    update.revision,
                                    update.input_received_ms,
                                    update.audio_committed_ms,
                                    update.buffered_ms,
                                );
                                if update.committed_changed || update.tentative_changed {
                                    let text = stream.text();
                                    perf.record_emit();
                                    let (committed, tentative): (String, String) = if scrap_that {
                                        crate::audio_toolkit::scrub_stream_preview(
                                            &text.committed,
                                            &text.tentative,
                                        )
                                    } else {
                                        (text.committed.to_string(), text.tentative.to_string())
                                    };
                                    self.emit_stream_text(&committed, &tentative);
                                }
                                perf.maybe_log();
                            }
                            Err(e) => {
                                perf.record_compute(feed_start.elapsed());
                                warn!("stream feed failed: {}", e);
                            }
                        }
                    }
                    StreamCmd::Finalize(reply) => {
                        let finalize_start = Instant::now();
                        let result = match stream.finalize() {
                            // After finalize the committed prefix holds the full
                            // text; display() = committed + tentative is the safe read.
                            Ok(update) => {
                                perf.record_compute(finalize_start.elapsed());
                                perf.record_update(
                                    update.revision,
                                    update.input_received_ms,
                                    update.audio_committed_ms,
                                    update.buffered_ms,
                                );
                                Some(stream.text().display())
                            }
                            Err(e) => {
                                perf.record_compute(finalize_start.elapsed());
                                error!(
                                    "stream finalize failed: {}; falling back to batch transcription",
                                    e
                                );
                                None
                            }
                        };
                        let chars = match &result {
                            Some(text) => text.len(),
                            _ => 0,
                        };
                        perf.log_finalized(chars);
                        finalize_reply = Some(reply);
                        finalize_result = Some(result);
                        break;
                    }
                    StreamCmd::Cancel => {
                        stream.reset();
                        break;
                    }
                }
            }

            true
        };
        // `stream` + the `&mut engine` borrow are released here.

        if !stream_started {
            // Stream never began (model doesn't support streaming or begin
            // failed); drain so the finalize handshake still completes and the
            // caller falls back to batch transcription. Return the engine first
            // so the fallback can immediately use it.
            self.return_engine(engine, &model_id);
            drain_until_finalize(rx);
            return;
        }

        self.return_engine(engine, &model_id);
        if let (Some(reply), Some(result)) = (finalize_reply, finalize_result) {
            let _ = reply.send(result);
        }
        // `_worker` drops here, clearing this worker's active/lease flags after
        // the engine has been returned to the pool.
    }

    /// Return the leased engine to the mutex, unless the model was switched or
    /// unloaded during transcription (in which case the stale engine is dropped).
    fn return_engine(&self, engine: LoadedEngine, expected_model_id: &str) {
        let still_current =
            self.current_model_id.lock().unwrap().as_deref() == Some(expected_model_id);
        if still_current {
            *self.lock_engine() = Some(engine);
        } else {
            info!(
                "Model changed/unloaded during transcription; dropping stale engine (was '{}')",
                expected_model_id
            );
            // `engine` drops here, freeing its resources.
        }
    }

    /// Flush the active stream and return its final, post-filtered text.
    ///
    /// `Ok(None)` means no usable stream was active and the caller may fall back
    /// to batch transcription. `Err` means finalize itself failed or timed out.
    /// A timeout may still leave the worker holding the engine, so callers
    /// should surface it instead of immediately starting a batch fallback.
    pub fn finalize_stream(&self) -> Result<Option<String>> {
        let Some(tx) = self.router.take() else {
            return Ok(None);
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(StreamCmd::Finalize(reply_tx)).is_err() {
            return Ok(None);
        }
        let raw = match reply_rx.recv_timeout(STREAM_FINALIZE_REPLY_TIMEOUT) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(anyhow::anyhow!(
                    "Timed out waiting {:?} for live transcription to finalize",
                    STREAM_FINALIZE_REPLY_TIMEOUT
                ));
            }
        };

        let settings = get_settings(&self.app_handle);
        // Streaming models do not receive a decode prompt, so custom words
        // always go through the shared fuzzy post-correction path.
        let filtered = post_process_transcription_text(raw, &settings, false);

        self.maybe_unload_immediately("streaming transcription");
        Ok(Some(filtered))
    }

    /// Abandon any active stream without producing text (e.g. on cancel).
    pub fn cancel_stream(&self) {
        if let Some(tx) = self.router.take() {
            let _ = tx.send(StreamCmd::Cancel);
        }
    }

    fn emit_stream_text(&self, committed: &str, tentative: &str) {
        let _ = StreamTextEvent {
            committed: committed.to_string(),
            tentative: tentative.to_string(),
        }
        .emit(&self.app_handle);
        // [GRAIN] Mirror the live snapshot to the native pill's Studio Window
        // over the WS event bus. Both parts are cumulative snapshots (SET, not
        // append): `committed` is the stable prefix, `tentative` the volatile
        // tail — the pill needs the tail so the preview keeps moving while the
        // engine's auto-commit is between commit points.
        crate::bridge::emit(
            &self.app_handle,
            grain_core::DaemonEvent::AsrStreamText {
                session_id: crate::actions::current_session_id(),
                committed: committed.to_string(),
                tentative: tentative.to_string(),
            },
        );
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        #[cfg(debug_assertions)]
        if std::env::var("HANDY_FORCE_TRANSCRIPTION_FAILURE").is_ok() {
            return Err(anyhow::anyhow!(
                "Simulated transcription failure (HANDY_FORCE_TRANSCRIPTION_FAILURE)"
            ));
        }

        // Update last activity timestamp
        self.touch_activity();

        let st = std::time::Instant::now();
        let audio_len = audio.len();

        debug!("Audio vector length: {}", audio_len);

        if audio.is_empty() {
            debug!("Empty audio vector");
            self.maybe_unload_immediately("empty audio");
            return Ok(String::new());
        }

        // Check if model is loaded, if not try to load it
        {
            // If the model is loading, wait for it to complete.
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }

            let engine_guard = self.lock_engine();
            if engine_guard.is_none() {
                return Err(anyhow::anyhow!("Model is not loaded for transcription."));
            }
        }

        // Get current settings for configuration
        let settings = get_settings(&self.app_handle);

        // Validate selected language against the model's supported languages.
        // If the language isn't supported, fall back to "auto" to prevent errors.
        // Validate against the model that's actually loaded (which can differ
        // from settings.selected_model when a caller loaded a specific model —
        // e.g. the --transcribe-file path's --model), not the persisted
        // selection.
        let active_model = self
            .get_current_model()
            .unwrap_or_else(|| settings.selected_model.clone());
        // Resolve the persisted language *intent* into the language this model
        // will actually use. The coercion is capability-aware (a must-pick model
        // never receives "auto") and computed fresh here — it is never written
        // back to settings, so the intent survives switching models and back.
        let validated_language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &active_model);
        if validated_language != settings.selected_language {
            debug!(
                "Language intent '{}' resolved to '{}' for model '{}'",
                settings.selected_language, validated_language, active_model
            );
        }

        // Run the decode on the shared engine with crash isolation. The closure
        // probes the loaded session's real capabilities (source of truth, not
        // the ModelManager copy), builds the run plan, and returns both the text
        // and whether the model took the custom-word prompt (gates the fuzzy
        // post-correction below).
        let (result, model_takes_initial_prompt) =
            self.with_engine_session(&active_model, |session| {
                let model = session.model();
                let caps = model.capabilities();
                let takes_prompt = model.supports(Feature::InitialPrompt);
                let model_supports_translate = caps.supports_translate;
                let model_languages = caps.languages;
                debug!(
                    "transcribe-cpp model '{}' on '{}': initial_prompt={}, translate={}, languages={:?}",
                    settings.selected_model,
                    model.backend(),
                    takes_prompt,
                    model_supports_translate,
                    model_languages
                );

                // Custom words become the initial prompt ONLY for whisper-family
                // models. Non-whisper archs (e.g. Voxtral Small 24B) can advertise
                // Feature::InitialPrompt but still reject the WhisperRunOptions
                // extension with INVALID_ARG (see upstream #1601/#1603). Gate on
                // the arch string, not the feature flag, so they fall through to
                // the fuzzy post-correction path instead.
                let model_is_whisper = model.arch() == "whisper";
                let family = if settings.custom_words.is_empty() || !model_is_whisper {
                    None
                } else {
                    Some(RunExtension::Whisper(WhisperRunOptions {
                        initial_prompt: Some(settings.custom_words.join(", ")),
                        ..Default::default()
                    }))
                };

                let run_plan = transcribe_cpp_run_plan(
                    settings.translate_to_english,
                    &validated_language,
                    &model_languages,
                    model_supports_translate,
                );

                // Timestamps come from the `TimestampKind::Auto` default
                // (upstream #1602): the crate resolves the richest supported
                // granularity per family, which keeps whisper's long-form
                // (>30s) decode stable with an initial prompt — the repetition
                // loop the old explicit Segment/None selection guarded against.
                let run_options = RunOptions {
                    task: run_plan.task,
                    language: run_plan.language,
                    target_language: run_plan.target_language,
                    family,
                    ..Default::default()
                };

                debug!(
                    "transcribe-cpp run: task={:?}, language={:?}, initial_prompt={}",
                    run_options.task,
                    run_options.language,
                    run_options.family.is_some()
                );

                session
                    .run(&audio, &run_options)
                    .map(|t| (t.text, takes_prompt))
                    .map_err(|e| anyhow::anyhow!("transcribe-cpp transcription failed: {}", e))
            })?;

        // Apply fuzzy word correction if custom words are configured — UNLESS the
        // words were already handed to the model as an initial prompt (whisper
        // family). Non-whisper transcribe-cpp models can't take a prompt, so they
        // still get fuzzy correction here.
        let filtered_result =
            post_process_transcription_text(result, &settings, model_takes_initial_prompt);

        let et = std::time::Instant::now();
        let translation_note = if settings.translate_to_english {
            " (translated)"
        } else {
            ""
        };
        // Real-time factor. Input PCM is 16 kHz mono, so audio length in seconds
        // is samples / 16000. `speedup` is audio_secs / elapsed_secs — e.g. 4.00x
        // means transcribed 4x faster than real time
        let elapsed_secs = (et - st).as_secs_f64();
        let audio_secs = audio_len as f64 / 16_000.0;
        let speedup = real_time_factor(audio_secs, elapsed_secs);
        info!(
            "Transcription completed in {:.2}s for {:.2}s of audio ({:.2}x real-time){}",
            elapsed_secs, audio_secs, speedup, translation_note
        );

        let final_result = filtered_result;

        if final_result.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription result: {}", final_result);
        }

        self.maybe_unload_immediately("transcription");

        Ok(final_result)
    }

    /// [GRAIN] Run one decode on the shared engine with crash isolation. Takes
    /// the engine out of the mutex (so no lock is held during the native call),
    /// runs `f` on its session inside `catch_unwind`, and either returns the
    /// engine to the pool (success / normal error) or drops it and clears the
    /// model id (panic — the engine is left in an unknown state). Shared by the
    /// batch `transcribe` path and the rolling chunk path so both get identical
    /// crash handling. Caller must have already waited for any in-flight load.
    fn with_engine_session<T>(
        &self,
        active_model: &str,
        f: impl FnOnce(&mut Session) -> Result<T>,
    ) -> Result<T> {
        // Wait for any in-flight model load before taking the engine.
        {
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }
        }

        // Take the engine out so we own it during the native call. On a panic we
        // simply don't put it back (effectively unloading it) instead of
        // poisoning the mutex.
        let mut engine = match self.lock_engine().take() {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!(
                    "Model is not loaded for transcription. Please check your model settings."
                ));
            }
        };

        let outcome = {
            let LoadedEngine::TranscribeCpp(session) = &mut engine;
            catch_unwind(AssertUnwindSafe(|| f(session)))
        };

        match outcome {
            Ok(inner) => {
                // Return the engine unless a model switch/unload invalidated it
                // while it was in use.
                self.return_engine(engine, active_model);
                inner
            }
            Err(panic_payload) => {
                // Engine panicked — do NOT put it back (dropped here = unloaded).
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                error!(
                    "Transcription engine panicked: {}. Model has been unloaded.",
                    panic_msg
                );
                {
                    let mut current_model = self
                        .current_model_id
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    *current_model = None;
                }
                let _ = self.app_handle.emit(
                    "model-state-changed",
                    ModelStateEvent {
                        event_type: "unloaded".to_string(),
                        model_id: None,
                        model_name: None,
                        error: Some(format!("Engine panicked: {}", panic_msg)),
                    },
                );
                Err(anyhow::anyhow!(
                    "Transcription engine panicked: {}. The model has been unloaded and will reload on next attempt.",
                    panic_msg
                ))
            }
        }
    }

    /// [GRAIN] Decode one rolling-window chunk and return the FULL transcript
    /// (text + word timings), NOT post-processed. The rolling driver owns the
    /// dedup/assembly and applies custom-word + filler correction ONCE on the
    /// assembled transcript at session end, so this path deliberately skips
    /// per-chunk post-processing AND the idle/immediate unload (the session is
    /// still live). Word-level timestamps are requested so the timeline
    /// assembler can dedup overlaps by position; `context_prompt` (the committed
    /// tail) conditions whisper-family models across the chunk seam.
    pub fn transcribe_rolling_chunk(
        &self,
        audio: &[f32],
        context_prompt: Option<&str>,
    ) -> Result<Transcript> {
        if audio.is_empty() {
            return Ok(Transcript::default());
        }
        self.touch_activity();

        let settings = get_settings(&self.app_handle);
        let active_model = self
            .get_current_model()
            .unwrap_or_else(|| settings.selected_model.clone());
        let validated_language =
            effective_language_for_model(&settings, self.model_manager.as_ref(), &active_model);

        self.with_engine_session(&active_model, |session| {
            let model = session.model();
            let caps = model.capabilities();
            let takes_prompt = model.supports(Feature::InitialPrompt);
            let model_supports_translate = caps.supports_translate;
            let model_languages = caps.languages;

            // Whisper-family models accept a decode prompt; feed custom words +
            // the committed tail so spelling/casing stay consistent across the
            // seam. Non-whisper arches reject the extension, so skip it there.
            let family = if !takes_prompt {
                None
            } else {
                let mut prompt = settings.custom_words.join(", ");
                if let Some(ctx) = context_prompt.filter(|c| !c.trim().is_empty()) {
                    if !prompt.is_empty() {
                        prompt.push(' ');
                    }
                    prompt.push_str(ctx.trim());
                }
                if prompt.is_empty() {
                    None
                } else {
                    Some(RunExtension::Whisper(WhisperRunOptions {
                        initial_prompt: Some(prompt),
                        ..Default::default()
                    }))
                }
            };

            let run_plan = transcribe_cpp_run_plan(
                settings.translate_to_english,
                &validated_language,
                &model_languages,
                model_supports_translate,
            );

            // Word timestamps give the timeline assembler the best positional
            // dedup. But not every backend/model supports word-level granularity
            // — some (seen in release builds) reject it outright with
            // "unsupported timestamp granularity". Rather than dropping the chunk,
            // retry with the SAME granularity the (working) batch path uses and
            // let `synthesize_word_timings` fill in evenly-spaced timings; rolling
            // still assembles, just with coarser word positions.
            let mut run_options = RunOptions {
                task: run_plan.task,
                language: run_plan.language,
                target_language: run_plan.target_language,
                timestamps: TimestampKind::Word,
                family,
                ..Default::default()
            };

            match session.run(audio, &run_options) {
                Ok(t) => Ok(t),
                Err(_) => {
                    run_options.timestamps = if takes_prompt {
                        TimestampKind::Segment
                    } else {
                        TimestampKind::None
                    };
                    session
                        .run(audio, &run_options)
                        .map_err(|e| anyhow::anyhow!("rolling chunk transcription failed: {}", e))
                }
            }
        })
    }
}

struct StreamPerf {
    feed_count: u64,
    emit_count: u64,
    streamed_samples: u64,
    stream_compute_elapsed: Duration,
    last_log: Instant,
    latest_revision: i32,
    latest_input_received_ms: i64,
    latest_audio_committed_ms: i64,
    latest_buffered_ms: i64,
}

impl StreamPerf {
    fn new() -> Self {
        Self {
            feed_count: 0,
            emit_count: 0,
            streamed_samples: 0,
            stream_compute_elapsed: Duration::ZERO,
            last_log: Instant::now(),
            latest_revision: 0,
            latest_input_received_ms: 0,
            latest_audio_committed_ms: 0,
            latest_buffered_ms: 0,
        }
    }

    fn record_feed(&mut self, samples: usize) {
        self.feed_count += 1;
        self.streamed_samples += samples as u64;
    }

    fn record_compute(&mut self, elapsed: Duration) {
        self.stream_compute_elapsed += elapsed;
    }

    fn record_update(
        &mut self,
        revision: i32,
        input_received_ms: i64,
        audio_committed_ms: i64,
        buffered_ms: i64,
    ) {
        self.latest_revision = revision;
        self.latest_input_received_ms = input_received_ms;
        self.latest_audio_committed_ms = audio_committed_ms;
        self.latest_buffered_ms = buffered_ms;
    }

    fn record_emit(&mut self) {
        self.emit_count += 1;
    }

    fn maybe_log(&mut self) {
        if self.last_log.elapsed() < STREAM_PERF_LOG_INTERVAL {
            return;
        }

        let audio_secs = self.audio_secs();
        let compute_secs = self.compute_secs();
        debug!(
            "Live preview perf: {:.2}s streamed audio, {:.2}s model compute ({:.2}x real-time), \
             input_received={:.2}s, committed_audio={:.2}s, buffered={}ms, revision={}, \
             {} frames fed, {} updates emitted",
            audio_secs,
            compute_secs,
            real_time_factor(audio_secs, compute_secs),
            self.latest_input_received_ms as f64 / 1000.0,
            self.latest_audio_committed_ms as f64 / 1000.0,
            self.latest_buffered_ms,
            self.latest_revision,
            self.feed_count,
            self.emit_count,
        );
        self.last_log = Instant::now();
    }

    fn log_finalized(&self, chars: usize) {
        let audio_secs = self.audio_secs();
        let compute_secs = self.compute_secs();
        info!(
            "Live preview finalized in {:.2}s model compute for {:.2}s streamed audio ({:.2}x real-time): \
             input_received={:.2}s, committed_audio={:.2}s, buffered={}ms, revision={}, \
             {} frames fed, {} updates emitted, {} chars",
            compute_secs,
            audio_secs,
            real_time_factor(audio_secs, compute_secs),
            self.latest_input_received_ms as f64 / 1000.0,
            self.latest_audio_committed_ms as f64 / 1000.0,
            self.latest_buffered_ms,
            self.latest_revision,
            self.feed_count,
            self.emit_count,
            chars
        );
    }

    fn audio_secs(&self) -> f64 {
        self.streamed_samples as f64 / 16_000.0
    }

    fn compute_secs(&self) -> f64 {
        self.stream_compute_elapsed.as_secs_f64()
    }
}

fn real_time_factor(audio_secs: f64, compute_secs: f64) -> f64 {
    if compute_secs > 0.0 {
        audio_secs / compute_secs
    } else {
        0.0
    }
}

fn normalize_cjk_language(language: &str) -> &str {
    match language {
        "zh-Hans" | "zh-Hant" => "zh",
        other => other,
    }
}

/// Resolve the persisted language intent into the language a specific model can
/// use without writing the coerced value back to settings.
fn effective_language_for_model(
    settings: &AppSettings,
    model_manager: &ModelManager,
    model_id: &str,
) -> String {
    match model_manager.get_model_info(model_id) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

struct TranscribeCppRunPlan {
    task: Task,
    language: Option<String>,
    target_language: Option<String>,
}

/// Build the transcribe-cpp language/task options shared by batch and live
/// streaming paths.
fn transcribe_cpp_run_plan(
    translate_to_english: bool,
    effective_language: &str,
    model_languages: &[String],
    model_supports_translate: bool,
) -> TranscribeCppRunPlan {
    let requested_language = match effective_language {
        "auto" => None,
        other => Some(normalize_cjk_language(other).to_string()),
    };
    // Only pass a language the loaded model actually advertises (per
    // capabilities().languages); otherwise auto-detect rather than failing with
    // UNSUPPORTED_LANGUAGE. Language-agnostic models report an empty list, so
    // they always stay on auto.
    let language = requested_language.filter(|lang| model_languages.iter().any(|l| l == lang));
    let (task, target_language) = cpp_translation_task(
        translate_to_english,
        model_supports_translate,
        language.as_deref(),
    );

    TranscribeCppRunPlan {
        task,
        language,
        target_language,
    }
}

fn post_process_transcription_text(
    raw: String,
    settings: &AppSettings,
    custom_words_already_prompted: bool,
) -> String {
    // [GRAIN] "Scrap that" runs FIRST, on the raw transcript: anything spoken
    // before the last reset phrase is discarded so the rest of the pipeline
    // (custom words / fillers / snippets) only sees the kept remainder.
    let raw = if settings.scrap_that_enabled {
        crate::audio_toolkit::strip_scrapped(&raw)
    } else {
        raw
    };

    let corrected = if !settings.custom_words.is_empty() && !custom_words_already_prompted {
        apply_custom_words(
            &raw,
            &settings.custom_words,
            settings.word_correction_threshold,
        )
    } else {
        raw
    };

    let filtered = filter_transcription_output(
        &corrected,
        &settings.app_language,
        &settings.custom_filler_words,
    );

    // [GRAIN] Voice snippets run LAST, on the corrected/filtered full
    // transcript — this covers the local batch and stream-finalize paths
    // (rolling + cloud STT expand via `finalize_transcript`).
    crate::audio_toolkit::apply_snippets(&filtered, &settings.snippets)
}

/// Decide a transcribe-cpp run's task + translation target from settings.
///
/// "Translate to English" only fires where the model advertises translation.
/// Unlike transcribe-rs (which forces the target to English itself when its
/// `translate` flag is set), transcribe-cpp requires an explicit
/// `target_language`: a null target defaults to the *source*, so a non-English
/// source silently becomes e.g. es→es and Canary rejects the unadvertised pair.
/// An English source is skipped entirely — en→en is not a real translation, and
/// it's reachable by default since auto-detect-less models coerce intent to "en".
///
/// Returns `(task, target_language)` ready to drop into `RunOptions`.
fn cpp_translation_task(
    translate_to_english: bool,
    model_supports_translate: bool,
    source_language: Option<&str>,
) -> (Task, Option<String>) {
    let translate_to_en =
        translate_to_english && model_supports_translate && source_language != Some("en");
    if translate_to_en {
        (Task::Translate, Some("en".to_string()))
    } else {
        (Task::Transcribe, None)
    }
}

/// Drain a stream command channel, ignoring fed audio, until the caller
/// finalizes or cancels. Used when streaming can't actually run (model not
/// loaded / not streaming-capable) so the finalize handshake still completes
/// and the caller falls back to batch transcription.
fn drain_until_finalize(rx: mpsc::Receiver<StreamCmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            StreamCmd::Feed(_) => {}
            StreamCmd::Finalize(reply) => {
                let _ = reply.send(None);
                break;
            }
            StreamCmd::Cancel => break,
        }
    }
}

/// Initialize the transcribe-cpp native backend once at startup: route native +
/// ggml diagnostics into the `log` facade and register compute backend modules.
/// In a static build (macOS Metal) `init_backends_default` is a harmless no-op;
/// in a `dynamic-backends` build it loads the per-ISA CPU / GPU modules. Must run
/// before the first model load.
pub fn init_transcribe_backend() {
    transcribe_cpp::init_logging();
    match transcribe_cpp::init_backends_default() {
        Ok(()) => {
            let devices = transcribe_cpp::devices();
            info!(
                "transcribe-cpp initialized with {} compute device(s): [{}]",
                devices.len(),
                devices
                    .iter()
                    .map(|d| format!("{} ({})", d.name, d.kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Err(e) => warn!("Failed to initialize transcribe-cpp backends: {}", e),
    }
}

/// Human-readable list of the transcribe-cpp compute devices registered at
/// startup, for the `--list-devices` flag. The reported `index` is the
/// value to pass to `--device-index`. Backends must be initialized first
/// (see [`init_transcribe_backend`]).
pub fn describe_compute_devices() -> Vec<String> {
    transcribe_cpp::devices()
        .into_iter()
        .map(|d| {
            let idx = d
                .index
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".to_string());
            let name = if d.description.is_empty() {
                d.name
            } else {
                d.description
            };
            let vram_mb = d.memory_total / (1024 * 1024);
            format!(
                "index={} kind={} name={} vram={}MB",
                idx, d.kind, name, vram_mb
            )
        })
        .collect()
}

/// Resolve a `--list-devices` registry index to the (backend, gpu_device) pair
/// for a transcribe-cpp model load (the `--device-index` flag). The
/// backend is set explicitly from the device's kind, so there's no "index 0 =
/// auto" ambiguity. Errors if the index isn't a registered, loadable device.
fn resolve_device_index(index: usize) -> Result<(Backend, i32)> {
    let device = transcribe_cpp::devices()
        .into_iter()
        .find(|d| d.index == Some(index))
        .ok_or_else(|| {
            anyhow::anyhow!("No compute device with index {index} (see --list-devices)")
        })?;
    let backend = match device.kind.as_str() {
        "cpu" => Backend::Cpu,
        "metal" => Backend::Metal,
        "cuda" => Backend::Cuda,
        "vulkan" => Backend::Vulkan,
        other => {
            return Err(anyhow::anyhow!(
                "Device index {index} has kind '{other}', which cannot host a model"
            ))
        }
    };
    // gpu_device is a registry index used only by GPU backends; CPU ignores it.
    let gpu_device = if matches!(backend, Backend::Cpu) {
        0
    } else {
        index as i32
    };
    Ok((backend, gpu_device))
}

/// Map Handy's whisper accelerator setting to a transcribe-cpp [`Backend`].
///
/// `Auto` lets the library pick the best device (with CPU fallback). `Cpu` forces
/// strict CPU. `Gpu` requests the platform GPU backend, but only if a device for
/// it is actually registered — otherwise it falls back to `Auto` so the load
/// never fails outright on a machine without that GPU backend.
fn select_transcribe_backend(setting: TranscribeAcceleratorSetting) -> Backend {
    match setting {
        TranscribeAcceleratorSetting::Cpu => Backend::Cpu,
        TranscribeAcceleratorSetting::Auto => Backend::Auto,
        TranscribeAcceleratorSetting::Gpu => {
            #[cfg(target_os = "macos")]
            let candidates = [Backend::Metal];
            #[cfg(not(target_os = "macos"))]
            let candidates = [Backend::Cuda, Backend::Vulkan];

            match candidates
                .into_iter()
                .find(|&b| transcribe_cpp::backend_available(b))
            {
                Some(b) => b,
                None => {
                    warn!("No GPU backend available for transcribe.cpp; falling back to Auto");
                    Backend::Auto
                }
            }
        }
    }
}

/// Resolve the user's stored GPU device choice into a [`ModelOptions::gpu_device`]
/// registry index for the next model load.
///
/// Settings store a registry index into [`transcribe_cpp::devices`] (`-1` is the
/// UI's auto/CPU sentinel); transcribe-cpp treats `0` as "auto / first match" and
/// rejects an out-of-range or non-GPU index. So an explicit selection is honored
/// only when the user chose the GPU accelerator and the stored index still
/// resolves to a registered GPU device — otherwise fall back to `0` so a stale
/// selection can never fail the load.
fn resolve_gpu_device(setting: TranscribeAcceleratorSetting, gpu_device: i32) -> i32 {
    if setting != TranscribeAcceleratorSetting::Gpu || gpu_device <= 0 {
        return 0;
    }
    let still_valid = transcribe_cpp::devices()
        .iter()
        .any(|d| d.index == Some(gpu_device as usize) && d.kind != "cpu" && d.kind != "accel");
    if still_valid {
        gpu_device
    } else {
        warn!(
            "Stored transcribe GPU device index {} is no longer available; using auto",
            gpu_device
        );
        0
    }
}

/// Log the user's accelerator preference on startup / before loading a model.
///
/// The transcribe.cpp backend is not set here: it is chosen at model-load time
/// from [`select_transcribe_backend`], so changing the accelerator only needs a
/// model reload (see `reload_model_on_next_use`). [GRAIN] The old ORT
/// (transcribe-rs) global is gone with the ONNX engine removal.
pub fn apply_accelerator_settings(app: &tauri::AppHandle) {
    let settings = get_settings(app);

    info!(
        "transcribe.cpp accelerator preference: {:?} (applied on next model load)",
        settings.transcribe_accelerator
    );
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub id: i32,
    pub name: String,
    pub total_vram_mb: usize,
}

static GPU_DEVICES: OnceLock<Vec<GpuDeviceOption>> = OnceLock::new();

fn cached_gpu_devices() -> &'static [GpuDeviceOption] {
    // GPU compute devices transcribe-cpp registered at startup. `id` is the
    // device's registry index (`Device::index`, not a re-counted position) so it
    // feeds straight back as `ModelOptions::gpu_device` (see `resolve_gpu_device`).
    // `total_vram_mb` is the backend-reported capacity, 0 when unreported (some
    // Metal/Vulkan drivers).
    GPU_DEVICES.get_or_init(|| {
        transcribe_cpp::devices()
            .into_iter()
            .filter(|d| d.kind != "cpu" && d.kind != "accel")
            .map(|d| GpuDeviceOption {
                id: d.index.unwrap_or(0) as i32,
                name: if d.description.is_empty() {
                    d.name
                } else {
                    d.description
                },
                total_vram_mb: (d.memory_total / (1024 * 1024)) as usize,
            })
            .collect()
    })
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct AvailableAccelerators {
    pub transcribe: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceOption>,
}

/// Return which accelerators are compiled into this build. [GRAIN] The ORT
/// (transcribe-rs) accelerator list is gone with the ONNX engine removal.
pub fn get_available_accelerators() -> AvailableAccelerators {
    let transcribe_options = vec!["auto".to_string(), "cpu".to_string(), "gpu".to_string()];

    AvailableAccelerators {
        transcribe: transcribe_options,
        gpu_devices: cached_gpu_devices().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn languages(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_string()).collect()
    }

    #[test]
    fn transcribe_cpp_run_plan_maps_chinese_variants() {
        let plan = transcribe_cpp_run_plan(false, "zh-Hant", &languages(&["zh"]), true);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("zh"));
        assert_eq!(plan.target_language, None);
    }

    #[test]
    fn transcribe_cpp_run_plan_skips_english_translation() {
        let plan = transcribe_cpp_run_plan(true, "en", &languages(&["en", "es"]), true);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("en"));
        assert_eq!(plan.target_language, None);
    }

    #[test]
    fn transcribe_cpp_run_plan_translates_supported_non_english() {
        let plan = transcribe_cpp_run_plan(true, "es", &languages(&["en", "es"]), true);

        assert!(matches!(plan.task, Task::Translate));
        assert_eq!(plan.language.as_deref(), Some("es"));
        assert_eq!(plan.target_language.as_deref(), Some("en"));
    }

    #[test]
    fn transcribe_cpp_run_plan_requires_model_translation_support() {
        let plan = transcribe_cpp_run_plan(true, "es", &languages(&["en", "es"]), false);

        assert!(matches!(plan.task, Task::Transcribe));
        assert_eq!(plan.language.as_deref(), Some("es"));
        assert_eq!(plan.target_language, None);
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        // Skip shutdown unless this is the very last clone. TranscriptionManager
        // is cloned by initiate_model_load() and the watcher thread — those
        // clones dropping must not kill the watcher. The watcher thread holds
        // its own clone, so engine's strong_count is always >= 2 while the
        // watcher is alive. When it reaches 1, only this instance remains
        // and we can safely shut down.
        if Arc::strong_count(&self.engine) > 1 {
            return;
        }

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Wait for the thread to finish gracefully.
        // Use match instead of unwrap to avoid panicking if the mutex is
        // poisoned — a panic inside Drop calls abort().
        let mut guard = match self.watcher_handle.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("Recovered poisoned watcher_handle mutex during TranscriptionManager drop — a panic occurred earlier this session");
                e.into_inner()
            }
        };
        if let Some(handle) = guard.take() {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}
