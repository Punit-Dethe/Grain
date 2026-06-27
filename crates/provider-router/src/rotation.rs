//! Smart LLM rotation — live-signal provider selection.
//!
//! Ported from `open_voice_router/services/llm_rotation.py` (plus the pure
//! `_parse_retry_after` helper from `llm_client.py`).
//!
//! The tracker learns from three LIVE signals, best first: rate-limit headers,
//! observed usage (sliding 60 s token window + daily counters), and real 429
//! cooldowns. Static free-tier caps act as conservative ordering hints only.
//!
//! THE SAFETY RULE: static caps and our own estimates only affect ORDERING —
//! they can never exclude a provider. Only a real 429 (cooldown) excludes, and
//! even cooldowns degrade to ordering when every provider is cooling down. The
//! caller's per-provider daily quota (see [`crate::router`]) is the only hard
//! gate and is deliberately NOT part of this score.
//!
//! `now` is always supplied by the caller (monotonic seconds), keeping the
//! policy pure and deterministic.

use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use crate::model::ProviderConfig;

/// Headers considered fresh for this long; afterward fall back to unknown (1.0).
const HEADER_TTL_S: f64 = 300.0;
const WINDOW_S: f64 = 60.0;
const DEFAULT_COOLDOWN_S: f64 = 60.0;
/// Reserved completion-token allowance added to every request estimate.
pub const COMPLETION_RESERVE_TOKENS: i64 = 800;

/// Rough request-size estimate: ~4 chars/token + completion reserve.
pub fn estimate_tokens(text: &str) -> i64 {
    let chars = text.chars().count() as i64;
    (chars / 4).max(1) + COMPLETION_RESERVE_TOKENS
}

/// Extract `(remaining_requests, remaining_tokens)` from response headers.
/// Returns `None` for anything missing/unparseable. Port of
/// `parse_rate_limit_headers`.
pub fn parse_rate_limit_headers(headers: &HashMap<String, String>) -> (Option<i64>, Option<i64>) {
    let int_of = |name: &str| -> Option<i64> {
        headers
            .get(name)
            .and_then(|v| v.parse::<f64>().ok())
            .map(|f| f as i64)
    };
    (
        int_of("x-ratelimit-remaining-requests"),
        int_of("x-ratelimit-remaining-tokens"),
    )
}

/// Read Retry-After (seconds form) or x-ratelimit-reset; fall back to 60 s.
/// Port of `llm_client._parse_retry_after`. Sub-second resets clamp to a 1 s floor.
pub fn parse_retry_after(headers: &HashMap<String, String>) -> f64 {
    for name in [
        "retry-after",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ] {
        let Some(value) = headers.get(name) else {
            continue;
        };
        let text = value.trim().to_lowercase();
        let parsed = if let Some(stripped) = text.strip_suffix("ms") {
            stripped.parse::<f64>().ok().map(|f| (f / 1000.0).max(1.0))
        } else if let Some(stripped) = text.strip_suffix('m') {
            // Note: "...ms" already handled above; this is the minutes suffix.
            stripped.parse::<f64>().ok().map(|f| (f * 60.0).max(1.0))
        } else {
            let core = text.strip_suffix('s').unwrap_or(&text);
            core.parse::<f64>().ok().map(|f| f.max(1.0))
        };
        if let Some(v) = parsed {
            return v;
        }
    }
    60.0
}

/// Per-provider live usage/limit state.
#[derive(Default)]
struct ProviderHealth {
    /// Sliding window of (timestamp, total_tokens) for effective-TPM tracking.
    token_events: VecDeque<(f64, i64)>,
    request_events: VecDeque<f64>,
    remaining_requests: Option<i64>,
    remaining_tokens: Option<i64>,
    header_time: f64,
    cooldown_until: f64,
}

impl ProviderHealth {
    fn prune(&mut self, now: f64) {
        while let Some(&(ts, _)) = self.token_events.front() {
            if now - ts > WINDOW_S {
                self.token_events.pop_front();
            } else {
                break;
            }
        }
        while let Some(&ts) = self.request_events.front() {
            if now - ts > WINDOW_S {
                self.request_events.pop_front();
            } else {
                break;
            }
        }
    }

    fn tokens_in_window(&mut self, now: f64) -> i64 {
        self.prune(now);
        self.token_events.iter().map(|(_, t)| *t).sum()
    }

    fn requests_in_window(&mut self, now: f64) -> i64 {
        self.prune(now);
        self.request_events.len() as i64
    }
}

/// Live usage/limit state for every provider + the selection policy.
#[derive(Default)]
pub struct RotationTracker {
    health: HashMap<String, ProviderHealth>,
    tiebreak: usize,
}

impl RotationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    fn h(&mut self, provider_id: &str) -> &mut ProviderHealth {
        self.health.entry(provider_id.to_string()).or_default()
    }

    // -- feedback from completed requests ----------------------------------

    pub fn record_success(
        &mut self,
        provider_id: &str,
        total_tokens: Option<i64>,
        remaining_requests: Option<i64>,
        remaining_tokens: Option<i64>,
        now: f64,
    ) {
        let h = self.h(provider_id);
        h.request_events.push_back(now);
        if let Some(t) = total_tokens {
            if t != 0 {
                h.token_events.push_back((now, t));
            }
        }
        if remaining_requests.is_some() || remaining_tokens.is_some() {
            h.remaining_requests = remaining_requests;
            h.remaining_tokens = remaining_tokens;
            h.header_time = now;
        }
        // A successful call proves any cooldown is over.
        h.cooldown_until = 0.0;
    }

    pub fn record_rate_limited(&mut self, provider_id: &str, retry_after_s: Option<f64>, now: f64) {
        let h = self.h(provider_id);
        let delay = match retry_after_s {
            Some(r) if r > 0.0 => r,
            _ => DEFAULT_COOLDOWN_S,
        };
        // Cap pathological Retry-After values so a provider can always return.
        h.cooldown_until = now + delay.min(15.0 * 60.0);
        h.remaining_tokens = Some(0);
        h.remaining_requests = Some(0);
        h.header_time = now;
        // A 429 still consumed a request slot at the provider — count it so the
        // request-headroom denominator reflects real load, not just successes.
        h.request_events.push_back(now);
    }

    /// Non-429 failure (5xx, timeout) — brief cooldown so retries fan out.
    pub fn record_error(&mut self, provider_id: &str, now: f64) {
        let h = self.h(provider_id);
        h.cooldown_until = h.cooldown_until.max(now + 20.0);
        // A failed attempt typically still hit the provider; count it toward the
        // request window so headroom isn't overstated for a flaky provider.
        h.request_events.push_back(now);
    }

    pub fn is_cooling_down(&mut self, provider_id: &str, now: f64) -> bool {
        self.h(provider_id).cooldown_until > now
    }

    // -- selection ---------------------------------------------------------

    /// Headroom score for this provider in `[0, 1]`. Driven purely by live
    /// rate-limit headers when fresh (within HEADER_TTL_S); falls back to
    /// full score (1.0) when no signal is available — "unknown = assume plenty".
    /// We make NO assumptions about subscription tier or provider limits;
    /// the user's per-provider `quota_limit` is enforced by the caller, not here.
    pub fn headroom_score(&mut self, provider: &ProviderConfig, _est_tokens: i64, now: f64) -> f64 {
        let h = self.health.entry(provider.id.clone()).or_default();
        let headers_fresh = (now - h.header_time) <= HEADER_TTL_S && h.header_time > 0.0;

        if !headers_fresh {
            // No live signal yet — treat as fully healthy so all providers
            // start equal and the tiebreak round-robin distributes load.
            return 1.0;
        }

        // --- token headroom from live headers ---
        let token_frac = match h.remaining_tokens {
            Some(rt) => {
                let cap = (rt + h.tokens_in_window(now)).max(1);
                (rt as f64 / cap as f64).clamp(0.0, 1.0)
            }
            None => 1.0,
        };

        // --- request headroom from live headers ---
        let req_frac = match h.remaining_requests {
            Some(rr) => {
                let denom = (rr + h.requests_in_window(now)).max(1);
                (rr as f64 / denom as f64).clamp(0.0, 1.0)
            }
            None => 1.0,
        };

        token_frac.min(req_frac)
    }

    /// Return `providers` ordered best-first for this request. Cooling-down
    /// providers go to the back (soonest recovery first) — present but
    /// deprioritized, so the caller's fallback chain still reaches them.
    pub fn select(
        &mut self,
        providers: &[ProviderConfig],
        est_tokens: i64,
        now: f64,
    ) -> Vec<ProviderConfig> {
        let mut ready: Vec<(f64, ProviderConfig)> = Vec::new();
        let mut cooling: Vec<(f64, ProviderConfig)> = Vec::new();

        for p in providers {
            let cooldown_until = self.h(&p.id).cooldown_until;
            if cooldown_until > now {
                cooling.push((cooldown_until, p.clone()));
            } else {
                let score = self.headroom_score(p, est_tokens, now);
                ready.push((score, p.clone()));
            }
        }

        // Stable sort by score desc.
        ready.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

        // Rotate the equal-top group round-robin so equally-healthy providers
        // share the load.
        if ready.len() > 1 && ready[0].0 == ready[1].0 {
            let top = ready[0].0;
            let group: Vec<(f64, ProviderConfig)> =
                ready.iter().filter(|sp| sp.0 == top).cloned().collect();
            let rest: Vec<(f64, ProviderConfig)> =
                ready.iter().filter(|sp| sp.0 != top).cloned().collect();
            let k = self.tiebreak % group.len();
            self.tiebreak += 1;
            let mut rotated: Vec<(f64, ProviderConfig)> = Vec::with_capacity(ready.len());
            rotated.extend_from_slice(&group[k..]);
            rotated.extend_from_slice(&group[..k]);
            rotated.extend(rest);
            ready = rotated;
        }

        cooling.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        ready
            .into_iter()
            .map(|(_, p)| p)
            .chain(cooling.into_iter().map(|(_, p)| p))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: &str) -> ProviderConfig {
        ProviderConfig::new(pid, "https://api.example.com/v1")
    }
    fn p_host(pid: &str, base_url: &str) -> ProviderConfig {
        ProviderConfig::new(pid, base_url)
    }

    fn ids(order: &[ProviderConfig]) -> Vec<String> {
        order.iter().map(|c| c.id.clone()).collect()
    }

    // -- helpers -----------------------------------------------------------

    #[test]
    fn estimate_tokens_includes_reserve() {
        assert!(estimate_tokens("") > 0); // completion reserve even for empty input
        assert!(estimate_tokens(&"a".repeat(400)) > estimate_tokens(&"a".repeat(4)));
    }

    #[test]
    fn parse_rate_limit_headers_works() {
        let mut h = HashMap::new();
        h.insert("x-ratelimit-remaining-requests".into(), "12".into());
        h.insert("x-ratelimit-remaining-tokens".into(), "3450".into());
        assert_eq!(parse_rate_limit_headers(&h), (Some(12), Some(3450)));
        assert_eq!(parse_rate_limit_headers(&HashMap::new()), (None, None));
        let mut junk = HashMap::new();
        junk.insert("x-ratelimit-remaining-tokens".into(), "junk".into());
        assert_eq!(parse_rate_limit_headers(&junk), (None, None));
    }

    fn hdr(name: &str, value: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(name.into(), value.into());
        m
    }

    #[test]
    fn parse_retry_after_formats() {
        assert_eq!(parse_retry_after(&hdr("retry-after", "30")), 30.0);
        assert_eq!(parse_retry_after(&hdr("retry-after", "2.5s")), 2.5);
        assert_eq!(parse_retry_after(&hdr("retry-after", "1m")), 60.0);
        // Sub-second resets clamp to a 1 s floor.
        assert_eq!(
            parse_retry_after(&hdr("x-ratelimit-reset-tokens", "500ms")),
            1.0
        );
        assert_eq!(parse_retry_after(&HashMap::new()), 60.0);
        assert_eq!(parse_retry_after(&hdr("retry-after", "garbage")), 60.0);
    }

    // -- cooldown = the only hard exclusion --------------------------------

    #[test]
    fn rate_limit_puts_provider_at_back_until_retry_after() {
        let mut t = RotationTracker::new();
        let (a, b) = (p("a"), p("b"));
        t.record_rate_limited("a", Some(30.0), 1000.0);
        let order = t.select(&[a, b], 100, 1000.0);
        assert_eq!(order[0].id, "b"); // healthy first
        assert_eq!(order.last().unwrap().id, "a"); // cooling last
        assert!(t.is_cooling_down("a", 1000.0));
        assert_eq!(
            ids(&order)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            2
        );
        assert!(!t.is_cooling_down("a", 1031.0));
    }

    #[test]
    fn success_clears_cooldown() {
        let mut t = RotationTracker::new();
        t.record_rate_limited("a", Some(300.0), 0.0);
        assert!(t.is_cooling_down("a", 10.0));
        t.record_success("a", Some(50), None, None, 10.0);
        assert!(!t.is_cooling_down("a", 10.0));
    }

    #[test]
    fn all_cooling_down_still_returns_all() {
        let mut t = RotationTracker::new();
        let (a, b) = (p("a"), p("b"));
        t.record_rate_limited("a", Some(10.0), 0.0);
        t.record_rate_limited("b", Some(60.0), 0.0);
        let order = t.select(&[a, b], 100, 0.0);
        assert_eq!(ids(&order), ["a", "b"]); // a recovers sooner → first
    }

    // -- headroom ordering -------------------------------------------------

    #[test]
    fn live_headers_drive_ordering() {
        let mut t = RotationTracker::new();
        let (a, b) = (p_host("a", "https://a.com"), p_host("b", "https://b.com"));
        t.record_success("a", Some(10), Some(29), Some(5900), 100.0);
        t.record_success("b", Some(10), Some(2), Some(200), 100.0);
        let order = t.select(&[b, a], 100, 100.0);
        assert_eq!(order[0].id, "a"); // more headroom per the headers
    }

    #[test]
    fn unknown_provider_assumed_healthy() {
        let mut t = RotationTracker::new();
        let custom = p_host("custom", "https://my-llm.local/v1");
        let order = t.select(std::slice::from_ref(&custom), 5000, 0.0);
        assert_eq!(ids(&order), ["custom"]);
    }

    #[test]
    fn equal_headroom_rotates_round_robin() {
        let mut t = RotationTracker::new();
        let (a, b, c) = (p("a"), p("b"), p("c")); // all unknown host → equal full score
        let firsts: Vec<String> = (0..3)
            .map(|_| {
                t.select(&[a.clone(), b.clone(), c.clone()], 10, 0.0)[0]
                    .id
                    .clone()
            })
            .collect();
        // Round-robin tie-break means the front rotates rather than sticking.
        assert!(
            firsts
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1
        );
    }

    #[test]
    fn rate_limited_attempts_count_toward_request_window() {
        // A 429 consumes a request slot at the provider. After the cooldown
        // elapses, the provider's request-headroom must reflect that consumed
        // attempt: the window denominator includes it, not only successes.
        let mut t = RotationTracker::new();
        let p = p_host("a", "https://a.com");
        // 429 at t=0 (record_rate_limited records the attempt in the window).
        t.record_rate_limited("a", Some(5.0), 0.0);
        // After cooldown a call succeeds, refreshing the header to remaining=1.
        t.record_success("a", Some(10), Some(1), None, 6.0);
        // Window now holds two attempts (the 429 at 0 and the success at 6),
        // both within 60s. req_frac = remaining / (remaining + in_window)
        //                          = 1 / (1 + 2) = 0.333  (not 1/2 = 0.5).
        let score = t.headroom_score(&p, 10, 6.0);
        assert!(
            (score - (1.0 / 3.0)).abs() < 1e-6,
            "429 attempt must be counted in the request window, got {score}"
        );
    }
}
