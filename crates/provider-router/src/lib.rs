//! Pure AI text provider rotation: cooldowns, rate-limit headroom and token budgets.
//! No network or Tauri; callers own persistence and quota enforcement.
pub mod model;
pub mod rotation;
pub use model::ProviderConfig;
pub use rotation::{
    estimate_tokens, parse_rate_limit_headers, parse_retry_after, RotationTracker,
    COMPLETION_RESERVE_TOKENS,
};
