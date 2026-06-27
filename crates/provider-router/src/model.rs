//! Shared data types for provider routing.
//!
//! Minimal ports of the fields `router`/`rotation` actually need from the Python
//! `models.ProviderConfig` / `models.AppSettings` and `exceptions.ProviderError`.
//! The full daemon settings map onto these at the wiring layer.

use std::fmt;

/// Configuration for a single STT or LLM provider.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// `None` = unlimited.
    pub quota_limit: Option<i64>,
    pub quota_used_today: i64,
    /// Whether this provider participates in routing.
    pub enabled: bool,
}

impl ProviderConfig {
    /// Construct with sensible defaults (unlimited quota, enabled, `name == id`).
    pub fn new(id: &str, base_url: &str) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            base_url: base_url.to_string(),
            model: "m".to_string(),
            quota_limit: None,
            quota_used_today: 0,
            enabled: true,
        }
    }

    /// Builder: set the daily quota limit and the count already used today.
    pub fn with_quota(mut self, limit: Option<i64>, used_today: i64) -> Self {
        self.quota_limit = limit;
        self.quota_used_today = used_today;
        self
    }
}

/// The provider lists the router reads/persists. (Subset of the Python
/// `AppSettings` — only what routing touches.)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppSettings {
    pub stt_providers: Vec<ProviderConfig>,
    pub llm_providers: Vec<ProviderConfig>,
}

impl AppSettings {
    pub fn new(stt_providers: Vec<ProviderConfig>, llm_providers: Vec<ProviderConfig>) -> Self {
        Self {
            stt_providers,
            llm_providers,
        }
    }
}

/// Raised when no eligible provider remains. Port of `exceptions.ProviderError`
/// for the one case the router produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    AllExhausted,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::AllExhausted => write!(f, "All providers exhausted"),
        }
    }
}

impl std::error::Error for ProviderError {}
