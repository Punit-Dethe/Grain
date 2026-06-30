//! [GRAIN] M6: Native ASR session manager.
//!
//! Owns the single live Native ASR session: it spawns the worker thread that
//! runs [`super::worker::drive_session`], feeding it frames from the
//! [`NativeAsrInput`] fan-out and emitting the stabilized stream onto the core's
//! `DaemonEvent` bus (so the pill renders partial/commit/final exactly like the
//! Batch/Rolling paths).
//!
//! The backend is injected (`start(backend, …)`): Milestone 6 drives it with the
//! scripted fake; Milestone 5's Sherpa backend drops into the same call. The
//! `start`/`stop`/`cancel` surface is wired to a shortcut/action in Milestone 7.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use grain_asr_core::model::AsrModelSpec;
use grain_asr_core::session::{AsrSessionConfig, ContextHints, NativeAsrBackend};
use grain_asr_core::stabilizer::StabilizerConfig;
use tauri::AppHandle;

use super::input::NativeAsrInput;
use super::worker::{drive_session, FrameCmd};

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

    /// Start a Native ASR session against `backend`/`spec`. Stops any prior
    /// session first. Returns the new session id. The worker emits
    /// `Asr*` `DaemonEvent`s and the final text lands on `AsrSessionFinal`.
    pub fn start(
        &self,
        backend: Box<dyn NativeAsrBackend>,
        spec: AsrModelSpec,
        language: Option<String>,
        hints: ContextHints,
    ) -> u64 {
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

                let config = AsrSessionConfig {
                    session_id,
                    language,
                    hints,
                    want_word_timestamps: true,
                };
                let emit = |ev| crate::bridge::emit(&app, ev);

                match drive_session(backend, &spec, config, StabilizerConfig::default(), next, emit)
                {
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
        // The worker still runs `finish()` on Stop; for a hard cancel we just
        // disarm the input first so no further frames are buffered, then stop.
        self.input.disarm();
        self.stop();
    }
}
