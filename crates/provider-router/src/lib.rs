//! Pure provider routing + smart rotation.
//!
//! Ported from Grain's Python implementation:
//! - [`router`]   ← `router.py`       (round-robin selection, daily quota gate)
//! - [`rotation`] ← `llm_rotation.py` (live-signal smart rotation: cooldowns,
//!   headroom, effective-context routing) + `llm_client._parse_retry_after`
//!
//! No network, no Tauri. Decides *which* provider to use next given live state.
//! The same engine backs both router instances: one for STT providers, one for
//! LLM providers. UI-layer concerns (the rotation-off "radio" provider-enable
//! behavior tested in `test_llm_provider_routing.py`) live in the settings layer,
//! not here.

pub mod model;
pub mod rotation;
pub mod router;

pub use model::{AppSettings, ProviderConfig, ProviderError};
pub use rotation::{
    estimate_tokens, parse_rate_limit_headers, parse_retry_after, RotationTracker,
    COMPLETION_RESERVE_TOKENS,
};
pub use router::{ProviderPool, Router, SettingsStore};
