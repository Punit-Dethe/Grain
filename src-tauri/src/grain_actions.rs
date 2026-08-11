//! [GRAIN] Grain's own shortcut actions, out of the Handy-derived `actions.rs`
//! (Handy Isolation phase 6). Everything here has no upstream counterpart:
//! rolling real-time dictation, Native ASR streaming, the prompt switcher,
//! master chords, the Agent, and Grain Space bindings. `actions.rs` keeps
//! upstream's actions and calls [`register`] once from its `ACTION_MAP`.

use crate::actions::{
    process_transcription_output, FinishGuard, RecordingErrorEvent, ShortcutAction,
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
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// Monotonic id for the current recording session (pill events).
pub(crate) static SESSION_ID: AtomicU64 = AtomicU64::new(0);

/// The current pill session id, for emitters outside this module (the
/// unified TranscriptionManager mirrors live stream text to the pill).
pub(crate) fn current_session_id() -> u64 {
    SESSION_ID.load(Ordering::Relaxed)
}

/// Claim the next session id. Split from [`session_started`] for the rolling
/// path, which needs the id before the engine session opens so its live-preview
/// events carry the same id as `RecordingStarted`.
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
    crate::bridge::emit(
        app,
        DaemonEvent::OverlayConfig {
            position: get_settings(app).overlay_position,
        },
    );
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

/// Stop pressed: the pill switches to "processing" while the transcript is
/// produced. Returns the session id to carry into the async tail so the matching
/// [`emit_processing_complete`] reuses it.
pub(crate) fn emit_recording_stopped(app: &AppHandle) -> u64 {
    let session_id = current_session_id();
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
/// the master chords (Alt+1 Prompt Record / Alt+2 switcher) and — unless
/// push-to-talk owns the key — send-to-AI. Both defer their actual registration
/// internally, which is what keeps this safe to call from inside a
/// `ShortcutAction`.
pub(crate) fn register_session_shortcuts(app: &AppHandle) {
    crate::master_key::register_chords(app);
    // [GRAIN] send-to-AI is no longer taken here. It is registered globally at
    // init, because it now also *starts* a capture from idle — a key that only
    // exists once you are already recording cannot do that. Re-registering it
    // per session would just collide with the global one.
    // [GRAIN] Read the focused field's distinctive terms for this recording,
    // off-thread, so they can bias the recognizer. Deliberately here and not at
    // transcription time: this overlaps the user speaking, which is dead time
    // already, instead of putting a UI-Automation round-trip in front of the
    // decoder. No-ops entirely when the opt-in is off.
    crate::context_bias::arm_session(app);
}

/// Release what [`register_session_shortcuts`] took.
pub(crate) fn unregister_session_shortcuts(app: &AppHandle) {
    crate::master_key::unregister_chords(app);
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
/// `utils::cancel_current_operation`: the master chords, any rolling session,
/// and any live stream worker (whose command channel would otherwise stay open
/// and block the next `start_stream`) — then hide the pill. The discarded
/// transcript is intentionally dropped.
pub(crate) fn cancel_session(app: &AppHandle) {
    crate::extension_session::cancel(app);
    crate::master_key::unregister_chords(app);
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

// Prompt switcher — cycles the active post-processing prompt and shows
// the new title in the pill. A tap shortcut: the switch happens on press.
struct PromptSwitchAction {
    delta: i32,
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

// Master chord Alt+1 — Prompt Record. Exactly the pill-click path: arm
// the audio-mark split on the recording manager, then echo
// `PromptRecordingChanged` so the pill turns blue only once the mark is real.
// Arming is a no-op when not recording or already armed (one-way per session).
struct MasterPromptRecordAction;

impl ShortcutAction for MasterPromptRecordAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let rm = app.state::<Arc<AudioRecordingManager>>();
        if rm.arm_prompt_record() {
            crate::bridge::emit(
                app,
                DaemonEvent::PromptRecordingChanged {
                    session_id: current_session_id(),
                    active: true,
                },
            );
        }
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

// Grain Space quick add (Input C) — a tap shortcut that silently saves
// the current selection as a raw note. All work happens off the input thread
// inside `grain_space::capture::quick_add` (selection grab polls the clipboard).
struct GrainSpaceQuickAddAction;

impl ShortcutAction for GrainSpaceQuickAddAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::grain_space::capture::quick_add(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] There is no `grain_space_open` action any more. It toggled a second
// frameless notes window; the notebook is the Notes tab of the main window now,
// so "open my notes" is opening Grain. Removing a global chord that duplicated
// the app's own front door is part of the shortcut-bloat cleanup
// (NOTES-TAB-PLAN.md Phase E).

// Grain Recall (RECALL-PLAN R1) — summons the Agent surfaces in memory
// mode (ask your notes, get an answer). Its OWN binding, distinct from
// summon_agent: the mode is fixed by which key fired, never guessed.
//
// [GRAIN] The Agent can now do this from its own door too — it carries the
// notebook as tools (`grain_space::agent_tools`), so "what did I note about X"
// works from the single summon chord. This binding survives because retiring it
// means retiring `AgentMode::Recall`, and that cascade runs through the pill's
// submit flow; see NOTES-TAB-PLAN.md Phase E.
struct GrainSpaceRecallAction;

impl ShortcutAction for GrainSpaceRecallAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon_memory(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Grain Space note capture — summons the Agent surfaces in Capture mode:
// speak OR type a note (and any selected text comes along as the body), then it
// is structured and saved. Its OWN binding; mode fixed here.
//
// [GRAIN] Also reachable from the Agent's `save_note` tool now. The difference is
// real, which is why this stayed: Capture confirms the save IN the pill card and
// never opens a panel, where the tool path answers in the reply panel. Collapsing
// the two is a UX decision, not a refactor.
struct GrainSpaceCaptureAction;

impl ShortcutAction for GrainSpaceCaptureAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon_capture(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Real-time rolling-window transcribe action. Streams audio through the
// rolling engine in the background (no partial display); pastes the assembled
// transcript on stop, with a batch fallback if rolling yields nothing.
struct RealtimeTranscribeAction {
    post_process_override: AtomicBool,
}

impl ShortcutAction for RealtimeTranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rt = Arc::clone(&app.state::<Arc<crate::rolling::RollingTranscriber>>());
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

        // Session id up front so the (optional) live-preview worker can
        // tag its AsrStreamText events with the same id the RecordingStarted
        // event below carries. Preview is opt-in; when off the worker takes the
        // zero-overhead path and no preview events fire.
        let preview = get_settings(app).rolling_live_preview;
        let sid = next_session_id();
        let rolling_started = rt.start_session(app.clone(), sid, preview);
        // Register the new session generation before the asynchronous model
        // load begins. A cancelled predecessor can finish cleanup concurrently,
        // but can no longer unload the model underneath this recording.
        {
            let app = app.clone();
            let rt = rt.clone();
            std::thread::spawn(move || {
                if let Err(e) = rt.ensure_loaded(&app) {
                    warn!("[GRAIN] rolling model load failed: {e}");
                }
            });
        }
        {
            let rm = Arc::clone(&rm);
            std::thread::spawn(move || {
                let _ = rm.preload_vad();
            });
        }

        change_tray_icon(app, TrayIconState::Recording);
        // C1: no Handy webview overlay on the real-time path — the winit
        // pill is the only surface, driven by the DaemonEvents below.

        let binding_id = binding_id.to_string();
        let is_always_on = get_settings(app).always_on_microphone;
        // Rolling receives EVERY frame via the sample callback no matter
        // the policy; the policy shapes the batch-fallback buffer and gives the
        // rolling cursor its per-frame voice decisions. Offline profile — the
        // `vad_enabled` toggle was never ported into grain-core settings.
        let vad_policy = VadPolicy::Offline;
        let mut recording_error: Option<String> = None;
        if is_always_on {
            let rm_mute = Arc::clone(&rm);
            let app2 = app.clone();
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app2, SoundType::Start);
                rm_mute.apply_mute();
            });
            let start = if rolling_started {
                rm.try_start_recording_low_ram(&binding_id, vad_policy)
            } else {
                rm.try_start_recording(&binding_id, vad_policy)
            };
            if let Err(e) = start {
                recording_error = Some(e);
            }
        } else {
            let start = if rolling_started {
                rm.try_start_recording_low_ram(&binding_id, vad_policy)
            } else {
                rm.try_start_recording(&binding_id, vad_policy)
            };
            match start {
                Ok(()) => {
                    let app2 = app.clone();
                    let rm = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        play_feedback_sound_blocking(&app2, SoundType::Start);
                        rm.apply_mute();
                    });
                }
                Err(e) => recording_error = Some(e),
            }
        }

        if recording_error.is_none() {
            // With the live preview on, use the Studio Window (NativeAsr) so the
            // growing caption has room; otherwise the compact dictation pill.
            emit_session_started(
                app,
                sid,
                if preview {
                    SessionMode::NativeAsr
                } else {
                    SessionMode::Dictation
                },
            );
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
        shortcut::unregister_cancel_shortcut(app);
        unregister_session_shortcuts(app);
        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
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
        let post_process = self.post_process_override.load(Ordering::Relaxed);
        // Snapshot before the stop so a cancel landing during the extra
        // recording buffer (or later, mid-pipeline) is observed.
        let cancel_generation = rm.cancel_generation();
        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());

            // Empty on the normal rolling path: its PCM16 journal owns audio.
            // A Vec is retained only if journal creation failed and recording
            // deliberately fell back to the legacy capture contract.
            let samples = rm
                .stop_recording(&binding_id, cancel_generation)
                .unwrap_or_default();
            // Prompt Record mark (the pill-click / Alt+1 split point),
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
            let (final_text, spoken_prompt, post_process) = if let Some(m) =
                prompt_mark.filter(|&m| m > 0 && m < audio_len)
            {
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
                let content = rolling_window::canonicalize_text(&content_res.unwrap_or_default());
                (content, spoken.clone(), post_process || spoken.is_some())
            } else {
                let assembled = !rolling_text.trim().is_empty();
                let ft = if assembled {
                    // Apply the shared final-text stage (custom-word dictionary
                    // + filler/stutter filtering) ONCE on the assembled transcript.
                    // The rolling engine never biases via Whisper `initial_prompt`, so
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
                } else if audio_len > 0 {
                    warn!("[GRAIN] rolling produced no text — falling back to batch");
                    // `tm.transcribe` already runs finalize_transcript internally, so
                    // the fallback text is finalized; don't finalize it again.
                    let fallback_audio = match rolling.as_ref() {
                        Some(output) => output.materialize_audio().unwrap_or_else(|error| {
                            error!("Failed to read rolling fallback audio: {error}");
                            Vec::new()
                        }),
                        None => samples.clone(),
                    };
                    tm.transcribe(fallback_audio).unwrap_or_default()
                } else {
                    String::new()
                };
                (rolling_window::canonicalize_text(&ft), None, post_process)
            };

            let processed =
                process_transcription_output(&ah, &final_text, post_process, spoken_prompt).await;
            let final_text = processed.final_text;

            if audio_len > 0 {
                let file_name = format!("grain-{}.wav", chrono::Utc::now().timestamp());
                let wav_path = hm.recordings_dir().join(&file_name);
                if let Some(output) = rolling.as_ref() {
                    let output = output.clone();
                    let result =
                        tauri::async_runtime::spawn_blocking(move || output.save_wav(&wav_path))
                            .await;
                    if let Ok(Err(error)) = result {
                        error!("Failed to save rolling journal WAV: {error}");
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
                    final_text.clone(),
                    post_process,
                    processed.post_processed_text.clone(),
                    processed.post_process_prompt.clone(),
                ) {
                    error!("Failed to save history entry: {e}");
                }
            }

            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste(final_text, ah_clone.clone()) {
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

            // B2: processing finished → pill hides.
            emit_processing_complete(&ah, session_id);
        });
    }
}

// Native ASR — push-to-talk live streaming, on the SAME unified
// TranscriptionManager engine as Batch/Rolling: the shortcut loads the selected
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
        shortcut::unregister_cancel_shortcut(app);
        // Release the master chords (and the switcher, if open).
        crate::master_key::unregister_chords(app);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
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
            let _guard = FinishGuard(ah.clone());

            // The mic frames already reached the stream worker live; keep the
            // captured samples only as the batch-fallback input (mirrors Handy:
            // a model that turned out not to stream still yields a transcript).
            let samples = rm
                .stop_recording(&binding_id, cancel_generation)
                .unwrap_or_default();
            // Prompt Record split mark (a pill click on the Studio waveform).
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

            let final_text = if let Some(m) = prompt_mark.filter(|&m| m > 0 && m < samples.len()) {
                // Prompt Record on the streaming path: the live transcript
                // covered content + the spoken instruction together, so it can't be
                // split. Re-transcribe the two audio slices and post-process the
                // content with the spoken instruction (AI forced on, regardless of
                // which shortcut stopped the session). `process_transcription_output`
                // also runs voice actions on the content.
                let (content_res, spoken) =
                    crate::prompt_record::transcribe_split(&ah, samples.clone(), Some(m)).await;
                let content = content_res.unwrap_or_default();
                let processed = process_transcription_output(&ah, &content, true, spoken).await;
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
                ft
            } else {
                if finalized.trim().is_empty() {
                    String::new()
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
                    finalized
                }
            };

            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste(final_text, ah_clone.clone()) {
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
    // Real-time rolling-window transcription.
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
    // Master chords (transiently registered by `master_key` while a
    // recording session is live) + the switcher's transient arrow keys.
    map.insert(
        "master_prompt_record".to_string(),
        Arc::new(MasterPromptRecordAction) as Arc<dyn ShortcutAction>,
    );
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
    // Grain Space bindings. Quick add is the one the Agent's note tools cannot
    // replace: no UI, no model, no wait — and it now ships unbound, because a
    // global chord is scarce and a feature that is off by default should not hold
    // one before the user asks. All register only while `grain_space_enabled` is on.
    map.insert(
        "grain_space_quick_add".to_string(),
        Arc::new(GrainSpaceQuickAddAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "grain_space_capture".to_string(),
        Arc::new(GrainSpaceCaptureAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "grain_space_recall".to_string(),
        Arc::new(GrainSpaceRecallAction) as Arc<dyn ShortcutAction>,
    );
}
