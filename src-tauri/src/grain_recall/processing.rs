//! Saved-audio processing: one attempt turns a recorded session into a
//! published, speaker-attributed transcript revision.
//!
//! FluidMeet's phase order with one resident model at a time: prepare every
//! track's analysis audio, diarize each whole track (one speaker state per
//! track), release the diarizer, transcribe each track with bounded Parakeet
//! windows, then attribute words and publish. Unlike FluidMeet, a diarizer
//! failure does not suppress ASR: that track is published without speakers.

use std::path::{Path, PathBuf};

use super::analysis::{self, TrackAnalysis, Unprepared};
use super::storage::{self, SessionState};
use super::transcript::{
    self, Activity, Attribution, MeetingWord, ModelRef, TrackStatus, TrackTranscript,
    TranscriptRevision, Word,
};
use crate::engine_supervisor::LoadedInfo;
use crate::grain_engine::Role;
use crate::grain_transcription::MeetingEngine;
use crate::tdt_flow::{TdtAccumulator, TdtRunConfig};

pub const CANCELLED: &str = "Meeting processing cancelled";

/// The phase an attempt is in, for progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub enum Stage {
    Preparing,
    IdentifyingSpeakers,
    Transcribing,
    Publishing,
}

pub struct Models {
    pub diarizer: PathBuf,
    /// A reviewed Parakeet TDT v2/v3 model (the Flow contract).
    pub asr: PathBuf,
}

/// Sortformer's offline-file operating point (FluidMeet and Glimpse use the
/// same offline point for whole-file diarization).
fn offline() -> transcribe_cpp::DiarizeOptions {
    transcribe_cpp::DiarizeOptions {
        family: Some(transcribe_cpp::DiarizeExtension::Sortformer(
            transcribe_cpp::SortformerDiarizeOptions {
                preset: Some(transcribe_cpp::SortformerPreset::VeryHighLatency),
            },
        )),
    }
}

fn model_ref(path: &Path, info: &LoadedInfo) -> ModelRef {
    ModelRef {
        file: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        arch: info.arch.clone(),
        variant: info.variant.clone(),
        backend: info.backend.clone(),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// Process a recorded session and publish the next transcript revision.
/// Cancellation publishes nothing; every other failure is per track.
pub fn process(
    session: &Path,
    engine: &MeetingEngine<'_>,
    models: &Models,
    attempt_id: &str,
    on_stage: &dyn Fn(Stage),
) -> Result<TranscriptRevision, String> {
    let stop = |engine: &MeetingEngine<'_>| -> Result<(), String> {
        if engine.cancelled() {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    };
    let manifest = storage::load_manifest(session)?;
    if manifest.state != SessionState::Recorded {
        return Err("Finish or recover the recording before processing it".into());
    }

    on_stage(Stage::Preparing);
    // Analysis audio: disk-backed, deleted when this attempt ends.
    let mut prepared: Vec<Result<TrackAnalysis, Unprepared>> =
        Vec::with_capacity(manifest.tracks.len());
    for track in &manifest.tracks {
        stop(engine)?;
        prepared.push(analysis::prepare(session, track));
    }
    let any_audio = prepared.iter().any(Result::is_ok);

    // Phase A: one diarization run per whole track.
    let mut diarizer = None;
    let mut speakers: Vec<Result<Vec<Activity>, String>> = prepared
        .iter()
        .map(|_| Err("No audio".to_string()))
        .collect();
    if any_audio {
        on_stage(Stage::IdentifyingSpeakers);
        match engine.load(&models.diarizer, Role::Diarize) {
            Ok(info) => {
                diarizer = Some(model_ref(&models.diarizer, &info));
                for (index, analysis) in prepared.iter().enumerate() {
                    let Ok(analysis) = analysis else { continue };
                    stop(engine)?;
                    speakers[index] = engine
                        .diarize(analysis.journal.path(), offline())
                        .map(|diarization| activity(analysis, &diarization.segments));
                    if speakers[index].is_err() {
                        stop(engine)?;
                    }
                }
            }
            Err(error) => {
                stop(engine)?;
                log::warn!("Meeting diarizer unavailable: {error}");
                for result in &mut speakers {
                    *result = Err(format!("Speaker detection is unavailable: {error}"));
                }
            }
        }
    }

    // Phase B: bounded Parakeet decoding per track. Loading ASR releases the diarizer.
    let mut asr = None;
    let mut texts: Vec<Result<(String, Vec<transcript::AnalysisWord>), String>> = prepared
        .iter()
        .map(|_| Err("No audio".to_string()))
        .collect();
    if any_audio {
        on_stage(Stage::Transcribing);
        match engine.load(&models.asr, Role::Asr) {
            Ok(info) => {
                asr = Some(model_ref(&models.asr, &info));
                let config = TdtRunConfig {
                    model_id: asr.as_ref().map(|m| m.file.clone()).unwrap_or_default(),
                    language: None,
                };
                for (index, analysis) in prepared.iter().enumerate() {
                    let Ok(analysis) = analysis else { continue };
                    stop(engine)?;
                    texts[index] = transcribe(engine, &config, analysis);
                    if texts[index].is_err() {
                        stop(engine)?;
                    }
                }
            }
            Err(error) => {
                stop(engine)?;
                for result in &mut texts {
                    *result = Err(format!("Transcription is unavailable: {error}"));
                }
            }
        }
    }
    stop(engine)?;
    on_stage(Stage::Publishing);

    let mut tracks = Vec::with_capacity(manifest.tracks.len());
    for (index, track) in manifest.tracks.iter().enumerate() {
        tracks.push(assemble_track(
            track.id,
            &prepared[index],
            &speakers[index],
            &texts[index],
        ));
    }
    let revision = TranscriptRevision {
        version: transcript::TRANSCRIPT_VERSION,
        revision: transcript::next_revision(session)?,
        session_id: manifest.id.clone(),
        attempt_id: attempt_id.to_string(),
        created_unix_ms: now_ms(),
        diarizer,
        asr,
        speakers: transcript::speakers(&tracks),
        tracks,
    };
    transcript::publish(session, &revision)?;
    Ok(revision)
}

/// Map native speaker segments (analysis milliseconds) onto the meeting clock,
/// clipped to the runs they overlap.
fn activity(
    analysis: &TrackAnalysis,
    segments: &[transcribe_cpp::SpeakerSegment],
) -> Vec<Activity> {
    let mut out = Vec::with_capacity(segments.len());
    for segment in segments {
        let (start, end) = (
            analysis::ms_to_samples(segment.t0_ms),
            analysis::ms_to_samples(segment.t1_ms),
        );
        for (_, start_ns, end_ns) in analysis.meeting_intervals(start, end) {
            out.push(Activity {
                slot: segment.speaker_id,
                start_ns,
                end_ns,
            });
        }
    }
    out.sort_by_key(|a| (a.start_ns, a.slot));
    out
}

fn transcribe(
    engine: &MeetingEngine<'_>,
    config: &TdtRunConfig,
    analysis: &TrackAnalysis,
) -> Result<(String, Vec<transcript::AnalysisWord>), String> {
    // A fresh decoder per track: dropping the session resets native state.
    let mut session = engine.asr()?;
    let mut accumulator = TdtAccumulator::new_structured(&session, config)?;
    let mut reader = analysis
        .journal
        .reader()
        .map_err(|e| format!("Could not read analysis audio: {e}"))?;
    let mut audio = Vec::with_capacity(accumulator.max_input_samples());
    let result = accumulator.render_structured(
        &mut session,
        config,
        &analysis.journal,
        &mut reader,
        &mut audio,
        analysis.journal.frame_count(),
    )?;
    drop(audio);
    let words = transcript::assemble_words(&result.windows, &mut |ids| {
        crate::tdt_flow::Inference::detokenize(&session, ids)
    })?;
    Ok((result.text, words))
}

fn assemble_track(
    track: u32,
    prepared: &Result<TrackAnalysis, Unprepared>,
    speakers: &Result<Vec<Activity>, String>,
    text: &Result<(String, Vec<transcript::AnalysisWord>), String>,
) -> TrackTranscript {
    let empty = |status| TrackTranscript {
        track,
        status,
        text: String::new(),
        words: Vec::new(),
        activity: Vec::new(),
    };
    let analysis = match prepared {
        Ok(analysis) => analysis,
        Err(Unprepared::Empty) => return empty(TrackStatus::Empty),
        Err(Unprepared::TooLong {
            minutes,
            limit_minutes,
        }) => {
            return empty(TrackStatus::TooLong {
                minutes: *minutes,
                limit_minutes: *limit_minutes,
            })
        }
        Err(Unprepared::Failed(reason)) => {
            return empty(TrackStatus::Failed {
                reason: reason.clone(),
            })
        }
    };
    let (text, words) = match text {
        Ok(result) => result,
        Err(reason) => {
            let mut failed = empty(TrackStatus::Failed {
                reason: reason.clone(),
            });
            failed.activity = speakers.as_ref().cloned().unwrap_or_default();
            return failed;
        }
    };
    let words: Vec<MeetingWord> = words
        .iter()
        .map(|word| {
            let (start, end) = (
                analysis::ms_to_samples(word.t0_ms),
                analysis::ms_to_samples(word.t1_ms),
            );
            let piece = analysis.piece_at((start + end) / 2);
            MeetingWord {
                text: word.text.clone(),
                start_ns: analysis.meeting_ns(piece, start),
                end_ns: analysis.meeting_ns(piece, end),
                piece,
            }
        })
        .collect();
    let (status, attribution, activity) = match speakers {
        Ok(activity) => {
            let mut attribution = transcript::attribute(&words, activity);
            transcript::fold_brief_speakers(&words, &mut attribution);
            (TrackStatus::Complete, attribution, activity.clone())
        }
        Err(reason) => (
            TrackStatus::SpeakersUnavailable {
                reason: reason.clone(),
            },
            vec![Attribution::Unavailable; words.len()],
            Vec::new(),
        ),
    };
    TrackTranscript {
        track,
        status,
        text: text.clone(),
        words: words
            .into_iter()
            .zip(attribution)
            .map(|(word, speaker)| Word {
                text: word.text,
                start_ms: word.start_ns / 1_000_000,
                end_ms: word.end_ns / 1_000_000,
                speaker,
            })
            .collect(),
        activity,
    }
}

/// Real-worker pipeline checks: the built Grain executable, Sortformer v2.1,
/// a reviewed Parakeet model and a 16 kHz mono WAV fixture. Run with
/// `--ignored --test-threads=1`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_supervisor::{DeviceSelector, EngineSupervisor};
    use crate::grain_recall::storage::{Block, SessionWriter, TrackSource};
    use std::sync::atomic::AtomicBool;

    fn env_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("set {name}"))
            .into()
    }

    fn fixture() -> Vec<f32> {
        let mut reader = hound::WavReader::open(env_path("GRAIN_RECALL_TEST_AUDIO")).unwrap();
        assert_eq!(
            (reader.spec().sample_rate, reader.spec().channels),
            (16_000, 1)
        );
        match reader.spec().sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i16>()
                .map(|s| f32::from(s.unwrap()) / 32768.0)
                .collect(),
            hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
        }
    }

    fn normalized(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    #[ignore = "requires Grain worker executable, Sortformer, Parakeet and WAV fixtures"]
    fn saved_meeting_becomes_a_speaker_attributed_transcript() {
        const S: u64 = 1_000_000_000;
        let pcm = fixture();
        let seconds = pcm.len() as u64 / 16_000;
        // Run 1 crosses the 60 s chunk boundary; a 2 s gap; then run 2.
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), "Fixture meeting").unwrap();
        let track = writer
            .add_track(TrackSource::Imported {
                name: "fixture".into(),
            })
            .unwrap();
        let first_run = 6u64;
        for i in 0..first_run {
            let block = Block {
                start_ns: i * seconds * S,
                sample_rate: 16_000,
                channels: 1,
                samples: &pcm,
                discontinuity: None,
            };
            writer.append(track, block).unwrap();
        }
        let resume = first_run * seconds * S + 2 * S;
        for i in 0..2 {
            let block = Block {
                start_ns: resume + i * seconds * S,
                sample_rate: 16_000,
                channels: 1,
                samples: &pcm,
                discontinuity: None,
            };
            writer.append(track, block).unwrap();
        }
        let dir = writer.dir().to_path_buf();
        let manifest = writer.finish().unwrap();
        assert!(manifest.tracks[0].chunks.len() >= 3);

        let backend = match std::env::var("GRAIN_RECALL_TEST_BACKEND")
            .as_deref()
            .unwrap_or("cpu")
        {
            "vulkan" => transcribe_cpp::Backend::Vulkan,
            _ => transcribe_cpp::Backend::Cpu,
        };
        let engine = EngineSupervisor::new(true);
        let cancelled = AtomicBool::new(false);
        let attempt = MeetingEngine {
            engine: &engine,
            mark: engine.cancel_mark(),
            cancelled: &cancelled,
            backend,
            device: DeviceSelector::Auto,
            role: std::cell::Cell::new(None),
        };

        // A missing diarizer still yields text, published without speakers.
        let missing = Models {
            diarizer: dir.join("absent.gguf"),
            asr: env_path("GRAIN_RECALL_TEST_ASR"),
        };
        let degraded = process(&dir, &attempt, &missing, "attempt-1", &|_| {}).unwrap();
        assert_eq!(degraded.revision, 1);
        let t = &degraded.tracks[0];
        assert!(
            matches!(t.status, TrackStatus::SpeakersUnavailable { .. }),
            "{:?}",
            t.status
        );
        assert!(!t.words.is_empty());
        assert!(t
            .words
            .iter()
            .all(|w| w.speaker == Attribution::Unavailable));
        assert!(degraded.speakers.is_empty());

        let models = Models {
            diarizer: env_path("GRAIN_RECALL_TEST_DIARIZER"),
            asr: env_path("GRAIN_RECALL_TEST_ASR"),
        };
        let stages = std::cell::RefCell::new(Vec::new());
        let started = std::time::Instant::now();
        let revision = process(&dir, &attempt, &models, "attempt-2", &|stage| {
            stages.borrow_mut().push(stage)
        })
        .unwrap();
        eprintln!(
            "processed {} s of audio in {:?}",
            (first_run + 2) * seconds,
            started.elapsed()
        );
        assert_eq!(revision.revision, 2);
        assert_eq!(
            *stages.borrow(),
            [
                Stage::Preparing,
                Stage::IdentifyingSpeakers,
                Stage::Transcribing,
                Stage::Publishing
            ]
        );
        let t = &revision.tracks[0];
        assert_eq!(t.status, TrackStatus::Complete);
        // Cross-window word assembly loses and duplicates nothing.
        let joined = t
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(normalized(&joined), normalized(&t.text));
        // Every word lies in recorded audio, never in the gap.
        let gap = (first_run * seconds * 1_000, resume / 1_000_000);
        for w in &t.words {
            assert!(
                w.end_ms <= gap.0 || w.start_ms >= gap.1,
                "word {:?} in the gap",
                w
            );
            assert!(w.start_ms <= w.end_ms);
        }
        // Speaker continuity survives chunking and the gap: the fixture's two voices.
        assert_eq!(revision.speakers.len(), 2, "{:?}", revision.speakers);
        let assigned = t
            .words
            .iter()
            .filter(|w| matches!(w.speaker, Attribution::Speaker(_)))
            .count();
        eprintln!(
            "{} words, {assigned} attributed, speakers {:?}",
            t.words.len(),
            revision
                .speakers
                .iter()
                .map(|s| &s.label)
                .collect::<Vec<_>>()
        );
        assert!(
            assigned * 10 >= t.words.len() * 7,
            "{assigned} of {} words attributed",
            t.words.len()
        );
        assert_eq!(transcript::latest(&dir).unwrap().unwrap(), revision);
        engine.unload().wait();
    }
}
