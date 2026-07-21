//! [GRAIN] S3: STT dispatcher — routes a transcription to the right backend.
//!
//! - smart rotation OFF (default): the LOCAL in-process model (Handy's
//!   `TranscriptionManager`) — identical to today, no network, no surprise spike.
//! - smart rotation ON: round-robin across ENABLED CLOUD providers (`kind !=
//!   Local`) with per-provider daily quota + failover. The local model is
//!   deliberately excluded from the pool (locked decision 5 in the plan).
//!
//! Reads the STT pool from grain-core's owned `AppContext` settings (the headless,
//! front-end-independent home), and records quota usage back through it.

use std::sync::Arc;

use grain_core::{AppContext, SttProvider, SttProviderKind};
use tauri::{AppHandle, Manager};

use crate::managers::transcription::TranscriptionManager;
use crate::rotation_state::{CallOutcome, RotationTrackers};
use crate::stt_client::SttError;

/// Hard ceiling on a single cloud STT provider call. A provider that accepts the
/// upload but never returns a transcript must not stall the pipeline and must
/// let rotation move on. 90s covers AssemblyAI's async upload + its own 60s poll
/// deadline while still bounding a genuinely hung call. (The Agent and
/// post-process LLM paths use their own 120s ceiling.)
const STT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

fn is_eligible(p: &SttProvider) -> bool {
    p.enabled
        && match p.quota_limit {
            Some(limit) => p.quota_used_today < limit,
            None => true,
        }
}

/// [GRAIN] The cloud-only rotation pool for the current settings: every enabled,
/// under-quota provider whose `kind != Local`. Single source of truth shared by
/// [`transcribe`] (what it actually routes to) and [`will_route_to_cloud`] (what
/// the batch press path uses to decide whether to warm the local model), so the
/// two can never drift apart.
fn cloud_pool(settings: &grain_core::AppSettings) -> Vec<SttProvider> {
    settings
        .stt_providers
        .iter()
        .filter(|p| p.kind != SttProviderKind::Local && p.enabled && is_eligible(p))
        .cloned()
        .collect()
}

/// [GRAIN] True when a batch transcription started right now would be routed to a
/// cloud provider instead of the in-process model: smart rotation is on AND at
/// least one cloud provider is eligible (enabled + under today's quota).
///
/// `TranscribeAction::start` uses this to skip eagerly loading the local model
/// when the recording will go to the cloud — the model would otherwise sit
/// resident in RAM until the idle/immediate unload fires. Mirrors exactly the
/// branch [`transcribe`] takes at stop time.
pub fn will_route_to_cloud(app: &AppHandle) -> bool {
    let Some(ctx) = app.try_state::<Arc<AppContext>>() else {
        return false;
    };
    ctx.with_settings(|s| s.stt_smart_rotation && !cloud_pool(s).is_empty())
}

/// Transcribe `samples` honoring the STT routing settings. Returns the final
/// transcript text (already the assembled/plain string the caller pastes).
pub async fn transcribe(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    let ctx = app
        .try_state::<Arc<AppContext>>()
        .ok_or("STT routing: AppContext not available")?
        .inner()
        .clone();

    // [GRAIN] S5: roll daily quotas over lazily when the local date changes.
    reset_quota_if_new_day(&ctx);

    let settings = ctx.settings();

    // Default path: local, in-process — exactly today's behavior.
    if !settings.stt_smart_rotation {
        return local(app, samples).await;
    }

    // Rotation ON: cloud-only pool (local never rotated), hard-gated by the
    // per-provider daily quota. The quota gate is OURS; the tracker only orders.
    // Shared with `will_route_to_cloud` so the batch press path's load-skip
    // decision matches what we route to here.
    let eligible: Vec<SttProvider> = cloud_pool(&settings);
    if eligible.is_empty() {
        // Rotation is on but nothing is eligible (no enabled cloud provider, or
        // all over quota). Fall back to the local model rather than failing —
        // `local()` loads it on demand, so this stays correct even though
        // `TranscribeAction::start` skipped the eager warm-up.
        log::warn!(
            "[GRAIN] STT smart rotation is on but no cloud provider is eligible — falling back to local"
        );
        return local(app, samples).await;
    }

    let trackers = app
        .try_state::<Arc<RotationTrackers>>()
        .ok_or("STT routing: RotationTrackers not available")?;

    // Order best-first: cooling-down providers (recent 429s) go to the back,
    // healthy ones lead by header/cap headroom. STT carries no token budget, so a
    // nominal estimate keeps the token-headroom path inert.
    let candidates: Vec<(String, String)> = eligible
        .iter()
        .map(|p| (p.id.clone(), p.base_url.clone()))
        .collect();

    let client = app
        .try_state::<reqwest::Client>()
        .ok_or("STT routing: HTTP client not available")?
        .inner()
        .clone();

    // Failover walk lives in the shared driver; the closure maps one STT call
    // (timeout + SttResult/SttError) into a CallOutcome the driver records.
    crate::rotation_state::run_with_rotation(
        &trackers.stt,
        &candidates,
        1, // nominal token estimate: STT carries no token budget
        |id| {
            let client = client.clone();
            let eligible = &eligible;
            let settings = &settings;
            let samples = &samples;
            async move {
                let Some(provider) = eligible.iter().find(|p| p.id == id) else {
                    return CallOutcome::Failed;
                };
                let key = settings
                    .stt_api_keys
                    .get(&provider.id)
                    .cloned()
                    .unwrap_or_default();

                let call = tokio::time::timeout(
                    STT_REQUEST_TIMEOUT,
                    crate::stt_client::transcribe(&client, provider, samples, &key),
                )
                .await
                .unwrap_or_else(|_| {
                    log::warn!(
                        "[GRAIN] STT provider '{}' timed out after {}s",
                        provider.id,
                        STT_REQUEST_TIMEOUT.as_secs()
                    );
                    Err(SttError::Other("request timed out".to_string()))
                });

                match call {
                    Ok(res) => {
                        // [GRAIN] Cloud transcripts skip the local engine, so apply the
                        // shared final-text stage (custom-word dictionary + filler/stutter
                        // filtering) here. No Whisper `initial_prompt` biasing on the cloud
                        // path, so the fuzzy custom-word pass runs (skip = false). The local
                        // branch finalizes inside `tm.transcribe`, so it is NOT finalized
                        // again here.
                        CallOutcome::Ok {
                            text: crate::audio_toolkit::finalize_transcript(
                                &res.text,
                                &settings.custom_words,
                                settings.word_correction_threshold,
                                &settings.app_language,
                                &settings.custom_filler_words,
                                false,
                                // [GRAIN] Snippets built-in extension gate (SPEC 10.1): disabled ->
                            // empty slice, the zero-cost no-op path.
                            if settings.snippets_enabled { &settings.snippets } else { &[] },
                                settings.scrap_that_enabled,
                            ),
                            remaining_requests: res.remaining_requests,
                            remaining_tokens: res.remaining_tokens,
                            total_tokens: None,
                        }
                    }
                    Err(SttError::RateLimited { retry_after_s }) => {
                        log::warn!(
                            "[GRAIN] STT '{}' rate-limited — cooling down, trying next",
                            provider.id
                        );
                        CallOutcome::RateLimited { retry_after_s }
                    }
                    Err(e) => {
                        log::warn!(
                            "[GRAIN] STT provider '{}' failed: {e} — trying next",
                            provider.id
                        );
                        CallOutcome::Failed
                    }
                }
            }
        },
        |id| {
            record_usage(&ctx, id);
            log::info!("[GRAIN] STT routed to '{id}'");
        },
    )
    .await
}

/// Run the in-process model off the async runtime (it blocks for the inference).
///
/// Loads the model on demand if it isn't resident: `TranscribeAction::start`
/// only warms it eagerly when the recording is staying local, so on the local
/// path we must be self-sufficient. `initiate_model_load` is idempotent and
/// non-blocking, and `transcribe` waits on the load condvar before inferring, so
/// this is race-free with an in-flight warm-up.
async fn local(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    if !tm.is_model_loaded() {
        tm.initiate_model_load();
    }
    tokio::task::spawn_blocking(move || tm.transcribe(samples))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| e.to_string())
}

/// [GRAIN] S5: if the local date rolled over since the last reset, zero every
/// provider's `quota_used_today` and stamp today's date. Idempotent within a day.
pub fn reset_quota_if_new_day(ctx: &Arc<AppContext>) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let needs_reset = ctx.with_settings(|s| s.stt_quota_reset_date != today);
    if !needs_reset {
        return;
    }
    if let Err(e) = ctx.update_settings(|s| {
        for p in s.stt_providers.iter_mut() {
            p.quota_used_today = 0;
        }
        s.stt_quota_reset_date = today.clone();
    }) {
        log::warn!("[GRAIN] failed to reset STT daily quotas: {e}");
    } else {
        log::info!("[GRAIN] STT daily quotas reset for {today}");
    }
}

/// Increment the provider's daily quota counter and persist it.
fn record_usage(ctx: &Arc<AppContext>, provider_id: &str) {
    let id = provider_id.to_string();
    if let Err(e) = ctx.update_settings(|s| {
        if let Some(p) = s.stt_providers.iter_mut().find(|p| p.id == id) {
            p.quota_used_today += 1;
        }
    }) {
        log::warn!("[GRAIN] failed to persist STT quota usage: {e}");
    }
}
