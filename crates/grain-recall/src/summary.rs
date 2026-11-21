//! A meeting's saved AI summary: `summary.json`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const SUMMARY_FILE: &str = "summary.json";

/// A saved AI summary, tied to the transcript revision it was written from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MeetSummaryDoc {
    pub transcript_revision: u32,
    pub created_unix_ms: u64,
    /// Markdown.
    pub text: String,
}

/// The saved summary, if one exists and is readable.
pub fn read_summary(dir: &Path) -> Option<MeetSummaryDoc> {
    let bytes = fs::read(dir.join(SUMMARY_FILE)).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(doc) => Some(doc),
        Err(error) => {
            log::warn!("Ignoring unreadable meeting summary: {error}");
            None
        }
    }
}
