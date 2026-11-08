//! Speaker-attributed transcript evidence and its published revisions.
//!
//! Everything here is pure except revision storage. Word attribution follows
//! Glimpse (`library/speakers.rs`: `voice_at`, `assign_voices`,
//! `fold_minor_voices`), which runs the same Nemotron 3 diarizer on the same
//! offline preset; Grain's native output is identical to Glimpse's for the
//! same audio, so the rules decide the visible difference.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::tdt_flow::WindowEvidence;

pub const TRANSCRIPT_VERSION: u32 = 1;
const TRANSCRIPTS: &str = "transcripts";
const PREVIEW: &str = "preview.json";
const PREVIEW_CHARS: usize = 160;
/// Glimpse `fold_minor_voices`: a slot under 1/20 (5%) of a track's words.
const MINOR_SPEAKER_DIVISOR: usize = 20;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Activity {
    pub slot: i32,
    pub start_ns: u64,
    pub end_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum Attribution {
    Speaker(i32),
    /// The diarizer found no speech on this track.
    Unassigned,
    /// Speaker detection did not run or failed for this track.
    Unavailable,
}

/// A word placed on the meeting clock, with its recorded run (smoothing
/// never crosses runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingWord {
    pub text: String,
    pub start_ns: u64,
    pub end_ns: u64,
    pub piece: usize,
}

/// Glimpse's word rule (`speakers::voice_at`): the slot with the most total
/// overlap; with no overlap (a pause, or a point-timed word), the nearest
/// turn. A track with diarization never leaves a word without a speaker.
pub fn assign(start_ns: u64, end_ns: u64, activity: &[Activity]) -> Attribution {
    // Slots in first-appearance order; the last of equal totals wins, as in Glimpse.
    let mut totals: Vec<(i32, u64)> = Vec::new();
    for a in activity {
        let overlap = a
            .end_ns
            .min(end_ns)
            .saturating_sub(a.start_ns.max(start_ns));
        if overlap == 0 {
            continue;
        }
        match totals.iter_mut().find(|(slot, _)| *slot == a.slot) {
            Some((_, total)) => *total += overlap,
            None => totals.push((a.slot, overlap)),
        }
    }
    if let Some(&(slot, _)) = totals.iter().max_by_key(|(_, total)| *total) {
        return Attribution::Speaker(slot);
    }
    activity
        .iter()
        .min_by_key(|a| {
            if a.end_ns <= start_ns {
                start_ns - a.end_ns
            } else {
                a.start_ns.saturating_sub(end_ns)
            }
        })
        .map_or(Attribution::Unassigned, |a| Attribution::Speaker(a.slot))
}

/// Attribute every word of one track, then smooth boundary jitter as Glimpse
/// does: a lone word between two words of the same speaker takes that
/// speaker. Smoothing stays within one recorded run (Glimpse: one segment).
pub fn attribute(words: &[MeetingWord], activity: &[Activity]) -> Vec<Attribution> {
    let mut result: Vec<Attribution> = words
        .iter()
        .map(|w| assign(w.start_ns, w.end_ns, activity))
        .collect();
    for i in 1..words.len().saturating_sub(1) {
        let same_run = words[i - 1].piece == words[i].piece && words[i].piece == words[i + 1].piece;
        if same_run && result[i - 1] == result[i + 1] {
            result[i] = result[i - 1].clone();
        }
    }
    result
}

/// Glimpse's `fold_minor_voices`, for a track that belongs to a known party
/// (the microphone or system audio of a two-track meeting): a slot holding
/// under 5% of the track's attributed words is leftover bleed or noise, so its
/// words go to the track's main slot.
pub fn fold_minor_speakers(attribution: &mut [Attribution]) {
    let mut counts: Vec<(i32, usize)> = Vec::new();
    for a in attribution.iter() {
        if let Attribution::Speaker(slot) = a {
            match counts.iter_mut().find(|(known, _)| known == slot) {
                Some((_, count)) => *count += 1,
                None => counts.push((*slot, 1)),
            }
        }
    }
    let total: usize = counts.iter().map(|(_, count)| count).sum();
    let Some(&(main, _)) = counts.iter().max_by_key(|(_, count)| *count) else {
        return;
    };
    let minor: Vec<i32> = counts
        .iter()
        .filter(|(_, count)| count * MINOR_SPEAKER_DIVISOR < total)
        .map(|(slot, _)| *slot)
        .collect();
    for a in attribution.iter_mut() {
        if matches!(a, Attribution::Speaker(slot) if minor.contains(slot)) {
            *a = Attribution::Speaker(main);
        }
    }
}

// --- Published revisions ---------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Word {
    pub text: String,
    /// Meeting clock, milliseconds from the session start.
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Attribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Speaker {
    /// Stable within a revision: `t{track}-s{slot}`.
    pub id: String,
    pub track: u32,
    pub slot: i32,
    /// Anonymous label, numbered across the meeting.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ModelRef {
    pub file: String,
    pub arch: String,
    pub variant: String,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

/// The newest revision number without reading any transcript.
pub fn latest_revision(session: &Path) -> Result<Option<u32>, String> {
    Ok(revisions(session)?.last().copied())
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
    })?;
    // The list reads this small sidecar instead of the transcript; it is
    // rebuilt from the transcript if this write fails.
    let _ = write_preview(session, &preview_of(revision));
    Ok(())
}

/// What the recordings list shows of a transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub revision: u32,
    pub speakers: u32,
    /// The opening words, at most about `PREVIEW_CHARS` characters.
    pub text: String,
}

pub fn preview_of(revision: &TranscriptRevision) -> Preview {
    let mut words: Vec<&Word> = revision.tracks.iter().flat_map(|t| &t.words).collect();
    words.sort_by_key(|word| word.start_ms);
    let mut text = String::new();
    for word in words {
        if text.chars().count() + word.text.chars().count() + 1 > PREVIEW_CHARS {
            text.push('…');
            break;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&word.text);
    }
    Preview {
        revision: revision.revision,
        speakers: u32::try_from(revision.speakers.len()).unwrap_or(u32::MAX),
        text,
    }
}

fn write_preview(session: &Path, preview: &Preview) -> std::io::Result<()> {
    let tmp = session.join(format!("{PREVIEW}.tmp"));
    fs::write(&tmp, serde_json::to_vec(preview)?)?;
    fs::rename(&tmp, session.join(PREVIEW))
}

/// The newest revision's preview: the sidecar when it is current, otherwise
/// rebuilt once from the transcript and saved.
pub fn preview(session: &Path) -> Result<Option<Preview>, String> {
    let Some(newest) = latest_revision(session)? else {
        return Ok(None);
    };
    let saved = fs::read(session.join(PREVIEW))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Preview>(&bytes).ok())
        .filter(|preview| preview.revision == newest);
    if saved.is_some() {
        return Ok(saved);
    }
    let Some(revision) = latest(session)? else {
        return Ok(None);
    };
    let built = preview_of(&revision);
    let _ = write_preview(session, &built);
    Ok(Some(built))
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
    fn a_word_takes_the_slot_with_the_most_overlap_or_the_nearest_turn() {
        let activity = [act(1, 0, 1_000), act(2, 900, 2_000), act(3, 1_500, 1_510)];
        assert_eq!(
            assign(100 * MS, 300 * MS, &activity),
            Attribution::Speaker(1)
        );
        // 100 ms each way across the change: equal totals, the later slot wins
        // (Glimpse's `max_by_key` keeps the last maximum).
        assert_eq!(
            assign(850 * MS, 1_050 * MS, &activity),
            Attribution::Speaker(2)
        );
        // Overlap is summed per slot: 3 x 300 ms beats 100 ms.
        assert_eq!(
            assign(
                0,
                1_000 * MS,
                &[act(1, 0, 300), act(1, 300, 600), act(2, 900, 1_000)]
            ),
            Attribution::Speaker(1)
        );
        // In a pause, the nearest turn; a point-timed word inside a turn is 0 away.
        assert_eq!(
            assign(2_300 * MS, 2_400 * MS, &activity),
            Attribution::Speaker(2)
        );
        assert_eq!(
            assign(500 * MS, 500 * MS, &activity),
            Attribution::Speaker(1)
        );
        assert_eq!(assign(0, 100 * MS, &[]), Attribution::Unassigned);
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
    fn a_lone_word_between_the_same_speaker_is_smoothed_within_a_run() {
        let activity = [act(1, 0, 1_000), act(2, 1_000, 1_200), act(1, 1_200, 3_000)];
        let words = [
            word("so", 600, 1_000, 0),
            word("yes", 1_000, 1_200, 0),
            word("and", 1_200, 1_500, 0),
        ];
        assert_eq!(
            attribute(&words, &activity),
            vec![Attribution::Speaker(1); 3]
        );
        // Two words of the other slot are a real turn, not jitter.
        let turn = [
            word("so", 600, 1_000, 0),
            word("no", 1_000, 1_100, 0),
            word("way", 1_100, 1_200, 0),
            word("and", 1_200, 1_500, 0),
        ];
        assert_eq!(
            attribute(&turn, &activity)[1..3],
            [Attribution::Speaker(2), Attribution::Speaker(2)]
        );
        // Never across a recorded run boundary.
        let runs = [
            word("so", 600, 1_000, 0),
            word("yes", 1_000, 1_200, 1),
            word("and", 1_200, 1_500, 1),
        ];
        assert_eq!(attribute(&runs, &activity)[1], Attribution::Speaker(2));
    }

    #[test]
    fn minor_speakers_fold_into_the_main_one() {
        let mut attribution: Vec<Attribution> = (0..20).map(|_| Attribution::Speaker(1)).collect();
        attribution.push(Attribution::Speaker(2)); // 1 of 21 words: under 5%
        attribution.extend((0..4).map(|_| Attribution::Speaker(3))); // 4 of 25: kept
        fold_minor_speakers(&mut attribution);
        assert_eq!(attribution[20], Attribution::Speaker(1));
        assert_eq!(attribution[21], Attribution::Speaker(3));
        let mut none = vec![Attribution::Unavailable];
        fold_minor_speakers(&mut none);
        assert_eq!(none, vec![Attribution::Unavailable]);
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

    #[test]
    fn the_list_preview_follows_the_newest_revision_and_rebuilds_itself() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(preview(dir.path()).unwrap(), None);
        publish(dir.path(), &revision(1)).unwrap();
        let first = preview(dir.path()).unwrap().unwrap();
        assert_eq!((first.revision, first.speakers), (1, 1));
        assert_eq!(first.text, "hi");
        // A missing or stale sidecar is rebuilt from the transcript.
        std::fs::remove_file(dir.path().join(PREVIEW)).unwrap();
        publish(dir.path(), &revision(2)).unwrap();
        std::fs::write(dir.path().join(PREVIEW), b"{").unwrap();
        assert_eq!(preview(dir.path()).unwrap().unwrap().revision, 2);
        assert!(dir.path().join(PREVIEW).is_file());
        // Long transcripts are cut at a word with an ellipsis.
        let mut long = revision(3);
        long.tracks[0].words = (0..100)
            .map(|i| Word {
                text: format!("word{i}"),
                start_ms: i,
                end_ms: i + 1,
                speaker: Attribution::Speaker(2),
            })
            .collect();
        let cut = preview_of(&long).text;
        assert!(cut.ends_with('…') && cut.chars().count() <= PREVIEW_CHARS + 1);
        assert!(cut.starts_with("word0 word1 "));
    }
}
