//! [GRAIN] Post-process (LLM) routing — the rotation counterpart to `stt_router`.
//!
//! Post-processing keeps its OWN provider list (separate from STT). When
//! `post_process_smart_rotation` is on, requests fan out across ENABLED
//! post-process providers (health-ordered by `select_order` + per-provider
//! daily quota + failover); when off, the single selected provider is used
//! (today's behavior).
//!
//! Only the quota bookkeeping + pool selection live here; the actual LLM call
//! (Apple / structured-output / legacy) stays in `actions.rs` where its deps are.
//! Reads/writes go through grain-core's owned `AppContext`, so this is headless.

use std::sync::Arc;

use grain_core::{AppContext, AppSettings, PostProcessProvider};
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
