//! [GRAIN] Grain's own shortcut actions, out of the Handy-derived `actions.rs`
//! (Handy Isolation phase 6). Everything here has no upstream counterpart:
//! rolling real-time dictation
//! and the Agent bindings. `actions.rs` keeps
//! upstream's actions and calls [`register`] once from its `ACTION_MAP`.

use crate::actions::{
    process_transcription_output, FinishGuard, ProcessedTranscription, RecordingErrorEvent,
    ShortcutAction,
};
use crate::audio_feedback::{play_feedback_sound, SoundType};
use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use grain_core::{DaemonEvent, SessionMode};
use log::error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

/// Monotonic id for the current recording session (pill events).
pub(crate) static SESSION_ID: AtomicU64 = AtomicU64::new(0);

/// Serialize only capture startup. The recorder itself arbitrates ownership,
/// but some dictation actions prewarm or change UI before claiming it.
static CAPTURE_START_GATE: Mutex<()> = Mutex::new(());

pub(crate) fn capture_start_guard() -> MutexGuard<'static, ()> {
    CAPTURE_START_GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The current pill session id, for emitters outside this module (the
/// unified TranscriptionManager mirrors live stream text to the pill).
pub(crate) fn current_session_id() -> u64 {
    SESSION_ID.load(Ordering::Relaxed)
}

/// Claim the next session id. Flow needs it before its worker starts so the
/// recording and transcription lifecycle use the same id.
pub(crate) fn next_session_id() -> u64 {
    SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1
}

/// Announce a started recording to the pill: `OverlayConfig` first so it anchors
/// (or stays hidden when the user chose no overlay position), then
/// `RecordingStarted`. Emitted only after capture actually starts, so a failed
/// start never shows a pill that must be torn down.
pub(crate) fn emit_session_started(app: &AppHandle, session_id: u64, mode: SessionMode) {
    crate::grain_dictionary::cancel_watch();
    crate::bridge::emit(
        app,
        DaemonEvent::OverlayConfig {
            position: get_settings(app).overlay_position,
        },
    );
    // [GRAIN] Pill identity: the icon of whatever we are dictating into, before
    // the pill is told to show itself, so a cached icon is already there on the
    // first painted frame. A cold app emits nothing here and resolves behind the
    // session — this call never blocks the start.
    crate::pill_icon::emit_for_session(app);
    // [GRAIN] …and keep following it: switching windows mid-dictation moves the
    // paste target, and the pill should end up agreeing with post-processing
    // (which resolves its context at paste time, after every switch).
    crate::surface_watch::start(app);
    crate::prompt_record::capture_started(app, session_id);
    crate::bridge::emit(app, DaemonEvent::RecordingStarted { session_id, mode });
}

/// [`next_session_id`] + [`emit_session_started`] — the whole pill-start step for
/// capture paths that don't need the id beforehand (batch, Native ASR).
pub(crate) fn session_started(app: &AppHandle, mode: SessionMode) -> u64 {
    let session_id = next_session_id();
    emit_session_started(app, session_id, mode);
    session_id
}

/// Stop pressed: the pill switches to "processing" while the transcript is
/// produced. Returns the session id to carry into the async tail so the matching
/// [`emit_processing_complete`] reuses it.
pub(crate) fn emit_recording_stopped(app: &AppHandle) -> u64 {
    app.state::<Arc<AudioRecordingManager>>()
        .invalidate_recording_readiness();
    let session_id = current_session_id();
    // [GRAIN] Nothing left to follow — drop the hook rather than leave it live
    // between sessions, and invalidate any icon resolution still in flight.
    crate::surface_watch::stop(app);
    crate::bridge::emit(app, DaemonEvent::RecordingStopped { session_id });
    session_id
}

/// Processing finished (success, empty result, or error) → the pill hides. Every
/// terminal branch of a capture path must call this exactly once, or the pill
/// stays up.
pub(crate) fn emit_processing_complete(app: &AppHandle, session_id: u64) {
    crate::bridge::emit(
        app,
        DaemonEvent::ProcessingComplete {
            session_id,
            text: String::new(),
        },
    );
}

/// Paste delays, scripts and clipboard restoration must not block the UI or an
/// async executor thread. The shared clipboard module dispatches only input
/// injection back to the UI thread, preserving macOS keyboard API requirements.
pub(crate) async fn paste_transcription(
    app: AppHandle,
    text: String,
    suppress_trailing_space: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let watch = crate::grain_dictionary::prepare(&app, &text, suppress_trailing_space);
        let result = utils::paste_with_options(text, app.clone(), suppress_trailing_space);
        #[cfg(windows)]
        if let (Ok(()), Some(watch)) = (&result, watch) {
            watch.start(app);
        }
        result
    })
    .await
    .map_err(|error| format!("Paste worker failed: {error}"))?
}

/// One capture owns the shared recorder at a time. The coordinator checks this
/// before starting any dictation engine, including when Agent has released its
/// microphone for typing but still owns the input card.
pub(crate) fn dictation_start_blocked(app: &AppHandle) -> bool {
    crate::agent::blocks_dictation(app)
        || app
            .try_state::<Arc<AudioRecordingManager>>()
            .is_some_and(|audio| audio.is_recording())
}

/// Mirror a live streaming snapshot to the public SDK bus and WebView context. Both parts are cumulative snapshots (SET, not append):
/// `committed` is the stable prefix, `tentative` the volatile tail — the pill
/// needs the tail so the preview keeps moving while the engine's auto-commit is
/// between commit points.
pub(crate) fn mirror_stream_text(app: &AppHandle, committed: &str, tentative: &str) {
    crate::bridge::emit(
        app,
        DaemonEvent::AsrStreamText {
            session_id: current_session_id(),
            committed: committed.to_string(),
            tentative: tentative.to_string(),
        },
    );
}

/// Tear down every Grain surface a cancel has to clear, on top of upstream's
/// `utils::cancel_current_operation`: foreground watching,
/// any rolling session, and any live stream worker (whose command channel would
/// otherwise stay open and block the next `start_stream`) — then hide the pill.
/// The discarded transcript is intentionally dropped.
pub(crate) fn cancel_session(app: &AppHandle) {
    // Normal Stop tears this down in `emit_recording_stopped`; Cancel bypasses
    // that transition, so it must retire the OS hooks and pending icon resolve
    // here. Leaving them installed keeps observing foreground app/site changes
    // after Grain has returned to idle.
    crate::surface_watch::stop(app);
    if let Some(rt) = app.try_state::<Arc<crate::rolling::RollingTranscriber>>() {
        rt.cancel_session();
    }
    if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
        tm.cancel_stream();
    }
    crate::bridge::emit(
        app,
        DaemonEvent::SessionCancelled {
            session_id: current_session_id(),
        },
    );
}

// Summon the Agent — a voice-first AI scratchpad on the current selection.
// A tap shortcut: it fires on press and hands off to `agent::summon`, which does
// the selection capture + window creation off the input thread.
struct SummonAgentAction;

/// Mark the current dictation buffer; release and repeated presses do nothing.
struct PromptRecordAction;

impl ShortcutAction for PromptRecordAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if !crate::prompt_record::shortcut_is_active() {
            return;
        }
        if let Some(manager) = app.try_state::<Arc<AudioRecordingManager>>() {
            let session_id = current_session_id();
            if manager.arm_prompt_record() {
                crate::grain_overlay::mark_prompt_record(app, session_id);
            }
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

impl ShortcutAction for SummonAgentAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

struct AgentSubmitAction;

impl ShortcutAction for AgentSubmitAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::global_submit(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

struct AgentCloseAction;

impl ShortcutAction for AgentCloseAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::global_close(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Deliver a transcript whose paste missed the text field. Registered
// transiently by paste_catch.rs while a hold is armed — never global. All work
// (including releasing this very shortcut) happens off the input thread inside
// `paste_catch::deliver`.
struct PasteCatchDeliverAction;

impl ShortcutAction for PasteCatchDeliverAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::paste_catch::deliver(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Parakeet TDT Flow journals exact capture audio while a serial worker
// finalizes stable windows and refreshes the mutable tail.
struct RealtimeTranscribeAction {
    post_process_override: AtomicBool,
}

impl ShortcutAction for RealtimeTranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rt = Arc::clone(&app.state::<Arc<crate::rolling::RollingTranscriber>>());
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

        let sid = next_session_id();
        // start_session registers this generation and synchronously establishes
        // the manager's load predicate before it spawns the rolling worker. The
        // model itself still loads asynchronously, so recording startup does not
        // wait for weights or allocate a second engine.
        if let Err(error) = rt.start_session(app.clone(), sid) {
            rt.cancel_session();
            crate::bridge::emit(app, DaemonEvent::ModelError { error });
            return;
        }

        let binding_id = binding_id.to_string();
        // Flow uses exact continuous audio and does not load or run the ASR VAD.
        let vad_policy = VadPolicy::Disabled;
        let overlay_generation = crate::grain_overlay::show_capture(app, false);
        let mut readiness = None;
        let mut recording_error = None;
        match rm.try_start_recording_low_ram(&binding_id, vad_policy) {
            Ok(ready) => readiness = Some(ready),
            Err(error) => recording_error = Some(error),
        }

        if recording_error.is_none() {
            change_tray_icon(app, TrayIconState::Recording);
            emit_session_started(app, sid, SessionMode::Dictation);
            if let Some(readiness) = readiness {
                crate::grain_capture::announce_ready(app, &rm, readiness, true);
            }
            shortcut::register_cancel_shortcut(app);
        } else {
            crate::grain_overlay::hide_failed_capture(app, overlay_generation);
            rt.cancel_session();
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = crate::managers::audio::recording_error_type(&err);
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err.to_string()),
                    },
                );
            }
        }
    }

    fn set_post_process_override(&self, override_val: bool) {
        self.post_process_override
            .store(override_val, Ordering::Relaxed);
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let post_process = self.post_process_override.load(Ordering::Relaxed);
        let stop_context = (crate::settings::get_settings(app).context_awareness_enabled
            && (post_process || rm.has_prompt_mark()))
        .then(crate::context_detect::capture_stop_context)
        .unwrap_or_default();
        shortcut::unregister_cancel_shortcut(app);
        let ah = app.clone();
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let rt = Arc::clone(&app.state::<Arc<crate::rolling::RollingTranscriber>>());

        // Stop pressed → pill enters "processing" while the remaining chunks
        // finalize (recording overrode processing until now).
        let session_id = emit_recording_stopped(app);

        change_tray_icon(app, TrayIconState::Transcribing);
        // C1: pill already showed "processing" from RecordingStopped above.
        rm.remove_mute();
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string();
        // Snapshot before the stop so a cancel landing during the extra
        // recording buffer (or later, mid-pipeline) is observed.
        let cancel_generation = rm.cancel_generation();
        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone(), Arc::clone(&tm));

            // Empty on Flow: its Float32 journal owns the complete recording.
            let stopped = rm.stop_recording(&binding_id, cancel_generation);
            let Some(samples) = stopped else {
                rt.cancel_session();
                if !rm.was_cancelled_since(cancel_generation) {
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::ModelError {
                            error: "Audio capture failed; no partial transcript was produced"
                                .to_string(),
                        },
                    );
                }
                return;
            };
            // Prompt Record mark (the explicit pill-control split point),
            // taken before draining the worker.
            let prompt_mark = rm.take_prompt_mark();
            // Drain the rolling worker → final assembled transcript. Always done,
            // even under Prompt Record, so the worker never leaks — its text is
            // just unused in that case (it mixed content + instruction).
            let rolling = rt.finish_session();
            let rolling_text = rolling
                .as_ref()
                .map(|output| output.text.as_str())
                .unwrap_or_default();
            let audio_len = rolling
                .as_ref()
                .map(|output| output.frame_count())
                .unwrap_or(samples.len());

            // Prompt Record: the rolling-assembled text covers the WHOLE
            // utterance (content + spoken instruction mixed), so it can't be split.
            // Re-transcribe the two audio slices batch-style instead. This extra
            // pass only happens when the user actually armed Prompt Record.
            let (final_text, spoken_prompt, post_process, pipeline_error) =
                if let Some(m) = prompt_mark.filter(|&m| m > 0 && m < audio_len) {
                    let prompt_audio = match rolling.as_ref() {
                        Some(output) => output.materialize_audio().unwrap_or_else(|error| {
                            error!("Failed to read rolling Prompt Record audio: {error}");
                            Vec::new()
                        }),
                        None => samples.clone(),
                    };
                    let (content_res, spoken) =
                        crate::prompt_record::transcribe_split(&ah, prompt_audio, Some(m)).await;
                    // `transcribe_split` routes through the STT dispatcher, which
                    // finalizes internally — don't finalize again. Batch-style
                    // re-transcription has no rolling seams.
                    let content = content_res.unwrap_or_default();
                    (
                        content,
                        spoken.clone(),
                        post_process || spoken.is_some(),
                        None,
                    )
                } else if let Some(error) = rolling
                    .as_ref()
                    .and_then(|output| output.error.as_ref())
                    .cloned()
                {
                    (String::new(), None, false, Some(error))
                } else {
                    let assembled = !rolling_text.trim().is_empty();
                    let ft = if assembled {
                        // Apply the shared final-text stage (custom-word dictionary
                        // + filler/stutter filtering) ONCE on the assembled transcript.
                        // Flow never biases windows through a prior text prompt, so
                        // the fuzzy custom-word pass must run here. Done once per dictation,
                        // NOT per 15-20s chunk.
                        let settings = get_settings(&ah);
                        crate::audio_toolkit::finalize_transcript(
                            &rolling_text,
                            &settings.custom_words,
                            settings.word_correction_threshold,
                            // [GRAIN] #1738: filler removal keys on the transcription
                            // output language (intent), not the UI language.
                            &settings.selected_language,
                            &settings.custom_filler_words,
                            settings.filler_word_removal_enabled,
                            false,
                            // [GRAIN] Snippets built-in extension gate (SPEC 10.1): disabled ->
                            // empty slice, the zero-cost no-op path.
                            if settings.snippets_enabled {
                                &settings.snippets
                            } else {
                                &[]
                            },
                            settings.scrap_that_enabled,
                        )
                    } else {
                        String::new()
                    };
                    (ft, None, post_process, None)
                };

            // Keep the pre-LLM transcript alive through persistence. History's
            // Original/AI comparison is only meaningful when the raw side is
            // not overwritten by the text that will be pasted.
            let transcription_text = final_text;
            let processed = if let Some(error) = pipeline_error.as_ref() {
                error!("[GRAIN] {error}");
                ProcessedTranscription {
                    final_text: String::new(),
                    post_processed_text: None,
                    post_process_prompt: None,
                    suppress_trailing_space: false,
                }
            } else {
                process_transcription_output(
                    &ah,
                    &transcription_text,
                    post_process,
                    spoken_prompt,
                    Some(&stop_context),
                )
                .await
            };
            let suppress_trailing_space = processed.suppress_trailing_space;
            let final_text = processed.final_text;

            // Deliver text before WAV export and history I/O. Neither is needed
            // by the paste path, and both can take noticeable time on slow disks.
            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                if let Err(e) =
                    paste_transcription(ah.clone(), final_text, suppress_trailing_space).await
                {
                    error!("Failed to paste real-time transcription: {e}");
                    let _ = ah.emit("paste-error", ());
                }
                if current_session_id() == session_id {
                    change_tray_icon(&ah, TrayIconState::Idle);
                }
            }

            if audio_len > 0 {
                let file_name = format!("grain-{}.wav", chrono::Utc::now().timestamp());
                let wav_path = hm.recordings_dir().join(&file_name);
                if let Some(output) = rolling.as_ref() {
                    let output = output.clone();
                    let result =
                        tauri::async_runtime::spawn_blocking(move || output.save_wav(&wav_path))
                            .await;
                    if let Ok(Err(error)) = result {
                        error!("Failed to save Flow journal WAV: {error}");
                    }
                } else {
                    let samples_for_wav = samples.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    })
                    .await;
                }
                if let Err(e) = hm.save_entry(
                    file_name,
                    transcription_text,
                    post_process,
                    processed.post_processed_text.clone(),
                    processed.post_process_prompt.clone(),
                ) {
                    error!("Failed to save history entry: {e}");
                }
            }

            // B2: processing finished → pill hides.
            emit_processing_complete(&ah, session_id);
            // ProcessingComplete resets the session state; emit the terminal
            // failure afterwards so the pill remains visibly in fallback.
            if let Some(error) = pipeline_error.filter(|_| current_session_id() == session_id) {
                crate::bridge::emit(&ah, DaemonEvent::ModelError { error });
            }
        });
    }
}

/// Register every Grain action into the shared `ACTION_MAP`. Called once from
/// `actions.rs` — the single hook the Handy-derived registry needs.
pub(crate) fn register(map: &mut HashMap<String, Arc<dyn ShortcutAction>>) {
    map.insert("prompt_record".to_string(), Arc::new(PromptRecordAction));
    // Parakeet TDT Flow transcription.
    map.insert(
        "transcribe_realtime".to_string(),
        Arc::new(RealtimeTranscribeAction {
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    // Summon the Agent window.
    map.insert(
        "summon_agent".to_string(),
        Arc::new(SummonAgentAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "agent_submit".to_string(),
        Arc::new(AgentSubmitAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "agent_close".to_string(),
        Arc::new(AgentCloseAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "paste_catch_deliver".to_string(),
        Arc::new(PasteCatchDeliverAction) as Arc<dyn ShortcutAction>,
    );
}
