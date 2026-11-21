//! A transcript as readable lines: "[mm:ss] Speaker N: text" on the meeting
//! clock, merging every track. The summary prompt and the MCP both read this.

use crate::transcript::{Attribution, TranscriptRevision};

/// A pause this long starts a new line even for the same speaker.
pub const LINE_GAP_MS: u64 = 1_500;
const UNKNOWN_SPEAKER: &str = "Unknown speaker";

/// One speaker turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Anonymous label ("Speaker 1") or "Unknown speaker".
    pub speaker: String,
    pub text: String,
}

impl Line {
    /// "[mm:ss] Speaker N: text".
    pub fn render(&self) -> String {
        format!("[{}] {}: {}", clock(self.start_ms), self.speaker, self.text)
    }
}

/// "mm:ss", or "h:mm:ss" from an hour on.
pub fn clock(ms: u64) -> String {
    let total = ms / 1_000;
    let (h, m, s) = (total / 3_600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn speaker_label<'a>(revision: &'a TranscriptRevision, track: u32, who: &Attribution) -> &'a str {
    match who {
        Attribution::Speaker(slot) => revision
            .speakers
            .iter()
            .find(|s| s.track == track && s.slot == *slot)
            .map_or(UNKNOWN_SPEAKER, |s| s.label.as_str()),
        _ => UNKNOWN_SPEAKER,
    }
}

/// Speaker turns in time order: a new line on a speaker change or a pause
/// longer than [`LINE_GAP_MS`].
pub fn lines(revision: &TranscriptRevision) -> Vec<Line> {
    let mut words: Vec<(u64, u64, &str, &str)> = Vec::new();
    for track in &revision.tracks {
        for word in &track.words {
            let label = speaker_label(revision, track.track, &word.speaker);
            words.push((word.start_ms, word.end_ms, word.text.as_str(), label));
        }
    }
    words.sort_by_key(|word| word.0);

    let mut out: Vec<Line> = Vec::new();
    for (start, end, text, label) in words {
        match out.last_mut() {
            Some(line)
                if line.speaker == label && start.saturating_sub(line.end_ms) <= LINE_GAP_MS =>
            {
                line.text.push(' ');
                line.text.push_str(text);
                line.end_ms = end;
            }
            _ => out.push(Line {
                start_ms: start,
                end_ms: end,
                speaker: label.to_string(),
                text: text.to_string(),
            }),
        }
    }
    out
}

/// The whole transcript as newline-separated rendered lines.
pub fn transcript_text(revision: &TranscriptRevision) -> String {
    lines(revision)
        .iter()
        .map(Line::render)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Speaker, TrackStatus, TrackTranscript, Word};

    fn word(text: &str, start_ms: u64, end_ms: u64, slot: i32) -> Word {
        Word {
            text: text.into(),
            start_ms,
            end_ms,
            speaker: Attribution::Speaker(slot),
        }
    }

    fn revision(words: Vec<Word>) -> TranscriptRevision {
        TranscriptRevision {
            version: 1,
            revision: 3,
            session_id: "s".into(),
            attempt_id: "a".into(),
            created_unix_ms: 0,
            diarizer: None,
            asr: None,
            speakers: vec![
                Speaker {
                    id: "t0-s0".into(),
                    track: 0,
                    slot: 0,
                    label: "Speaker 1".into(),
                },
                Speaker {
                    id: "t0-s1".into(),
                    track: 0,
                    slot: 1,
                    label: "Speaker 2".into(),
                },
            ],
            tracks: vec![TrackTranscript {
                track: 0,
                status: TrackStatus::Complete,
                text: String::new(),
                words,
                activity: Vec::new(),
            }],
        }
    }

    #[test]
    fn transcript_lines_follow_speaker_changes_and_long_pauses() {
        let rev = revision(vec![
            word("Hello", 0, 400, 0),
            word("there.", 450, 900, 0),
            word("Hi!", 1_200, 1_500, 1),
            word("Later", 70_000, 70_400, 1),
        ]);
        assert_eq!(
            transcript_text(&rev),
            "[00:00] Speaker 1: Hello there.\n[00:01] Speaker 2: Hi!\n[01:10] Speaker 2: Later"
        );
        let turns = lines(&rev);
        assert_eq!(turns[0].end_ms, 900);
        assert_eq!(turns[2].start_ms, 70_000);
    }

    #[test]
    fn unattributed_words_are_unknown_speaker() {
        let mut rev = revision(vec![word("Hm", 0, 100, 7)]);
        rev.tracks[0].words.push(Word {
            text: "ok".into(),
            start_ms: 5_000,
            end_ms: 5_100,
            speaker: Attribution::Unavailable,
        });
        assert_eq!(
            transcript_text(&rev),
            "[00:00] Unknown speaker: Hm\n[00:05] Unknown speaker: ok"
        );
    }

    #[test]
    fn clock_switches_to_hours() {
        assert_eq!(clock(59_000), "00:59");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}
