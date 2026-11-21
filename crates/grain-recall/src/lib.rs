//! Read model of Recall's meetings on disk.
//!
//! ```text
//! <root>/<session-uuid>/manifest.json            session + audio layout
//! <root>/<session-uuid>/transcripts/000001.json  published revisions
//! <root>/<session-uuid>/summary.json             AI summary, if written
//! ```
//!
//! The app (`src-tauri/src/grain_meet`) records and writes these files; this
//! crate holds the formats and the read side (`query`), shared by the Recall
//! MCP sidecar and Grain's Agent so both give the same answers. Nothing
//! here writes.

#[cfg(any(test, feature = "fixtures"))]
pub mod fixture;
pub mod manifest;
pub mod query;
pub mod render;
pub mod spec;
pub mod summary;
pub mod text;
pub mod transcript;

/// Present in the meetings folder while the user lets AI apps read Recall
/// (Grain's "Use your meetings in AI apps" switch). The app creates and
/// deletes it; the MCP sidecar refuses every request while it is absent.
pub const GATE_FILE: &str = ".mcp-enabled";
