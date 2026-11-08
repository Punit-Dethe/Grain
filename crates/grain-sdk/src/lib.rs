//! # grain-sdk — Grain's public wire contract
//!
//! The **dependency leaf** of the workspace (SPEC §7.3): this crate depends
//! only on `serde`/`specta`, and everything that speaks Grain's protocol —
//! `grain-core`, the Tauri shell, and third-party
//! extensions — depends on *it*, never the reverse. That direction is what
//! lets the contract be versioned independently of Grain's internals.
//!
//! Contents:
//! - [`event`] — the typed [`DaemonEvent`] stream for Grain-owned surfaces.
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
pub mod compatibility;
pub mod error;
pub mod event;
mod fixture_trust;
pub mod manifest;
pub mod mcp;
pub mod protocol;

pub use authoring::ExtensionProjectManifest;
pub use error::{HostError, HostErrorCode};
pub use event::{DaemonEvent, OverlayPosition, ResolvedTheme, SessionMode, PILL_ICON_PX};
pub use fixture_trust::Trust;
pub use manifest::{
    png_dimensions, validate_extension_id, validate_extension_version, AuthenticationDecl,
    AuthenticationType, Contributes, ExtensionManifest, GrainPack, PackPayloads, RedirectMethod,
    Tier, ICON_MASTER_DIM, ICON_MAX_BYTES, KNOWN_CAPABILITIES, PACK_ENTRY_MAX_BYTES,
    PACK_MAX_BYTES,
};
pub use protocol::{
    ClientHello, ClientRequest, DevControlFrame, DevReloadResult, HostCall, HostCallResult,
    HostFrame, ServerResponse, ServerWelcome, GRAIN_API_VERSION,
};
