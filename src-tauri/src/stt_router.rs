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
use crate::rotation_state::{
    now_secs, record_outcome, select_order, CallOutcome, RotationTrackers,
};
use crate::stt_client::SttError;

fn is_eligible(p: &SttProvider) -> bool {
    p.enabled
        && match p.quota_limit {
            Some(limit) => p.quota_used_today < limit,
            None => true,
        }
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
    let eligible: Vec<SttProvider> = settings
        .stt_providers
        .iter()
        .filter(|p| p.kind != SttProviderKind::Local && p.enabled && is_eligible(p))
        .cloned()
        .collect();
    if eligible.is_empty() {
        return Err(
            "smart rotation is on, but no cloud STT providers are configured/enabled (or all are over quota today)".to_string(),
        );
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
    let order = select_order(&trackers.stt, &candidates, 1, now_secs());

    let mut last_err = "all STT providers exhausted (quota, cooldown, or errors)".to_string();
    for id in &order {
        let Some(provider) = eligible.iter().find(|p| &p.id == id) else {
            continue;
        };
        let key = settings
            .stt_api_keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        
        let client = app
            .try_state::<reqwest::Client>()
            .ok_or("STT routing: HTTP client not available")?
            .inner();

        match crate::stt_client::transcribe(client, provider, &samples, &key).await {
            Ok(res) => {
                record_outcome(
                    &trackers.stt,
                    &provider.id,
                    &CallOutcome::Ok {
                        text: res.text.clone(),
                        remaining_requests: res.remaining_requests,
                        remaining_tokens: res.remaining_tokens,
                        total_tokens: None,
                    },
                    now_secs(),
                );
                record_usage(&ctx, &provider.id);
                log::info!(
                    "[GRAIN] STT routed to '{}' ({:?})",
                    provider.id,
                    provider.kind
                );
                return Ok(res.text);
            }
            Err(SttError::RateLimited { retry_after_s }) => {
                record_outcome(
                    &trackers.stt,
                    &provider.id,
                    &CallOutcome::RateLimited { retry_after_s },
                    now_secs(),
                );
                log::warn!(
                    "[GRAIN] STT '{}' rate-limited — cooling down, trying next",
                    provider.id
                );
                last_err = format!("{} rate-limited", provider.id);
            }
            Err(e) => {
                record_outcome(
                    &trackers.stt,
                    &provider.id,
                    &CallOutcome::Failed,
                    now_secs(),
                );
                log::warn!(
                    "[GRAIN] STT provider '{}' failed: {e} — trying next",
                    provider.id
                );
                last_err = e.to_string();
            }
        }
    }
    Err(last_err)
}

/// Run the in-process model off the async runtime (it blocks for the inference).
async fn local(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
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
