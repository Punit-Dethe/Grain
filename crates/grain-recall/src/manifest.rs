//! Session manifest: the authoritative description of one meeting directory.
//!
//! The app writes it atomically (`manifest.json.tmp` → `manifest.json`); a
//! reader only ever sees a complete file.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST: &str = "manifest.json";
pub const MANIFEST_TMP: &str = "manifest.json.tmp";
pub const MAX_CHANNELS: u16 = 8;
const NANOS: u128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SessionState {
    /// Capture may still append; unsealed chunks may exist.
    Recording,
    /// Every chunk is sealed; the recording is complete or recovered.
    Recorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackSource {
    Microphone { device: String },
    Application { name: String },
    Imported { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapReason {
    /// The source stopped delivering audio (device removed, app exited).
    SourceLost,
    /// Captured audio was dropped before it could be written.
    Overflow,
    /// The device or its format changed.
    DeviceChanged,
    /// Recording was paused.
    Paused,
    /// Audio that was recorded is no longer on disk.
    Missing,
    /// The source delivered nothing because nothing was playing (system
    /// audio loopback). Known silence, kept compact on disk.
    Silent,
    /// Capture resumed late without a stated reason.
    Unexplained,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Meeting-clock interval in nanoseconds from the session start.
    pub start_ns: u64,
    pub end_ns: u64,
    pub reason: GapReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub seq: u32,
    pub sample_rate: u32,
    pub channels: u16,
    /// Meeting-clock position of the chunk's first frame.
    pub start_ns: u64,
    pub frames: u64,
    pub sealed: bool,
    /// Sealed by crash recovery rather than by the writer.
    pub recovered: bool,
}

impl Chunk {
    pub fn end_ns(&self) -> u64 {
        self.start_ns + frames_to_ns(self.frames, self.sample_rate)
    }

    /// File name of the chunk's audio inside its track directory.
    pub fn file_name(&self) -> String {
        if self.sealed {
            format!("{:06}.wav", self.seq)
        } else {
            format!("{:06}.wav.partial", self.seq)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub source: TrackSource,
    pub chunks: Vec<Chunk>,
    pub gaps: Vec<Gap>,
}

impl Track {
    /// Meeting-clock end of the track's recorded audio.
    pub fn end_ns(&self) -> u64 {
        self.chunks.last().map_or(0, Chunk::end_ns)
    }

    pub fn next_seq(&self) -> u32 {
        self.chunks.last().map_or(0, |chunk| chunk.seq + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub created_unix_ms: u64,
    pub state: SessionState,
    pub tracks: Vec<Track>,
}

impl Manifest {
    /// Longest track extent on the meeting clock.
    pub fn duration_ns(&self) -> u64 {
        self.tracks.iter().map(Track::end_ns).max().unwrap_or(0)
    }

    /// Structural checks; `dir_name`, when given, must equal the session id.
    pub fn validate(&self, dir_name: Option<&str>) -> Result<(), String> {
        if self.version != MANIFEST_VERSION {
            return Err(format!(
                "unsupported meeting manifest version {}",
                self.version
            ));
        }
        parse_session_id(&self.id)?;
        if dir_name.is_some_and(|name| name != self.id) {
            return Err("meeting manifest does not belong to its directory".into());
        }
        let mut ids = std::collections::HashSet::new();
        for track in &self.tracks {
            if !ids.insert(track.id) {
                return Err(format!("duplicate meeting track {}", track.id));
            }
            let mut previous: Option<&Chunk> = None;
            for chunk in &track.chunks {
                if chunk.sample_rate == 0 || chunk.channels == 0 || chunk.channels > MAX_CHANNELS {
                    return Err(format!(
                        "track {} chunk {} has an invalid format",
                        track.id, chunk.seq
                    ));
                }
                if let Some(previous) = previous {
                    if chunk.seq <= previous.seq
                        || chunk.start_ns < previous.end_ns()
                        || !previous.sealed
                    {
                        return Err(format!("track {} chunks are out of order", track.id));
                    }
                }
                previous = Some(chunk);
            }
            if self.state == SessionState::Recorded && track.chunks.iter().any(|c| !c.sealed) {
                return Err(format!("recorded track {} has an unsealed chunk", track.id));
            }
            for gap in &track.gaps {
                if gap.end_ns <= gap.start_ns {
                    return Err(format!("track {} has an empty gap", track.id));
                }
            }
        }
        Ok(())
    }
}

pub fn frames_to_ns(frames: u64, sample_rate: u32) -> u64 {
    u64::try_from(u128::from(frames) * NANOS / u128::from(sample_rate.max(1))).unwrap_or(u64::MAX)
}

/// Session ids are generated UUIDs; anything else (including path syntax) is refused.
pub fn parse_session_id(id: &str) -> Result<uuid::Uuid, String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "invalid meeting id".to_string())?;
    if parsed.hyphenated().to_string() != id {
        return Err("invalid meeting id".into());
    }
    Ok(parsed)
}

pub fn session_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    parse_session_id(id)?;
    Ok(root.join(id))
}

pub fn io_error(context: &str, error: io::Error) -> String {
    format!("{context}: {error}")
}

pub fn load_manifest(session: &Path) -> Result<Manifest, String> {
    let bytes = fs::read(session.join(MANIFEST))
        .map_err(|error| io_error("Could not read the meeting manifest", error))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Meeting manifest is corrupt: {error}"))?;
    manifest.validate(session.file_name().and_then(|name| name.to_str()))?;
    Ok(manifest)
}

/// Lightweight listing data, read from the manifest only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_unix_ms: u64,
    pub state: SessionState,
    pub duration_ms: u64,
}

/// List sessions under `root`, newest first. Unreadable entries are skipped
/// and logged; no audio is opened.
pub fn list_sessions(root: &Path) -> Result<Vec<SessionSummary>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error("Could not read the meetings folder", error)),
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if parse_session_id(name).is_err() || !path.is_dir() {
            continue;
        }
        match load_manifest(&path) {
            Ok(manifest) => sessions.push(SessionSummary {
                duration_ms: manifest.duration_ns() / 1_000_000,
                id: manifest.id,
                title: manifest.title,
                created_unix_ms: manifest.created_unix_ms,
                state: manifest.state,
            }),
            Err(error) => log::warn!("Skipping meeting {name}: {error}"),
        }
    }
    sessions.sort_by(|a, b| {
        b.created_unix_ms
            .cmp(&a.created_unix_ms)
            .then(a.id.cmp(&b.id))
    });
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_refuse_path_syntax_and_non_canonical_forms() {
        let id = "0b5c8f1e-2d3a-4c5b-9e6f-7a8b9c0d1e2f";
        assert!(parse_session_id(id).is_ok());
        assert!(parse_session_id(&id.to_uppercase()).is_err());
        assert!(parse_session_id("../etc").is_err());
        assert!(parse_session_id("0b5c8f1e2d3a4c5b9e6f7a8b9c0d1e2f").is_err());
        assert!(session_dir(Path::new("root"), "..\\x").is_err());
    }
}
