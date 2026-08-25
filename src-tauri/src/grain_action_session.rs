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
//! Started by [`start`], which is called by Grain's dedicated, persisted
//! `extension_mode` shortcut (and remains callable through the Tauri command for
//! trusted surfaces). The separate trigger makes the disclosure boundary
//! unambiguous before audio starts.

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
    bias_generation: u64,
    pill_session_id: u64,
    pill_completed: bool,
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
    if is_active() || recording.is_recording() || crate::extension_session::is_active() {
        return Err(StartError::Busy);
    }

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

    // Only acquire model resources after the microphone reservation succeeds.
    // Their startup still hides behind capture, without warming anything for a
    // losing/failed start attempt.
    if !crate::stt_router::will_route_to_cloud(app) {
        transcription.initiate_model_load();
    }
    crate::grain_space::embed::touch_extension_mode(app);

    // Bias the recogniser with what the installed extensions actually say.
    // Publish only after the recorder is successfully reserved so a failed
    // start cannot leak this vocabulary into the next unrelated dictation.
    let bias_generation =
        crate::context_bias::arm_action_session(crate::extension_host::action_vocabulary());

    // Only retire an older chooser once this capture has successfully claimed
    // the microphone. `try_start_recording` is the singleton reservation, so no
    // second starter can enter between this point and publishing `active`.
    supersede(app);
    crate::extension_view::capture_output_target(app);
    let pill_session_id = crate::grain_actions::extension_mode_started(app);

    let mut slot = active().lock().unwrap();
    *slot = Some(ActiveSession {
        generation,
        binding_id,
        bias_generation,
        pill_session_id,
        pill_completed: false,
        phase: Phase::Recording,
    });
    drop(slot);
    crate::shortcut::register_cancel_shortcut(app);
    // Grain's powerless standard renderer warms behind capture and is destroyed
    // on every path that never presents extension UI.
    if let Err(error) = crate::extension_view::warm(app, pill_session_id) {
        log::warn!("[GRAIN] extension view: warm failed: {error}");
        cancel_if_pill_session(app, pill_session_id);
        return Err(StartError::Unavailable(format!(
            "Could not prepare Extension Mode: {error}"
        )));
    }
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
    crate::grain_actions::emit_recording_stopped(app);
    if let Err(error) = crate::extension_view::present_routing(app, snapshot.pill_session_id) {
        log::warn!("[GRAIN] extension mode: could not show routing surface: {error}");
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        finish(app, recording, snapshot, cancel_generation).await;
    });
}

/// Abandon the session without handing anything over.
pub fn cancel(app: &AppHandle) -> bool {
    let Some(session) = active().lock().unwrap().take() else {
        return false;
    };
    cancel_owned(app, session)
}

fn cancel_owned(app: &AppHandle, session: ActiveSession) -> bool {
    let recording = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    recording.cancel_recording();
    recording.remove_mute();
    crate::context_bias::clear_action_session(session.bias_generation);
    crate::shortcut::unregister_cancel_shortcut(app);
    crate::extension_view::destroy(app);
    crate::bridge::emit(
        app,
        grain_core::DaemonEvent::SessionCancelled {
            session_id: session.pill_session_id,
        },
    );
    log::info!("[GRAIN] action: cancelled");
    true
}

/// Cancel only the action session that owns this native pill. Renderer jobs are
/// queued on Tauri's UI thread, so their failure can arrive after cancellation
/// or even after another capture starts; it must never cancel the newer owner.
pub fn cancel_if_pill_session(app: &AppHandle, pill_session_id: u64) -> bool {
    let session = {
        let mut slot = active().lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|session| session.pill_session_id == pill_session_id)
        {
            slot.take()
        } else {
            None
        }
    };
    session.is_some_and(|session| cancel_owned(app, session))
}

pub fn owns_pill_session(pill_session_id: u64) -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|session| session.pill_session_id == pill_session_id)
}

/// Whether an Extension Mode session owns the microphone right now.
///
/// Kept here because the busy rule belongs next to the thing that enforces it,
/// not scattered across callers.
pub fn is_active() -> bool {
    active().lock().unwrap().is_some()
}

/// Whether the dedicated shortcut is currently capturing audio. Toggle mode
/// uses this to distinguish its second press from a busy processing session.
pub fn is_recording() -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|session| session.phase == Phase::Recording)
}

fn owns_generation(generation: u64) -> bool {
    active()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|session| session.generation == generation)
}

async fn finish(
    app: AppHandle,
    recording: Arc<AudioRecordingManager>,
    session: ActiveSession,
    cancel_generation: u64,
) {
    let Some(samples) = recording.stop_recording(&session.binding_id, cancel_generation) else {
        crate::context_bias::clear_action_session(session.bias_generation);
        crate::extension_view::destroy(&app);
        complete(&app, &session);
        return;
    };
    if samples.is_empty() || recording.was_cancelled_since(cancel_generation) {
        crate::context_bias::clear_action_session(session.bias_generation);
        crate::extension_view::destroy(&app);
        complete(&app, &session);
        return;
    }

    // Raw ASR. `transcribe_split` with no mark is the plain path — no
    // post-processing, no prompt stack, nothing that rewrites wording for a
    // text field.
    let (transcription, _) = crate::prompt_record::transcribe_split(&app, samples, None).await;
    // Usually consumed by the transcription path itself; also covers an early
    // decoder failure before that one-shot bias was read.
    crate::context_bias::clear_action_session(session.bias_generation);
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
            crate::extension_view::destroy(&app);
            complete(&app, &session);
            return;
        }
    };
    if recording.was_cancelled_since(cancel_generation) || !owns_generation(session.generation) {
        crate::extension_view::destroy(&app);
        complete(&app, &session);
        return;
    }

    deliver(&app, &heard).await;
    complete(&app, &session);
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
    id: u64,
    presentation_id: u64,
    request: String,
    declined: Vec<String>,
    /// Extension ids on the currently presented chooser. The reverse channel is
    /// authenticated, but it is still an input boundary: an old or malformed
    /// client must not hand the transcript to an enabled non-searchable pack.
    allowed: Vec<String>,
}

fn pending() -> &'static Mutex<Option<Pending>> {
    static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Monotonic request epoch. Starting any new capture bumps it, which prevents a
/// late ranking or a late extension decline from resurrecting a superseded
/// chooser.
static REQUEST_EPOCH: AtomicU64 = AtomicU64::new(0);
/// Serialises request invalidation with standard-view presentation. Without
/// this gate, a slow hand-off could pass an epoch check, lose a race to a new
/// recording, then publish its stale view after `supersede` had destroyed it.
fn request_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}
/// A new nonce for every chooser presentation, including decline-and-reopen.
/// Unlike `REQUEST_EPOCH`, this distinguishes two views of the same request.
static PRESENTATION_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Rank the searchable extensions for one captured request and present it.
///
/// Split from [`finish`] so it can be driven from a test or a replay of the
/// action log without a microphone.
///
/// **The V1-P1/P2 seam.** Ranking and presentation begin here; [`accept`] and
/// [`run_hand_off`] own the transcript-bearing hand-off, while [`decline`]
/// re-enters this path without asking the user to record again.
pub async fn deliver(app: &AppHandle, heard: &str) {
    let id = {
        let _gate = request_gate().lock().unwrap();
        let id = REQUEST_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
        *pending().lock().unwrap() = Some(Pending {
            id,
            presentation_id: 0,
            request: heard.to_string(),
            declined: Vec::new(),
            allowed: Vec::new(),
        });
        id
    };
    if let Err(error) = crate::extension_view::present_ranking(app, id, heard) {
        log::warn!("[GRAIN] extension mode: could not update routing surface: {error}");
    }
    // Auto-send may fire only on this first presentation (§5) — never on a
    // decline-and-reopen, where the user is already actively choosing.
    present(app, id, heard.to_string(), Vec::new(), true).await;
}

/// Rank `request` (excluding `declined`), emit the recommendation event, and
/// record the outcome. The one place ranking is turned into a surface event, so
/// deliver and decline present identically.
async fn present(
    app: &AppHandle,
    request_id: u64,
    request: String,
    declined: Vec<String>,
    allow_auto_send: bool,
) {
    // Each ranking is a semantic use — refresh the warmth so an accept or a
    // decline-and-reopen right after does not race the reaper (§6).
    crate::grain_space::embed::touch_extension_mode(app);
    // `recommend` embeds the query and is blocking; keep it off the async
    // runtime's poll threads.
    let ranked = {
        let app = app.clone();
        let request = request.clone();
        let declined = declined.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (mut ranked, semantic_available) =
                crate::extension_host::recommend(&request, &declined);
            crate::extension_misroutes::apply(&app, &mut ranked);
            let pool = crate::extension_host::searchable_ids()
                .into_iter()
                .filter(|id| !declined.contains(id))
                .map(|id| {
                    let (name, purpose) = crate::extension_host::recommendation_display(&app, &id)
                        .unwrap_or_else(|| (id.clone(), String::new()));
                    let icon = crate::extension_icons::ui_icon(&app, &id);
                    (id, name, purpose, icon)
                })
                .collect();
            (ranked, semantic_available, pool)
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), false, Vec::new()))
    };
    let (ranked, semantic_available, pool): (_, _, Vec<(String, String, String, Option<String>)>) =
        ranked;

    let display = |id: &str| {
        pool.iter()
            .find(|(candidate_id, _, _, _)| candidate_id == id)
            .map(|(_, name, purpose, icon)| (name.clone(), purpose.clone(), icon.clone()))
            .unwrap_or_else(|| (id.to_string(), String::new(), None))
    };

    let candidates: Vec<RecommendationCandidate> = ranked
        .iter()
        .map(|r| {
            let (name, purpose, _) = display(&r.extension_id);
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

    // Auto-send (§5): a clear semantic top to an eligible extension is handed
    // over WITHOUT a chooser — but the event still carries who it went to, so the
    // surface shows a Notice ("frictionless and afterwards obvious"). Only on the
    // first presentation, and never on a named hit (enforced in `auto_send_target`).
    let auto_sent = if allow_auto_send {
        auto_send_decision(app, &ranked)
    } else {
        None
    };

    if let Some(extension_id) = auto_sent {
        // Claim and present under the request gate. Without the gate, a new
        // capture could supersede this request after the claim and the old
        // ranking could then resurrect its Running surface over the new one.
        let claimed = {
            let _gate = request_gate().lock().unwrap();
            if REQUEST_EPOCH.load(Ordering::SeqCst) != request_id {
                false
            } else {
                let claimed = {
                    let mut slot = pending().lock().unwrap();
                    if slot.as_ref().is_some_and(|p| p.id == request_id) {
                        slot.take();
                        true
                    } else {
                        false
                    }
                };
                if claimed {
                    let _ = app.emit_to(
                        "main",
                        ExtensionRecommendation::NAME,
                        ExtensionRecommendation {
                            presentation_id: 0,
                            request: request.clone(),
                            candidates: candidates.clone(),
                            name_only: !semantic_available,
                            auto_sent: Some(extension_id.clone()),
                        },
                    );
                    if let Err(error) = crate::extension_view::present_running(
                        app,
                        request_id,
                        &extension_id,
                        &request,
                        true,
                    ) {
                        log::warn!(
                            "[GRAIN] extension mode: could not show auto-send progress: {error}"
                        );
                    }
                }
                claimed
            }
        };
        if !claimed {
            return;
        }
        log::info!("[GRAIN] extension mode: auto-sent to {extension_id}");
        let score = ranked.first().map(|r| r.score);
        action_log::record(
            &request,
            Some(extension_id.clone()),
            None,
            score,
            ActionLogOutcome::Chose,
        );
        // No accept is coming — clear the pending request and hand off now.
        run_hand_off(app, request_id, extension_id, request, declined);
        return;
    }

    // Not auto-sending: transition the prewarmed host window into the chooser.
    // It lists every searchable extension — ranked picks first, followed by the
    // alphabetical remainder so search always has the complete approved pool.
    let choice_candidates: Vec<crate::extension_view::ExtensionChoiceCandidate> = {
        let ranked_ids: std::collections::HashSet<&str> =
            ranked.iter().map(|r| r.extension_id.as_str()).collect();
        let mut picks: Vec<crate::extension_view::ExtensionChoiceCandidate> = ranked
            .iter()
            .map(|r| {
                let (name, purpose, icon) = display(&r.extension_id);
                crate::extension_view::ExtensionChoiceCandidate {
                    extension_id: r.extension_id.clone(),
                    name,
                    purpose,
                    signal: match r.signal {
                        grain_core::recommend::Signal::Named => "named",
                        grain_core::recommend::Signal::Topical => "topical",
                    }
                    .to_string(),
                    icon,
                }
            })
            .collect();
        let mut rest: Vec<crate::extension_view::ExtensionChoiceCandidate> = pool
            .into_iter()
            .filter(|(id, _, _, _)| !ranked_ids.contains(id.as_str()))
            .map(
                |(id, name, purpose, icon)| crate::extension_view::ExtensionChoiceCandidate {
                    extension_id: id,
                    name,
                    purpose,
                    signal: "none".to_string(),
                    icon,
                },
            )
            .collect();
        rest.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.extension_id.cmp(&b.extension_id))
        });
        picks.append(&mut rest);
        picks
    };

    // Publish only if this is still the live request, and bind accept to the
    // exact ids this presentation contains before either UI can answer.
    let presentation_id = PRESENTATION_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    // Bind and publish under the same gate used by supersede. A cancelled or
    // newly captured request must not publish a chooser after its renderer was
    // destroyed.
    let presentation = {
        let _gate = request_gate().lock().unwrap();
        if REQUEST_EPOCH.load(Ordering::SeqCst) != request_id {
            return;
        }
        {
            let mut slot = pending().lock().unwrap();
            let Some(p) = slot.as_mut().filter(|p| p.id == request_id) else {
                return;
            };
            p.presentation_id = presentation_id;
            p.allowed = choice_candidates
                .iter()
                .map(|candidate| candidate.extension_id.clone())
                .collect();
        }
        // The Tauri event remains for the settings-side/headless consumer.
        // Empty candidates is "nothing matched".
        let _ = app.emit_to(
            "main",
            ExtensionRecommendation::NAME,
            ExtensionRecommendation {
                presentation_id,
                request: request.clone(),
                candidates: candidates.clone(),
                name_only: !semantic_available,
                auto_sent: None,
            },
        );
        crate::extension_view::present_choice(
            app,
            request_id,
            presentation_id,
            &request,
            choice_candidates,
            !semantic_available,
        )
    };
    if let Err(error) = presentation {
        log::warn!("[GRAIN] extension mode: could not show chooser: {error}");
    }

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

/// Whether this ranking should Auto-send, and to whom (§5). Applies the
/// *configuration* half of the four conditions — beta gate, global toggle, the
/// user's per-extension deny-list — then defers the match half (semantic, clear
/// of the runner-up, never named) to [`grain_core::recommend::auto_send_target`].
fn auto_send_decision(
    app: &AppHandle,
    ranked: &[grain_core::recommend::Recommendation],
) -> Option<String> {
    let settings = crate::settings::get_settings(app);
    // Off by default AND beta-gated: Auto-send stays behind the experimental flag
    // until it is proven (§1, §5).
    if !settings.experimental_enabled || !settings.auto_send_enabled {
        return None;
    }
    let mut eligible = crate::extension_host::auto_send_eligible();
    // The user may only make it stricter — remove what they turned off; they
    // can never add an extension the author did not mark eligible.
    for id in &settings.auto_send_disabled {
        eligible.remove(id);
    }
    grain_core::recommend::auto_send_target(
        ranked,
        &eligible,
        grain_core::recommend::AUTO_SEND_MIN_MARGIN,
    )
}

/// The user declined an extension on the surface; reopen the chooser with it
/// struck out (G2). One keypress, not one re-recording — the request is the one
/// [`deliver`] already captured.
///
/// Called by the surface (behind the §6b gate); no-op if there is no pending
/// request or it does not match, so a stale click after a new capture does
/// nothing.
pub async fn decline(app: &AppHandle, presentation_id: u64, extension_id: &str) {
    let (request_id, request, declined) = {
        let mut slot = pending().lock().unwrap();
        let Some(p) = slot.as_mut() else {
            return;
        };
        if p.presentation_id != presentation_id || !p.allowed.iter().any(|id| id == extension_id) {
            return;
        }
        if !p.declined.iter().any(|id| id == extension_id) {
            p.declined.push(extension_id.to_string());
        }
        // Freeze the old presentation while it is being re-ranked. A second
        // click from that surface is stale even though the request text matches.
        p.presentation_id = 0;
        p.allowed.clear();
        (p.id, p.request.clone(), p.declined.clone())
    };
    action_log::record(
        &request,
        Some(extension_id.to_string()),
        None,
        None,
        ActionLogOutcome::Cancelled,
    );
    crate::extension_misroutes::record_decline(app, extension_id);
    present(app, request_id, request, declined, false).await;
}

/// Re-run recommendation after the user installs the optional semantic model.
/// The old nonce is invalidated before the blocking pass so a delayed click
/// cannot race the refreshed chooser.
pub async fn rerank(app: &AppHandle, request_id: u64) {
    let snapshot = {
        let mut slot = pending().lock().unwrap();
        let Some(pending) = slot.as_mut().filter(|pending| pending.id == request_id) else {
            return;
        };
        pending.presentation_id = 0;
        pending.allowed.clear();
        (pending.request.clone(), pending.declined.clone())
    };
    present(app, request_id, snapshot.0, snapshot.1, false).await;
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
pub fn accept(app: &AppHandle, presentation_id: u64, extension_id: &str) -> Result<(), String> {
    let accepted = {
        let mut slot = pending().lock().unwrap();
        let Some(pending) = slot.as_ref() else {
            return Err("there is no pending extension request".into());
        };
        if pending.presentation_id != presentation_id
            || !pending.allowed.iter().any(|id| id == extension_id)
        {
            log::warn!("[GRAIN] extension mode: ignored stale or invalid choice {extension_id}");
            return Err("stale or invalid extension choice".into());
        }
        slot.take().unwrap()
    };
    let Pending {
        id,
        request,
        declined,
        ..
    } = accepted;
    log::info!("[GRAIN] extension mode: accepted {extension_id}");
    if let Err(error) =
        crate::extension_view::present_running(app, id, extension_id, &request, false)
    {
        log::warn!("[GRAIN] extension mode: could not show hand-off progress: {error}");
    }
    action_log::record(
        &request,
        Some(extension_id.to_string()),
        None,
        None,
        ActionLogOutcome::Chose,
    );
    run_hand_off(app, id, extension_id.to_string(), request, declined);
    Ok(())
}

/// The user dismissed the recommendation surface without choosing (§8). Clears
/// the pending request so a later stale click does nothing and hides the
/// surface. No-op past the first dismissal.
pub fn dismiss(app: &AppHandle, presentation_id: u64) {
    if !dismiss_from_view(presentation_id) {
        return;
    }
    crate::extension_view::destroy(app);
    log::info!("[GRAIN] extension mode: surface dismissed");
}

/// Retire a chooser whose own host window is already closing. This is separate
/// from [`dismiss`] to avoid recursively destroying the same native window.
pub fn dismiss_from_view(presentation_id: u64) -> bool {
    let _gate = request_gate().lock().unwrap();
    let dismissed = {
        let mut slot = pending().lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|pending| pending.presentation_id == presentation_id)
        {
            slot.take();
            true
        } else {
            false
        }
    };
    if !dismissed {
        return false;
    }
    REQUEST_EPOCH.fetch_add(1, Ordering::SeqCst);
    true
}

/// Suppress a late hand-off outcome after the user closes routing/progress.
/// The extension may already have observed the request, so this only retires
/// Grain's UI epoch; it deliberately does not pretend to cancel remote work.
pub fn dismiss_request_from_view(request_id: u64) -> bool {
    let _gate = request_gate().lock().unwrap();
    if REQUEST_EPOCH.load(Ordering::SeqCst) != request_id {
        return false;
    }
    REQUEST_EPOCH.fetch_add(1, Ordering::SeqCst);
    let mut slot = pending().lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|pending| pending.id == request_id)
    {
        slot.take();
    }
    true
}

/// Invalidate Extension Mode because another capture has begun. Unlike
/// [`dismiss`], this emits only when there was a chooser to withdraw; the epoch
/// still advances every time so a handed-off extension cannot decline back over
/// a newer recording.
pub fn supersede(app: &AppHandle) {
    let _gate = request_gate().lock().unwrap();
    REQUEST_EPOCH.fetch_add(1, Ordering::SeqCst);
    // A fresh capture can begin while Extension Mode is ranking after its audio
    // recorder has already returned to idle. Retire that processing generation
    // now; its async tail checks ownership before it can publish a chooser.
    let retired_processing = {
        let mut slot = active().lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|session| session.phase == Phase::Processing)
        {
            slot.take().is_some()
        } else {
            false
        }
    };
    if retired_processing {
        crate::shortcut::unregister_cancel_shortcut(app);
    }
    pending().lock().unwrap().take();
    crate::extension_view::destroy(app);
}

/// Wake the chosen extension, deliver the request, and record the outcome. Shared
/// by [`accept`] (the user chose) and Auto-send (Grain chose) — the hand-off is
/// identical; only how the choice was made differs, and that is recorded before
/// this is called.
///
/// Spawned rather than awaited because the callers must return at once — the
/// extension may call a model or the network, which is what
/// [`crate::extension_host::hand_off`]'s generous deadline is for.
fn run_hand_off(
    app: &AppHandle,
    request_id: u64,
    extension_id: String,
    request: String,
    mut declined: Vec<String>,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use crate::extension_host::HandOffOutcome;
        match crate::extension_host::hand_off(&app, &extension_id, &request).await {
            HandOffOutcome::Done(message) => {
                crate::extension_misroutes::record_accepted_route(&app, &extension_id);
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
                let _gate = request_gate().lock().unwrap();
                if REQUEST_EPOCH.load(Ordering::SeqCst) != request_id {
                    return;
                }
                if let Some(message) = message.filter(|message| !message.trim().is_empty()) {
                    if let Err(error) = crate::extension_view::present_result(
                        &app,
                        &extension_id,
                        message,
                        crate::extension_view::ResultTone::Success,
                    ) {
                        log::warn!("[GRAIN] extension mode: could not show result: {error}");
                        crate::extension_view::destroy(&app);
                    }
                } else if let Err(error) =
                    crate::extension_view::present_completion(&app, &extension_id)
                {
                    log::warn!("[GRAIN] extension mode: could not show completion: {error}");
                    crate::extension_view::destroy(&app);
                }
            }
            HandOffOutcome::View(view) => {
                crate::extension_misroutes::record_accepted_route(&app, &extension_id);
                let _gate = request_gate().lock().unwrap();
                if REQUEST_EPOCH.load(Ordering::SeqCst) != request_id {
                    return;
                }
                if let Err(reason) =
                    crate::extension_view::present(&app, &extension_id, &request, view)
                {
                    log::warn!("[GRAIN] extension mode: could not present view: {reason}");
                    action_log::record(
                        &request,
                        Some(extension_id.clone()),
                        None,
                        None,
                        ActionLogOutcome::Failed {
                            reason: reason.clone(),
                        },
                    );
                    crate::extension_view::destroy(&app);
                }
            }
            HandOffOutcome::Declined(reason) => {
                crate::extension_misroutes::record_decline(&app, &extension_id);
                log::info!(
                    "[GRAIN] extension mode: {extension_id} declined the request — {reason}"
                );
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    // This is the G3 misroute signal: Grain offered/handed off
                    // to this extension and it said it was the wrong owner.
                    ActionLogOutcome::Cancelled,
                );
                if !declined.iter().any(|id| id == &extension_id) {
                    declined.push(extension_id.clone());
                }
                // Reopen only if no newer capture/cancel has advanced the epoch.
                // The compare happens again under the pending lock so a new
                // delivery cannot slip between the check and the write.
                let reopen = if REQUEST_EPOCH.load(Ordering::SeqCst) == request_id {
                    let mut slot = pending().lock().unwrap();
                    if REQUEST_EPOCH.load(Ordering::SeqCst) == request_id && slot.is_none() {
                        *slot = Some(Pending {
                            id: request_id,
                            presentation_id: 0,
                            request: request.clone(),
                            declined: declined.clone(),
                            allowed: Vec::new(),
                        });
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if reopen {
                    present(&app, request_id, request, declined, false).await;
                }
            }
            HandOffOutcome::Failed(reason) => {
                crate::extension_misroutes::record_accepted_route(&app, &extension_id);
                log::warn!("[GRAIN] extension mode: {extension_id} failed — {reason}");
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Failed {
                        reason: reason.clone(),
                    },
                );
                let _gate = request_gate().lock().unwrap();
                if REQUEST_EPOCH.load(Ordering::SeqCst) == request_id {
                    if let Err(error) = crate::extension_view::present_result(
                        &app,
                        &extension_id,
                        reason,
                        crate::extension_view::ResultTone::Danger,
                    ) {
                        log::warn!("[GRAIN] extension mode: could not show failure: {error}");
                        crate::extension_view::destroy(&app);
                    }
                }
            }
            // Reported as its own thing, never as failure: for anything that
            // left the machine a timeout does not mean it did not happen.
            HandOffOutcome::Unknown => {
                crate::extension_misroutes::record_accepted_route(&app, &extension_id);
                action_log::record(
                    &request,
                    Some(extension_id.clone()),
                    None,
                    None,
                    ActionLogOutcome::Unknown,
                );
                let _gate = request_gate().lock().unwrap();
                if REQUEST_EPOCH.load(Ordering::SeqCst) == request_id {
                    if let Err(error) = crate::extension_view::present_result(
                        &app,
                        &extension_id,
                        "Grain stopped waiting, so the extension's final outcome is unknown."
                            .into(),
                        crate::extension_view::ResultTone::Warning,
                    ) {
                        log::warn!(
                            "[GRAIN] extension mode: could not show unknown result: {error}"
                        );
                        crate::extension_view::destroy(&app);
                    }
                }
            }
        }
    });
}

/// Complete the native-pill side only after the prewarmed Tauri surface is
/// visible. The flag prevents [`complete`] from emitting the same terminal
/// event again when transcription/ranking finishes a moment later.
pub fn surface_ready(app: &AppHandle, pill_session_id: u64) {
    let should_complete = {
        let mut slot = active().lock().unwrap();
        match slot
            .as_mut()
            .filter(|session| session.pill_session_id == pill_session_id)
        {
            Some(session) if session.pill_completed => false,
            Some(session) => {
                session.pill_completed = true;
                true
            }
            // A very fast ranking pass can release the recorder session before
            // a cold webview paints. The renderer still owns the hand-off and
            // must complete it when it finally becomes visible.
            None => true,
        }
    };
    if should_complete {
        crate::grain_actions::emit_processing_complete(app, pill_session_id);
    }
}

fn complete(app: &AppHandle, session: &ActiveSession) {
    let renderer_owns_pill = crate::extension_view::owns_pill_handoff(session.pill_session_id);
    let mut slot = active().lock().unwrap();
    // A newer session already owns the slot; this one finished late and must not
    // clear it. Same generation guard as `extension_session`, and for the same
    // reason: without it a slow transcription can unlock a microphone that
    // something else has since claimed.
    if slot
        .as_ref()
        .is_some_and(|active| active.generation == session.generation)
    {
        let pill_completed = slot.as_ref().is_some_and(|active| active.pill_completed);
        *slot = None;
        drop(slot);
        crate::shortcut::unregister_cancel_shortcut(app);
        if !pill_completed && !renderer_owns_pill {
            crate::grain_actions::emit_processing_complete(app, session.pill_session_id);
        }
    }
}
