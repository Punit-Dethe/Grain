//! Test meetings on disk, written in the app's own formats. Built for this
//! crate's tests and, through the `fixtures` feature, for the sidecar's and
//! the app's tests.

use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{
    Chunk, Manifest, SessionState, Track, TrackSource, MANIFEST, MANIFEST_VERSION,
};
use crate::summary::{MeetSummaryDoc, SUMMARY_FILE};
use crate::transcript::{
    self, revision_path, Attribution, TrackStatus, TrackTranscript, TranscriptRevision, Word,
    TRANSCRIPTS, TRANSCRIPT_VERSION,
};

pub struct Fixture {
    pub dir: tempfile::TempDir,
}

#[allow(clippy::new_without_default)]
impl Fixture {
    /// An empty meetings folder with the switch on.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("meetings")).unwrap();
        let fixture = Self { dir };
        fixture.set_enabled(true);
        fixture
    }

    /// Grain's app data folder (holds `meetings/`).
    pub fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    /// The meetings folder the queries read.
    pub fn root(&self) -> PathBuf {
        self.dir.path().join("meetings")
    }

    pub fn set_enabled(&self, on: bool) {
        let gate = self.dir.path().join("meetings").join(crate::GATE_FILE);
        if on {
            fs::write(gate, b"").unwrap();
        } else {
            let _ = fs::remove_file(gate);
        }
    }

    /// A recorded meeting of `seconds`, with transcript lines
    /// `(start_ms, speaker slot, text)` when given; returns its id.
    pub fn meeting(
        &self,
        title: &str,
        created_unix_ms: u64,
        seconds: u64,
        lines: Option<&[(u64, i32, &str)]>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = self.dir.path().join("meetings").join(&id);
        fs::create_dir_all(&dir).unwrap();
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            id: id.clone(),
            title: title.into(),
            created_unix_ms,
            state: SessionState::Recorded,
            tracks: vec![Track {
                id: 0,
                source: TrackSource::Microphone {
                    device: "Desk mic".into(),
                },
                chunks: vec![Chunk {
                    seq: 0,
                    sample_rate: 16_000,
                    channels: 1,
                    start_ns: 0,
                    frames: seconds * 16_000,
                    sealed: true,
                    recovered: false,
                }],
                gaps: Vec::new(),
            }],
        };
        fs::write(dir.join(MANIFEST), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let Some(lines) = lines else {
            return id;
        };
        let mut words = Vec::new();
        for (start, slot, text) in lines {
            for (k, w) in text.split_whitespace().enumerate() {
                let at = start + k as u64 * 300;
                words.push(Word {
                    text: w.into(),
                    start_ms: at,
                    end_ms: at + 250,
                    speaker: Attribution::Speaker(*slot),
                });
            }
        }
        let tracks = vec![TrackTranscript {
            track: 0,
            status: TrackStatus::Complete,
            text: String::new(),
            words,
            activity: Vec::new(),
        }];
        let revision = TranscriptRevision {
            version: TRANSCRIPT_VERSION,
            revision: 1,
            session_id: id.clone(),
            attempt_id: "a".into(),
            created_unix_ms,
            diarizer: None,
            asr: None,
            speakers: transcript::speakers(&tracks),
            tracks,
        };
        fs::create_dir_all(dir.join(TRANSCRIPTS)).unwrap();
        fs::write(
            revision_path(&dir, 1),
            serde_json::to_vec(&revision).unwrap(),
        )
        .unwrap();
        id
    }

    pub fn summary(&self, id: &str, revision: u32, text: &str) {
        let doc = MeetSummaryDoc {
            transcript_revision: revision,
            created_unix_ms: 0,
            text: text.into(),
        };
        let path = self.dir.path().join("meetings").join(id).join(SUMMARY_FILE);
        fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
    }
}
