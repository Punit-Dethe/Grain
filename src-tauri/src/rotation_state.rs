//! [GRAIN] Smart-rotation runtime state shared by the STT and post-process (LLM)
//! routers.
//!
//! The heavy lifting — cooldowns from real 429s, Retry-After parsing, live
//! rate-limit-header headroom scoring, round-robin among equally-healthy
//! providers — lives in the pure `provider-router` crate (`RotationTracker`).
//! This module is the thin Tauri-side glue: it holds one tracker per domain in
//! managed state, supplies a monotonic `now`, converts reqwest headers, and maps
//! a single provider call's result into the signal the tracker learns from.
//!
//! The tracker only ORDERS providers and applies cooldowns. The hard per-provider
//! daily quota gate stays in the routers (over grain-core `AppContext`), exactly
//! as the crate's design intends (quota is the caller's responsibility).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use once_cell::sync::Lazy;
use provider_router::{ProviderConfig, RotationTracker};

/// One live `RotationTracker` per routing domain (STT vs LLM), held in Tauri
/// managed state so health/cooldowns persist across requests for the process
/// lifetime. The two domains never share health — a 429 on a cloud STT provider
/// must not cool down an LLM provider that happens to share an id.
#[derive(Default)]
pub struct RotationTrackers {
    pub stt: Mutex<RotationTracker>,
    pub llm: Mutex<RotationTracker>,
}

/// Monotonic seconds since process start — the clock the tracker compares
/// cooldown deadlines against. Monotonic (not wall-clock) so an NTP step can't
/// extend or cut short a cooldown.
static START: Lazy<Instant> = Lazy::new(Instant::now);
pub fn now_secs() -> f64 {
    START.elapsed().as_secs_f64()
}

/// The outcome of ONE provider call, normalized across STT and LLM so a single
/// `record_outcome` teaches the tracker. `remaining_*` come from rate-limit
/// headers when present (else `None`); `total_tokens` is the request's measured
/// or estimated token cost (LLM only; `None` for STT).
pub enum CallOutcome {
    Ok {
        text: String,
        remaining_requests: Option<i64>,
        remaining_tokens: Option<i64>,
        total_tokens: Option<i64>,
    },
    /// A real HTTP 429. `retry_after_s` is parsed from Retry-After / reset headers
    /// (falls back to the crate's default cooldown when absent).
    RateLimited { retry_after_s: Option<f64> },
    /// Any other failure (network, 5xx, parse, missing key → 401). Brief cooldown
    /// so retries fan out, but the provider is never hard-excluded.
    Failed,
}

/// Lowercase reqwest headers into the plain map the crate's parsers expect.
pub fn headers_to_map(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|s| (k.as_str().to_ascii_lowercase(), s.to_string()))
        })
        .collect()
}

/// Order the candidate providers best-first for this request (healthy by
/// headroom first, cooling-down providers at the back, soonest-recovery first),
/// returning their ids in call order. `candidates` is `(id, base_url)` for each
/// already-quota-eligible provider; `est_tokens` is this request's rough size.
pub fn select_order(
    tracker: &Mutex<RotationTracker>,
    candidates: &[(String, String)],
    est_tokens: i64,
    now: f64,
) -> Vec<String> {
    let configs: Vec<ProviderConfig> = candidates
        .iter()
        .map(|(id, base_url)| ProviderConfig::new(id, base_url))
        .collect();
    tracker
        .lock()
        .unwrap()
        .select(&configs, est_tokens, now)
        .into_iter()
        .map(|c| c.id)
        .collect()
}

/// Teach the tracker from one call's outcome.
pub fn record_outcome(tracker: &Mutex<RotationTracker>, id: &str, outcome: &CallOutcome, now: f64) {
    let mut t = tracker.lock().unwrap();
    match outcome {
        CallOutcome::Ok {
            remaining_requests,
            remaining_tokens,
            total_tokens,
            ..
        } => t.record_success(
            id,
            *total_tokens,
            *remaining_requests,
            *remaining_tokens,
            now,
        ),
        CallOutcome::RateLimited { retry_after_s } => {
            t.record_rate_limited(id, *retry_after_s, now)
        }
        CallOutcome::Failed => t.record_error(id, now),
    }
}
