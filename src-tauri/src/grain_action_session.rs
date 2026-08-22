//! [GRAIN] The Extension Mode session (`docs/Extensions V1/PLAN.md` §3).
//!
//! A host-owned sibling of [`crate::extension_session`]: it opens a recording
//! through the same coordinator, owns its own finish, and **never enters
//! Handy's `process_transcription_output`**.
//!
//! # What it deliberately does not do
//!
//! What comes out of here is **raw ASR output**. Explicit non-goals, each of
//! which would be a bug rather than a missing feature:
//!
//! - no `post_process_transcription` — it is an **LLM call**, so passing the
//!   request through it would break the "no model in the common path" promise
//!   before ranking even started, and blow the latency budget by an order of
//!   magnitude;
//! - no prompt stack, no contributed prompt layers, no context-aware
//!   formatting — all of which rewrite wording for a *text field*;
//! - no transforms, no snippets, no rolling repair, no scrap-that;
//! - no paste, and no dictation history. "Tell Jack I'm late" is a request,
//!   not something the user dictated, and filing it in their transcript history
//!   would be surprising. The action log is where it is recorded.
//!
//! # Where the request goes
//!
//! Under V1 the host does not resolve the request. It ranks which **extension**
//! should own it, the user accepts or corrects that, and the extension receives
//! the whole original transcript — see `docs/Extensions V1/PLAN.md` §3. That
//! ranking lands in V1-P1 and the surface in V1-P2b; until then [`deliver`] is
//! the single seam where it attaches, and it records what was heard so the
//! capture half can be exercised on its own.
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
    /// session, or a request still being processed. One recording at a time is
    /// a hard singleton and this inherits it by going through the same
    /// coordinator rather than reimplementing the rule.
    Busy,
    /// Nothing installed can receive a request.
    NothingInstalled,
    Unavailable(String),
}

/// Begin listening for a request.
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

    // Bias the recogniser with what the installed extensions actually say,
    // before a single sample is captured. Free, because the phrases are already
    // in the index — and it is the highest-leverage thing in this file, since
    // the words that identify an extension are exactly the ones ASR mangles.
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
    // Prompt Record are mid-dictation tools; mid-request they are meaningless at
    // best.
    log::info!("[GRAIN] action: listening");
    Ok(())
}

/// Stop listening and hand over what was said. Idempotent.
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

/// Abandon the session without handing anything over.
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

/// Whether an Extension Mode session owns the microphone right now.
///
/// Kept here because the busy rule belongs next to the thing that enforces it,
/// not scattered across callers.
#[allow(dead_code)]
pub fn is_active() -> bool {
    active().lock().unwrap().is_some()
}

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

    deliver(&app, &heard).await;
    complete(session.generation);
}

/// Hand one captured request onward.
///
/// Split from [`finish`] so it can be driven from a test or a replay of the
/// action log without a microphone.
///
/// **The V1-P1 seam.** Ranking which extension should own this, the accept /
/// correct step, and the hand-off itself all attach here. Until they land the
/// request is recorded and goes no further — which is the honest behaviour, not
/// a stub: capture works, and nothing is delivered to an extension that has not
/// been ranked.
pub async fn deliver(_app: &AppHandle, heard: &str) {
    action_log::record(
        heard,
        None,
        None,
        None,
        ActionLogOutcome::Refused {
            reason: "nothing installed can do that".into(),
        },
    );
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
