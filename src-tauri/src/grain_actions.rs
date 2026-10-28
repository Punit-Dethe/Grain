//! [GRAIN] Grain's own shortcut actions, out of the Handy-derived `actions.rs`
//! (Handy Isolation phase 6). Everything here has no upstream counterpart:
//! rolling real-time dictation, Native ASR streaming, the prompt switcher,
//! master chords and the Agent bindings. `actions.rs` keeps
//! upstream's actions and calls [`register`] once from its `ACTION_MAP`.

use crate::actions::{
    process_transcription_output, FinishGuard, ProcessedTranscription, RecordingErrorEvent,
    ShortcutAction,
};
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error, VadPolicy};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use grain_core::{DaemonEvent, SessionMode};
use log::{error, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};

/// [GRAIN] The action log (`docs/Extensions V1/PLAN.md`).
///
/// A submodule for the same reason `context_detect::prompt_stack` is one: a peer
/// module needs a line in the Handy-derived `lib.rs`, whose module block is a
/// live merge-conflict surface (see `Upstream/UPSTREAM.md`). It also sits
/// honestly here — this module owns Grain's own invocations, and the log is the
/// record of what one of them did.
#[path = "grain_action_log.rs"]
pub(crate) mod action_log;

/// [GRAIN] The Extension Mode session (`docs/Extensions V1/PLAN.md`) — a
/// host-owned sibling of `extension_session` that captures raw ASR instead of
/// pasting post-processed text. A submodule for the same divergence reason as
/// [`action_log`].
#[path = "grain_action_session.rs"]
pub(crate) mod action_session;

/// [GRAIN] The headless `grain-ext eval` subcommand (`docs/Extensions V1/PLAN.md`
/// §10 P3). A submodule for the same divergence reason as [`action_log`]: it
/// needs a line in `lib.rs` otherwise, and that module block is a live
/// merge-conflict surface.
#[path = "grain_eval.rs"]
pub(crate) mod eval;

/// [GRAIN] Cross-extension recommendation mode for the same headless eval
/// command. Kept beside `eval` because it loads whole extension pools while the
/// original module continues to evaluate one extension's private commands.
#[path = "grain_recommend_eval.rs"]
pub(crate) mod recommend_eval;

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
    emit_session_started_with_owner(app, session_id, mode, None);
}

fn emit_session_started_with_owner(
    app: &AppHandle,
    session_id: u64,
    mode: SessionMode,
    owner: Option<String>,
) {
    // A fresh capture supersedes a pending Extension Mode chooser (or a
    // handed-off request that might still decline back into it). Invalidate the
    // host-side request before RecordingStarted clears the pill's local surface.
    action_session::supersede(app);
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
    // [GRAIN] Tell the pill which prompt is active BEFORE it shows. The switcher
    // capsule is revealed by hover on the expanded card, not only by a switch,
    // so without this it would open with empty space between its arrows until
    // the user happened to cycle a prompt. Silent by contract — `PromptActive`
    // never arms the riser (see `DaemonEvent::PromptActive`).
    if let Some(name) = current_prompt_name(app) {
        crate::bridge::emit(app, DaemonEvent::PromptActive { name });
    }
    crate::bridge::emit(
        app,
        DaemonEvent::RecordingStarted {
            session_id,
            mode,
            owner,
        },
    );
}

/// [`next_session_id`] + [`emit_session_started`] — the whole pill-start step for
/// capture paths that don't need the id beforehand (batch, Native ASR).
pub(crate) fn session_started(app: &AppHandle, mode: SessionMode) -> u64 {
    let session_id = next_session_id();
    emit_session_started(app, session_id, mode);
    session_id
}

/// Extension sessions always use the batch capture presentation; the owner is
/// a host-derived fact rendered by the pill, never extension-supplied UI.
pub(crate) fn extension_session_started(app: &AppHandle, owner: &str) -> u64 {
    let session_id = next_session_id();
    emit_session_started_with_owner(app, session_id, SessionMode::Batch, Some(owner.to_string()));
    session_id
}

/// Start the native capture-pill lifecycle for Extension Mode without arming
/// dictation-only resources such as focused-field watching or prompt switching.
/// Recommendation and everything after capture live in the prewarmed Tauri
/// interaction window; this native surface never becomes a chooser.
pub(crate) fn extension_mode_started(app: &AppHandle) -> u64 {
    let session_id = next_session_id();
    crate::bridge::emit(
        app,
        DaemonEvent::OverlayConfig {
            position: get_settings(app).overlay_position,
        },
    );
    crate::pill_icon::emit_for_session(app);
    crate::bridge::emit(
        app,
        DaemonEvent::RecordingStarted {
            session_id,
            mode: SessionMode::Batch,
            owner: None,
        },
    );
    session_id
}

/// Stop pressed: the pill switches to "processing" while the transcript is
/// produced. Returns the session id to carry into the async tail so the matching
/// [`emit_processing_complete`] reuses it.
pub(crate) fn emit_recording_stopped(app: &AppHandle) -> u64 {
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

/// Register the shortcuts that live only while a recording session is open:
/// the transient Alt+2 prompt switcher chord. Registration is deferred
/// internally, which keeps this safe to call from inside a `ShortcutAction`.
pub(crate) fn register_session_shortcuts(app: &AppHandle) {
    crate::master_key::register_chords(app);
    // [GRAIN] send-to-AI is no longer taken here. It is registered globally at
    // init, because it now also *starts* a capture from idle — a key that only
    // exists once you are already recording cannot do that. Re-registering it
    // per session would just collide with the global one.
}

/// Release what [`register_session_shortcuts`] took.
pub(crate) fn unregister_session_shortcuts(app: &AppHandle) {
    crate::master_key::unregister_chords(app);
}

/// One capture owns the shared recorder at a time. The coordinator checks this
/// before starting any dictation engine, including when Agent has released its
/// microphone for typing but still owns the input card or reply panel.
pub(crate) fn dictation_start_blocked(app: &AppHandle) -> bool {
    crate::agent::blocks_dictation(app)
        || app
            .try_state::<Arc<AudioRecordingManager>>()
            .is_some_and(|audio| audio.is_recording())
}

/// Mirror a live streaming snapshot to the native pill's Studio Window over the
/// WS event bus. Both parts are cumulative snapshots (SET, not append):
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
/// `utils::cancel_current_operation`: foreground watching, the master chords,
/// any rolling session, and any live stream worker (whose command channel would
/// otherwise stay open and block the next `start_stream`) — then hide the pill.
/// The discarded transcript is intentionally dropped.
pub(crate) fn cancel_session(app: &AppHandle) {
    // Normal Stop tears this down in `emit_recording_stopped`; Cancel bypasses
    // that transition, so it must retire the OS hooks and pending icon resolve
    // here. Leaving them installed keeps observing foreground app/site changes
    // after Grain has returned to idle.
    crate::surface_watch::stop(app);
    crate::extension_session::cancel(app);
    let extension_mode_cancelled = action_session::cancel(app);
    crate::master_key::unregister_chords(app);
    if let Some(rt) = app.try_state::<Arc<crate::rolling::RollingTranscriber>>() {
        rt.cancel_session();
    }
    if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
        tm.cancel_stream();
    }
    if !extension_mode_cancelled {
        crate::bridge::emit(
            app,
            DaemonEvent::SessionCancelled {
                session_id: current_session_id(),
            },
        );
    }
}

// Prompt switcher — cycles the active post-processing prompt and shows
// the new title in the pill. A tap shortcut: the switch happens on press.
struct PromptSwitchAction {
    delta: i32,
}

/// The active post-processing prompt's display name, or `None` only when there
/// are no prompts at all. With none explicitly selected this falls back to the
/// FIRST prompt — the same index [`cycle_prompt`] treats as current — so the
/// switcher capsule never shows empty arrows over a prompt the keys would
/// actually cycle away from. Single lookup shared by the switcher and the
/// session-start announcement, so the two can never disagree.
pub(crate) fn current_prompt_name(app: &AppHandle) -> Option<String> {
    let settings = get_settings(app);
    let selected = settings
        .post_process_selected_prompt_id
        .as_deref()
        .and_then(|id| settings.post_process_prompts.iter().find(|p| p.id == id));
    selected
        .or_else(|| settings.post_process_prompts.first())
        .map(|p| p.name.clone())
}

/// Cycle the active post-processing prompt by `delta` (wrapping) and show
/// the new title in the pill's switcher capsule. Shared by the hold-shortcut
/// [`PromptSwitchAction`] and the switcher's transient arrow keys ([`SwitcherArrowAction`]).
pub fn cycle_prompt(app: &AppHandle, delta: i32) {
    let mut settings = get_settings(app);
    let n = settings.post_process_prompts.len() as i32;
    if n == 0 {
        return;
    }
    let cur_idx = settings
        .post_process_selected_prompt_id
        .as_deref()
        .and_then(|id| {
            settings
                .post_process_prompts
                .iter()
                .position(|p| p.id == id)
        })
        .unwrap_or(0) as i32;
    // Wrapping modulo that stays correct for negative deltas.
    let new_idx = (((cur_idx + delta) % n) + n) % n;
    let chosen = &settings.post_process_prompts[new_idx as usize];
    let chosen_id = chosen.id.clone();
    let chosen_name = chosen.name.clone();

    settings.post_process_selected_prompt_id = Some(chosen_id);
    crate::settings::write_settings(app, settings);

    // Show the new title in the pill.
    crate::bridge::emit(app, DaemonEvent::PromptChanged { name: chosen_name });
}

impl ShortcutAction for PromptSwitchAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        cycle_prompt(app, self.delta);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// The transient arrow keys registered by `master_key` while the Alt+2
// prompt switcher is open. Cycles like `PromptSwitchAction`, then re-arms the
// switcher's idle-close timer so it stays open while the user keeps cycling.
struct SwitcherArrowAction {
    delta: i32,
}

impl ShortcutAction for SwitcherArrowAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        cycle_prompt(app, self.delta);
        crate::master_key::bump_switcher(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Master chord Alt+2 — open the prompt switcher (capsule + arrow keys).
struct MasterPromptSwitchAction;

impl ShortcutAction for MasterPromptSwitchAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::master_key::open_switcher(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Summon the Agent — a voice-first AI scratchpad on the current selection.
// A tap shortcut: it fires on press and hands off to `agent::summon`, which does
// the selection capture + window creation off the input thread.
struct SummonAgentAction;

impl ShortcutAction for SummonAgentAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// Extension Mode's dedicated capture key. Toggle mode uses one press to start
/// and the next to stop; push-to-talk mirrors the normal capture preference and
/// stops on release. The request session itself owns all recorder and pill
/// cleanup so command and shortcut callers cannot drift apart.
struct ExtensionModeAction;

impl ShortcutAction for ExtensionModeAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if !get_settings(app).push_to_talk && action_session::is_recording() {
            action_session::stop(app);
            return;
        }
        match action_session::start(app) {
            Ok(()) => {}
            Err(action_session::StartError::Busy) => {
                log::debug!("[GRAIN] extension mode: shortcut ignored while busy");
            }
            Err(action_session::StartError::NothingInstalled) => {
                log::info!(
                    "[GRAIN] extension mode: shortcut ignored; no searchable extension is installed"
                );
                if let Err(error) = crate::extension_view::present_unavailable(
                    app,
                    "No searchable extension is installed. Install or enable one from Extensions to use Extension Mode.",
                ) {
                    log::error!("[GRAIN] extension mode: could not show first-run guidance: {error}");
                }
            }
            Err(action_session::StartError::Unavailable(reason)) => {
                log::warn!("[GRAIN] extension mode: could not start: {reason}");
                if let Err(error) = crate::extension_view::present_unavailable(app, &reason) {
                    log::error!("[GRAIN] extension mode: could not show start failure: {error}");
                }
            }
        }
    }

    fn stop(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if get_settings(app).push_to_talk {
            action_session::stop(app);
        }
    }
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

// Ask a follow-up on the Agent's latest reply. Registered transiently by
// agent.rs while an Agent surface (panel / pill offer) is live — never global.
struct AgentFollowupAction;

impl ShortcutAction for AgentFollowupAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::open_followup(app);
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
        let rolling_error = rt.start_session(app.clone(), sid).err();

        // C1: no Handy webview overlay on the real-time path — the winit
        // pill is the only surface, driven by the DaemonEvents below.

        let binding_id = binding_id.to_string();
        // Flow uses exact continuous audio and does not load or run the ASR VAD.
        let vad_policy = VadPolicy::Disabled;
        let mut recording_error = rolling_error;
        if recording_error.is_none() {
            // A speaker cue is ordinary microphone input. Flow uses the visual
            // recording indicator so short speech can begin immediately without
            // the cue masking its first word or delaying capture.
            if let Err(e) = rm.try_start_recording_low_ram(&binding_id, vad_policy) {
                recording_error = Some(e);
            } else {
                rm.apply_mute();
            }
        }

        if recording_error.is_none() {
            change_tray_icon(app, TrayIconState::Recording);
            emit_session_started(app, sid, SessionMode::Dictation);
            shortcut::register_cancel_shortcut(app);
            register_session_shortcuts(app);
        } else {
            rt.cancel_session();
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
        unregister_session_shortcuts(app);
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
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste_with_options(
                        final_text,
                        ah_clone.clone(),
                        suppress_trailing_space,
                    ) {
                        error!("Failed to paste real-time transcription: {e}");
                        let _ = ah_clone.emit("paste-error", ());
                    }
                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                })
                .unwrap_or_else(|e| {
                    error!("Failed to run paste on main thread: {e:?}");
                    change_tray_icon(&ah, TrayIconState::Idle);
                });
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
            if let Some(error) = pipeline_error {
                crate::bridge::emit(&ah, DaemonEvent::ModelError { error });
            }
        });
    }
}

// Native ASR — push-to-talk live streaming, on the SAME unified
// TranscriptionManager engine as Batch/Flow: the shortcut loads the selected
// streaming model into the shared slot, opens the mic (frames fan out to the
// manager's StreamRouter), and the manager's stream worker emits live committed
// text to the Studio Window (`AsrStreamText` `DaemonEvent`s — this action only
// owns the recording lifecycle, not the live text). `stop` finalizes the
// stream, pastes the transcript, and saves history.
struct NativeAsrAction;

impl ShortcutAction for NativeAsrAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Require a selected + installed + streaming-capable model. Without one,
        // surface a clear, actionable error to the pill and don't open the mic.
        let selected = get_settings(app).selected_asr_model;
        let mm = app.state::<Arc<ModelManager>>();
        let ok = mm
            .get_model_info(&selected)
            .is_some_and(|m| m.is_downloaded && m.supports_streaming);
        if !ok {
            warn!("Native ASR: no streaming model selected/installed");
            crate::bridge::emit(
                app,
                DaemonEvent::ModelError {
                    error: "Install and select a streaming model in Settings → Speech to Text"
                        .into(),
                },
            );
            return;
        }

        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

        // Load the streaming model into the shared engine slot (swaps out a
        // resident Batch model if needed), then open the stream worker: it waits
        // for the load, and frames queued on the router are never lost.
        tm.initiate_model_load_for(selected);
        tm.start_stream();

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);

        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        // Streaming-capable model verified above → the streaming VAD profile
        // (longer post-speech tail). No `vad_enabled` toggle in
        // grain-core settings — VAD is always on.
        let vad_policy = VadPolicy::Streaming;
        let mut recording_error: Option<String> = None;
        if is_always_on {
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });
            if let Err(e) = rm.try_start_recording(&binding_id, vad_policy) {
                recording_error = Some(e);
            }
        } else {
            match rm.try_start_recording(&binding_id, vad_policy) {
                Ok(()) => {
                    let app2 = app.clone();
                    let rm2 = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        play_feedback_sound_blocking(&app2, SoundType::Start);
                        rm2.apply_mute();
                    });
                }
                Err(e) => recording_error = Some(e),
            }
        }

        if recording_error.is_none() {
            session_started(app, SessionMode::NativeAsr);

            shortcut::register_cancel_shortcut(app);
            // Master chords for the live session. Native ASR has no send-to-AI
            // binding, so the chords are registered directly rather than via
            // `register_session_shortcuts`.
            crate::master_key::register_chords(app);
        } else {
            // Tear down the pending stream worker so its channel doesn't leak
            // and block the next start_stream.
            tm.cancel_stream();
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
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let stop_context = (crate::settings::get_settings(app).context_awareness_enabled
            && rm.has_prompt_mark())
        .then(crate::context_detect::capture_stop_context)
        .unwrap_or_default();
        shortcut::unregister_cancel_shortcut(app);
        // Release the master chords (and the switcher, if open).
        crate::master_key::unregister_chords(app);

        let ah = app.clone();
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let binding_id = binding_id.to_string();

        let session_id = emit_recording_stopped(app);

        change_tray_icon(app, TrayIconState::Transcribing);
        rm.remove_mute();
        play_feedback_sound(app, SoundType::Stop);

        // Snapshot before the stop so a cancel landing during the extra
        // recording buffer (or later, mid-pipeline) is observed.
        let cancel_generation = rm.cancel_generation();
        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone(), Arc::clone(&tm));

            // The mic frames already reached the stream worker live; keep the
            // captured samples only as the batch-fallback input (mirrors Handy:
            // a model that turned out not to stream still yields a transcript).
            let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) else {
                tm.cancel_stream();
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
            // Prompt Record split mark (the explicit Studio hover control).
            let prompt_mark = rm.take_prompt_mark();

            // `finalize_stream` blocks up to its internal timeout while the worker
            // flushes, so keep the wait off the async executor. Always run it (even
            // under Prompt Record) so the stream worker never leaks — its text is
            // just unused when we re-transcribe the sliced audio below.
            let tm_finalize = Arc::clone(&tm);
            let samples_for_fallback = samples.clone();
            let finalized = tauri::async_runtime::spawn_blocking(move || {
                match tm_finalize.finalize_stream() {
                    // A finalized stream with usable text wins (already
                    // custom-word/filler processed by finalize_stream).
                    Ok(Some(text)) if !text.trim().is_empty() => text,
                    // No usable stream → batch-transcribe the captured audio.
                    Ok(_) if !samples_for_fallback.is_empty() => {
                        warn!("Native ASR: stream produced no text — batch fallback");
                        tm_finalize
                            .transcribe(samples_for_fallback)
                            .unwrap_or_default()
                    }
                    Ok(_) => String::new(),
                    Err(e) => {
                        error!("Native ASR: stream finalize failed: {e}");
                        String::new()
                    }
                }
            })
            .await
            .unwrap_or_default();

            let (final_text, suppress_trailing_space) = if let Some(m) =
                prompt_mark.filter(|&m| m > 0 && m < samples.len())
            {
                // Prompt Record on the streaming path: the live transcript
                // covered content + the spoken instruction together, so it can't be
                // split. Re-transcribe the two audio slices and post-process the
                // content with the spoken instruction (AI forced on, regardless of
                // which shortcut stopped the session). `process_transcription_output`
                // also runs voice actions on the content.
                let (content_res, spoken) =
                    crate::prompt_record::transcribe_split(&ah, samples.clone(), Some(m)).await;
                let content = content_res.unwrap_or_default();
                let processed =
                    process_transcription_output(&ah, &content, true, spoken, Some(&stop_context))
                        .await;
                let suppress_trailing_space = processed.suppress_trailing_space;
                let ft = processed.final_text;
                if !ft.trim().is_empty() {
                    if let Err(e) = hm.save_entry(
                        String::new(),
                        content.clone(),
                        true,
                        processed.post_processed_text.clone(),
                        processed.post_process_prompt.clone(),
                    ) {
                        error!("Failed to save Native ASR history entry: {e}");
                    }
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::AsrSessionFinal {
                            session_id,
                            text: ft.clone(),
                        },
                    );
                }
                (ft, suppress_trailing_space)
            } else {
                if finalized.trim().is_empty() {
                    (String::new(), false)
                } else {
                    if let Err(e) =
                        hm.save_entry(String::new(), finalized.clone(), false, None, None)
                    {
                        error!("Failed to save Native ASR history entry: {e}");
                    }
                    // Protocol parity with the old worker: announce the session's
                    // final transcript on the event bus.
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::AsrSessionFinal {
                            session_id,
                            text: finalized.clone(),
                        },
                    );
                    (finalized, false)
                }
            };

            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste_with_options(
                        final_text,
                        ah_clone.clone(),
                        suppress_trailing_space,
                    ) {
                        error!("Failed to paste Native ASR transcription: {e}");
                        let _ = ah_clone.emit("paste-error", ());
                    }
                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                })
                .unwrap_or_else(|e| {
                    error!("Failed to run paste on main thread: {e:?}");
                    change_tray_icon(&ah, TrayIconState::Idle);
                });
            }

            // processing finished → pill/Studio Window hides.
            emit_processing_complete(&ah, session_id);
        });
    }
}

/// Register every Grain action into the shared `ACTION_MAP`. Called once from
/// `actions.rs` — the single hook the Handy-derived registry needs.
pub(crate) fn register(map: &mut HashMap<String, Arc<dyn ShortcutAction>>) {
    // Parakeet TDT Flow transcription.
    map.insert(
        "transcribe_realtime".to_string(),
        Arc::new(RealtimeTranscribeAction {
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    // Native ASR — streaming dictation in the Studio Window.
    map.insert(
        "transcribe_native_asr".to_string(),
        Arc::new(NativeAsrAction) as Arc<dyn ShortcutAction>,
    );
    // Prompt switcher (cycles the active post-processing prompt).
    map.insert(
        "prompt_next".to_string(),
        Arc::new(PromptSwitchAction { delta: 1 }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "prompt_prev".to_string(),
        Arc::new(PromptSwitchAction { delta: -1 }) as Arc<dyn ShortcutAction>,
    );
    // Master switch chord (transiently registered by `master_key` while a
    // recording session is live) + the switcher's transient arrow keys.
    map.insert(
        "master_prompt_switch".to_string(),
        Arc::new(MasterPromptSwitchAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "switcher_prompt_next".to_string(),
        Arc::new(SwitcherArrowAction { delta: 1 }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "switcher_prompt_prev".to_string(),
        Arc::new(SwitcherArrowAction { delta: -1 }) as Arc<dyn ShortcutAction>,
    );
    // Summon the Agent window.
    map.insert(
        "summon_agent".to_string(),
        Arc::new(SummonAgentAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "extension_mode".to_string(),
        Arc::new(ExtensionModeAction) as Arc<dyn ShortcutAction>,
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
        "agent_followup".to_string(),
        Arc::new(AgentFollowupAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "paste_catch_deliver".to_string(),
        Arc::new(PasteCatchDeliverAction) as Arc<dyn ShortcutAction>,
    );
}
