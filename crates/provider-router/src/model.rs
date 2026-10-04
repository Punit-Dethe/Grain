//! Shared data types for provider routing.
//!
//! The AI text rotation tracker uses this compact provider configuration.

/// Configuration for a single AI text provider.
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
