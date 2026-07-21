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

pub mod event;
pub mod protocol;

pub use event::{AgentInputKind, DaemonEvent, OverlayPosition, PillAction, SessionMode};
pub use protocol::{ClientHello, ServerWelcome, GRAIN_API_VERSION};
