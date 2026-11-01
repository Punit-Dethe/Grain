//! Meeting microphone capture into a durable session.
//!
//! The microphone is the shared dictation recorder (Handy's 16 kHz mono speech
//! stream); no second device stream is opened. While a meeting records, the
//! recorder's `Idle` check refuses dictation, and dictation refuses a meeting.
//! The recorder's delivery thread hands each frame to [`feed`], which only
//! timestamps it and queues it without blocking. One writer thread places
//! frames on the meeting clock and writes them through [`SessionWriter`].
//!
//! Frame counts position audio within a run. A frame arriving more than 0.5 s
//! after the audio already written, a full queue, or a restarted source ends
//! the run with an explicit gap. If the source stops delivering, the writer
//! restarts the microphone and records the loss as a `SourceLost` gap.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::storage::{Block, GapReason, Manifest, SessionWriter, TrackSource};

/// The shared recorder's output format for meeting capture.
pub const MIC_RATE: u32 = 16_000;
/// Queue capacity between the delivery thread and the writer, in frames
/// (512-sample blocks): about 10 s, so a slow disk sync never drops audio.
const QUEUE_BLOCKS: usize = 320;
/// No audio for this long while recording means the source is gone.
const SOURCE_LOST_AFTER: Duration = Duration::from_secs(3);
/// Minimum interval between microphone restart attempts.
const RESTART_EVERY: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(250);

struct Frame {
    arrived: Instant,
    samples: Vec<f32>,
}

struct Route {
    sender: SyncSender<Frame>,
    dropped: Arc<AtomicU64>,
}

/// The active meeting's queue; `None` when no meeting is recording.
static ROUTE: Mutex<Option<Route>> = Mutex::new(None);

/// Called on the recorder's delivery thread for every meeting frame. Never
/// blocks: a full queue drops the frame and counts it.
pub(crate) fn feed(samples: &[f32]) {
    let route = ROUTE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(route) = route.as_ref() else {
        return;
    };
    let frame = Frame {
        arrived: Instant::now(),
        samples: samples.to_vec(),
    };
    match route.sender.try_send(frame) {
        Ok(()) => {}
        Err(TrySendError::Full(frame)) | Err(TrySendError::Disconnected(frame)) => {
            route
                .dropped
                .fetch_add(frame.samples.len() as u64, Ordering::AcqRel);
        }
    }
}

/// What the writer needs from the microphone owner.
pub(crate) trait Source: Send + 'static {
    /// Stop and start capture again after the source stopped delivering.
    fn restart(&self) -> Result<(), String>;
    /// Stop capture after a write failure, keeping committed audio.
    fn stop(&self);
}

/// Live counters for status queries.
#[derive(Default)]
pub(crate) struct Progress {
    pub frames: AtomicU64,
    pub source_active: AtomicBool,
    pub error: Mutex<Option<String>>,
}

/// One recording in progress.
pub(crate) struct Recording {
    pub session_id: String,
    pub started: Instant,
    pub progress: Arc<Progress>,
    writer: Option<std::thread::JoinHandle<Result<Manifest, String>>>,
}

impl Recording {
    /// Create the session, route frames to a new writer thread, and return.
    /// The caller starts the microphone after this.
    pub(crate) fn begin(
        root: &Path,
        title: &str,
        device: String,
        source: impl Source,
    ) -> Result<Self, String> {
        let mut writer = SessionWriter::create(root, title)?;
        let track = writer.add_track(TrackSource::Microphone { device })?;
        let session_id = writer.id().to_string();
        let (sender, receiver) = mpsc::sync_channel(QUEUE_BLOCKS);
        let dropped = Arc::new(AtomicU64::new(0));
        let progress = Arc::new(Progress::default());
        let started = Instant::now();
        {
            let mut route = ROUTE.lock().unwrap_or_else(|e| e.into_inner());
            if route.is_some() {
                return Err("A meeting is already recording".into());
            }
            *route = Some(Route {
                sender,
                dropped: Arc::clone(&dropped),
            });
        }
        let thread_progress = Arc::clone(&progress);
        let spawned = std::thread::Builder::new()
            .name("grain-meet-writer".into())
            .spawn(move || {
                write_loop(
                    writer,
                    track,
                    receiver,
                    dropped,
                    started,
                    thread_progress,
                    source,
                )
            });
        match spawned {
            Ok(handle) => Ok(Self {
                session_id,
                started,
                progress,
                writer: Some(handle),
            }),
            Err(error) => {
                close_route();
                Err(format!("Could not start the meeting writer: {error}"))
            }
        }
    }

    /// End the recording after the microphone has stopped delivering: close
    /// the queue, let the writer drain it, and seal the session.
    pub(crate) fn finish(mut self) -> Result<Manifest, String> {
        close_route();
        let handle = self.writer.take().expect("writer runs until finish");
        handle
            .join()
            .map_err(|_| "The meeting writer stopped unexpectedly".to_string())?
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        // Destroyed without finish: stop routing; the writer seals what it has.
        if let Some(handle) = self.writer.take() {
            close_route();
            let _ = handle.join();
        }
    }
}

fn close_route() {
    ROUTE.lock().unwrap_or_else(|e| e.into_inner()).take();
}

fn frames_ns(frames: u64) -> u64 {
    frames * 1_000_000_000 / u64::from(MIC_RATE)
}

/// The writer thread: place frames on the meeting clock and write them.
fn write_loop(
    mut writer: SessionWriter,
    track: u32,
    frames: Receiver<Frame>,
    dropped: Arc<AtomicU64>,
    origin: Instant,
    progress: Arc<Progress>,
    source: impl Source,
) -> Result<Manifest, String> {
    let mut last_arrival = Instant::now();
    let mut last_restart: Option<Instant> = None;
    let mut source_lost = false;
    let mut failure: Option<String> = None;
    loop {
        match frames.recv_timeout(POLL) {
            Ok(frame) => {
                last_arrival = frame.arrived;
                progress.source_active.store(true, Ordering::Release);
                if failure.is_some() {
                    continue; // Drain without writing after a disk failure.
                }
                let lost = dropped.swap(0, Ordering::AcqRel);
                let discontinuity = if std::mem::take(&mut source_lost) {
                    Some(GapReason::SourceLost)
                } else if lost > 0 {
                    log::warn!("Meeting capture dropped {lost} frames: the writer fell behind");
                    Some(GapReason::Overflow)
                } else {
                    None
                };
                let len = frame.samples.len() as u64;
                // Within a run, frame counts are the clock: the recorder's
                // ring buffers through delivery stalls, so late arrival alone
                // is not missing audio. Only a known loss, or the first frame,
                // is placed by arrival (the frame ended when it arrived).
                let end = writer.manifest().tracks[track as usize].end_ns();
                let start_ns = if discontinuity.is_some() || end == 0 {
                    (frame.arrived.saturating_duration_since(origin).as_nanos() as u64)
                        .saturating_sub(frames_ns(len))
                        .max(end)
                } else {
                    end
                };
                let block = Block {
                    start_ns,
                    sample_rate: MIC_RATE,
                    channels: 1,
                    samples: &frame.samples,
                    discontinuity,
                };
                match writer.append(track, block) {
                    Ok(()) => {
                        progress.frames.fetch_add(len, Ordering::AcqRel);
                    }
                    Err(error) => {
                        log::error!("Meeting recording stopped: {error}");
                        *progress.error.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(error.clone());
                        failure = Some(error);
                        source.stop();
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if failure.is_some() || last_arrival.elapsed() < SOURCE_LOST_AFTER {
                    continue;
                }
                progress.source_active.store(false, Ordering::Release);
                source_lost = true;
                if last_restart.is_none_or(|at| at.elapsed() >= RESTART_EVERY) {
                    last_restart = Some(Instant::now());
                    match source.restart() {
                        Ok(()) => log::warn!("Meeting microphone stopped delivering; restarted it"),
                        Err(error) => log::warn!("Meeting microphone restart failed: {error}"),
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
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

    #[derive(Clone, Default)]
    struct FakeSource {
        restarts: Arc<AtomicU64>,
        stops: Arc<AtomicU64>,
    }

    impl Source for FakeSource {
        fn restart(&self) -> Result<(), String> {
            self.restarts.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// Tests share the process-wide route; run them one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn deliver(seconds: f64) {
        let block = vec![0.1f32; 512];
        let blocks = (seconds * f64::from(MIC_RATE) / 512.0) as usize;
        for _ in 0..blocks {
            feed(&block);
        }
    }

    #[test]
    fn frames_are_written_without_blocking_and_finish_seals_the_session() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let source = FakeSource::default();
        let recording =
            Recording::begin(root.path(), "Standup", "Mic".into(), source.clone()).unwrap();
        assert!(
            Recording::begin(root.path(), "Second", "Mic".into(), FakeSource::default()).is_err()
        );
        // Delivered in a burst faster than real time: frame counts, not
        // arrival stamps, place audio within the run.
        deliver(2.0);
        let progress = Arc::clone(&recording.progress);
        let manifest = recording.finish().unwrap();
        assert_eq!(
            manifest.state,
            super::super::storage::SessionState::Recorded
        );
        let frames: u64 = manifest.tracks[0].chunks.iter().map(|c| c.frames).sum();
        assert_eq!(frames, progress.frames.load(Ordering::Acquire));
        assert_eq!(frames, (2 * MIC_RATE as u64 / 512) * 512);
        assert!(manifest.tracks[0].gaps.is_empty());
        assert!(
            matches!(&manifest.tracks[0].source, TrackSource::Microphone { device } if device == "Mic")
        );
        // Nothing is routed once finished.
        feed(&[0.0; 512]);
        assert_eq!(source.stops.load(Ordering::Acquire), 0);
    }

    #[test]
    fn a_silent_source_is_restarted_and_its_loss_is_an_explicit_gap() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let source = FakeSource::default();
        let recording =
            Recording::begin(root.path(), "Loss", "Mic".into(), source.clone()).unwrap();
        deliver(0.5);
        std::thread::sleep(SOURCE_LOST_AFTER + Duration::from_millis(600));
        assert!(!recording.progress.source_active.load(Ordering::Acquire));
        assert_eq!(source.restarts.load(Ordering::Acquire), 1);
        deliver(0.5);
        let manifest = recording.finish().unwrap();
        let track = &manifest.tracks[0];
        assert_eq!(track.chunks.len(), 2);
        assert_eq!(track.gaps.len(), 1);
        assert_eq!(track.gaps[0].reason, GapReason::SourceLost);
        let gap_s = (track.gaps[0].end_ns - track.gaps[0].start_ns) as f64 / 1e9;
        assert!(gap_s > 3.0, "the gap covers the silent time: {gap_s} s");
    }

    #[test]
    fn a_full_queue_drops_and_records_an_overflow_gap() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let recording =
            Recording::begin(root.path(), "Overflow", "Mic".into(), FakeSource::default()).unwrap();
        deliver(0.5);
        std::thread::sleep(Duration::from_millis(400));
        // Report a drop as the delivery thread would, then resume 1 s later.
        ROUTE
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .dropped
            .fetch_add(16_000, Ordering::AcqRel);
        std::thread::sleep(Duration::from_millis(1_000));
        deliver(0.5);
        let manifest = recording.finish().unwrap();
        let gaps = &manifest.tracks[0].gaps;
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert_eq!(gaps[0].reason, GapReason::Overflow);
    }

    #[test]
    fn a_delivery_stall_without_loss_is_not_a_gap() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let recording =
            Recording::begin(root.path(), "Stall", "Mic".into(), FakeSource::default()).unwrap();
        deliver(0.5);
        // The recorder's ring held the audio through a 1 s stall, then burst it.
        std::thread::sleep(Duration::from_millis(1_000));
        deliver(1.0);
        let manifest = recording.finish().unwrap();
        let track = &manifest.tracks[0];
        assert!(track.gaps.is_empty());
        assert_eq!(track.chunks.len(), 1);
    }
}
