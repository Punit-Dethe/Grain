//! Turning any model's output into chunk-relative word timings for the
//! assembler's time-based dedup — the "we own the timeline" mechanism.
//!
//! Two sources, preferred first:
//! 1. [`segments_to_words`] — the model's real segment timestamps, split into
//!    words (covers Parakeet word-granularity, Whisper/SenseVoice phrases).
//! 2. [`synthesize_words`] — for models that emit NO timestamps, distribute the
//!    words evenly across the chunk duration WE control. Dedup then happens by
//!    position on our timeline rather than by the model's (possibly
//!    inconsistent) overlap text — so every model deduplicates cleanly.

use rolling_window::WordTiming;
use transcribe_rs::TranscriptionResult;

/// transcribe-rs requires 16 kHz mono input across every engine.
pub const SAMPLE_RATE: usize = 16_000;

/// Map a model's segment timings to chunk-relative word timings by spreading
/// each segment's text across its `[start, end)` span. Handles word-granularity
/// (Parakeet: one word per segment) and phrase-granularity (Whisper, SenseVoice)
/// uniformly. Returns `None` when there are no usable timed segments.
pub fn segments_to_words(result: &TranscriptionResult) -> Option<Vec<WordTiming>> {
    let segments = result.segments.as_ref()?;
    let mut words = Vec::new();
    for seg in segments {
        let toks: Vec<&str> = seg.text.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        let start = seg.start as f64;
        let span = (seg.end - seg.start).max(0.0) as f64;
        let per = span / toks.len() as f64;
        for (i, w) in toks.iter().enumerate() {
            words.push(WordTiming {
                word: w.to_string(),
                start: start + i as f64 * per,
                end: start + (i + 1) as f64 * per,
            });
        }
    }
    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

/// Synthesize evenly spaced, chunk-relative word timings for a timestamp-less
/// model. The chunk spans a known duration (we own the frame cursor), so
/// distributing the words across it gives each word a position good enough for
/// the time-based overlap dedup — far more robust than text matching when the
/// model transcribes the overlap region differently across chunks.
pub fn synthesize_words(text: &str, chunk_dur_sec: f64) -> Option<Vec<WordTiming>> {
    let toks: Vec<&str> = text.split_whitespace().collect();
    if toks.is_empty() {
        return None;
    }
    let per = chunk_dur_sec / toks.len() as f64;
    let words = toks
        .iter()
        .enumerate()
        .map(|(i, w)| WordTiming {
            word: w.to_string(),
            start: i as f64 * per,
            end: (i + 1) as f64 * per,
        })
        .collect();
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcribe_rs::TranscriptionSegment;

    #[test]
    fn synthesize_spreads_words_across_chunk() {
        let w = synthesize_words("one two three four", 8.0).unwrap();
        assert_eq!(w.len(), 4);
        assert_eq!(w[0].start, 0.0);
        assert_eq!(w[3].end, 8.0);
        // midpoints strictly increasing and within [0, dur]
        assert!(w[0].start < w[1].start && w[1].start < w[2].start);
    }

    #[test]
    fn synthesize_empty_is_none() {
        assert!(synthesize_words("   ", 5.0).is_none());
    }

    #[test]
    fn segments_split_into_words() {
        let result = TranscriptionResult {
            text: "hello there world".into(),
            segments: Some(vec![
                TranscriptionSegment { start: 0.0, end: 2.0, text: "hello there".into() },
                TranscriptionSegment { start: 2.0, end: 3.0, text: "world".into() },
            ]),
        };
        let w = segments_to_words(&result).unwrap();
        assert_eq!(w.iter().map(|x| x.word.as_str()).collect::<Vec<_>>(), ["hello", "there", "world"]);
        // first segment's two words split its [0,2) span
        assert_eq!(w[0].start, 0.0);
        assert_eq!(w[1].start, 1.0);
        assert_eq!(w[2].start, 2.0);
    }

    #[test]
    fn no_segments_is_none() {
        let result = TranscriptionResult { text: "x".into(), segments: None };
        assert!(segments_to_words(&result).is_none());
    }
}
