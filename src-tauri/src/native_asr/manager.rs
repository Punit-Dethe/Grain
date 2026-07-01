//! [GRAIN] Native ASR session manager (transcribe-cpp).
//!
//! Owns the single live streaming session: it spawns the worker thread that runs
//! [`super::worker::drive_stream`], feeding it frames from the [`NativeAsrInput`]
//! fan-out and emitting the committed transcript onto the core's `DaemonEvent`
//! bus (so the pill renders it exactly like before). The model is a GGUF file on
//! disk; `start` takes its path + an optional language hint.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tauri::AppHandle;

use super::input::NativeAsrInput;
use super::worker::{drive_stream, FrameCmd};

struct Running {
    session_id: u64,
    stop: Arc<AtomicBool>,
    /// The worker returns the assembled final transcript (or `None` on error).
    handle: JoinHandle<Option<String>>,
}

/// Coordinates one live Native ASR session at a time.
pub struct NativeAsrManager {
    app: AppHandle,
    input: Arc<NativeAsrInput>,
    running: Mutex<Option<Running>>,
    next_session_id: AtomicU64,
}

impl NativeAsrManager {
    pub fn new(app: AppHandle, input: Arc<NativeAsrInput>) -> Self {
        Self {
            app,
            input,
            running: Mutex::new(None),
            next_session_id: AtomicU64::new(1),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().unwrap().is_some()
    }

    /// Start a streaming session against the GGUF model at `gguf_path`. Stops any
    /// prior session first. Returns the new session id. The worker emits
    /// `AsrStreamText` + `AsrSessionFinal`; the final text also comes back from
    /// [`stop`](Self::stop) for the caller's paste/history step.
    pub fn start(&self, gguf_path: PathBuf, language: Option<String>) -> u64 {
        self.stop(); // single live session

        let session_id = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        self.input.arm();
        self.input.open_session();

        let stop = Arc::new(AtomicBool::new(false));
        let input = self.input.clone();
        let app = self.app.clone();
        let stop_for_worker = stop.clone();

        let handle = std::thread::Builder::new()
            .name(format!("native-asr-{session_id}"))
            .spawn(move || {
                // Frame source: drain available frames, then Stop once signalled.
                // Watches the overflow counter so sustained queue drops surface as
                // a degraded-session warning (logged once per 50 dropped frames).
                let mut last_dropped_bucket = 0u64;
                let next = move || {
                    let dropped = input.dropped_total();
                    if dropped / 50 > last_dropped_bucket {
                        last_dropped_bucket = dropped / 50;
                        log::warn!(
                            "[GRAIN] native ASR session {session_id} degraded: {dropped} frames dropped (queue overflow — worker not keeping up)"
                        );
                    }
                    loop {
                        if let Some(f) = input.next_frame() {
                            return FrameCmd::Frame(f);
                        }
                        if stop_for_worker.load(Ordering::Acquire) {
                            return FrameCmd::Stop;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                };

                let emit = |ev| crate::bridge::emit(&app, ev);

                match drive_stream(&gguf_path, language, session_id, next, emit) {
                    Ok(text) => Some(text),
                    Err(e) => {
                        log::error!("[GRAIN] native ASR session {session_id} failed: {e}");
                        crate::bridge::emit(
                            &app,
                            grain_core::DaemonEvent::AsrError {
                                session_id,
                                recoverable: false,
                                message: e.to_string(),
                            },
                        );
                        None
                    }
                }
            })
            .expect("failed to spawn native ASR worker");

        *self.running.lock().unwrap() = Some(Running {
            session_id,
            stop,
            handle,
        });
        session_id
    }

    /// Stop the live session (finalize + emit `AsrSessionFinal`) and join.
    /// Returns the assembled final transcript for the caller's paste/history step.
    pub fn stop(&self) -> Option<String> {
        let running = self.running.lock().unwrap().take();
        if let Some(r) = running {
            r.stop.store(true, Ordering::Release);
            let joined = r.handle.join().ok().flatten();
            self.input.disarm();
            log::info!("[GRAIN] native ASR session {} stopped", r.session_id);
            joined
        } else {
            None
        }
    }

    /// Abort the live session without waiting for a clean final (cancel).
    pub fn cancel(&self) {
        // Disarm the input first so no further frames are buffered, then stop.
        self.input.disarm();
        self.stop();
    }
}
