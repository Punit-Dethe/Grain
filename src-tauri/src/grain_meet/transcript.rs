//! Speaker-attributed transcript evidence and its published revisions.
//!
//! Everything here is pure except revision storage. Attribution follows
//! FluidMeet's policy (`MeetingParakeetNemotronBackend.assignment`,
//! `MeetingWordBoundaryRecovery`, `MeetingTranscriptAssembler.makeSpeakers`),
//! with Grain's explicit handling of point-timed words.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::tdt_flow::WindowEvidence;

pub const TRANSCRIPT_VERSION: u32 = 1;
const TRANSCRIPTS: &str = "transcripts";
/// FluidMeet `nemotronCadenceMaximumFloorSeconds`: one 80 ms diarizer frame.
const CADENCE_NS: u64 = 80_000_000;
/// FluidMeet `SpeakerDominancePolicy`.
const LEADER_MIN_FRACTION: f64 = 0.5;
const RUNNER_UP_MAX_FRACTION: f64 = 0.2;
const LEADER_TO_RUNNER_UP: f64 = 2.0;
/// FluidMeet `MeetingWordBoundaryRecovery`.
const RECOVERY_HOP_NS: u64 = 80_000_000;
const RECOVERY_MAX_WORD_NS: u64 = 1_200_000_000;
/// FluidMeet `MeetingTranscriptAssembler.minimumSpeakerWords/Seconds`.
const MIN_SPEAKER_WORDS: usize = 10;
const MIN_SPEAKER_NS: u64 = 5_000_000_000;

/// A word assembled across bounded ASR windows, in analysis milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisWord {
    pub text: String,
    pub t0_ms: i64,
    pub t1_ms: i64,
}

/// Join native per-window words into canonical words. A window edge may split
/// a word: native rows open a word at every window's first token, so a first
/// token without the SentencePiece word marker (decoded as a leading space)
/// continues the previous word, and the joined token ids are detokenized once
/// through the native tokenizer. All other words keep their native text.
pub fn assemble_words(
    windows: &[WindowEvidence],
    detokenize: &mut dyn FnMut(&[i32]) -> Result<String, String>,
) -> Result<Vec<AnalysisWord>, String> {
    let mut words: Vec<AnalysisWord> = Vec::new();
    // Token ids of the last word, kept only while it may still continue.
    let mut last_ids: Vec<i32> = Vec::new();
    for window in windows {
        let transcript = &window.transcript;
        let mut first_in_window = true;
        let mut index = 0;
        while index < transcript.tokens.len() {
            let word_index = transcript.tokens[index].word_index;
            let end = transcript.tokens[index..]
                .iter()
                .position(|t| t.word_index != word_index)
                .map_or(transcript.tokens.len(), |n| index + n);
            let tokens = &transcript.tokens[index..end];
            let native = usize::try_from(word_index)
                .ok()
                .and_then(|i| transcript.words.get(i))
                .ok_or("Native token has no word row")?;
            let t0 = tokens[0].t0_ms;
            let t1 = tokens[tokens.len() - 1].t1_ms;
            if t1 < t0 {
                return Err("Native word has reversed timing".into());
            }
            let ids = tokens.iter().map(|t| t.id);
            let continues =
                first_in_window && !words.is_empty() && !tokens[0].text.starts_with(' ');
            if continues {
                last_ids.extend(ids);
                let word = words.last_mut().expect("checked non-empty");
                word.text = detokenize(&last_ids)?.trim_start_matches(' ').to_string();
                word.t1_ms = word.t1_ms.max(t1);
            } else {
                last_ids.clear();
                last_ids.extend(ids);
                words.push(AnalysisWord {
                    text: native.text.clone(),
                    t0_ms: t0,
                    t1_ms: t1,
                });
            }
            first_in_window = false;
            index = end;
        }
    }
    words.retain(|word| !word.text.trim().is_empty());
    Ok(words)
}

/// One speaker slot's activity on the meeting clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub slot: i32,
    pub start_ns: u64,
    pub end_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attribution {
    Speaker(i32),
    /// Several slots overlap the word without a clear leader.
    Ambiguous(Vec<i32>),
    /// No credible slot (silence, crosstalk, or a voice too brief to name).
    Unassigned,
    /// Speaker detection did not run or failed for this track.
    Unavailable,
}

/// A word placed on the meeting clock, with its run for boundary recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingWord {
    pub text: String,
    pub start_ns: u64,
    pub end_ns: u64,
    pub piece: usize,
}

fn union_ns(mut intervals: Vec<(u64, u64)>) -> u64 {
    intervals.sort_unstable();
    let mut total = 0;
    let mut current: Option<(u64, u64)> = None;
    for (s, e) in intervals {
        match current {
            Some((cs, ce)) if s <= ce => current = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                current = Some((s, e));
            }
            None => current = Some((s, e)),
        }
    }
    total + current.map_or(0, |(s, e)| e - s)
}

/// FluidMeet's coverage attribution for one word.
pub fn assign(start_ns: u64, end_ns: u64, activity: &[Activity]) -> Attribution {
    if end_ns <= start_ns {
        // A point-timed word: only containment is evidence.
        let slots: BTreeSet<i32> = activity
            .iter()
            .filter(|a| a.start_ns <= start_ns && start_ns <= a.end_ns)
            .map(|a| a.slot)
            .collect();
        return match slots.len() {
            0 => Attribution::Unassigned,
            1 => Attribution::Speaker(*slots.first().expect("one slot")),
            _ => Attribution::Ambiguous(slots.into_iter().collect()),
        };
    }
    let duration = end_ns - start_ns;
    let threshold = CADENCE_NS.min(duration / 5);
    let mut by_slot: BTreeMap<i32, Vec<(u64, u64)>> = BTreeMap::new();
    for a in activity {
        let (s, e) = (a.start_ns.max(start_ns), a.end_ns.min(end_ns));
        if e > s {
            by_slot.entry(a.slot).or_default().push((s, e));
        }
    }
    let mut ranked: Vec<(i32, u64)> = by_slot
        .into_iter()
        .map(|(slot, intervals)| (slot, union_ns(intervals)))
        .filter(|&(_, covered)| covered > threshold)
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    match ranked.as_slice() {
        [] => Attribution::Unassigned,
        [(slot, _)] => Attribution::Speaker(*slot),
        [(leader, lead), (_, runner), ..] => {
            let d = duration as f64;
            let (lead, runner) = (*lead as f64, *runner as f64);
            if lead / d >= LEADER_MIN_FRACTION
                && runner / d <= RUNNER_UP_MAX_FRACTION
                && lead >= LEADER_TO_RUNNER_UP * runner
            {
                Attribution::Speaker(*leader)
            } else {
                Attribution::Ambiguous(ranked.iter().map(|(slot, _)| *slot).collect())
            }
        }
    }
}

/// Attribute every word of one track, then apply FluidMeet's boundary
/// recovery: an unassigned word directly after a word anchored to a speaker
/// whose activity ends at the word's onset inherits that speaker, unless any
/// other speaker starts there. Recovered words never anchor further words.
pub fn attribute(words: &[MeetingWord], activity: &[Activity]) -> Vec<Attribution> {
    let mut result: Vec<Attribution> = words
        .iter()
        .map(|w| assign(w.start_ns, w.end_ns, activity))
        .collect();
    let mut recovered = vec![false; words.len()];
    for i in 1..words.len() {
        let (word, previous) = (&words[i], &words[i - 1]);
        let Attribution::Speaker(slot) = result[i - 1] else {
            continue;
        };
        if recovered[i - 1]
            || result[i] != Attribution::Unassigned
            || word.piece != previous.piece
            || word.end_ns <= word.start_ns
            || previous.end_ns <= previous.start_ns
            || word.end_ns - word.start_ns > RECOVERY_MAX_WORD_NS
            || word.start_ns.abs_diff(previous.end_ns) > RECOVERY_HOP_NS
        {
            continue;
        }
        let previous_duration = previous.end_ns - previous.start_ns;
        let anchored = activity.iter().any(|a| {
            a.slot == slot
                && a.end_ns
                    .min(previous.end_ns)
                    .saturating_sub(a.start_ns.max(previous.start_ns))
                    * 2
                    >= previous_duration
                && a.end_ns.abs_diff(word.start_ns) <= RECOVERY_HOP_NS
        });
        if !anchored {
            continue;
        }
        let competing = activity.iter().any(|a| {
            a.slot != slot
                && a.end_ns.min(word.end_ns + RECOVERY_HOP_NS) > a.start_ns.max(previous.start_ns)
        });
        if !competing {
            result[i] = Attribution::Speaker(slot);
            recovered[i] = true;
        }
    }
    result
}

/// FluidMeet's naming rule for one track: beside a substantial speaker
/// (10 words or 5 s), a slot heard only briefly is not a person; its words
/// stay in the transcript unassigned. A track where every slot is brief keeps
/// them all.
pub fn fold_brief_speakers(words: &[MeetingWord], attribution: &mut [Attribution]) {
    let mut evidence: BTreeMap<i32, (usize, u64)> = BTreeMap::new();
    for (word, attribution) in words.iter().zip(attribution.iter()) {
        if let Attribution::Speaker(slot) = attribution {
            let entry = evidence.entry(*slot).or_default();
            entry.0 += word.text.split_whitespace().count();
            entry.1 += word.end_ns.saturating_sub(word.start_ns);
        }
    }
    let substantial: BTreeSet<i32> = evidence
        .iter()
        .filter(|(_, (count, ns))| *count >= MIN_SPEAKER_WORDS || *ns >= MIN_SPEAKER_NS)
        .map(|(slot, _)| *slot)
        .collect();
    if substantial.is_empty() {
        return;
    }
    for attribution in attribution.iter_mut() {
        if matches!(attribution, Attribution::Speaker(slot) if !substantial.contains(slot)) {
            *attribution = Attribution::Unassigned;
        }
    }
}

// --- Published revisions ---------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Longer than the per-track processing cap; its audio is kept.
    TooLong {
        minutes: u64,
        limit_minutes: u64,
    },
    /// The track holds no audio.
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    /// Meeting clock, milliseconds from the session start.
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Attribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaker {
    /// Stable within a revision: `t{track}-s{slot}`.
    pub id: String,
    pub track: u32,
    pub slot: i32,
    /// Anonymous label, numbered across the meeting.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ModelRef {
    pub file: String,
    pub arch: String,
    pub variant: String,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

fn revision_path(session: &Path, revision: u32) -> PathBuf {
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

/// Publish a revision atomically. Earlier revisions are never modified.
pub fn publish(session: &Path, revision: &TranscriptRevision) -> Result<(), String> {
    let dir = session.join(TRANSCRIPTS);
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create meeting transcripts: {e}"))?;
    let path = revision_path(session, revision.revision);
    if path.exists() {
        return Err(format!(
            "Transcript revision {} already exists",
            revision.revision
        ));
    }
    let tmp = dir.join(format!("{:06}.json.tmp", revision.revision));
    let json = serde_json::to_vec(revision).map_err(|e| e.to_string())?;
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        fs::rename(&tmp, &path)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not save the meeting transcript: {e}")
    })
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
    use grain_tdt::Window;

    const MS: u64 = 1_000_000;

    fn token(id: i32, t0: i64, t1: i64, word: i32, text: &str) -> transcribe_cpp::Token {
        transcribe_cpp::Token {
            id,
            p: 0.9,
            t0_ms: t0,
            t1_ms: t1,
            word_index: word,
            text: text.into(),
            ..Default::default()
        }
    }

    fn window(tokens: Vec<transcribe_cpp::Token>, words: &[&str]) -> WindowEvidence {
        WindowEvidence {
            window: Window {
                chunk_start: 0,
                read_start: 0,
                read_end: 0,
                is_last: false,
            },
            transcript: transcribe_cpp::Transcript {
                tokens,
                words: words
                    .iter()
                    .map(|w| transcribe_cpp::Word {
                        text: (*w).into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn words_split_by_a_window_edge_are_joined_through_the_tokenizer() {
        let windows = [
            window(
                vec![token(1, 0, 80, 0, " Hello"), token(2, 160, 240, 1, " wor")],
                &["Hello", "wor"],
            ),
            // Window 2 opens with "ld" (no marker): it continues "wor".
            window(
                vec![
                    token(3, 240, 320, 0, "ld"),
                    token(4, 320, 320, 0, ","),
                    token(5, 400, 480, 1, " again"),
                ],
                &["ld,", "again"],
            ),
            // Window 3 opens with a marked token: a new word.
            window(vec![token(6, 560, 640, 0, " now")], &["now"]),
        ];
        let mut calls = Vec::new();
        let words = assemble_words(&windows, &mut |ids| {
            calls.push(ids.to_vec());
            Ok(" world,".into())
        })
        .unwrap();
        assert_eq!(
            calls,
            vec![vec![2, 3, 4]],
            "one native detokenization per split word"
        );
        let rows: Vec<(&str, i64, i64)> = words
            .iter()
            .map(|w| (w.text.as_str(), w.t0_ms, w.t1_ms))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("Hello", 0, 80),
                ("world,", 160, 320),
                ("again", 400, 480),
                ("now", 560, 640)
            ]
        );
        assert!(assemble_words(
            &[window(vec![token(1, 0, 80, 3, " x")], &["x"])],
            &mut |_| Ok(String::new())
        )
        .is_err());
    }

    fn act(slot: i32, start_ms: u64, end_ms: u64) -> Activity {
        Activity {
            slot,
            start_ns: start_ms * MS,
            end_ns: end_ms * MS,
        }
    }

    #[test]
    fn attribution_follows_fluidmeet_coverage_and_dominance() {
        let activity = [act(1, 0, 1_000), act(2, 900, 2_000), act(3, 1_500, 1_510)];
        // Sole candidate.
        assert_eq!(
            assign(100 * MS, 300 * MS, &activity),
            Attribution::Speaker(1)
        );
        // Slot 2's 5 ms of a 55 ms word is under the floor min(80 ms, 20%).
        assert_eq!(
            assign(850 * MS, 905 * MS, &activity),
            Attribution::Speaker(1)
        );
        // 100 ms each way across the change: no clear leader.
        assert_eq!(
            assign(850 * MS, 1_050 * MS, &activity),
            Attribution::Ambiguous(vec![1, 2])
        );
        // Leader 1000 ms, runner-up 100 ms (10%) of 1000 ms: dominance.
        assert_eq!(
            assign(0, 1_000 * MS, &[act(1, 0, 1_000), act(2, 900, 2_000)]),
            Attribution::Speaker(1)
        );
        // Runner-up at 30% is above the 20% limit.
        assert_eq!(
            assign(0, 1_000 * MS, &[act(1, 0, 1_000), act(2, 700, 2_000)]),
            Attribution::Ambiguous(vec![1, 2])
        );
        // Duplicate segments are unioned: 300 ms, not 900 ms, so no dominance.
        assert_eq!(
            assign(
                0,
                1_000 * MS,
                &[
                    act(1, 0, 300),
                    act(1, 0, 300),
                    act(1, 0, 300),
                    act(2, 900, 1_000)
                ]
            ),
            Attribution::Ambiguous(vec![1, 2])
        );
        assert_eq!(
            assign(3_000 * MS, 3_200 * MS, &activity),
            Attribution::Unassigned
        );
        // Point-timed words use containment, never a division by zero.
        assert_eq!(
            assign(500 * MS, 500 * MS, &activity),
            Attribution::Speaker(1)
        );
        assert_eq!(
            assign(950 * MS, 950 * MS, &activity),
            Attribution::Ambiguous(vec![1, 2])
        );
        assert_eq!(
            assign(2_500 * MS, 2_500 * MS, &activity),
            Attribution::Unassigned
        );
    }

    fn word(text: &str, start_ms: u64, end_ms: u64, piece: usize) -> MeetingWord {
        MeetingWord {
            text: text.into(),
            start_ns: start_ms * MS,
            end_ns: end_ms * MS,
            piece,
        }
    }

    #[test]
    fn boundary_recovery_extends_one_word_and_never_crosses_a_competitor() {
        let activity = [act(1, 0, 1_000)];
        let words = [
            word("so", 600, 1_000, 0),
            word("yes", 1_040, 1_300, 0), // starts 40 ms after slot 1 ends
            word("and", 1_310, 1_500, 0), // a recovered word never anchors
        ];
        assert_eq!(
            attribute(&words, &activity),
            vec![
                Attribution::Speaker(1),
                Attribution::Speaker(1),
                Attribution::Unassigned
            ]
        );
        // Another speaker starting at the boundary vetoes the inheritance.
        let competing = [act(1, 0, 1_000), act(2, 1_200, 1_250)];
        assert_eq!(
            attribute(&words[..2], &competing)[1],
            Attribution::Unassigned
        );
        // Never across a run boundary or a large pause.
        let other_run = [word("so", 600, 1_000, 0), word("yes", 1_040, 1_300, 1)];
        assert_eq!(attribute(&other_run, &activity)[1], Attribution::Unassigned);
        let pause = [word("so", 600, 1_000, 0), word("yes", 1_200, 1_300, 0)];
        assert_eq!(attribute(&pause, &activity)[1], Attribution::Unassigned);
    }

    #[test]
    fn brief_voices_beside_a_real_speaker_are_left_unassigned() {
        let mut words: Vec<MeetingWord> = (0..12)
            .map(|i| word("hi", i * 300, i * 300 + 200, 0))
            .collect();
        words.push(word("cough", 4_000, 4_200, 0));
        let mut attribution: Vec<Attribution> = (0..12).map(|_| Attribution::Speaker(1)).collect();
        attribution.push(Attribution::Speaker(2));
        fold_brief_speakers(&words, &mut attribution);
        assert_eq!(attribution[12], Attribution::Unassigned);
        assert_eq!(attribution[0], Attribution::Speaker(1));
        // When every slot is brief, all keep their labels.
        let mut few = vec![Attribution::Speaker(1), Attribution::Speaker(2)];
        fold_brief_speakers(&words[..2], &mut few);
        assert_eq!(few, vec![Attribution::Speaker(1), Attribution::Speaker(2)]);
    }

    fn revision(number: u32) -> TranscriptRevision {
        let tracks = vec![TrackTranscript {
            track: 0,
            status: TrackStatus::Complete,
            text: "hi".into(),
            words: vec![Word {
                text: "hi".into(),
                start_ms: 0,
                end_ms: 100,
                speaker: Attribution::Speaker(2),
            }],
            activity: vec![act(2, 0, 100)],
        }];
        TranscriptRevision {
            version: TRANSCRIPT_VERSION,
            revision: number,
            session_id: "s".into(),
            attempt_id: "a".into(),
            created_unix_ms: 1,
            diarizer: None,
            asr: None,
            speakers: speakers(&tracks),
            tracks,
        }
    }

    #[test]
    fn revisions_are_immutable_and_the_latest_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(latest(dir.path()).unwrap(), None);
        assert_eq!(next_revision(dir.path()).unwrap(), 1);
        publish(dir.path(), &revision(1)).unwrap();
        assert!(
            publish(dir.path(), &revision(1)).is_err(),
            "never overwritten"
        );
        assert_eq!(next_revision(dir.path()).unwrap(), 2);
        publish(dir.path(), &revision(2)).unwrap();
        let read = latest(dir.path()).unwrap().unwrap();
        assert_eq!(read, revision(2));
        assert_eq!(read.speakers[0].id, "t0-s2");
        assert_eq!(read.speakers[0].label, "Speaker 1");
    }
}
