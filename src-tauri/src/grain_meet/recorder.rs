//! Meeting capture: every enabled source into one durable session.
//!
//! One writer thread owns the session and every capture stream for exactly the
//! length of the recording. Device callbacks only copy frames into a bounded
//! queue and never block; a full queue drops the frame and counts it. The
//! writer places frames on one meeting clock: within a run, frame counts are
//! the clock; a run starts at the frame's arrival time.
//!
//! The sources differ in what silence looks like. A microphone always
//! delivers, so 3 s without audio means it is gone: it is reopened and the loss
//! becomes a `SourceLost` gap. Windows loopback delivers nothing while no sound
//! plays, so a late resume is a `Silent` gap (known silence, kept compact on
//! disk), never a fault. A device that reports a failure is reopened.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::sources::{self, Deliver, SourceSettings};
use super::storage::{Block, GapReason, Manifest, SessionWriter, TrackSource};

/// Queue capacity in delivered blocks (10 ms each): about 10 s for two sources.
const QUEUE_BLOCKS: usize = 2_048;
/// No microphone audio for this long means the device is gone.
const SOURCE_LOST_AFTER: Duration = Duration::from_secs(3);
/// Minimum interval between reopen attempts for one source.
const RESTART_EVERY: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub enum SourceKind {
    Microphone,
    SystemAudio,
}

struct Frame {
    source: usize,
    arrived: Instant,
    rate: u32,
    channels: u16,
    samples: Vec<f32>,
}

/// Live counters for one source, read by status queries.
pub(crate) struct SourceProgress {
    pub kind: SourceKind,
    pub name: String,
    pub frames_ns: AtomicU64,
    /// Peak level since the last status read, 0..1, as f32 bits (non-negative
    /// f32 bits order like the values, so etch_max keeps the peak).
    pub level: AtomicU32,
    pub active: AtomicBool,
}

#[derive(Default)]
pub(crate) struct Progress {
    pub sources: Vec<Arc<SourceProgress>>,
    pub error: Mutex<Option<String>>,
}

/// Opens one source's capture stream. Dropping the returned value closes it.
pub(crate) trait Opener: Send + 'static {
    fn open(
        &self,
        kind: SourceKind,
        deliver: Deliver,
        failed: Arc<AtomicBool>,
    ) -> Result<Box<dyn Send>, String>;
}

/// The real devices, per Recall's source settings.
pub(crate) struct DeviceOpener(pub SourceSettings);

impl Opener for DeviceOpener {
    fn open(
        &self,
        kind: SourceKind,
        deliver: Deliver,
        failed: Arc<AtomicBool>,
    ) -> Result<Box<dyn Send>, String> {
        Ok(match kind {
            SourceKind::Microphone => Box::new(sources::MicCapture::start(
                &self.0.microphone,
                deliver,
                failed,
            )?),
            SourceKind::SystemAudio => Box::new(sources::SystemCapture::start(
                &self.0.system_audio,
                deliver,
                failed,
            )?),
        })
    }
}

/// One recording in progress.
pub(crate) struct Recording {
    pub session_id: String,
    pub started: Instant,
    pub progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    writer: Option<std::thread::JoinHandle<Result<Manifest, String>>>,
}

struct Source {
    kind: SourceKind,
    track: u32,
    progress: Arc<SourceProgress>,
    dropped: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    capture: Option<Box<dyn Send>>,
    last_arrival: Instant,
    last_restart: Option<Instant>,
    lost: bool,
}

fn deliver_to(sender: SyncSender<Frame>, source: usize, dropped: Arc<AtomicU64>) -> Deliver {
    Arc::new(move |samples: &[f32], rate: u32, channels: u16| {
        if samples.is_empty() || channels == 0 {
            return;
        }
        // More than stereo is mixed to mono: meeting audio is speech.
        let (samples, channels) = if channels > 2 {
            let n = usize::from(channels);
            (
                samples
                    .chunks_exact(n)
                    .map(|f| f.iter().sum::<f32>() / n as f32)
                    .collect(),
                1,
            )
        } else {
            (samples.to_vec(), channels)
        };
        let frame = Frame {
            source,
            arrived: Instant::now(),
            rate,
            channels,
            samples,
        };
        if let Err(TrySendError::Full(frame) | TrySendError::Disconnected(frame)) =
            sender.try_send(frame)
        {
            let frames = frame.samples.len() / usize::from(frame.channels);
            dropped.fetch_add(frames as u64, Ordering::AcqRel);
        }
    })
}

impl Recording {
    /// Create the session and start every source named in `kinds`. Fails, and
    /// removes the empty session, if any source cannot start.
    pub(crate) fn begin(
        root: &Path,
        title: &str,
        kinds: &[(SourceKind, String)],
        opener: impl Opener,
    ) -> Result<Self, String> {
        if kinds.is_empty() {
            return Err("Turn on the microphone, system audio, or both".into());
        }
        let mut writer = SessionWriter::create(root, title)?;
        let session_id = writer.id().to_string();
        let session_dir = writer.dir().to_path_buf();
        let cleanup = |error: String| {
            let _ = std::fs::remove_dir_all(&session_dir);
            error
        };
        let (sender, frames) = mpsc::sync_channel(QUEUE_BLOCKS);
        let mut list = Vec::with_capacity(kinds.len());
        for (index, (kind, name)) in kinds.iter().enumerate() {
            let source = match kind {
                SourceKind::Microphone => TrackSource::Microphone {
                    device: name.clone(),
                },
                SourceKind::SystemAudio => TrackSource::Application { name: name.clone() },
            };
            let track = writer.add_track(source).map_err(cleanup)?;
            let dropped = Arc::new(AtomicU64::new(0));
            let failed = Arc::new(AtomicBool::new(false));
            let capture = opener
                .open(
                    *kind,
                    deliver_to(sender.clone(), index, Arc::clone(&dropped)),
                    Arc::clone(&failed),
                )
                .map_err(cleanup)?;
            list.push(Source {
                kind: *kind,
                track,
                progress: Arc::new(SourceProgress {
                    kind: *kind,
                    name: name.clone(),
                    frames_ns: AtomicU64::new(0),
                    level: AtomicU32::new(0),
                    active: AtomicBool::new(false),
                }),
                dropped,
                failed,
                capture: Some(capture),
                last_arrival: Instant::now(),
                last_restart: None,
                lost: false,
            });
        }
        let progress = Arc::new(Progress {
            sources: list.iter().map(|s| Arc::clone(&s.progress)).collect(),
            error: Mutex::new(None),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let thread_progress = Arc::clone(&progress);
        let thread_stop = Arc::clone(&stop);
        let writer = std::thread::Builder::new()
            .name("grain-meet-writer".into())
            .spawn(move || {
                write_loop(
                    writer,
                    list,
                    sender,
                    frames,
                    started,
                    thread_progress,
                    thread_stop,
                    opener,
                )
            })
            .map_err(|e| cleanup(format!("Could not start the meeting writer: {e}")))?;
        Ok(Self {
            session_id,
            started,
            progress,
            stop,
            writer: Some(writer),
        })
    }

    /// Close every source, write what they delivered, and seal the session.
    pub(crate) fn finish(mut self) -> Result<Manifest, String> {
        self.stop.store(true, Ordering::Release);
        let handle = self.writer.take().expect("writer runs until finish");
        handle
            .join()
            .map_err(|_| "The meeting writer stopped unexpectedly".to_string())?
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(handle) = self.writer.take() {
            self.stop.store(true, Ordering::Release);
            let _ = handle.join();
        }
    }
}

fn since_origin(origin: Instant, at: Instant) -> u64 {
    at.saturating_duration_since(origin).as_nanos() as u64
}

fn frames_ns(frames: u64, rate: u32) -> u64 {
    frames * 1_000_000_000 / u64::from(rate.max(1))
}

#[allow(clippy::too_many_arguments)]
fn write_loop(
    mut writer: SessionWriter,
    mut sources: Vec<Source>,
    sender: SyncSender<Frame>,
    frames: Receiver<Frame>,
    origin: Instant,
    progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    opener: impl Opener,
) -> Result<Manifest, String> {
    let mut failure: Option<String> = None;
    let write = |writer: &mut SessionWriter, sources: &mut [Source], frame: Frame| {
        let source = &mut sources[frame.source];
        source.last_arrival = frame.arrived;
        source.progress.active.store(true, Ordering::Release);
        let peak = frame
            .samples
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
            .min(1.0);
        source
            .progress
            .level
            .fetch_max(peak.to_bits(), Ordering::AcqRel);
        let lost = source.dropped.swap(0, Ordering::AcqRel);
        let end = writer.manifest().tracks[source.track as usize].end_ns();
        let len = (frame.samples.len() / usize::from(frame.channels)) as u64;
        let arrival_start =
            since_origin(origin, frame.arrived).saturating_sub(frames_ns(len, frame.rate));
        let discontinuity = if std::mem::take(&mut source.lost) {
            Some(GapReason::SourceLost)
        } else if lost > 0 {
            log::warn!("Meeting capture dropped {lost} frames: the writer fell behind");
            Some(GapReason::Overflow)
        } else if source.kind == SourceKind::SystemAudio
            && end > 0
            && arrival_start > end + super::storage::GAP_ROTATE_NS
        {
            Some(GapReason::Silent)
        } else {
            None
        };
        let start_ns = if discontinuity.is_some() || end == 0 {
            arrival_start.max(end)
        } else {
            end
        };
        let block = Block {
            start_ns,
            sample_rate: frame.rate,
            channels: frame.channels,
            samples: &frame.samples,
            discontinuity,
        };
        writer.append(source.track, block)?;
        source.progress.frames_ns.store(
            writer.manifest().tracks[source.track as usize].end_ns(),
            Ordering::Release,
        );
        Ok::<(), String>(())
    };
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        match frames.recv_timeout(POLL) {
            Ok(frame) => {
                if failure.is_some() {
                    continue;
                }
                if let Err(error) = write(&mut writer, &mut sources, frame) {
                    log::error!("Meeting recording stopped: {error}");
                    *progress.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.clone());
                    failure = Some(error);
                    for source in &mut sources {
                        source.capture = None;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if failure.is_some() {
            continue;
        }
        // Reopen failed or (for the microphone) silent sources.
        for (index, source) in sources.iter_mut().enumerate() {
            let silent = source.kind == SourceKind::Microphone
                && source.last_arrival.elapsed() >= SOURCE_LOST_AFTER;
            if !silent && !source.failed.load(Ordering::Acquire) {
                continue;
            }
            source.progress.active.store(false, Ordering::Release);
            source.lost = true;
            if source
                .last_restart
                .is_some_and(|at| at.elapsed() < RESTART_EVERY)
            {
                continue;
            }
            source.last_restart = Some(Instant::now());
            source.capture = None;
            source.failed.store(false, Ordering::Release);
            match opener.open(
                source.kind,
                deliver_to(sender.clone(), index, Arc::clone(&source.dropped)),
                Arc::clone(&source.failed),
            ) {
                Ok(capture) => {
                    log::warn!("Meeting {:?} stopped delivering; reopened it", source.kind);
                    source.capture = Some(capture);
                    source.last_arrival = Instant::now();
                }
                Err(error) => {
                    log::warn!("Meeting {:?} could not be reopened: {error}", source.kind);
                    source.failed.store(true, Ordering::Release);
                }
            }
        }
    }
    // Close the devices first: each delivers its last audio before closing.
    for source in &mut sources {
        source.capture = None;
    }
    drop(sender);
    while let Ok(frame) = frames.try_recv() {
        if failure.is_none() {
            if let Err(error) = write(&mut writer, &mut sources, frame) {
                failure = Some(error);
            }
        }
    }
    let manifest = writer.finish()?;
    match failure {
        Some(error) => Err(error),
        None => Ok(manifest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_meet::storage::SessionState;

    type Opened = (SourceKind, Deliver, Arc<AtomicBool>);

    /// Test devices: each open hands the test its delivery function.
    #[derive(Clone, Default)]
    struct FakeOpener {
        delivers: Arc<Mutex<Vec<Opened>>>,
        fail: Arc<AtomicBool>,
    }

    impl Opener for FakeOpener {
        fn open(
            &self,
            kind: SourceKind,
            deliver: Deliver,
            failed: Arc<AtomicBool>,
        ) -> Result<Box<dyn Send>, String> {
            if self.fail.load(Ordering::Acquire) {
                return Err("no device".into());
            }
            self.delivers.lock().unwrap().push((kind, deliver, failed));
            Ok(Box::new(()))
        }
    }

    impl FakeOpener {
        fn latest(&self, kind: SourceKind) -> (Deliver, Arc<AtomicBool>) {
            let all = self.delivers.lock().unwrap();
            let (_, deliver, failed) = all.iter().rev().find(|(k, _, _)| *k == kind).unwrap();
            (Arc::clone(deliver), Arc::clone(failed))
        }
        fn opens(&self, kind: SourceKind) -> usize {
            self.delivers
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _, _)| *k == kind)
                .count()
        }
    }

    fn push(deliver: &Deliver, seconds: f64, rate: u32, channels: u16) {
        let block = vec![0.2f32; rate as usize / 100 * usize::from(channels)];
        for _ in 0..(seconds * 100.0) as usize {
            deliver(&block, rate, channels);
        }
    }

    fn both() -> Vec<(SourceKind, String)> {
        vec![
            (SourceKind::Microphone, "USB mic".into()),
            (SourceKind::SystemAudio, "Speakers".into()),
        ]
    }

    #[test]
    fn both_sources_record_to_their_own_tracks() {
        let root = tempfile::tempdir().unwrap();
        let opener = FakeOpener::default();
        let recording = Recording::begin(root.path(), "Call", &both(), opener.clone()).unwrap();
        push(&opener.latest(SourceKind::Microphone).0, 1.0, 48_000, 1);
        push(&opener.latest(SourceKind::SystemAudio).0, 1.0, 48_000, 2);
        let manifest = recording.finish().unwrap();
        assert_eq!(manifest.state, SessionState::Recorded);
        let mic = &manifest.tracks[0];
        let system = &manifest.tracks[1];
        assert!(matches!(&mic.source, TrackSource::Microphone { device } if device == "USB mic"));
        assert!(matches!(&system.source, TrackSource::Application { name } if name == "Speakers"));
        assert_eq!(mic.chunks.iter().map(|c| c.frames).sum::<u64>(), 48_000);
        assert_eq!(system.chunks.iter().map(|c| c.frames).sum::<u64>(), 48_000);
        assert_eq!(system.chunks[0].channels, 2);
    }

    #[test]
    fn a_source_that_cannot_open_fails_the_start_and_leaves_nothing() {
        let root = tempfile::tempdir().unwrap();
        let opener = FakeOpener::default();
        opener.fail.store(true, Ordering::Release);
        assert!(Recording::begin(root.path(), "x", &both(), opener).is_err());
        assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());
        assert!(Recording::begin(root.path(), "x", &[], FakeOpener::default()).is_err());
    }

    #[test]
    fn loopback_silence_is_a_silent_gap_and_never_reopens_the_device() {
        let root = tempfile::tempdir().unwrap();
        let opener = FakeOpener::default();
        let kinds = vec![(SourceKind::SystemAudio, "Speakers".into())];
        let recording = Recording::begin(root.path(), "Quiet", &kinds, opener.clone()).unwrap();
        let (deliver, _) = opener.latest(SourceKind::SystemAudio);
        push(&deliver, 0.3, 48_000, 2);
        std::thread::sleep(SOURCE_LOST_AFTER + Duration::from_millis(500));
        push(&deliver, 0.3, 48_000, 2);
        let manifest = recording.finish().unwrap();
        let track = &manifest.tracks[0];
        assert_eq!(opener.opens(SourceKind::SystemAudio), 1);
        assert_eq!(track.gaps.len(), 1);
        assert_eq!(track.gaps[0].reason, GapReason::Silent);
    }

    #[test]
    fn a_silent_microphone_is_reopened_and_its_loss_is_a_gap() {
        let root = tempfile::tempdir().unwrap();
        let opener = FakeOpener::default();
        let kinds = vec![(SourceKind::Microphone, "Mic".into())];
        let recording = Recording::begin(root.path(), "Loss", &kinds, opener.clone()).unwrap();
        push(&opener.latest(SourceKind::Microphone).0, 0.3, 48_000, 1);
        std::thread::sleep(SOURCE_LOST_AFTER + Duration::from_millis(500));
        assert_eq!(opener.opens(SourceKind::Microphone), 2, "reopened once");
        assert!(!recording.progress.sources[0].active.load(Ordering::Acquire));
        push(&opener.latest(SourceKind::Microphone).0, 0.3, 48_000, 1);
        let manifest = recording.finish().unwrap();
        let gaps = &manifest.tracks[0].gaps;
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].reason, GapReason::SourceLost);
    }

    #[test]
    fn a_reported_device_failure_reopens_that_source_only() {
        let root = tempfile::tempdir().unwrap();
        let opener = FakeOpener::default();
        let recording = Recording::begin(root.path(), "Fail", &both(), opener.clone()).unwrap();
        push(&opener.latest(SourceKind::SystemAudio).0, 0.2, 48_000, 2);
        opener
            .latest(SourceKind::SystemAudio)
            .1
            .store(true, Ordering::Release);
        // Keep the microphone alive meanwhile.
        for _ in 0..5 {
            push(&opener.latest(SourceKind::Microphone).0, 0.1, 48_000, 1);
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(opener.opens(SourceKind::SystemAudio), 2);
        assert_eq!(opener.opens(SourceKind::Microphone), 1);
        recording.finish().unwrap();
    }
}
