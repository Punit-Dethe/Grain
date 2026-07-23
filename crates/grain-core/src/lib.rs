//! Grain's headless core: Tauri-free, the shared substrate the daemon runs on
//! and the Tauri settings shell wraps.
//!
//! - [`context::AppContext`] — owned settings (`RwLock`), the event bus, and the
//!   resource/data paths. The headless replacement for `tauri::AppHandle`.
//! - [`event::DaemonEvent`] — the typed broadcast stream.
//! - [`settings`] — the full `AppSettings` schema + the production store.
//!
//! No dependency on Tauri, audio backends, or any ASR engine. The managers
//! (audio/model/transcription/history) migrate onto `AppContext` here over the
//! decoupling phase; until then this crate stands alone and tested.

pub mod context;
pub mod extensions;
// The event/action wire types moved to grain-sdk (the dependency leaf);
// this alias keeps every `grain_core::event::X` path compiling unchanged.
pub use grain_sdk as event;
pub mod settings;
// [GRAIN] Phase 5A: pinned-key verification of the signed extension catalogue.
pub mod trust;

pub use context::{settings_file_exists, AppContext};
pub use grain_sdk::{AgentInputKind, DaemonEvent, PillAction, SessionMode};
pub use settings::{
    ActionTarget, AppMatch, AppMode, AppSettings, DictCandidate, PostProcessProvider, SecretMap,
    SttProvider, SttProviderKind, VoiceAction, STT_LOCAL_PROVIDER_ID,
};
