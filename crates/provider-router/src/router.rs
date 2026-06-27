//! Round-robin provider selection with per-provider daily quota enforcement.
//!
//! Ported from `open_voice_router/router.py`.
//!
//! Deviations from the Python (ownership, not behavior):
//! - The round-robin index lives in the [`ProviderPool`] rather than in a
//!   `dict` keyed by `id(pool)` — Rust has no object identity to key on, and a
//!   per-pool index is behaviorally identical (independent index per pool).
//! - Persistence is injected via the [`SettingsStore`] trait instead of a
//!   concrete store, keeping this crate free of any real I/O.

use crate::model::{AppSettings, ProviderConfig, ProviderError};

/// Sink for persisting quota state after every mutation. The daemon implements
/// this over its real settings file; tests use an in-memory fake.
pub trait SettingsStore {
    fn save(&mut self, settings: &AppSettings);
}

/// Ordered list of providers for one layer (STT or LLM), carrying its own
/// round-robin cursor.
pub struct ProviderPool {
    pub providers: Vec<ProviderConfig>,
    rr_index: usize,
}

impl ProviderPool {
    pub fn new(providers: Vec<ProviderConfig>) -> Self {
        Self {
            providers,
            rr_index: 0,
        }
    }
}

/// Round-robin selection with daily quota enforcement.
pub struct Router<S: SettingsStore> {
    settings: AppSettings,
    store: S,
}

impl<S: SettingsStore> Router<S> {
    pub fn new(settings: AppSettings, store: S) -> Self {
        Self { settings, store }
    }

    /// Return the next eligible provider from `pool` in round-robin order. The
    /// pool's cursor advances past the returned provider.
    ///
    /// Returns [`ProviderError::AllExhausted`] if every provider in the pool has
    /// exceeded its daily quota (or the pool is empty).
    pub fn next_provider(&self, pool: &mut ProviderPool) -> Result<ProviderConfig, ProviderError> {
        let n = pool.providers.len();
        if n == 0 {
            return Err(ProviderError::AllExhausted);
        }
        let start = pool.rr_index;
        for offset in 0..n {
            let idx = (start + offset) % n;
            if self.is_eligible(&pool.providers[idx]) {
                pool.rr_index = (idx + 1) % n;
                return Ok(pool.providers[idx].clone());
            }
        }
        Err(ProviderError::AllExhausted)
    }

    /// True if `provider` has not exceeded its daily quota. A provider with
    /// `quota_limit = None` is always eligible (unlimited).
    pub fn is_eligible(&self, provider: &ProviderConfig) -> bool {
        match provider.quota_limit {
            None => true,
            Some(limit) => provider.quota_used_today < limit,
        }
    }

    /// Increment `quota_used_today` for `provider_id` (searching both pools) and
    /// persist. Unknown ids are a no-op, but persistence still happens — matching
    /// the Python contract.
    pub fn record_usage(&mut self, provider_id: &str) {
        for p in self
            .settings
            .stt_providers
            .iter_mut()
            .chain(self.settings.llm_providers.iter_mut())
        {
            if p.id == provider_id {
                p.quota_used_today += 1;
                break;
            }
        }
        self.store.save(&self.settings);
    }

    /// Reset `quota_used_today` to 0 for every provider in both pools, and
    /// persist. Called at midnight (timer set up externally).
    pub fn reset_daily_counts(&mut self) {
        for p in self
            .settings
            .stt_providers
            .iter_mut()
            .chain(self.settings.llm_providers.iter_mut())
        {
            p.quota_used_today = 0;
        }
        self.store.save(&self.settings);
    }

    /// Replace the in-memory settings (e.g. after the user edits provider config).
    pub fn update_settings(&mut self, settings: AppSettings) {
        self.settings = settings;
    }

    // -- accessors (Rust ownership: callers inspect state through the Router) --

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    /// `quota_used_today` for a provider id (searching both pools), for tests.
    pub fn quota_used(&self, provider_id: &str) -> Option<i64> {
        self.settings
            .stt_providers
            .iter()
            .chain(self.settings.llm_providers.iter())
            .find(|p| p.id == provider_id)
            .map(|p| p.quota_used_today)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory stand-in for the real settings store — records save() calls.
    #[derive(Default)]
    struct FakeStore {
        save_count: usize,
        last_saved: Option<AppSettings>,
    }
    impl SettingsStore for FakeStore {
        fn save(&mut self, settings: &AppSettings) {
            self.save_count += 1;
            self.last_saved = Some(settings.clone());
        }
    }

    fn provider(pid: &str, quota_limit: Option<i64>, used: i64) -> ProviderConfig {
        ProviderConfig::new(pid, "https://example.com").with_quota(quota_limit, used)
    }

    fn router(stt: Vec<ProviderConfig>, llm: Vec<ProviderConfig>) -> Router<FakeStore> {
        Router::new(AppSettings::new(stt, llm), FakeStore::default())
    }

    // -- round-robin -------------------------------------------------------

    #[test]
    fn round_robin_cycles_through_all_providers() {
        let ps = vec![
            provider("p1", None, 0),
            provider("p2", None, 0),
            provider("p3", None, 0),
        ];
        let r = router(ps.clone(), vec![]);
        let mut pool = ProviderPool::new(ps);
        let ids: Vec<String> = (0..3)
            .map(|_| r.next_provider(&mut pool).unwrap().id)
            .collect();
        assert_eq!(ids, ["p1", "p2", "p3"]);
    }

    #[test]
    fn round_robin_wraps_around() {
        let ps = vec![provider("p1", None, 0), provider("p2", None, 0)];
        let r = router(ps.clone(), vec![]);
        let mut pool = ProviderPool::new(ps);
        let ids: Vec<String> = (0..4)
            .map(|_| r.next_provider(&mut pool).unwrap().id)
            .collect();
        assert_eq!(ids, ["p1", "p2", "p1", "p2"]);
    }

    // -- quota / eligibility ----------------------------------------------

    #[test]
    fn exhausted_provider_is_skipped() {
        let ps = vec![provider("full", Some(5), 5), provider("ok", Some(10), 0)];
        let r = router(ps.clone(), vec![]);
        let mut pool = ProviderPool::new(ps);
        for _ in 0..5 {
            assert_eq!(r.next_provider(&mut pool).unwrap().id, "ok");
        }
    }

    #[test]
    fn all_exhausted_raises_provider_error() {
        let ps = vec![provider("p1", Some(3), 3), provider("p2", Some(1), 1)];
        let r = router(ps.clone(), vec![]);
        let mut pool = ProviderPool::new(ps);
        assert_eq!(r.next_provider(&mut pool), Err(ProviderError::AllExhausted));
    }

    #[test]
    fn unlimited_quota_never_exhausted() {
        let p = provider("unlimited", None, 9999);
        let r = router(vec![p.clone()], vec![]);
        let mut pool = ProviderPool::new(vec![p]);
        for _ in 0..10 {
            assert_eq!(r.next_provider(&mut pool).unwrap().id, "unlimited");
        }
    }

    #[test]
    fn is_eligible_reflects_limit() {
        let r = router(vec![], vec![]);
        assert!(!r.is_eligible(&provider("p", Some(5), 5)));
        assert!(r.is_eligible(&provider("p", Some(5), 4)));
        assert!(r.is_eligible(&provider("p", None, 10000)));
    }

    // -- record_usage ------------------------------------------------------

    #[test]
    fn record_usage_increments_counter() {
        let mut r = router(vec![provider("p1", Some(10), 2)], vec![]);
        r.record_usage("p1");
        assert_eq!(r.quota_used("p1"), Some(3));
    }

    #[test]
    fn record_usage_persists_to_store() {
        let mut r = router(vec![provider("p1", None, 0)], vec![]);
        r.record_usage("p1");
        assert_eq!(r.store().save_count, 1);
        assert_eq!(r.store().last_saved.as_ref(), Some(r.settings()));
    }

    #[test]
    fn record_usage_unknown_provider_is_noop_but_persists() {
        let mut r = router(vec![], vec![]);
        r.record_usage("nonexistent");
        assert_eq!(r.store().save_count, 1);
    }

    #[test]
    fn record_usage_works_for_llm_providers() {
        let mut r = router(vec![], vec![provider("llm1", Some(5), 0)]);
        r.record_usage("llm1");
        assert_eq!(r.quota_used("llm1"), Some(1));
    }

    // -- reset_daily_counts ------------------------------------------------

    #[test]
    fn reset_daily_counts_zeroes_all_and_persists() {
        let mut r = router(
            vec![provider("p1", Some(10), 7), provider("p2", Some(5), 5)],
            vec![provider("p3", None, 3)],
        );
        r.reset_daily_counts();
        assert_eq!(r.quota_used("p1"), Some(0));
        assert_eq!(r.quota_used("p2"), Some(0));
        assert_eq!(r.quota_used("p3"), Some(0));
        assert_eq!(r.store().save_count, 1);
    }

    // -- update_settings ---------------------------------------------------

    #[test]
    fn update_settings_replaces_provider_list() {
        let mut r = router(vec![provider("old", None, 0)], vec![]);
        r.update_settings(AppSettings::new(vec![provider("new", None, 0)], vec![]));
        let mut pool = ProviderPool::new(vec![provider("new", None, 0)]);
        assert_eq!(r.next_provider(&mut pool).unwrap().id, "new");
    }
}
