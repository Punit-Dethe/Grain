//! # grain-sdk — Grain's public wire contract
//!
//! The **dependency leaf** of the workspace (SPEC §7.3): this crate depends
//! only on `serde`/`specta`, and everything that speaks Grain's protocol —
//! `grain-core`, `grain-pill`, the Tauri shell, and eventually third-party
//! extensions — depends on *it*, never the reverse. That direction is what
//! lets the contract be versioned independently of Grain's internals.
//!
//! Contents:
//! - [`event`] — the typed [`DaemonEvent`] stream the core broadcasts, and the
//!   [`PillAction`] reverse channel.
//! - [`protocol`] — the authenticated-connection handshake for the local
//!   WebSocket ([`ClientHello`] / [`ServerWelcome`], SPEC §7.1): identity is
//!   bound to the *channel* by a per-client token presented in the first
//!   frame, never claimed in message payloads.
//!
//! Versioning: [`GRAIN_API_VERSION`] is the contract's semver. Additive
//! changes (new event variants, new optional fields) bump the minor; breaking
//! changes bump the major and are expected to be rare-to-never (R1: grant
//! narrowly, widen later).

pub mod authoring;
pub mod distribution;
pub mod error;
pub mod event;
pub mod flagged;
pub mod manifest;
pub mod pill_skin;
pub mod pill_theme;
pub mod protocol;
pub mod settings_schema;
pub mod view;

pub use authoring::{ExtensionProjectManifest, GRAIN_API_TYPESCRIPT};
pub use distribution::{
    is_category, Index, IndexEntry, MediaRef, RevocationEntry, RevocationState, Revocations, Roots,
    Trust, CATEGORIES, DISTRIBUTION_SPEC, EXPIRY_CLOCK_SKEW_SECS,
};
pub use error::{HostError, HostErrorCode};
pub use event::{
    daemon_event_capability, AgentInputKind, DaemonEvent, OverlayPosition, PillAction,
    RecommendCandidate, ResolvedTheme, SessionMode, DAEMON_EVENT_VARIANTS, PILL_ICON_PX,
};
pub use flagged::{flagged_combinations, FlaggedCombination};
pub use manifest::{
    png_dimensions, validate_extension_id, validate_extension_version, AuthenticationDecl,
    AuthenticationType, CompanionDecl, Contributes, ExtensionManifest, GrainPack, OverlayDecl,
    PackPayloads, PromptPackEntry, PromptTarget, RedirectMethod, SelectOption, SettingDecl,
    SettingKind, ShortcutDecl, Surfaces, Tier, WorkspaceDecl, ANCHORS, ICON_MASTER_DIM,
    ICON_MAX_BYTES, KNOWN_CAPABILITIES, KNOWN_SLOTS, PACK_ENTRY_MAX_BYTES, PACK_MAX_BYTES,
    PROMPT_CONTEXT_SLOT, PROMPT_MAIN_SLOT, SURFACE_PROMPTS,
};
pub use pill_skin::PillSkin;
pub use pill_theme::{PillPattern, PillStateTheme, PillTheme};
pub use protocol::{
    ClientHello, ClientRequest, DevControlFrame, DevReloadResult, HostCall, HostCallResult,
    HostFrame, ServerResponse, ServerWelcome, GRAIN_API_VERSION,
};
pub use settings_schema::Accepted;
pub use view::{
    ExtensionView, ExtensionViewEvent, ViewAction, ViewActionIntent, ViewActionKind, ViewAlign,
    ViewGap, ViewHeadingLevel, ViewNode, ViewOption, ViewTone, ViewValue, VIEW_MAX_ACTIONS,
    VIEW_MAX_CHILDREN, VIEW_MAX_DEPTH, VIEW_MAX_EVENT_BYTES, VIEW_MAX_FIELDS, VIEW_MAX_NODES,
    VIEW_MAX_OPTIONS_PER_SELECT, VIEW_MAX_PAYLOAD_BYTES, VIEW_MAX_TOTAL_OPTIONS,
    VIEW_MAX_TOTAL_TEXT_BYTES, VIEW_SCHEMA_VERSION,
};
