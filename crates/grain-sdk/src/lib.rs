//! Shared desktop events and MCP connection contracts.

pub mod action;
pub mod event;
mod identity;
pub mod mcp;
pub mod network;

pub use event::{DaemonEvent, OverlayPosition, ResolvedTheme, SessionMode, PILL_ICON_PX};
pub use identity::{validate_extension_id, validate_extension_version};
