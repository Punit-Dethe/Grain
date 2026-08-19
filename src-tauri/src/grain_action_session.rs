//! [GRAIN] The action session (`docs/Action Routing/PLAN.md` §3.2).
//!
//! A host-owned sibling of [`crate::extension_session`]: it opens a recording
//! through the same coordinator, owns its own finish, and **never enters
//! Handy's `process_transcription_output`**.
//!
//! # What it deliberately does not do
//!
//! The router consumes **raw ASR output**. Explicit non-goals, each of which
//! would be a bug rather than a missing feature:
//!
//! - no `post_process_transcription` — it is an **LLM call**, so routing
//!   through it would break the "no model in the common path" promise before
//!   routing even started, and blow the latency budget by an order of
//!   magnitude;
//! - no prompt stack, no contributed prompt layers, no context-aware
//!   formatting — all of which rewrite wording for a *text field*;
//! - no transforms, no snippets, no rolling repair, no scrap-that;
//! - no paste, and no dictation history. "Tell Jack I'm late" is a command,
//!   not something the user dictated, and filing it in their transcript history
//!   would be surprising. The action log (§8.3) is where it is recorded.
//!
//! # Invocation
//!
//! Started by [`start`], which is called from the extension surface's own
//! trigger. **This module creates no shortcut binding** — what it needs is only
//! that the user's intent was unambiguous by the time audio started.

use crate::audio_toolkit::VadPolicy;
use crate::grain_actions::action_log::{self, ActionLogOutcome};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use grain_core::action_decision::{Outcome, Preferences, RefuseReason, Selection};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Recording,
    Processing,
}

#[derive(Debug, Clone)]
struct ActiveSession {
    generation: u64,
    binding_id: String,
    phase: Phase,
}

fn active() -> &'static Mutex<Option<ActiveSession>> {
    static ACTIVE: OnceLock<Mutex<Option<ActiveSession>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// Something else already owns the microphone — a dictation, an extension
    /// session, or an action still being processed. One recording at a time is
    /// a hard singleton and this inherits it by going through the same
    /// coordinator rather than reimplementing the rule.
    Busy,
    /// Nothing installed declares an action, so there is nothing to route to.
    NothingInstalled,
    Unavailable(String),
}

/// Begin listening for an action.
pub fn start(app: &AppHandle) -> Result<(), StartError> {
    if crate::extension_host::action_vocabulary().is_empty() {
        return Err(StartError::NothingInstalled);
    }
    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let transcription = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    let mut slot = active().lock().unwrap();
    if slot.is_some() || recording.is_recording() {
        return Err(StartError::Busy);
    }

    if !crate::stt_router::will_route_to_cloud(app) {
        transcription.initiate_model_load();
    }

    // Bias the recogniser with what the installed actions actually say, before
    // a single sample is captured. Free, because the phrases are already in the
    // index — and it is the highest-leverage thing in this file, since the words
    // that identify an action are exactly the ones ASR mangles.
    crate::context_bias::arm_action_session(crate::extension_host::action_vocabulary());

    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    let binding_id = format!("grain-action:{generation}");
    recording
        .try_start_recording(&binding_id, VadPolicy::Offline)
        .map_err(|error| {
            if error == "Already recording" {
                StartError::Busy
            } else {
                StartError::Unavailable(error)
            }
        })?;

    *slot = Some(ActiveSession {
        generation,
        binding_id,
        phase: Phase::Recording,
    });
    drop(slot);
    // NOTE: master chords are deliberately NOT armed. The prompt switcher and
    // Prompt Record are mid-dictation tools; mid-action they are meaningless at
    // best.
    log::info!("[GRAIN] action: listening");
    Ok(())
}

/// Stop listening and route what was said. Idempotent.
pub fn stop(app: &AppHandle) {
    let snapshot = {
        let mut slot = active().lock().unwrap();
        let Some(session) = slot.as_mut() else {
            return;
        };
        if session.phase != Phase::Recording {
            return;
        }
        session.phase = Phase::Processing;
        session.clone()
    };

    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let cancel_generation = recording.cancel_generation();
    recording.remove_mute();

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        finish(app, recording, snapshot, cancel_generation).await;
    });
}

/// Abandon the session without routing anything.
pub fn cancel(app: &AppHandle) -> bool {
    if active().lock().unwrap().take().is_none() {
        return false;
    }
    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    recording.cancel_recording();
    recording.remove_mute();
    log::info!("[GRAIN] action: cancelled");
    true
}

/// Whether an action session owns the microphone right now.
///
/// Consumed by the pill and the chooser in P2; kept here because the busy rule
/// belongs next to the thing that enforces it, not scattered across callers.
#[allow(dead_code)]
pub fn is_active() -> bool {
    active().lock().unwrap().is_some()
}

/// How many declines make an action suspect.
///
/// Repeatedly offering something the user repeatedly walks away from is the
/// clearest signal the ranking has it wrong. For now this only says so in the
/// log — quarantining an action is a routing change and belongs with the
/// chooser that produces the signal honestly.
const SUSPICIOUS_DECLINES: usize = 3;

async fn finish(
    app: AppHandle,
    recording: Arc<AudioRecordingManager>,
    session: ActiveSession,
    cancel_generation: u64,
) {
    let Some(samples) = recording.stop_recording(&session.binding_id, cancel_generation) else {
        complete(session.generation);
        return;
    };
    if samples.is_empty() || recording.was_cancelled_since(cancel_generation) {
        complete(session.generation);
        return;
    }

    // Raw ASR. `transcribe_split` with no mark is the plain path — no
    // post-processing, no prompt stack, nothing that rewrites wording for a
    // text field.
    let (transcription, _) = crate::prompt_record::transcribe_split(&app, samples, None).await;
    let heard = match transcription {
        Ok(text) => text,
        Err(error) => {
            log::error!("[GRAIN] action: transcription failed: {error}");
            action_log::record(
                "",
                None,
                None,
                None,
                None,
                ActionLogOutcome::Refused {
                    reason: error.to_string(),
                },
            );
            complete(session.generation);
            return;
        }
    };
    if recording.was_cancelled_since(cancel_generation) {
        complete(session.generation);
        return;
    }

    route(&app, &heard).await;
    complete(session.generation);
}

/// Route one utterance and act on the decision.
///
/// Split from [`finish`] so it can be driven from a test or a replay of the
/// action log without a microphone.
pub async fn route(app: &AppHandle, heard: &str) {
    // "…on Spotify" names a provider and is removed before anything is matched,
    // so the name never lands in a span. A bare name mid-utterance stays an
    // entity — "play Spotify Wrapped" is a request, not a routing hint.
    let providers = crate::extension_host::action_provider_names(app);
    let (utterance, named_provider) =
        grain_core::action_decision::split_named_provider(heard, &providers);

    let preferences = Preferences {
        default_provider: named_provider
            .map(|id| {
                // An explicitly named provider is the top rung, and it outranks
                // every stored default — so it is applied by pinning every
                // domain to it rather than by a separate code path that could
                // disagree with the ladder.
                crate::extension_host::action_domains()
                    .into_iter()
                    .map(|domain| (domain, id.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        // Rungs 2-4 (stored defaults, the learned offer, the foreground
        // tie-break) arrive with the chooser; until then every genuine
        // ambiguity is asked about, which is the safe end of that ladder.
        foreground_extension: None,
        agent_available: false,
    };

    let Some(outcome) = crate::extension_host::route_action(&utterance, &preferences) else {
        action_log::record(
            heard,
            None,
            None,
            None,
            None,
            ActionLogOutcome::Refused {
                reason: "nothing installed can do that".into(),
            },
        );
        return;
    };

    match outcome {
        Outcome::Execute(selection) => {
            if selection.needs_confirmation() {
                // The read-back is not built yet, and running the action anyway
                // would be exactly the failure this tier exists to prevent: an
                // ASR substitution reverses intent, scores well, and sends the
                // wrong message. Refusing is the honest degradation.
                log::info!(
                    "[GRAIN] action: '{}' needs a read-back, which is not wired yet — not running it",
                    selection.action_id
                );
                log_selection(
                    heard,
                    &selection,
                    ActionLogOutcome::Refused {
                        reason: "this action asks before running, and that step is not ready"
                            .into(),
                    },
                );
                return;
            }
            execute(app, heard, selection).await;
        }
        Outcome::Choose { options, reason } => {
            // The chooser is P2. Until it exists an ambiguity is declined, not
            // guessed — picking the top candidate here would quietly convert
            // every "ask" into an execution and undo the whole decision layer.
            log::info!(
                "[GRAIN] action: ambiguous ({reason:?}) between {} option(s) — waiting for the chooser",
                options.len()
            );
            let first = options.first();
            if let Some(top) = first {
                let qualified = format!("{}:{}", top.extension_id, top.action_id);
                let declines = action_log::declines(&qualified);
                if declines >= SUSPICIOUS_DECLINES {
                    log::warn!(
                        "[GRAIN] action: '{qualified}' has been offered and declined {declines} \
                         times — its declared phrasings are probably too broad"
                    );
                }
            }
            action_log::record(
                heard,
                first.map(|s| format!("{}:{}", s.extension_id, s.action_id)),
                first.map(|s| s.title.clone()),
                first.map(|s| s.domain.clone()),
                first.map(|s| s.score),
                ActionLogOutcome::Cancelled,
            );
        }
        Outcome::Escalate(options) => {
            action_log::record(
                heard,
                None,
                None,
                None,
                options.first().map(|s| s.score),
                ActionLogOutcome::Escalated,
            );
        }
        Outcome::Refuse(reason) => {
            action_log::record(
                heard,
                None,
                None,
                None,
                None,
                ActionLogOutcome::Refused {
                    reason: describe(reason).into(),
                },
            );
        }
    }
}

async fn execute(app: &AppHandle, heard: &str, selection: Selection) {
    use crate::extension_host::ActionOutcome;

    crate::extension_host::wake_for_action(app, &selection.extension_id, &selection.action_id);
    let outcome = crate::extension_host::perform_action(
        app,
        &selection.extension_id,
        &selection.action_id,
        &selection.spans,
    )
    .await;

    let logged = match outcome {
        ActionOutcome::Done(message) => {
            log::info!(
                "[GRAIN] action: ran {}:{}{}",
                selection.extension_id,
                selection.action_id,
                message
                    .as_deref()
                    .map(|m| format!(" — {m}"))
                    .unwrap_or_default()
            );
            ActionLogOutcome::Ran { confirmed: false }
        }
        ActionOutcome::Ambiguous { param, options } => {
            log::info!(
                "[GRAIN] action: '{param}' resolved to {} candidates — waiting for the chooser",
                options.len()
            );
            ActionLogOutcome::Cancelled
        }
        ActionOutcome::Failed(reason) => {
            log::warn!("[GRAIN] action: failed — {reason}");
            ActionLogOutcome::Failed { reason }
        }
        // Reported as its own thing, never as failure. For anything that leaves
        // the machine a timeout does not mean it did not happen.
        ActionOutcome::Unknown => ActionLogOutcome::Unknown,
    };
    log_selection(heard, &selection, logged);
}

fn log_selection(heard: &str, selection: &Selection, outcome: ActionLogOutcome) {
    action_log::record(
        heard,
        Some(format!(
            "{}:{}",
            selection.extension_id, selection.action_id
        )),
        Some(selection.title.clone()),
        Some(selection.domain.clone()),
        Some(selection.score),
        outcome,
    );
}

fn describe(reason: RefuseReason) -> &'static str {
    match reason {
        RefuseReason::NothingHeard => "nothing was heard",
        RefuseReason::NothingInstalledCanDoThat => "nothing installed can do that",
        RefuseReason::NeedsAgentButNoneConfigured => {
            "that needs the assistant, which is not set up"
        }
    }
}

fn complete(generation: u64) {
    let mut slot = active().lock().unwrap();
    // A newer session already owns the slot; this one finished late and must not
    // clear it. Same generation guard as `extension_session`, and for the same
    // reason: without it a slow transcription can unlock a microphone that
    // something else has since claimed.
    if slot.as_ref().is_some_and(|s| s.generation == generation) {
        *slot = None;
    }
}
