//! Read model of Recall's meetings on disk.
//!
//! ```text
//! <root>/<session-uuid>/manifest.json            session + audio layout
//! <root>/<session-uuid>/transcripts/000001.json  published revisions
//! <root>/<session-uuid>/summary.json             AI summary, if written
//! ```
//!
//! The app (`src-tauri/src/grain_meet`) records and writes these files; this
//! crate holds the formats and the read side, so the Recall MCP sidecar can
//! read meetings without linking the app. Nothing here writes.

pub mod manifest;
pub mod summary;
pub mod text;
pub mod transcript;
