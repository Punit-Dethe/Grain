//! Durable meeting sessions: one directory per meeting holding a versioned
//! manifest and per-track float32 WAV chunks.
//!
//! ```text
//! <root>/<session-uuid>/manifest.json
//! <root>/<session-uuid>/tracks/<track-id>/000000.wav          sealed
//! <root>/<session-uuid>/tracks/<track-id>/000001.wav.partial  being written
//! ```
//!
//! The manifest is authoritative and is replaced atomically. A chunk is added
//! to it (unsealed) before its file exists, and marked sealed only after the
//! file is synced and renamed, so recovery never meets an unlisted file.
//! Within a continuous run, frame counts position audio; capture timestamps
//! only decide where a new run starts. Missing intervals are explicit gaps,
//! never silence.
//!
//! Durability: a process crash loses at most the active chunk's audio since
//! its last checkpoint (5 s). Sealed chunks and the manifest are synced to
//! disk. Power-loss safety of the active chunk is not claimed.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::wav::{self, ChunkFile};

pub const MANIFEST_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const MANIFEST_TMP: &str = "manifest.json.tmp";
/// FluidMeet's storage chunk length (`MeetingCaptureConfiguration.defaultChunkDuration`).
pub const CHUNK_SECONDS: u64 = 60;
/// FluidMeet rotates a chunk when capture resumes more than 0.5 s after the
/// audio it already holds (`MeetingAudioChunkWriter`). Smaller timestamp
/// jitter is absorbed: frame counts stay authoritative.
pub const GAP_ROTATE_NS: u64 = 500_000_000;
/// Active-chunk checkpoint interval, bounding loss on a process crash.
pub const CHECKPOINT_SECONDS: u64 = 5;
const MAX_CHANNELS: u16 = 8;
const NANOS: u128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

    fn file_name(&self) -> String {
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

    fn next_seq(&self) -> u32 {
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

    fn validate(&self, dir_name: Option<&str>) -> Result<(), String> {
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

fn frames_to_ns(frames: u64, sample_rate: u32) -> u64 {
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

fn track_dir(session: &Path, track: u32) -> PathBuf {
    session.join("tracks").join(track.to_string())
}

fn io_error(context: &str, error: io::Error) -> String {
    format!("{context}: {error}")
}

/// Atomically replace the manifest: write, sync, rename over the old one.
fn write_manifest(session: &Path, manifest: &Manifest) -> Result<(), String> {
    let tmp = session.join(MANIFEST_TMP);
    let json = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    let write = || -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        fs::rename(&tmp, session.join(MANIFEST))?;
        sync_dir(session)
    };
    write().map_err(|error| io_error("Could not save the meeting manifest", error))
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

// Windows offers no directory handle sync through std; NTFS journals renames.
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

pub fn load_manifest(session: &Path) -> Result<Manifest, String> {
    let bytes = fs::read(session.join(MANIFEST))
        .map_err(|error| io_error("Could not read the meeting manifest", error))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Meeting manifest is corrupt: {error}"))?;
    manifest.validate(session.file_name().and_then(|name| name.to_str()))?;
    Ok(manifest)
}

// Recovery and title edits both replace the manifest of a saved session.
static MANIFEST_EDIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn rename_session(session: &Path, title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Enter a meeting title".into());
    }
    let _guard = MANIFEST_EDIT.lock().unwrap_or_else(|e| e.into_inner());
    let mut manifest = load_manifest(session)?;
    if manifest.state != SessionState::Recorded {
        return Err("Stop the recording before renaming it".into());
    }
    if manifest.title != title {
        manifest.title = title.to_owned();
        write_manifest(session, &manifest)?;
    }
    Ok(manifest.title)
}

/// Path of a chunk's audio file.
pub fn chunk_path(session: &Path, track: &Track, chunk: &Chunk) -> PathBuf {
    track_dir(session, track.id).join(chunk.file_name())
}

/// One block of captured audio for a track.
#[derive(Debug, Clone, Copy)]
pub struct Block<'a> {
    /// Meeting-clock position of the block's first frame.
    pub start_ns: u64,
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved frames.
    pub samples: &'a [f32],
    /// Capture reports a discontinuity before this block.
    pub discontinuity: Option<GapReason>,
}

struct Active {
    file: ChunkFile,
    unchecked_frames: u64,
}

/// Writes one recording session. Single-threaded: capture delivers blocks to
/// it from one writer thread, never from a real-time callback.
pub struct SessionWriter {
    dir: PathBuf,
    manifest: Manifest,
    active: Vec<Option<Active>>,
}

impl SessionWriter {
    /// Create a new session directory under `root`.
    pub fn create(root: &Path, title: &str) -> Result<Self, String> {
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        let dir = root.join(&id);
        fs::create_dir_all(root)
            .map_err(|error| io_error("Could not create the meetings folder", error))?;
        fs::create_dir(&dir)
            .map_err(|error| io_error("Could not create the meeting folder", error))?;
        let created_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as u64);
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            id,
            title: title.to_string(),
            created_unix_ms,
            state: SessionState::Recording,
            tracks: Vec::new(),
        };
        write_manifest(&dir, &manifest)?;
        Ok(Self {
            dir,
            manifest,
            active: Vec::new(),
        })
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn add_track(&mut self, source: TrackSource) -> Result<u32, String> {
        let id = self.manifest.tracks.len() as u32;
        fs::create_dir_all(track_dir(&self.dir, id))
            .map_err(|error| io_error("Could not create the meeting track folder", error))?;
        self.manifest.tracks.push(Track {
            id,
            source,
            chunks: Vec::new(),
            gaps: Vec::new(),
        });
        self.active.push(None);
        write_manifest(&self.dir, &self.manifest)?;
        Ok(id)
    }

    /// Append one block, rotating at 60 s, a format change, a reported
    /// discontinuity, or a late resume (> 0.5 s), and recording any gap.
    pub fn append(&mut self, track: u32, block: Block<'_>) -> Result<(), String> {
        let index = self.track_index(track)?;
        if block.sample_rate == 0 || block.channels == 0 || block.channels > MAX_CHANNELS {
            return Err("audio block has an invalid format".into());
        }
        if !block
            .samples
            .len()
            .is_multiple_of(usize::from(block.channels))
        {
            return Err("audio block is not a whole number of frames".into());
        }
        let frames = (block.samples.len() / usize::from(block.channels)) as u64;
        if frames == 0 {
            return Ok(());
        }
        let expected = self.manifest.tracks[index].end_ns();
        let has_audio = !self.manifest.tracks[index].chunks.is_empty();
        let late = has_audio && block.start_ns > expected.saturating_add(GAP_ROTATE_NS);
        let reported = has_audio && block.discontinuity.is_some();
        if late || reported {
            self.seal_active(index)?;
            if block.start_ns > expected {
                self.manifest.tracks[index].gaps.push(Gap {
                    start_ns: expected,
                    end_ns: block.start_ns,
                    reason: block.discontinuity.unwrap_or(GapReason::Unexplained),
                });
            }
        }
        let format_changed = self.manifest.tracks[index]
            .chunks
            .last()
            .is_some_and(|c| c.sample_rate != block.sample_rate || c.channels != block.channels);
        if format_changed {
            self.seal_active(index)?;
        }
        // A new run starts at the capture time when audio resumed late (or at
        // the session's first block); otherwise it continues seamlessly. A
        // backwards clock never overlaps audio already recorded.
        let start_ns = if late || !has_audio {
            block.start_ns.max(expected)
        } else {
            expected
        };

        let mut written = 0usize;
        let channels = usize::from(block.channels);
        while written < block.samples.len() {
            if self.active[index].is_none() {
                let at = if written == 0 {
                    start_ns
                } else {
                    self.manifest.tracks[index].end_ns()
                };
                self.open_chunk(index, at, block.sample_rate, block.channels)?;
            }
            let capacity = CHUNK_SECONDS * u64::from(block.sample_rate);
            let in_chunk = self.manifest.tracks[index]
                .chunks
                .last()
                .map_or(0, |c| c.frames);
            let room = capacity.saturating_sub(in_chunk) as usize;
            if room == 0 {
                self.seal_active(index)?;
                continue;
            }
            let take = ((block.samples.len() - written) / channels).min(room) * channels;
            let slice = &block.samples[written..written + take];
            let active = self.active[index].as_mut().expect("chunk was just opened");
            active
                .file
                .append(slice)
                .map_err(|error| io_error("Could not write meeting audio", error))?;
            let added = (take / channels) as u64;
            active.unchecked_frames += added;
            if active.unchecked_frames >= CHECKPOINT_SECONDS * u64::from(block.sample_rate) {
                active
                    .file
                    .checkpoint()
                    .map_err(|error| io_error("Could not checkpoint meeting audio", error))?;
                active.unchecked_frames = 0;
            }
            let chunk = self.manifest.tracks[index]
                .chunks
                .last_mut()
                .expect("open chunk");
            chunk.frames += added;
            written += take;
            if chunk.frames >= capacity {
                self.seal_active(index)?;
            }
        }
        Ok(())
    }

    /// Record an interval with no audio (pause, overflow, source loss) and
    /// end the current run; the next block starts a new chunk.
    pub fn mark_gap(&mut self, track: u32, end_ns: u64, reason: GapReason) -> Result<(), String> {
        let index = self.track_index(track)?;
        self.seal_active(index)?;
        let start_ns = self.manifest.tracks[index].end_ns();
        if end_ns > start_ns {
            self.manifest.tracks[index].gaps.push(Gap {
                start_ns,
                end_ns,
                reason,
            });
            write_manifest(&self.dir, &self.manifest)?;
        }
        Ok(())
    }

    /// Seal every track and mark the recording complete.
    pub fn finish(mut self) -> Result<Manifest, String> {
        for index in 0..self.active.len() {
            self.seal_active(index)?;
        }
        self.manifest.state = SessionState::Recorded;
        write_manifest(&self.dir, &self.manifest)?;
        Ok(self.manifest)
    }

    fn track_index(&self, track: u32) -> Result<usize, String> {
        self.manifest
            .tracks
            .iter()
            .position(|t| t.id == track)
            .ok_or_else(|| format!("unknown meeting track {track}"))
    }

    fn open_chunk(
        &mut self,
        index: usize,
        start_ns: u64,
        sample_rate: u32,
        channels: u16,
    ) -> Result<(), String> {
        let track = &mut self.manifest.tracks[index];
        let chunk = Chunk {
            seq: track.next_seq(),
            sample_rate,
            channels,
            start_ns,
            frames: 0,
            sealed: false,
            recovered: false,
        };
        let path = track_dir(&self.dir, track.id).join(chunk.file_name());
        track.chunks.push(chunk);
        // Listed before the file exists: recovery never meets an unlisted file.
        write_manifest(&self.dir, &self.manifest)?;
        let file = ChunkFile::create(&path, sample_rate, channels)
            .map_err(|error| io_error("Could not create meeting audio", error))?;
        self.active[index] = Some(Active {
            file,
            unchecked_frames: 0,
        });
        Ok(())
    }

    fn seal_active(&mut self, index: usize) -> Result<(), String> {
        let Some(active) = self.active[index].take() else {
            return Ok(());
        };
        let track = &mut self.manifest.tracks[index];
        let chunk = track.chunks.last_mut().expect("active chunk is listed");
        let partial = track_dir(&self.dir, track.id).join(chunk.file_name());
        let frames = active
            .file
            .seal()
            .map_err(|error| io_error("Could not finish meeting audio", error))?;
        debug_assert_eq!(frames, chunk.frames);
        if frames == 0 {
            let _ = fs::remove_file(&partial);
            track.chunks.pop();
        } else {
            chunk.sealed = true;
            let sealed = track_dir(&self.dir, track.id).join(chunk.file_name());
            fs::rename(&partial, &sealed)
                .map_err(|error| io_error("Could not finish meeting audio", error))?;
            sync_dir(&track_dir(&self.dir, track.id))
                .map_err(|error| io_error("Could not finish meeting audio", error))?;
        }
        write_manifest(&self.dir, &self.manifest)
    }
}

/// What recovery found in an interrupted session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Unsealed chunks sealed from their salvaged frames.
    pub salvaged_chunks: usize,
    pub salvaged_frames: u64,
    /// Listed chunks whose audio was empty or absent and are now gaps.
    pub lost_chunks: usize,
}

/// Seal an interrupted session (state `Recording`) from what is on disk.
/// A completed session is returned unchanged. Never deletes recorded audio:
/// only empty or unreadable chunk files are removed, and their intervals are
/// recorded as `Missing` gaps.
pub fn recover(session: &Path) -> Result<(Manifest, RecoveryReport), String> {
    let _guard = MANIFEST_EDIT.lock().unwrap_or_else(|e| e.into_inner());
    let mut manifest = load_manifest(session)?;
    let mut report = RecoveryReport::default();
    let _ = fs::remove_file(session.join(MANIFEST_TMP));
    if manifest.state == SessionState::Recorded {
        return Ok((manifest, report));
    }
    for track in &mut manifest.tracks {
        let dir = track_dir(session, track.id);
        let Some(chunk) = track.chunks.last_mut().filter(|chunk| !chunk.sealed) else {
            continue;
        };
        let partial = dir.join(chunk.file_name());
        let frames = if partial.exists() {
            match wav::salvage(&partial, chunk.sample_rate, chunk.channels) {
                Ok(frames) => frames,
                Err(error) => {
                    log::warn!(
                        "Meeting chunk {} could not be salvaged: {error}",
                        partial.display()
                    );
                    0
                }
            }
        } else {
            0
        };
        if frames == 0 {
            // Nothing durable was written, so there is no known interval to report.
            let _ = fs::remove_file(&partial);
            track.chunks.pop();
            report.lost_chunks += 1;
            continue;
        }
        chunk.frames = frames;
        chunk.sealed = true;
        chunk.recovered = true;
        fs::rename(&partial, dir.join(chunk.file_name()))
            .map_err(|error| io_error("Could not recover meeting audio", error))?;
        report.salvaged_chunks += 1;
        report.salvaged_frames += frames;
    }
    for track in &mut manifest.tracks {
        let dir = track_dir(session, track.id);
        let mut kept = Vec::with_capacity(track.chunks.len());
        for chunk in std::mem::take(&mut track.chunks) {
            if dir.join(chunk.file_name()).is_file() {
                kept.push(chunk);
            } else {
                report.lost_chunks += 1;
                track.gaps.push(Gap {
                    start_ns: chunk.start_ns,
                    end_ns: chunk.end_ns().max(chunk.start_ns + 1),
                    reason: GapReason::Missing,
                });
            }
        }
        track.chunks = kept;
        track.gaps.sort_by_key(|gap| gap.start_ns);
    }
    manifest.state = SessionState::Recorded;
    manifest.validate(None)?;
    write_manifest(session, &manifest)?;
    Ok((manifest, report))
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

    const RATE: u32 = 16_000;
    const SECOND: u64 = 1_000_000_000;

    fn block(start_ns: u64, samples: &[f32]) -> Block<'_> {
        Block {
            start_ns,
            sample_rate: RATE,
            channels: 1,
            samples,
            discontinuity: None,
        }
    }

    fn read(session: &Path, track: &Track, chunk: &Chunk) -> Vec<f32> {
        hound::WavReader::open(chunk_path(session, track, chunk))
            .unwrap()
            .into_samples::<f32>()
            .map(Result::unwrap)
            .collect()
    }

    fn mic() -> TrackSource {
        TrackSource::Microphone {
            device: "Test mic".into(),
        }
    }

    #[test]
    fn renaming_updates_only_the_saved_title() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Standup").unwrap();
        let track = writer.add_track(mic()).unwrap();
        writer.append(track, block(0, &[0.5; 8])).unwrap();
        let dir = writer.dir().to_path_buf();
        assert!(rename_session(&dir, "New title").is_err());

        let mut expected = writer.finish().unwrap();
        assert!(rename_session(&dir, "  ").is_err());
        assert_eq!(load_manifest(&dir).unwrap(), expected);
        assert_eq!(rename_session(&dir, "  Planning  ").unwrap(), "Planning");
        expected.title = "Planning".into();
        assert_eq!(load_manifest(&dir).unwrap(), expected);
    }

    #[test]
    fn writer_rotates_every_sixty_seconds_and_keeps_every_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Standup").unwrap();
        let track = writer.add_track(mic()).unwrap();
        // 130 s in irregular blocks with capture-time jitter under 0.5 s.
        let second: Vec<f32> = (0..RATE).map(|i| i as f32 / RATE as f32).collect();
        for s in 0..130u64 {
            let jitter = if s % 2 == 1 { 200_000_000 } else { 0 };
            writer
                .append(track, block(s * SECOND + jitter, &second))
                .unwrap();
        }
        let dir = writer.dir().to_path_buf();
        let manifest = writer.finish().unwrap();
        assert_eq!(manifest.state, SessionState::Recorded);
        let t = &manifest.tracks[0];
        let frames: Vec<u64> = t.chunks.iter().map(|c| c.frames).collect();
        assert_eq!(frames, vec![60 * 16_000, 60 * 16_000, 10 * 16_000]);
        assert!(t.gaps.is_empty(), "jitter is not a gap");
        assert_eq!(t.chunks[1].start_ns, 60 * SECOND);
        assert_eq!(
            t.chunks[2].start_ns,
            120 * SECOND,
            "frames, not jittered stamps, place audio"
        );
        assert_eq!(read(&dir, t, &t.chunks[2])[..3], second[..3]);
        assert_eq!(load_manifest(&dir).unwrap(), manifest);
    }

    #[test]
    fn late_resume_and_reported_discontinuity_are_explicit_gaps() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Gaps").unwrap();
        let track = writer.add_track(mic()).unwrap();
        let second = vec![0.1f32; RATE as usize];
        writer.append(track, block(0, &second)).unwrap();
        // Resumes 3 s after the audio it holds ends.
        writer.append(track, block(4 * SECOND, &second)).unwrap();
        let mut lost = block(10 * SECOND, &second);
        lost.discontinuity = Some(GapReason::SourceLost);
        writer.append(track, lost).unwrap();
        writer
            .mark_gap(track, 20 * SECOND, GapReason::Paused)
            .unwrap();
        let manifest = writer.finish().unwrap();
        let t = &manifest.tracks[0];
        let starts: Vec<u64> = t.chunks.iter().map(|c| c.start_ns / SECOND).collect();
        assert_eq!(starts, vec![0, 4, 10]);
        let gaps: Vec<(u64, u64, GapReason)> = t
            .gaps
            .iter()
            .map(|g| (g.start_ns / SECOND, g.end_ns / SECOND, g.reason))
            .collect();
        assert_eq!(
            gaps,
            vec![
                (1, 4, GapReason::Unexplained),
                (5, 10, GapReason::SourceLost),
                (11, 20, GapReason::Paused),
            ]
        );
    }

    #[test]
    fn format_change_starts_a_new_chunk_and_backwards_clock_never_overlaps() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Formats").unwrap();
        let track = writer.add_track(mic()).unwrap();
        writer.append(track, block(0, &[0.0; 16_000])).unwrap();
        let stereo = [0.0f32; 96_000];
        writer
            .append(
                track,
                Block {
                    start_ns: 0, // clock went backwards
                    sample_rate: 48_000,
                    channels: 2,
                    samples: &stereo,
                    discontinuity: None,
                },
            )
            .unwrap();
        assert!(writer.append(track, block(0, &[0.0; 3])).is_ok());
        assert!(writer
            .append(
                track,
                Block {
                    channels: 2,
                    ..block(0, &[0.0; 3])
                }
            )
            .is_err());
        let manifest = writer.finish().unwrap();
        let chunks = &manifest.tracks[0].chunks;
        assert_eq!(chunks.len(), 3);
        assert_eq!((chunks[1].sample_rate, chunks[1].channels), (48_000, 2));
        assert_eq!(chunks[1].start_ns, SECOND);
        assert_eq!(chunks[2].start_ns, 2 * SECOND);
    }

    #[test]
    fn crash_recovery_salvages_checkpointed_audio_and_seals_the_session() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Crash").unwrap();
        let mic_track = writer.add_track(mic()).unwrap();
        let app = writer
            .add_track(TrackSource::Application {
                name: "Browser".into(),
            })
            .unwrap();
        // 70 s on the mic: one sealed chunk, 10 s in the active one (two
        // checkpoints). The app track opened a chunk but wrote nothing durable.
        let second = vec![0.25f32; RATE as usize];
        for s in 0..70 {
            writer
                .append(mic_track, block(s * SECOND, &second))
                .unwrap();
        }
        writer.append(app, block(0, &[0.5; 100])).unwrap();
        let dir = writer.dir().to_path_buf();
        // Simulate a process crash: no seal, no flush, no drop.
        std::mem::forget(writer);

        assert_eq!(load_manifest(&dir).unwrap().state, SessionState::Recording);
        let (manifest, report) = recover(&dir).unwrap();
        assert_eq!(manifest.state, SessionState::Recorded);
        let mic = &manifest.tracks[0];
        assert_eq!(mic.chunks.len(), 2);
        assert!(mic.chunks[1].recovered);
        // The last append ended exactly on the 10 s checkpoint.
        assert_eq!(mic.chunks[1].frames, 10 * u64::from(RATE));
        assert_eq!(
            read(&dir, mic, &mic.chunks[1]).len() as u64,
            mic.chunks[1].frames
        );
        // The app track's 100 frames never left the write buffer.
        assert!(manifest.tracks[1].chunks.is_empty());
        assert_eq!((report.salvaged_chunks, report.lost_chunks), (1, 1));
        // Recovering again is a no-op.
        assert_eq!(
            recover(&dir).unwrap(),
            (manifest, RecoveryReport::default())
        );
    }

    #[test]
    fn recovery_turns_missing_sealed_audio_into_an_explicit_gap() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Missing").unwrap();
        let track = writer.add_track(mic()).unwrap();
        let second = vec![0.1f32; RATE as usize];
        for s in 0..61 {
            writer.append(track, block(s * SECOND, &second)).unwrap();
        }
        let dir = writer.dir().to_path_buf();
        let manifest = writer.manifest().clone();
        std::mem::forget(writer);
        fs::remove_file(chunk_path(
            &dir,
            &manifest.tracks[0],
            &manifest.tracks[0].chunks[0],
        ))
        .unwrap();
        let (recovered, report) = recover(&dir).unwrap();
        let t = &recovered.tracks[0];
        assert_eq!(
            t.gaps[0],
            Gap {
                start_ns: 0,
                end_ns: 60 * SECOND,
                reason: GapReason::Missing
            }
        );
        assert!(report.lost_chunks >= 1);
        assert!(t.chunks.iter().all(|c| c.start_ns >= 60 * SECOND));
    }

    #[test]
    fn manifests_are_validated_and_listing_reads_only_manifests() {
        let root = tempfile::tempdir().unwrap();
        let mut first = SessionWriter::create(root.path(), "First").unwrap();
        let track = first.add_track(mic()).unwrap();
        first.append(track, block(0, &vec![0.0; 32_000])).unwrap();
        let first_id = first.id().to_string();
        first.finish().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = SessionWriter::create(root.path(), "Second").unwrap();
        let second_dir = second.dir().to_path_buf();
        second.finish().unwrap();
        fs::create_dir(root.path().join("not-a-meeting")).unwrap();
        let broken = root.path().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&broken).unwrap();
        fs::write(broken.join(MANIFEST), b"{").unwrap();

        let listed = list_sessions(root.path()).unwrap();
        assert_eq!(
            listed.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            ["Second", "First"]
        );
        assert_eq!(listed[1].duration_ms, 2_000);
        assert_eq!(listed[1].id, first_id);

        // A manifest moved under another id is refused.
        let moved = root.path().join(uuid::Uuid::new_v4().to_string());
        fs::rename(&second_dir, &moved).unwrap();
        assert!(load_manifest(&moved).unwrap_err().contains("directory"));
        assert!(session_dir(root.path(), "../etc").is_err());
        assert!(session_dir(root.path(), &first_id.to_uppercase()).is_err());
        assert!(list_sessions(&root.path().join("absent"))
            .unwrap()
            .is_empty());
    }
}
