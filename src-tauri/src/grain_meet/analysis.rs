//! Analysis audio for one track: its saved chunks as one 16 kHz mono float32
//! file, plus the exact map back to the meeting clock.
//!
//! As in FluidMeet, a track's runs (epochs) are joined with 0.5 s of silence
//! so one diarization run keeps one speaker state for the whole track. A run
//! is a stretch of chunks with no gap and one format; the resampler is
//! continuous within a run and restarts between runs. Separator audio maps
//! to no meeting time and is never transcript coverage.

use std::path::Path;

use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler, WindowFunction};

use super::storage::{chunk_path, Chunk, Track};
use crate::grain_audio_journal::PcmJournal;
use crate::grain_engine::{ANALYSIS_SAMPLE_RATE, MAX_ANALYSIS_SAMPLES};

const RATE: u64 = ANALYSIS_SAMPLE_RATE as u64;
/// FluidMeet's `epochJoinSilenceSeconds`.
pub const SEPARATOR_SAMPLES: u64 = RATE / 2;
/// Chunks continue one run when they meet within this tolerance.
const CONTIGUOUS_NS: u64 = 1_000_000;
const RESAMPLER_CHUNK: usize = 1024;
const READ_FRAMES: usize = 4096;
const MAX_TAIL_ROUNDS: usize = 16;

/// One run of a track in analysis samples and on the meeting clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub analysis_start: u64,
    pub samples: u64,
    pub meeting_start_ns: u64,
}

impl Piece {
    pub fn analysis_end(&self) -> u64 {
        self.analysis_start + self.samples
    }

    fn meeting_ns(&self, sample: u64) -> u64 {
        let offset = sample.saturating_sub(self.analysis_start).min(self.samples);
        self.meeting_start_ns + offset * 1_000_000_000 / RATE
    }
}

/// Why a track has no analysis audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unprepared {
    Empty,
    TooLong { minutes: u64, limit_minutes: u64 },
    Failed(String),
}

/// A track's disposable analysis file. Dropping it deletes the file.
pub struct TrackAnalysis {
    pub journal: PcmJournal,
    pub pieces: Vec<Piece>,
}

impl TrackAnalysis {
    pub fn samples(&self) -> u64 {
        self.pieces.last().map_or(0, Piece::analysis_end)
    }

    /// The run containing an analysis sample, or the nearer run edge for a
    /// sample inside a separator.
    pub fn piece_at(&self, sample: u64) -> usize {
        let after = self.pieces.partition_point(|p| p.analysis_start <= sample);
        let index = after.saturating_sub(1);
        let piece = &self.pieces[index];
        if sample < piece.analysis_end() || index + 1 == self.pieces.len() {
            return index;
        }
        let next = &self.pieces[index + 1];
        if sample - piece.analysis_end() <= next.analysis_start - sample {
            index
        } else {
            index + 1
        }
    }

    /// Meeting time of an analysis sample, clamped into run `piece`.
    pub fn meeting_ns(&self, piece: usize, sample: u64) -> u64 {
        let piece = &self.pieces[piece];
        piece.meeting_ns(sample.clamp(piece.analysis_start, piece.analysis_end()))
    }

    /// Map an analysis interval onto every run it overlaps, on the meeting
    /// clock. Separator time is dropped.
    pub fn meeting_intervals(&self, start: u64, end: u64) -> Vec<(usize, u64, u64)> {
        self.pieces
            .iter()
            .enumerate()
            .filter_map(|(index, piece)| {
                let s = start.max(piece.analysis_start);
                let e = end.min(piece.analysis_end());
                (e > s).then(|| (index, piece.meeting_ns(s), piece.meeting_ns(e)))
            })
            .collect()
    }
}

pub fn ms_to_samples(ms: i64) -> u64 {
    (ms.max(0) as u64) * RATE / 1_000
}

/// Analysis samples a run of `frames` at `rate` produces.
fn resampled_len(frames: u64, rate: u32) -> u64 {
    (u128::from(frames) * u128::from(RATE) / u128::from(rate)) as u64
}

/// Split a track's chunks into runs: a new run at any gap or format change.
fn runs(track: &Track) -> Vec<&[Chunk]> {
    let mut runs = Vec::new();
    let mut start = 0;
    for i in 1..=track.chunks.len() {
        let split = i == track.chunks.len() || {
            let (a, b) = (&track.chunks[i - 1], &track.chunks[i]);
            b.start_ns > a.end_ns() + CONTIGUOUS_NS
                || b.sample_rate != a.sample_rate
                || b.channels != a.channels
        };
        if split {
            if i > start {
                runs.push(&track.chunks[start..i]);
            }
            start = i;
        }
    }
    runs
}

/// Write a track's analysis file. Checks the 90-minute cap from the manifest
/// before reading any audio.
pub fn prepare(session: &Path, track: &Track) -> Result<TrackAnalysis, Unprepared> {
    let runs = runs(track);
    if runs.is_empty() {
        return Err(Unprepared::Empty);
    }
    let mut total = 0u64;
    for (index, run) in runs.iter().enumerate() {
        let frames: u64 = run.iter().map(|c| c.frames).sum();
        total += resampled_len(frames, run[0].sample_rate);
        if index > 0
            && run[0].start_ns > runs[index - 1].last().map_or(0, Chunk::end_ns) + CONTIGUOUS_NS
        {
            total += SEPARATOR_SAMPLES;
        }
    }
    if total > MAX_ANALYSIS_SAMPLES {
        return Err(Unprepared::TooLong {
            minutes: total / (60 * RATE),
            limit_minutes: MAX_ANALYSIS_SAMPLES / (60 * RATE),
        });
    }
    let failed = |error: String| Unprepared::Failed(error);
    let journal = PcmJournal::create()
        .map_err(|e| failed(format!("Could not create analysis audio: {e}")))?;
    let mut pieces = Vec::with_capacity(runs.len());
    let mut written = 0u64;
    let mut previous_end: Option<u64> = None;
    for run in runs {
        if previous_end.is_some_and(|end| run[0].start_ns > end + CONTIGUOUS_NS) {
            journal
                .append(&vec![0.0; SEPARATOR_SAMPLES as usize])
                .map_err(|e| failed(format!("Could not write analysis audio: {e}")))?;
            written += SEPARATOR_SAMPLES;
        }
        let samples = write_run(session, track, run, &journal).map_err(failed)?;
        pieces.push(Piece {
            analysis_start: written,
            samples,
            meeting_start_ns: run[0].start_ns,
        });
        written += samples;
        previous_end = run.last().map(Chunk::end_ns);
    }
    journal
        .close()
        .map_err(|e| failed(format!("Could not finish analysis audio: {e}")))?;
    debug_assert_eq!(written, total);
    Ok(TrackAnalysis { journal, pieces })
}

/// Downmix and resample one run into the journal. Returns samples written,
/// exactly `floor(frames * 16000 / rate)`.
fn write_run(
    session: &Path,
    track: &Track,
    run: &[Chunk],
    journal: &PcmJournal,
) -> Result<u64, String> {
    let rate = run[0].sample_rate;
    let channels = usize::from(run[0].channels);
    let frames: u64 = run.iter().map(|c| c.frames).sum();
    let expected = resampled_len(frames, rate);
    let mut resampler = MonoResampler::new(rate)?;
    let mut mono = Vec::with_capacity(READ_FRAMES);
    let mut emit = |samples: &[f32]| {
        journal
            .append(samples)
            .map_err(|e| format!("Could not write analysis audio: {e}"))
    };
    for chunk in run {
        let path = chunk_path(session, track, chunk);
        let reader = hound::WavReader::open(&path)
            .map_err(|e| format!("Meeting audio {} is unreadable: {e}", path.display()))?;
        let spec = reader.spec();
        if spec.sample_rate != rate || usize::from(spec.channels) != channels {
            return Err(format!(
                "Meeting audio {} does not match its manifest",
                path.display()
            ));
        }
        if u64::from(reader.duration()) != chunk.frames {
            return Err(format!(
                "Meeting audio {} is shorter than recorded",
                path.display()
            ));
        }
        let mut frame_sum = 0.0f32;
        let mut in_frame = 0usize;
        for sample in reader.into_samples::<f32>() {
            let sample = sample
                .map_err(|e| format!("Meeting audio {} is unreadable: {e}", path.display()))?;
            frame_sum += if sample.is_finite() { sample } else { 0.0 };
            in_frame += 1;
            if in_frame == channels {
                mono.push(frame_sum / channels as f32);
                frame_sum = 0.0;
                in_frame = 0;
                if mono.len() == READ_FRAMES {
                    resampler.push(&mono, &mut emit)?;
                    mono.clear();
                }
            }
        }
    }
    resampler.push(&mono, &mut emit)?;
    resampler.finish(expected, &mut emit)?;
    Ok(expected)
}

/// Mono resampler to 16 kHz with the leading filter delay removed, so output
/// sample `n` is input time `n / 16000`. Handy's rubato configuration: one
/// 1024-frame FFT block, BlackmanHarris2.
struct MonoResampler {
    fft: Option<Fft<f32>>,
    in_buf: Vec<f32>,
    out_buf: Vec<f32>,
    /// Leading output frames still to discard.
    skip: u64,
    emitted: u64,
}

impl MonoResampler {
    fn new(in_hz: u32) -> Result<Self, String> {
        if u64::from(in_hz) == RATE {
            return Ok(Self {
                fft: None,
                in_buf: Vec::new(),
                out_buf: Vec::new(),
                skip: 0,
                emitted: 0,
            });
        }
        let fft = Fft::<f32>::new_custom(
            in_hz as usize,
            RATE as usize,
            RESAMPLER_CHUNK,
            1,
            1,
            WindowFunction::BlackmanHarris2,
            FixedSync::Input,
        )
        .map_err(|e| format!("Unsupported meeting sample rate {in_hz} Hz: {e}"))?;
        Ok(Self {
            skip: fft.output_delay() as u64,
            out_buf: vec![0.0; fft.output_frames_max()],
            in_buf: Vec::with_capacity(RESAMPLER_CHUNK),
            fft: Some(fft),
            emitted: 0,
        })
    }

    fn push(
        &mut self,
        mut input: &[f32],
        emit: &mut impl FnMut(&[f32]) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.fft.is_none() {
            self.emitted += input.len() as u64;
            return if input.is_empty() {
                Ok(())
            } else {
                emit(input)
            };
        }
        while !input.is_empty() {
            let take = (RESAMPLER_CHUNK - self.in_buf.len()).min(input.len());
            self.in_buf.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.in_buf.len() == RESAMPLER_CHUNK {
                self.process(u64::MAX, emit)?;
            }
        }
        Ok(())
    }

    /// Flush the tail until exactly `expected` samples have been emitted.
    fn finish(
        &mut self,
        expected: u64,
        emit: &mut impl FnMut(&[f32]) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.fft.is_some() {
            if !self.in_buf.is_empty() {
                self.process(expected, emit)?;
            }
            let mut rounds = 0;
            while self.emitted < expected && rounds < MAX_TAIL_ROUNDS {
                rounds += 1;
                self.process(expected, emit)?;
            }
        }
        if self.emitted != expected {
            return Err(format!(
                "Resampler produced {} of {expected} analysis samples",
                self.emitted
            ));
        }
        Ok(())
    }

    /// Resample `in_buf` (zero-padded to a block), emitting at most up to `limit`.
    fn process(
        &mut self,
        limit: u64,
        emit: &mut impl FnMut(&[f32]) -> Result<(), String>,
    ) -> Result<(), String> {
        let fft = self.fft.as_mut().expect("resampling");
        let in_frames = self.in_buf.len();
        let out_len = self.out_buf.len();
        let input = SequentialSlice::new(&self.in_buf, 1, in_frames).map_err(|e| e.to_string())?;
        let mut output =
            SequentialSlice::new_mut(&mut self.out_buf, 1, out_len).map_err(|e| e.to_string())?;
        let indexing = Indexing::new().partial_len(in_frames);
        let (_, frames) = fft
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(|e| format!("Resampling failed: {e}"))?;
        self.in_buf.clear();
        let mut out = &self.out_buf[..frames];
        let skip = (self.skip as usize).min(out.len());
        self.skip -= skip as u64;
        out = &out[skip..];
        let room = limit.saturating_sub(self.emitted).min(out.len() as u64) as usize;
        if room > 0 {
            self.emitted += room as u64;
            emit(&out[..room])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_meet::storage::{Block, Manifest, SessionWriter, TrackSource};

    fn sine(rate: u32, frames: usize, hz: f64) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin() as f32 * 0.5
            })
            .collect()
    }

    fn resample(rate: u32, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        let mut r = MonoResampler::new(rate).unwrap();
        let mut emit = |s: &[f32]| {
            out.extend_from_slice(s);
            Ok(())
        };
        for block in input.chunks(777) {
            r.push(block, &mut emit).unwrap();
        }
        r.finish(resampled_len(input.len() as u64, rate), &mut emit)
            .unwrap();
        out
    }

    #[test]
    fn resampling_is_exact_in_length_and_free_of_filter_delay() {
        for rate in [16_000u32, 22_050, 44_100, 48_000] {
            let input = sine(rate, rate as usize * 2 + 123, 440.0);
            let out = resample(rate, &input);
            assert_eq!(
                out.len() as u64,
                resampled_len(input.len() as u64, rate),
                "{rate} Hz"
            );
            let want = sine(16_000, out.len(), 440.0);
            // Delay compensation: a non-periodic chirp aligns best at lag zero.
            let chirp = |rate: u32, frames: usize| -> Vec<f32> {
                (0..frames)
                    .map(|i| {
                        let t = i as f64 / f64::from(rate);
                        (2.0 * std::f64::consts::PI * (100.0 * t + 400.0 * t * t)).sin() as f32
                            * 0.5
                    })
                    .collect::<Vec<_>>()
            };
            let got = resample(rate, &chirp(rate, input.len()));
            let ideal = chirp(16_000, got.len());
            let correlate = |lag: i64| -> f64 {
                (2_000..30_000)
                    .map(|i| f64::from(got[i]) * f64::from(ideal[(i as i64 + lag) as usize]))
                    .sum()
            };
            let best = (-40..=40)
                .max_by(|a, b| correlate(*a).total_cmp(&correlate(*b)))
                .unwrap();
            assert_eq!(best, 0, "{rate} Hz: residual lag {best} samples");
            // Only a sub-sample residue remains (about 20 µs at 22.05 kHz), far
            // below the 80 ms model frames.
            let error = out[2_000..30_000]
                .iter()
                .zip(&want[2_000..30_000])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(error < 0.05, "{rate} Hz: max error {error}");
        }
    }

    fn record(root: &Path, plan: &[(u64, u32, u16, usize)]) -> (std::path::PathBuf, Manifest) {
        let mut writer = SessionWriter::create(root, "Analysis").unwrap();
        let track = writer
            .add_track(TrackSource::Imported {
                name: "fixture".into(),
            })
            .unwrap();
        for &(start_ns, rate, channels, frames) in plan {
            let mono = sine(rate, frames, 300.0);
            let samples: Vec<f32> = mono
                .iter()
                .flat_map(|&s| std::iter::repeat_n(s, usize::from(channels)))
                .collect();
            writer
                .append(
                    track,
                    Block {
                        start_ns,
                        sample_rate: rate,
                        channels,
                        samples: &samples,
                        discontinuity: None,
                    },
                )
                .unwrap();
        }
        let dir = writer.dir().to_path_buf();
        (dir, writer.finish().unwrap())
    }

    #[test]
    fn runs_join_with_one_separator_per_gap_and_map_back_to_the_meeting_clock() {
        let root = tempfile::tempdir().unwrap();
        const S: u64 = 1_000_000_000;
        // 70 s at 48 kHz stereo (two chunks, one run), a 5 s gap, then 2 s at
        // 16 kHz mono, contiguous with a 1 s run at 44.1 kHz (format change, no gap).
        let (dir, manifest) = record(
            root.path(),
            &[
                (0, 48_000, 2, 48_000 * 70),
                (75 * S, 16_000, 1, 32_000),
                (77 * S, 44_100, 1, 44_100),
            ],
        );
        let track = &manifest.tracks[0];
        assert_eq!(track.chunks.len(), 4);
        let analysis = prepare(&dir, track).unwrap();
        let pieces: Vec<(u64, u64, u64)> = analysis
            .pieces
            .iter()
            .map(|p| (p.analysis_start, p.samples, p.meeting_start_ns / S))
            .collect();
        assert_eq!(
            pieces,
            vec![
                (0, 16_000 * 70, 0),
                (16_000 * 70 + 8_000, 32_000, 75),
                (16_000 * 72 + 8_000, 16_000, 77),
            ]
        );
        assert_eq!(analysis.journal.frame_count(), analysis.samples());
        // Inside the separator: the nearer run edge.
        assert_eq!(analysis.piece_at(16_000 * 70 + 1_000), 0);
        assert_eq!(analysis.piece_at(16_000 * 70 + 7_000), 1);
        assert_eq!(analysis.meeting_ns(1, 16_000 * 70 + 8_000 + 16_000), 76 * S);
        let mapped = analysis.meeting_intervals(16_000 * 69, 16_000 * 71 + 8_000);
        assert_eq!(mapped, vec![(0, 69 * S, 70 * S), (1, 75 * S, 76 * S)]);
        let path = analysis.journal.path().to_path_buf();
        drop(analysis);
        assert!(!path.exists(), "analysis audio is disposable");
    }

    #[test]
    fn the_cap_is_checked_before_any_audio_is_read() {
        let root = tempfile::tempdir().unwrap();
        let (dir, mut manifest) = record(root.path(), &[(0, 16_000, 1, 16_000)]);
        // Pretend the run is 91 minutes; its audio is never opened.
        manifest.tracks[0].chunks[0].frames = 91 * 60 * 16_000;
        let error = prepare(&dir, &manifest.tracks[0]).err().unwrap();
        assert_eq!(
            error,
            Unprepared::TooLong {
                minutes: 91,
                limit_minutes: 90
            }
        );
        let mut empty = manifest.clone();
        empty.tracks[0].chunks.clear();
        assert_eq!(
            prepare(&dir, &empty.tracks[0]).err().unwrap(),
            Unprepared::Empty
        );
    }
}
