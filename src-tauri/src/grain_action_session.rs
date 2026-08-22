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
use crate::grain_events::{ExtensionRecommendation, RecommendationCandidate};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tauri_specta::Event as _;

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
    // Gate on the POOL, not on declared actions. A searchable extension may have
    // a `recommend` block and no command catalogue at all (a translator, §3.1);
    // gating on `action_vocabulary` here would refuse to start for exactly that
    // case, which is the one the whole "recommend exists with zero commands"
    // design is built around.
    if crate::extension_host::searchable_count() == 0 {
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

    // Warm the embedder on the keypress (§6): the recommendation will need it, and
    // starting the load now hides it behind the recording. A no-op when the model
    // is absent (name-only mode) or already resident. The TTL reclaims it if the
    // session leads nowhere.
    crate::grain_space::embed::touch_extension_mode(app);

    // Bias the recogniser with what the installed extensions actually say,
    // before a single sample is captured. Free, because the phrases are already
    // in the index. Empty for a translator with no actions — nothing to bias,
    // which is fine; the name is what the recogniser most needs to get right and
    // that comes from the aliases the pool already carries.
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

/// The request currently on the recommendation surface, and which extensions
/// the user has already declined for it.
///
/// Held between [`deliver`] and [`decline`]/[`accept`] because those are
/// separate user actions on the surface (behind the §6b gate), and the reopened
/// chooser must rank the *same* request without re-recording it (G2). Cleared
/// when the request is accepted, declined into nothing, or superseded by a new
/// capture.
#[derive(Clone, Default)]
struct Pending {
    request: String,
    declined: Vec<String>,
}

fn pending() -> &'static Mutex<Option<Pending>> {
    static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Rank the searchable extensions for one captured request and present it.
///
/// Split from [`finish`] so it can be driven from a test or a replay of the
/// action log without a microphone.
///
/// **The V1-P1 seam.** Ranking, the recommendation event, and decline-and-reopen
/// all run here and in [`decline`]. What does NOT run yet is the surface (behind
/// the §6b design gate) and the accepted hand-off into the extension's request
/// API (V1-P2, where the extension-facing side of the same wire contract is
/// defined — see [`accept`]). So today this ranks, emits the event a surface
/// will consume, and records — capture and ranking work end to end, and the
/// request is carried in the event ready to be handed off.
pub async fn deliver(app: &AppHandle, heard: &str) {
    *pending().lock().unwrap() = Some(Pending {
        request: heard.to_string(),
        declined: Vec::new(),
    });
    present(app, heard.to_string(), Vec::new()).await;
}

/// Rank `request` (excluding `declined`), emit the recommendation event, and
/// record the outcome. The one place ranking is turned into a surface event, so
/// deliver and decline present identically.
async fn present(app: &AppHandle, request: String, declined: Vec<String>) {
    // Each ranking is a semantic use — refresh the warmth so an accept or a
    // decline-and-reopen right after does not race the reaper (§6).
    crate::grain_space::embed::touch_extension_mode(app);
    // `recommend` embeds the query and is blocking; keep it off the async
    // runtime's poll threads.
    let ranked = {
        let request = request.clone();
        let declined = declined.clone();
        tauri::async_runtime::spawn_blocking(move || {
            (
                crate::extension_host::recommend(&request, &declined),
                crate::extension_host::semantic_available(),
            )
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), false))
    };
    let (ranked, semantic_available) = ranked;

    let candidates: Vec<RecommendationCandidate> = ranked
        .iter()
        .map(|r| {
            let (name, purpose) =
                crate::extension_host::recommendation_display(app, &r.extension_id)
                    .unwrap_or_else(|| (r.extension_id.clone(), String::new()));
            RecommendationCandidate {
                extension_id: r.extension_id.clone(),
                name,
                purpose,
                signal: match r.signal {
                    grain_core::recommend::Signal::Named => "named",
                    grain_core::recommend::Signal::Topical => "topical",
                }
                .to_string(),
                score: r.score,
            }
        })
        .collect();

    // The surface consumes this; the backend draws nothing. Empty candidates is
    // the "nothing matched" state, not a separate event.
    let _ = app.emit(
        ExtensionRecommendation::NAME,
        ExtensionRecommendation {
            request: request.clone(),
            candidates: candidates.clone(),
            name_only: !semantic_available,
        },
    );

    match ranked.first() {
        None => {
            action_log::record(
                &request,
                None,
                None,
                None,
                ActionLogOutcome::Refused {
                    reason: "nothing matched".into(),
                },
            );
        }
        Some(top) => {
            log::info!(
                "[GRAIN] extension mode: top recommendation {} ({:?}, {:.3})",
                top.extension_id,
                top.signal,
                top.score
            );
            action_log::record(
                &request,
                Some(top.extension_id.clone()),
                None,
                Some(top.score),
                // Escalated in the log's vocabulary: it reached the pool and
                // produced a recommendation, which is neither a run nor a
                // refusal. The dedicated accepted/declined outcomes arrive with
                // the surface and the hand-off.
                ActionLogOutcome::Escalated,
            );
        }
    }
}

/// The user declined an extension on the surface; reopen the chooser with it
/// struck out (G2). One keypress, not one re-recording — the request is the one
/// [`deliver`] already captured.
///
/// Called by the surface (behind the §6b gate); no-op if there is no pending
/// request or it does not match, so a stale click after a new capture does
/// nothing.
pub async fn decline(app: &AppHandle, request: &str, extension_id: &str) {
    let declined = {
        let mut slot = pending().lock().unwrap();
        let Some(p) = slot.as_mut() else {
            return;
        };
        if p.request != request {
            return;
        }
        if !p.declined.iter().any(|id| id == extension_id) {
            p.declined.push(extension_id.to_string());
        }
        p.declined.clone()
    };
    present(app, request.to_string(), declined).await;
}

/// The user accepted an extension: hand it the full request
/// (`docs/Extensions V1/PLAN.md` §3).
///
/// The whole transcript goes to the extension, which owns what happens next —
/// interpretation, any `match.*` ranking, clarification, the result. Grain wakes
/// it, delivers the request, and records the outcome; it resolves nothing first.
///
/// Spawned rather than awaited because the caller is a Tauri command that must
/// return at once — the extension may call a model or the network, which is
/// exactly what [`crate::extension_host::hand_off`]'s generous deadline is for.
pub fn accept(app: &AppHandle, extension_id: &str) {
    let request = {
        let mut slot = pending().lock().unwrap();
        match slot.take() {
            Some(p) => p.request,
            None => return,
        }
    };
    log::info!("[GRAIN] extension mode: accepted {extension_id}");
    action_log::record(
        &request,
        Some(extension_id.to_string()),
        None,
        None,
        ActionLogOutcome::Chose,
    );

    let app = app.clone();
    let extension_id = extension_id.to_string();
    tauri::async_runtime::spawn(async move {
        use crate::extension_host::HandOffOutcome;
        match crate::extension_host::hand_off(&app, &extension_id, &request).await {
            HandOffOutcome::Done(message) => {
                log::info!(
                    "[GRAIN] extension mode: {extension_id} handled the request{}",
                    message
                        .as_deref()
                        .map(|m| format!(" — {m}"))
                        .unwrap_or_default()
                );
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Ran { confirmed: false },
                );
            }
            HandOffOutcome::Failed(reason) => {
                log::warn!("[GRAIN] extension mode: {extension_id} failed — {reason}");
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Failed { reason },
                );
            }
            // Reported as its own thing, never as failure: for anything that
            // left the machine a timeout does not mean it did not happen.
            HandOffOutcome::Unknown => {
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Unknown,
                );
            }
        }
    });
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
