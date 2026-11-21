//! Grain Recall MCP server: lets AI apps (Claude, Gemini CLI, Codex, Cursor…)
//! read the user's recorded meetings.
//!
//! The AI app launches this binary over stdio only while it needs it; it is
//! never part of Grain's own process. It reads the meeting files directly and
//! never writes. Every request first checks the gate file Grain keeps while
//! the "Use your meetings in AI apps" switch is on, so turning the switch off
//! cuts access at once, even for a running session.

pub mod server;

use std::path::{Path, PathBuf};

/// Present while the user allows AI apps to read Recall. Grain owns it.
pub use grain_recall::GATE_FILE;

/// What every tool and resource answers while the switch is off.
pub const OFF: &str = "Recall access is turned off in Grain. The user can turn it on in \nGrain > Recall > Settings > \"Use your meetings in AI apps\".";

/// Where one Grain install keeps its meetings.
#[derive(Debug, Clone)]
pub struct Recall {
    meetings: PathBuf,
}

impl Recall {
    /// `data_dir` is Grain's app data folder (it holds `meetings/`).
    pub fn new(data_dir: &Path) -> Self {
        Self {
            meetings: data_dir.join("meetings"),
        }
    }

    pub fn meetings_root(&self) -> &Path {
        &self.meetings
    }

    pub fn enabled(&self) -> bool {
        self.meetings.join(GATE_FILE).is_file()
    }
}
