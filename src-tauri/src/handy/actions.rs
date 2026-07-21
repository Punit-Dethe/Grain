use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error, VadPolicy};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, AppSettings};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use grain_core::SessionMode; // [GRAIN] pill session mode
use log::{debug, error};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::Manager;
use tauri::{AppHandle, Emitter};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, serde::Serialize)]
pub(crate) struct RecordingErrorEvent {
    // [GRAIN] pub(crate): also constructed by grain_actions.
    pub(crate) error_type: String,
    pub(crate) detail: Option<String>,
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
pub(crate) struct FinishGuard(pub(crate) AppHandle); // [GRAIN] pub(crate): shared with grain_actions
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn set_post_process_override(&self, _override: bool) {}
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
    post_process_override: AtomicBool,
}

/// Field name for structured output JSON schema
pub(crate) const TRANSCRIPTION_FIELD: &str = "transcription"; // [GRAIN] pub(crate): shared with grain_post_process

/// Strip invisible Unicode characters that some LLMs may insert
pub(crate) fn strip_invisible_chars(s: &str) -> String { // [GRAIN] pub(crate)
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
pub(crate) fn build_system_prompt(prompt_template: &str) -> String { // [GRAIN] pub(crate)
    prompt_template.replace("${output}", "").trim().to_string()
}

/// Returns `true` when a transcription has no meaningful content to
/// post-process (empty or whitespace-only). Used to skip the post-processing
/// LLM call when nothing was actually transcribed, which would otherwise make
/// the model reply with an error message such as "you need to provide the
/// transcription".
pub(crate) fn is_blank_transcription(transcription: &str) -> bool { // [GRAIN] pub(crate)
    transcription.trim().is_empty()
}

/// Poll `is_cancelled` while awaiting `operation`; returns `None` the moment a
/// cancellation is observed, abandoning the (possibly stalled) operation.
async fn complete_unless_cancelled<F, C>(operation: F, is_cancelled: C) -> Option<F::Output>
where
    F: Future,
    C: Fn() -> bool,
{
    tokio::pin!(operation);

    loop {
        if is_cancelled() {
            return None;
        }

        if let Ok(result) =
            tokio::time::timeout(CANCELLATION_POLL_INTERVAL, operation.as_mut()).await
        {
            return Some(result);
        }
    }
}

// [GRAIN] Upstream's `post_process_transcription` lived here. Grain's
// replacement — multi-provider, context-aware, rotation-capable — is
// `grain_post_process::post_process_transcription`, and the LLM call it drives
// is `grain_post_process::run_one_provider`.
//
// Upstream's original is NOT kept inline the way `llm_client.rs`/`overlay.rs`
// keep theirs: those are whole files left un-compiled, while anything inline
// here must still typecheck — and upstream's version calls
// `llm_client::send_chat_completion*` with upstream's signature, which Grain's
// client no longer has. So this is a deliberate hole: expect a modify/delete
// conflict when upstream touches that function, and port the change into
// `grain_post_process.rs` by hand.

async fn maybe_convert_chinese_variant(
    effective_language: &str,
    transcription: &str,
) -> Option<String> {
    // Gate on the language the model actually transcribed in (the effective
    // language), not the persisted intent. A leftover zh-Hans/zh-Hant intent
    // from a previously selected model must not run OpenCC S2T/T2S over output a
    // non-Chinese model produced — that would silently rewrite any shared CJK
    // characters (e.g. Japanese kanji) in the result.
    let is_simplified = effective_language == "zh-Hans";
    let is_traditional = effective_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("effective language is not Simplified or Traditional Chinese; skipping conversion");
        return None;
    }

    debug!(
        "Starting Chinese variant conversion using OpenCC for language: {}",
        effective_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

/// Resolve the persisted language *intent* into the language the currently-loaded
/// model will actually use — the same capability-aware coercion the transcription
/// paths apply (see [`crate::managers::model::effective_language`]). Post-processing
/// resolves it independently so it agrees with the language the transcription ran
/// in, without threading a value through the pipeline.
fn resolve_effective_language(app: &AppHandle, settings: &AppSettings) -> String {
    let tm = app.state::<Arc<TranscriptionManager>>();
    let model_manager = app.state::<Arc<ModelManager>>();
    let active_model = tm
        .get_current_model()
        .unwrap_or_else(|| settings.selected_model.clone());
    match model_manager.get_model_info(&active_model) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
    // [GRAIN] Prompt Record: the spoken AI instruction for this transcript (audio
    // after the pill-click mark), already transcribed. `None` for a normal
    // dictation. When present, the caller also forces `post_process = true` so the
    // instruction is actually applied regardless of which shortcut stopped the
    // session.
    spoken_prompt: Option<String>,
    // [GRAIN] True when `transcription` came from the rolling-window assembler —
    // enables the token-efficient seam-repair prompt layer.
    rolling: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);

    // [GRAIN] Voice actions: fire any spoken trigger (open apps/sites) and strip
    // it from what we paste. Runs on the finalized transcript BEFORE
    // post-processing so a pure command ("start coding") never costs an LLM call
    // — if the whole utterance was the command, `final_text` is now empty and the
    // paste path below already skips empty output. Zero-cost when no actions
    // are configured (a single `is_empty()` check inside `intercept`).
    let mut final_text = crate::voice_actions::intercept(app, transcription);
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    // Resolve the language the transcription actually ran in (the persisted
    // intent coerced against the loaded model's capabilities) so OpenCC keys off
    // the effective language rather than a possibly-stale intent.
    // [GRAIN] Converts `final_text` (post voice-action strip), not the raw
    // transcription, so a stripped trigger phrase can't reach OpenCC.
    let effective_language = resolve_effective_language(app, &settings);
    if let Some(converted_text) =
        maybe_convert_chinese_variant(&effective_language, &final_text).await
    {
        final_text = converted_text;
    }

    // [GRAIN] extension transform pipeline (SPEC §3.1): enabled scripted
    // extensions rewrite the finalized text here — after Grain's fast stages,
    // before the slow LLM stage and paste. Each runs under a hard 150 ms
    // deadline; a cold/slow/failing worker leaves the text untouched.
    final_text = crate::extension_host::run_transforms(app, final_text).await;

    if post_process {
        if let Some(processed_text) = crate::grain_post_process::post_process_transcription(
            app,
            &settings,
            &final_text,
            spoken_prompt.as_deref(),
            rolling,
        )
        .await
        {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    } else if final_text != transcription {
        post_processed_text = Some(final_text.clone());
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // [GRAIN] Only warm the local ASR model when this recording will be
        // transcribed locally. When STT smart rotation routes batch to a cloud
        // provider, loading the on-device model here is wasted work that sits
        // resident in RAM until the idle/immediate unload fires. The cloud route
        // never touches it; if rotation later finds no eligible provider,
        // stt_router::local() loads the model on demand. VAD pre-load stays
        // unconditional below — recording needs it for either backend.
        let kickoff_started = Instant::now();
        if !crate::stt_router::will_route_to_cloud(app) {
            tm.initiate_model_load();
        } else {
            debug!("[GRAIN] batch routes to cloud STT — skipping local model warm-up");
        }
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        change_tray_icon(app, TrayIconState::Recording);
        let tray_elapsed = tray_started.elapsed();
        // [GRAIN] The winit pill is the single overlay surface for BOTH batch and
        // rolling — driven by the DaemonEvents below (emitted on successful start,
        // same pattern as the rolling path). No Handy webview overlay.

        // Get the microphone mode to determine audio feedback timing
        let plan_started = Instant::now();
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        let plan_elapsed = plan_started.elapsed();
        // Everything above runs before capture can begin, so each span here is
        // added keypress->capture latency. [GRAIN] No overlay step: the pill is
        // shown by DaemonEvents after the recording actually starts.
        debug!(
            "start-path pre-recording steps: model_kickoff={:?} tray={:?} settings={:?}",
            kickoff_elapsed, tray_elapsed, plan_elapsed
        );
        debug!("Microphone mode - always_on: {}", is_always_on);

        // Batch dictation never streams, so the session runs the offline VAD
        // profile. [GRAIN] grain-core settings have no `vad_enabled` toggle
        // (upstream's setting was never ported) — VAD is always on.
        let vad_policy = VadPolicy::Offline;

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id, vad_policy) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id, vad_policy) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    // Small delay to ensure microphone stream is active
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        // Helper handles disabled audio feedback by returning early, so we reuse it
                        // to keep mute sequencing consistent in every mode.
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // [GRAIN] the single pill is the overlay for every capture mode.
            crate::grain_actions::session_started(app, SessionMode::Batch);
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
            // [GRAIN] master chords + send-to-AI, for this session only.
            crate::grain_actions::register_session_shortcuts(app);
        } else {
            // Starting failed (e.g. blocked mic permissions). The pill was never
            // shown (we only emit on success), so nothing to tear down here.
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn set_post_process_override(&self, override_val: bool) {
        self.post_process_override
            .store(override_val, Ordering::Relaxed);
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);
        // [GRAIN] release the session-only shortcuts (chords + send-to-AI).
        crate::grain_actions::unregister_session_shortcuts(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        // [GRAIN] stop pressed → the pill enters "processing". The id is carried
        // into the async tail so the matching pill-hide reuses it.
        let session_id = crate::grain_actions::emit_recording_stopped(app);

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process || self.post_process_override.load(Ordering::Relaxed);

        // Snapshot NOW (before transcription starts): any cancel_recording()
        // after this point — including one landing mid-LLM — bumps the
        // generation and is observed by the output-processing task below.
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                // [GRAIN] A cancel landing between the stop and the decode must
                // bail BEFORE the expensive transcription. Upstream also hides
                // its overlay and resets the tray here; Grain's cancel initiator
                // already did both (SessionCancelled + tray), so this only stops.
                if rm.was_cancelled_since(cancel_generation) {
                    debug!("Transcription operation cancelled after recording stop");
                    return;
                }

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    crate::grain_actions::emit_processing_complete(&ah, session_id);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save. [GRAIN] S4: route
                    // through the STT dispatcher — local in-process by default,
                    // or cloud rotation when smart rotation is on. The WAV task
                    // above runs concurrently while this awaits.
                    //
                    // [GRAIN] Prompt Record: if the user clicked the pill mid-
                    // recording, the buffer is split at that mark into content +
                    // a spoken AI instruction, each transcribed independently. A
                    // recorded instruction forces the AI path regardless of which
                    // shortcut stopped the session. No mark → a single pass, as before.
                    let prompt_mark = rm.take_prompt_mark();
                    let transcription_time = Instant::now();
                    let (transcription_result, spoken_prompt) =
                        crate::prompt_record::transcribe_split(&ah, samples, prompt_mark).await;
                    let post_process = post_process || spoken_prompt.is_some();

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    // [GRAIN] Same as above: a cancel that landed while the decode
                    // ran stops here, before history/paste. No teardown — the
                    // cancel initiator owns the pill and tray.
                    if rm.was_cancelled_since(cancel_generation) {
                        debug!("Transcription operation cancelled before output handling");
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            // [GRAIN] pill is already in "processing" from the
                            // RecordingStopped above — no extra overlay call needed.
                            // A cancel during the LLM/paste stage is observed via
                            // the generation snapshot; the cancel initiator
                            // already hid the pill and reset the tray, so on
                            // cancellation we just stop before history/paste.
                            let Some(processed) = complete_unless_cancelled(
                                process_transcription_output(
                                    &ah,
                                    &transcription,
                                    post_process,
                                    spoken_prompt,
                                    false,
                                ),
                                || rm.was_cancelled_since(cancel_generation),
                            )
                            .await
                            else {
                                debug!("Transcription operation cancelled during output handling");
                                return;
                            };

                            if rm.was_cancelled_since(cancel_generation) {
                                debug!("Transcription operation cancelled before paste");
                                return;
                            }

                            // Save to history if WAV was saved
                            if wav_saved {
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    post_process,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            if processed.final_text.is_empty() {
                                crate::grain_actions::emit_processing_complete(&ah, session_id);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                ah.run_on_main_thread(move || {
                                    match utils::paste(final_text, ah_clone.clone()) {
                                        Ok(()) => debug!(
                                            "Text pasted successfully in {:?}",
                                            paste_time.elapsed()
                                        ),
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                    }
                                    crate::grain_actions::emit_processing_complete(&ah_clone, session_id);
                                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    crate::grain_actions::emit_processing_complete(&ah, session_id);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                });
                            }
                        }
                        Err(err) => {
                            error!("Transcription failed: {}", err);
                            // Surface the failure to the UI (toast). The full
                            // message is also in the log via the line above.
                            let _ = ah.emit("transcription-error", err.to_string());
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            crate::grain_actions::emit_processing_complete(&ah, session_id);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                crate::grain_actions::emit_processing_complete(&ah, session_id);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
        // [GRAIN] tear down Grain's session surfaces (chords, rolling worker,
        // live stream) and hide the pill.
        crate::grain_actions::cancel_session(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: true,
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] Grain's own actions (rolling, Native ASR, prompt switcher, master
    // chords, Agent, Grain Space) register here — see grain_actions.rs.
    crate::grain_actions::register(&mut map);
    map
});

#[cfg(test)]
mod tests {
    use super::{complete_unless_cancelled, is_blank_transcription};
    use std::future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn blank_transcription_is_detected() {
        assert!(is_blank_transcription(""));
        assert!(is_blank_transcription("   "));
        assert!(is_blank_transcription("\t\n  \r\n"));
    }

    #[test]
    fn non_blank_transcription_is_kept() {
        assert!(!is_blank_transcription("hello"));
        assert!(!is_blank_transcription("  hello  "));
    }

    #[test]
    fn completed_operation_returns_its_output() {
        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::ready("done"),
            || false,
        ));

        assert_eq!(result, Some("done"));
    }

    #[test]
    fn pending_operation_stops_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_thread = Arc::clone(&cancelled);
        let cancel_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancelled_for_thread.store(true, Ordering::Release);
        });

        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::pending::<()>(),
            || cancelled.load(Ordering::Acquire),
        ));

        cancel_thread.join().unwrap();
        assert_eq!(result, None);
    }
}
