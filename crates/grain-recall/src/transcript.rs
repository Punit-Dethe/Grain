//! Published transcript revisions: `transcripts/NNNNNN.json`, one file per
//! revision, written atomically and never modified afterwards.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const TRANSCRIPT_VERSION: u32 = 1;
pub const TRANSCRIPTS: &str = "transcripts";

/// One speaker slot's activity on the meeting clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Activity {
    pub slot: i32,
    pub start_ns: u64,
    pub end_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Attribution {
    Speaker(i32),
    /// The diarizer found no speech on this track.
    Unassigned,
    /// Speaker detection did not run or failed for this track.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TrackStatus {
    Complete,
    /// Text without speakers; the transcript is still useful.
    SpeakersUnavailable {
        reason: String,
    },
    /// No text for this track; its audio is kept for a retry.
    Failed {
        reason: String,
    },
    /// The track holds no audio.
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Word {
    pub text: String,
    /// Meeting clock, milliseconds from the session start.
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Attribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Speaker {
    /// Stable within a revision: `t{track}-s{slot}`.
    pub id: String,
    pub track: u32,
    pub slot: i32,
    /// Anonymous label, numbered across the meeting.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackTranscript {
    pub track: u32,
    pub status: TrackStatus,
    /// The reviewed full-sequence text of the track's ASR.
    pub text: String,
    pub words: Vec<Word>,
    /// Raw diarizer activity kept as evidence, so attribution can be
    /// recomputed without repeating capture or inference.
    pub activity: Vec<Activity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ModelRef {
    pub file: String,
    pub arch: String,
    pub variant: String,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TranscriptRevision {
    pub version: u32,
    pub revision: u32,
    pub session_id: String,
    pub attempt_id: String,
    pub created_unix_ms: u64,
    pub diarizer: Option<ModelRef>,
    pub asr: Option<ModelRef>,
    pub speakers: Vec<Speaker>,
    pub tracks: Vec<TrackTranscript>,
}

/// Number speakers across the meeting in track, then slot order.
pub fn speakers(tracks: &[TrackTranscript]) -> Vec<Speaker> {
    let mut speakers = Vec::new();
    for track in tracks {
        let slots: BTreeSet<i32> = track
            .words
            .iter()
            .filter_map(|w| match w.speaker {
                Attribution::Speaker(slot) => Some(slot),
                _ => None,
            })
            .collect();
        for slot in slots {
            speakers.push(Speaker {
                id: format!("t{}-s{slot}", track.track),
                track: track.track,
                slot,
                label: format!("Speaker {}", speakers.len() + 1),
            });
        }
    }
    speakers
}

pub fn revision_path(session: &Path, revision: u32) -> PathBuf {
    session
        .join(TRANSCRIPTS)
        .join(format!("{revision:06}.json"))
}

fn revisions(session: &Path) -> Result<Vec<u32>, String> {
    let dir = session.join(TRANSCRIPTS);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read meeting transcripts: {e}")),
    };
    let mut found: Vec<u32> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".json")?;
            (stem.len() == 6).then(|| stem.parse().ok()).flatten()
        })
        .collect();
    found.sort_unstable();
    Ok(found)
}

pub fn next_revision(session: &Path) -> Result<u32, String> {
    Ok(revisions(session)?.last().map_or(1, |last| last + 1))
}

/// The newest revision number without reading any transcript.
pub fn latest_revision(session: &Path) -> Result<Option<u32>, String> {
    Ok(revisions(session)?.last().copied())
}

/// The newest published revision, if any.
pub fn latest(session: &Path) -> Result<Option<TranscriptRevision>, String> {
    let Some(revision) = revisions(session)?.pop() else {
        return Ok(None);
    };
    let bytes = fs::read(revision_path(session, revision))
        .map_err(|e| format!("Could not read the meeting transcript: {e}"))?;
    let parsed: TranscriptRevision = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Meeting transcript is corrupt: {e}"))?;
    if parsed.version != TRANSCRIPT_VERSION || parsed.revision != revision {
        return Err("Meeting transcript has an unexpected version".into());
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(number: u32) -> TranscriptRevision {
        TranscriptRevision {
            version: TRANSCRIPT_VERSION,
            revision: number,
            session_id: "s".into(),
            attempt_id: "a".into(),
            created_unix_ms: 0,
            diarizer: None,
            asr: None,
            speakers: Vec::new(),
            tracks: Vec::new(),
        }
    }

    #[test]
    fn latest_reads_the_newest_revision_and_ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(latest(dir.path()).unwrap(), None);
        assert_eq!(next_revision(dir.path()).unwrap(), 1);
        fs::create_dir(dir.path().join(TRANSCRIPTS)).unwrap();
        for number in [1, 2] {
            let json = serde_json::to_vec(&revision(number)).unwrap();
            fs::write(revision_path(dir.path(), number), json).unwrap();
        }
        fs::write(dir.path().join(TRANSCRIPTS).join("000009.json.tmp"), b"{").unwrap();
        assert_eq!(latest_revision(dir.path()).unwrap(), Some(2));
        assert_eq!(latest(dir.path()).unwrap(), Some(revision(2)));
        assert_eq!(next_revision(dir.path()).unwrap(), 3);
    }

    #[test]
    fn a_revision_filed_under_the_wrong_number_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(TRANSCRIPTS)).unwrap();
        let json = serde_json::to_vec(&revision(5)).unwrap();
        fs::write(revision_path(dir.path(), 1), json).unwrap();
        assert!(latest(dir.path()).is_err());
    }
}
