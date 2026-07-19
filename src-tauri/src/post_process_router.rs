//! [GRAIN] Post-process (LLM) routing — the rotation counterpart to `stt_router`.
//!
//! Post-processing keeps its OWN provider list (separate from STT). When
//! `post_process_smart_rotation` is on, requests fan out across ENABLED
//! post-process providers (health-ordered by `select_order` + per-provider
//! daily quota + failover); when off, the single selected provider is used
//! (today's behavior).
//!
//! The quota bookkeeping, pool selection, and the rotation/failover walk
//! ([`post_process_rotated`]) live here; the actual LLM call (Apple /
//! structured-output / legacy `run_one_provider`) lives in
//! `grain_post_process.rs`; this module drives it through the shared timeout
//! wrapper.

use std::sync::Arc;

use crate::rotation_state::CallOutcome;
use grain_core::{AppContext, AppSettings, PostProcessProvider};
use log::{debug, warn};
use tauri::{AppHandle, Manager};

fn ctx(app: &AppHandle) -> Option<Arc<AppContext>> {
    app.try_state::<Arc<AppContext>>()
        .map(|s| s.inner().clone())
}

/// True if the provider may take a rotated request right now: enabled and within
/// its daily quota. (A missing model is filtered by the caller, which needs the
/// per-provider model map.)
fn is_eligible(p: &PostProcessProvider) -> bool {
    p.enabled
        && match p.quota_limit {
            Some(limit) => p.quota_used_today < limit,
            None => true,
        }
}

/// The rotation pool for this request: every eligible provider, in settings
/// order. Ordering is NOT done here — the caller passes this set to
/// `rotation_state::select_order`, which orders best-first by live health
/// (recent 429s cool down, headroom leads). This mirrors `stt_router::cloud_pool`
/// exactly so the two routers stay consistent: the pool filters, the tracker
/// orders. (Previously this pre-shuffled round-robin, which `select_order` then
/// discarded while a process-wide cursor desynced from the calls made.)
pub fn rotation_pool(settings: &AppSettings) -> Vec<PostProcessProvider> {
    settings
        .post_process_providers
        .iter()
        .filter(|p| is_eligible(p))
        .cloned()
        .collect()
}

/// If the local date rolled over since the last reset, zero every provider's
/// `quota_used_today` and stamp today's date. Idempotent within a day.
/// Returns `true` if quotas were actually reset (so the caller can re-read
/// settings to pick up zeroed counters).
pub fn reset_quota_if_new_day(app: &AppHandle) -> bool {
    let Some(ctx) = ctx(app) else { return false };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let needs_reset = ctx.with_settings(|s| s.post_process_quota_reset_date != today);
    if !needs_reset {
        return false;
    }
    if let Err(e) = ctx.update_settings(|s| {
        for p in s.post_process_providers.iter_mut() {
            p.quota_used_today = 0;
        }
        s.post_process_quota_reset_date = today.clone();
    }) {
        log::warn!("[GRAIN] failed to reset post-process daily quotas: {e}");
    } else {
        log::info!("[GRAIN] post-process daily quotas reset for {today}");
    }
    true
}

/// Increment the provider's daily quota counter and persist it.
pub fn record_usage(app: &AppHandle, provider_id: &str) {
    let Some(ctx) = ctx(app) else { return };
    let id = provider_id.to_string();
    if let Err(e) = ctx.update_settings(|s| {
        if let Some(p) = s.post_process_providers.iter_mut().find(|p| p.id == id) {
            p.quota_used_today += 1;
        }
    }) {
        log::warn!("[GRAIN] failed to persist post-process quota usage: {e}");
    }
}

/// Token-efficient seam-repair layer appended to the post-process prompt ONLY
/// for rolling-window transcripts. Rolling text is assembled from sequential
/// audio segments, so its residual defects are seam-shaped (casing, stray or
/// missing sentence punctuation, doubled boundary words) — this line aims the
/// LLM at exactly those, and at nothing else. ~40 tokens; invisible to the user.
pub const ROLLING_SEAM_PROMPT: &str = "\n[Live dictation]\nThe text was assembled \
from sequential speech segments. Repair segment-join artifacts: wrong \
capitalization, stray or missing periods/commas, doubled words, extra spaces. \
Never reword, reorder, or drop content.";

/// Hard ceiling on a single post-process provider call. A provider that accepts
/// the connection but never responds must not hang the transcribe→paste pipeline
/// (and in rotation mode must yield so the next provider is tried). Matches the
/// Agent's `AGENT_LLM_TIMEOUT` so all LLM paths behave the same.
const LLM_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// `run_one_provider` bounded by [`LLM_REQUEST_TIMEOUT`]. A timeout is
/// surfaced as [`CallOutcome::Failed`] — identical to any other failure — so the
/// single-provider path returns `None` and the rotation path fails over to the
/// next candidate instead of stalling. Both post-process call sites go through
/// here so neither can forget the deadline.
pub(crate) async fn run_one_provider_with_timeout(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    prompt: &str,
    transcription: &str,
) -> CallOutcome {
    match tokio::time::timeout(
        LLM_REQUEST_TIMEOUT,
        crate::grain_post_process::run_one_provider(client, provider, model, api_key, prompt, transcription),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            warn!(
                "post-process provider '{}' timed out after {}s",
                provider.id,
                LLM_REQUEST_TIMEOUT.as_secs()
            );
            CallOutcome::Failed
        }
    }
}

/// Rotation path: try ENABLED post-process providers best-first by live
/// health (recent 429s cool down, headroom leads — via `select_order`) until one
/// returns a result, recording quota usage on success. Returns None if none are
/// eligible or all fail (the caller then pastes the raw transcript).
pub(crate) async fn post_process_rotated(
    app: &AppHandle,
    prompt: &str,
    transcription: &str,
) -> Option<String> {
    // Roll daily quotas first; then read settings ONCE so the pool
    // reflects any newly-zeroed counters.
    reset_quota_if_new_day(app);
    let settings = crate::settings::get_settings(app);
    // Eligible = enabled + under daily quota (the hard gate) AND has a model
    // configured (no point routing to a provider that can't run). The quota gate
    // is ours; the tracker only orders what survives it.
    let eligible: Vec<PostProcessProvider> = rotation_pool(&settings)
        .into_iter()
        .filter(|p| {
            settings
                .post_process_models
                .get(&p.id)
                .map(|m| !m.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    if eligible.is_empty() {
        debug!(
            "Post-process smart rotation is on, but no eligible providers have a model configured"
        );
        return None;
    }

    let Some(trackers) = app.try_state::<Arc<crate::rotation_state::RotationTrackers>>() else {
        warn!("Post-process rotation: RotationTrackers unavailable");
        return None;
    };

    let Some(http_client) = app.try_state::<reqwest::Client>() else {
        warn!("Post-process rotation: shared HTTP client unavailable");
        return None;
    };
    let http_client = http_client.inner().clone();

    // Order best-first by live health (recent 429s cool down, headroom leads).
    let est_tokens = provider_router::estimate_tokens(transcription);
    let candidates: Vec<(String, String)> = eligible
        .iter()
        .map(|p| (p.id.clone(), p.base_url.clone()))
        .collect();

    // Failover walk lives in the shared driver; we supply only how to run one
    // provider and how to record quota on success.
    let result = crate::rotation_state::run_with_rotation(
        &trackers.llm,
        &candidates,
        est_tokens,
        |id| {
            let http_client = http_client.clone();
            let eligible = &eligible;
            let settings = &settings;
            async move {
                let Some(provider) = eligible.iter().find(|p| p.id == id) else {
                    return CallOutcome::Failed;
                };
                let model = settings
                    .post_process_models
                    .get(&provider.id)
                    .cloned()
                    .unwrap_or_default();
                let api_key = settings
                    .post_process_api_keys
                    .get(&provider.id)
                    .cloned()
                    .unwrap_or_default();
                debug!(
                    "Rotation: trying provider '{}' (model: {})",
                    provider.id, model
                );
                run_one_provider_with_timeout(
                    &http_client,
                    provider,
                    model,
                    api_key,
                    prompt,
                    transcription,
                )
                .await
            }
        },
        |id| {
            record_usage(app, id);
            log::info!("[GRAIN] post-process routed to '{id}'");
        },
    )
    .await;

    match result {
        Ok(text) => Some(text),
        Err(e) => {
            warn!("Post-process rotation: no provider produced output ({e})");
            None
        }
    }
}
