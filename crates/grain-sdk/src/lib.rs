//! Shared desktop events and MCP connection contracts.

pub mod action;
pub mod event;
pub mod mcp;
pub mod network;

pub use event::{DaemonEvent, OverlayPosition, ResolvedTheme, SessionMode, PILL_ICON_PX};
