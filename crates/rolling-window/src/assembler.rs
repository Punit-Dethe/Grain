//! Transcript assembly — deduplicates overlapping chunk transcripts.
//!
//! Ported from `open_voice_router/services/transcript_merger.py`.
//!
//! Two strategies, in order of preference:
//!
//! 1. [`TimelineAssembler`] (time-based, deterministic). Each chunk carries its
//!    absolute position on the session timeline (from our frame counter) and
//!    word-level timings from the model. A word is accepted iff its absolute
//!    midpoint falls in the chunk's fresh (not-previously-covered) region. Dedup
//!    becomes arithmetic — a repeated phrase can never be over-stripped and a
//!    differently-transcribed overlap can never eat new words.
//!
//! 2. [`merge_transcript`] (text-based, fallback). For models that provide no
//!    word timestamps: longest suffix/prefix word match over a 30-word window.

/// A single transcribed word with chunk-relative timing in seconds.
///
/// Port of `models.WordTiming`.
#[derive(Clone, Debug, PartialEq)]
pub struct WordTiming {
    pub word: String,
    pub start: f64,
    pub end: f64,
}

impl WordTiming {
    pub fn new(word: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            word: word.into(),
            start,
            end,
        }
    }

    pub fn midpoint(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// Cover ~10s of potential overlap at 3 words/sec.
const OVERLAP_SEARCH_WORDS: usize = 30;

/// Append `new_segment` to `existing`, deduplicating overlap words.
///
/// Inputs are expected to use the rolling-window canonical text contract. The
/// comparison key is still defensive so differently formatted model output does
/// not create a duplicate at the boundary.
pub fn merge_transcript(existing: &str, new_segment: &str) -> String {
    let new_segment = new_segment.trim();
    if new_segment.is_empty() {
        return existing.trim().to_string();
    }

    let existing = existing.trim();
    if existing.is_empty() {
        return new_segment.to_string();
    }

    let new_words: Vec<&str> = new_segment.split_whitespace().collect();
    if new_words.is_empty() {
        return existing.to_string();
    }

    // Only the last OVERLAP_SEARCH_WORDS words of `existing` can overlap.
    let existing_words: Vec<&str> = existing.split_whitespace().collect();
    let tail_start = existing_words.len().saturating_sub(OVERLAP_SEARCH_WORDS);
    let tail_words = &existing_words[tail_start..];

    let tail_norm: Vec<String> = tail_words.iter().map(|w| comparison_key(w)).collect();
    let new_norm: Vec<String> = new_words.iter().map(|w| comparison_key(w)).collect();

    let search_limit = tail_norm.len().min(new_norm.len());

    // Longest prefix of new_norm matching a suffix of tail_norm.
    let mut best_overlap = 0usize;
    for length in (1..=search_limit).rev() {
        if tail_norm[tail_norm.len() - length..] == new_norm[..length] {
            best_overlap = length;
            break;
        }
    }

    let remainder = &new_words[best_overlap..];
    if remainder.is_empty() {
        return existing.to_string();
    }

    format!("{} {}", existing, remainder.join(" "))
}

// ---------------------------------------------------------------------------
// Time-based assembly (preferred when word timings are available)
// ---------------------------------------------------------------------------

/// Model word timestamps jitter by ~one acoustic frame (~80 ms) plus decoding
/// slack; ±250 ms tolerance absorbs that without re-admitting whole words from
/// the 2 s overlap region.
const BOUNDARY_TOLERANCE_SEC: f64 = 0.25;

/// How many committed tail words to compare for seam dedup of boundary words
/// that the tolerance window let through twice.
const SEAM_SEARCH_WORDS: usize = 3;

use crate::merge::seam_overlap_len;
use crate::normalize::{canonicalize_text, canonicalize_token, comparison_key};
use crate::CutKind;

/// Builds the session transcript from time-tagged chunk results.
///
/// The session timeline is ground truth from the audio layer's frame cursor:
/// chunk N's fresh region is `[fresh_start, end)` and is covered by NO other
/// chunk, while `[start, fresh_start)` is overlap context that chunk N-1 already
/// transcribed. Words are accepted iff their absolute midpoint lies in the fresh
/// region (with a small tolerance for model timing jitter), so overlap dedup
/// never depends on the model transcribing the same audio the same way twice.
///
/// Falls back to [`merge_transcript`] for chunks without word timings.
#[derive(Default)]
pub struct TimelineAssembler {
    text: String,
    /// When `Some(window_sec)`, run an extra fuzzy/number-aware seam dedup over
    /// accepted words within `window_sec` of the fresh boundary — catches residue
    /// that synthesized (approximate) timings and exact text matching miss. `None`
    /// (default) keeps the exact behavior the ported Python tests pin.
    fuzzy_seam_window: Option<f64>,
}

impl TimelineAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable the fuzzy seam dedup over a `window_sec`-wide zone after the fresh
    /// boundary (typically the chunk overlap length). Used for synthesized-timing
    /// models; harmless for exact-timing models.
    pub fn with_fuzzy_seam(mut self, window_sec: f64) -> Self {
        self.fuzzy_seam_window = Some(window_sec);
        self
    }

    /// The assembled transcript so far.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn reset(&mut self) {
        self.text.clear();
    }

    /// Merge one chunk's transcription and return the updated transcript.
    ///
    /// * `chunk_start_sec` — absolute session time where the chunk AUDIO begins
    ///   (word timings in `words` are relative to this point).
    /// * `fresh_start_sec` — absolute session time where the chunk's fresh
    ///   (not-previously-covered) audio begins.
    /// * `text` — the chunk's plain transcript (used for the fallback path).
    /// * `words` — word timings relative to the chunk start, when available.
    /// * `boundary` — why this chunk's END was cut ([`AudioChunk::boundary`]);
    ///   retained as metadata for callers and future boundary policies.
    pub fn add_chunk(
        &mut self,
        chunk_start_sec: f64,
        fresh_start_sec: f64,
        text: &str,
        words: Option<&[WordTiming]>,
        _boundary: CutKind,
    ) -> &str {
        let canonical_words: Vec<WordTiming> = words
            .unwrap_or_default()
            .iter()
            .flat_map(|word| {
                canonicalize_token(&word.word)
                    .into_iter()
                    .map(|surface| WordTiming::new(surface, word.start, word.end))
            })
            .collect();
        let words = match canonical_words.as_slice() {
            w if !w.is_empty() => w,
            _ => {
                let text = canonicalize_text(text);
                if !text.is_empty() {
                    self.text = merge_transcript(&self.text, &text);
                }
                return &self.text;
            }
        };

        // A chunk with no audio PAST the fresh boundary carries only overlap
        // context the previous chunk already covered — e.g. `SessionCursor::stop`
        // flushing `[cursor - overlap, end)` when nothing is unsent
        // (`fresh_start == end`). Every word then sits before `fresh_start`;
        // admitting any (the boundary tolerance reaches `fresh_start - 0.25`)
        // would re-commit, and thus DUPLICATE, the already-assembled tail. The
        // timeline is ground truth, so the only correct action is to add nothing.
        let has_fresh_audio = words
            .iter()
            .any(|w| (chunk_start_sec + w.midpoint()) >= fresh_start_sec && !w.word.is_empty());
        if !has_fresh_audio {
            return &self.text;
        }

        let cutoff = fresh_start_sec - BOUNDARY_TOLERANCE_SEC;
        // Keep the fresh region plus the small timing-jitter allowance. The
        // larger overlap re-hearing is discarded by the timeline boundary.
        let mut accepted: Vec<&WordTiming> = Vec::new();
        for w in words {
            if w.word.is_empty() {
                continue;
            }
            if (chunk_start_sec + w.midpoint()) >= cutoff {
                accepted.push(w);
            }
        }

        // Seam cleanup: the tolerance window can re-admit a word the previous
        // chunk already committed right at the boundary. Drop leading accepted
        // words that exactly repeat the committed tail AND sit inside the
        // tolerance window — never anything beyond it.
        let existing_tail: Vec<&str> = {
            let all: Vec<&str> = self.text.split_whitespace().collect();
            let start = all.len().saturating_sub(SEAM_SEARCH_WORDS);
            all[start..].to_vec()
        };
        let max_n = SEAM_SEARCH_WORDS
            .min(accepted.len())
            .min(existing_tail.len());
        for n in (1..=max_n).rev() {
            let head = &accepted[..n];
            // Extends past the jitter window — real new speech, never strip.
            let extends_past = head.iter().any(|w| {
                (chunk_start_sec + w.midpoint()) >= fresh_start_sec + BOUNDARY_TOLERANCE_SEC
            });
            if extends_past {
                continue;
            }
            let head_norm: Vec<String> = head.iter().map(|w| comparison_key(&w.word)).collect();
            let tail_norm: Vec<String> = existing_tail[existing_tail.len() - n..]
                .iter()
                .map(|w| comparison_key(w))
                .collect();
            if head_norm == tail_norm {
                accepted.drain(..n);
                break;
            }
        }

        // Optional fuzzy/number-aware seam dedup (research §5): within a
        // position-bounded zone after the fresh boundary, drop leading accepted
        // words that fuzzily repeat the committed tail (handles `3`/`three`,
        // `O3`/`03`, token-count slips from approximate synthesized timing).
        if let Some(window) = self.fuzzy_seam_window {
            if !accepted.is_empty() && !self.text.is_empty() {
                let committed: Vec<&str> = self.text.split_whitespace().collect();
                let tail_start = committed.len().saturating_sub(8);
                let tail = &committed[tail_start..];
                let seam_n = accepted
                    .iter()
                    .take_while(|w| (chunk_start_sec + w.midpoint()) < fresh_start_sec + window)
                    .count();
                if seam_n > 0 {
                    let head: Vec<&str> =
                        accepted[..seam_n].iter().map(|w| w.word.as_str()).collect();
                    let drop = seam_overlap_len(tail, &head);
                    if drop > 0 {
                        accepted.drain(..drop);
                    }
                }
            }
        }

        if !accepted.is_empty() {
            let addition = accepted
                .iter()
                .map(|word| word.word.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            if self.text.is_empty() {
                self.text = addition;
            } else {
                self.text.push(' ');
                self.text.push_str(&addition);
            }
        }
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- merge_transcript (text-based fallback) ----------------------------

    #[test]
    fn empty_existing_returns_new() {
        assert_eq!(merge_transcript("", "hello world"), "hello world");
    }

    #[test]
    fn empty_new_returns_existing() {
        assert_eq!(merge_transcript("hello world", ""), "hello world");
    }

    #[test]
    fn both_empty() {
        assert_eq!(merge_transcript("", ""), "");
    }

    #[test]
    fn no_overlap_appends_with_space() {
        assert_eq!(
            merge_transcript("hello world", "how are you"),
            "hello world how are you"
        );
    }

    #[test]
    fn simple_overlap_deduplicated() {
        assert_eq!(
            merge_transcript("hello world", "world how are you"),
            "hello world how are you"
        );
    }

    #[test]
    fn multi_word_overlap_deduplicated() {
        assert_eq!(
            merge_transcript(
                "the quick brown fox jumps",
                "brown fox jumps over the lazy dog"
            ),
            "the quick brown fox jumps over the lazy dog"
        );
    }

    #[test]
    fn overlap_is_case_insensitive() {
        assert_eq!(
            merge_transcript("Hello World", "world how are you"),
            "Hello World how are you"
        );
    }

    #[test]
    fn overlap_ignores_punctuation() {
        assert_eq!(
            merge_transcript("hello world,", "world how are you"),
            "hello world, how are you"
        );
    }

    #[test]
    fn full_duplicate_chunk_adds_nothing() {
        assert_eq!(
            merge_transcript("hello world", "hello world"),
            "hello world"
        );
    }

    #[test]
    fn whitespace_only_new_segment() {
        assert_eq!(merge_transcript("hello world", "   "), "hello world");
    }

    #[test]
    fn longest_overlap_is_preferred() {
        assert_eq!(
            merge_transcript("a brown fox", "brown fox runs"),
            "a brown fox runs"
        );
    }

    #[test]
    fn realistic_streaming_sequence() {
        let mut t = String::new();
        t = merge_transcript(&t, "I went to the store");
        t = merge_transcript(&t, "to the store to buy some milk");
        t = merge_transcript(&t, "to buy some milk and eggs");
        assert_eq!(t, "I went to the store to buy some milk and eggs");
    }

    // -- TimelineAssembler (time-based preferred path) ---------------------

    /// Build evenly spaced WordTimings from "a b c", starting at `start` sec
    /// (chunk-relative).
    fn words(spec: &str, start: f64) -> Vec<WordTiming> {
        let per_word = 0.4;
        let mut out = Vec::new();
        let mut t = start;
        for w in spec.split_whitespace() {
            out.push(WordTiming::new(w, t, t + per_word));
            t += per_word;
        }
        out
    }

    /// Most fixtures use a hard-cut boundary; canonical output does not infer
    /// formatting from the cut kind.
    const HC: CutKind = CutKind::HardCut;

    #[test]
    fn assembler_first_chunk_accepts_everything() {
        let mut a = TimelineAssembler::new();
        let text = a
            .add_chunk(
                0.0,
                0.0,
                "hello world how are you",
                Some(&words("hello world how are you", 0.1)),
                HC,
            )
            .to_string();
        assert_eq!(text, "hello world how are you");
    }

    #[test]
    fn assembler_drops_overlap_words_by_time() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "one two three",
            Some(&words("one two three", 10.5)),
            HC,
        );
        // Chunk 2: audio range [10, 22), fresh from 12. Overlap re-transcribed
        // DIFFERENTLY ("won too tree") — text merge would fail; time must not.
        let text = a
            .add_chunk(
                10.0,
                12.0,
                "won too tree four five",
                Some(&words("won too tree four five", 0.5)),
                HC,
            )
            .to_string();
        let result_words: Vec<&str> = text.split_whitespace().collect();
        assert!(!result_words.contains(&"won"));
        assert!(!result_words.contains(&"too"));
        assert!(!result_words.contains(&"tree"));
        assert_eq!(text, "one two three four five");
    }

    #[test]
    fn assembler_repeated_phrase_is_not_over_stripped() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(0.0, 0.0, "yes yes", Some(&words("yes yes", 9.0)), HC);
        // Same words spoken again, clearly inside the fresh region (abs 13s+).
        let text = a
            .add_chunk(8.0, 10.0, "yes yes", Some(&words("yes yes", 5.0)), HC)
            .to_string();
        assert_eq!(text, "yes yes yes yes");
    }

    #[test]
    fn assembler_seam_dedup_drops_boundary_double() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "we should ship it",
            Some(&words("we should ship it", 8.5)),
            HC,
        );
        // Next chunk fresh from 10.0; "it" re-appears at abs ~9.95 (inside ±0.25).
        let chunk2 = vec![
            WordTiming::new("it", 1.85, 2.05),  // abs mid 9.95
            WordTiming::new("today", 2.1, 2.5), // abs mid 10.3
        ];
        let text = a
            .add_chunk(8.0, 10.0, "it today", Some(&chunk2), HC)
            .to_string();
        assert_eq!(text, "we should ship it today");
    }

    #[test]
    fn assembler_falls_back_to_text_merge_without_words() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(0.0, 0.0, "hello world", None, HC);
        let text = a
            .add_chunk(8.0, 10.0, "world how are you", None, HC)
            .to_string();
        assert_eq!(text, "hello world how are you");
    }

    #[test]
    fn assembler_empty_chunk_is_noop() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(0.0, 0.0, "hello", Some(&words("hello", 0.1)), HC);
        let text = a.add_chunk(8.0, 10.0, "", None, HC).to_string();
        assert_eq!(text, "hello");
    }

    #[test]
    fn fuzzy_seam_drops_number_word_overlap() {
        // Prior pass committed "N3 Mover"; this pass re-hears the overlap as
        // "N three Mover" then continues "are sophisticated". Time drops "N",
        // fuzzy drops the "three Mover" repeat — leaving no duplication.
        let mut a = TimelineAssembler::new().with_fuzzy_seam(2.0);
        a.add_chunk(
            0.0,
            0.0,
            "ship the N3 Mover",
            Some(&words("ship the N3 Mover", 8.0)),
            HC,
        );
        let c2 = vec![
            WordTiming::new("N", 1.6, 1.7),     // abs mid 9.65 → dropped by time
            WordTiming::new("three", 1.9, 2.0), // abs mid 9.95
            WordTiming::new("Mover", 2.2, 2.3), // abs mid 10.25
            WordTiming::new("are", 2.6, 2.7),   // abs mid 10.65
            WordTiming::new("sophisticated", 3.0, 3.2),
        ];
        let text = a
            .add_chunk(8.0, 10.0, "N three Mover are sophisticated", Some(&c2), HC)
            .to_string();
        assert_eq!(text, "ship the n3 mover are sophisticated");
    }

    #[test]
    fn fuzzy_seam_keeps_genuine_far_repeat() {
        // With fuzzy ON, a real repeat well inside the fresh region (abs 13s+,
        // beyond the 2s seam window) must NOT be stripped.
        let mut a = TimelineAssembler::new().with_fuzzy_seam(2.0);
        a.add_chunk(0.0, 0.0, "yes yes", Some(&words("yes yes", 9.0)), HC);
        let text = a
            .add_chunk(8.0, 10.0, "yes yes", Some(&words("yes yes", 5.0)), HC)
            .to_string();
        assert_eq!(text, "yes yes yes yes");
    }

    #[test]
    fn zero_fresh_chunk_adds_nothing() {
        // Reproduces `SessionCursor::stop` flushing when nothing is unsent: the
        // final chunk is pure overlap (fresh_start == end), so every word the
        // model returns is a re-transcription of already-committed audio. It must
        // NOT be appended — otherwise the transcript's tail is duplicated.
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "ship it today",
            Some(&words("ship it today", 0.1)),
            HC,
        );
        let before = a.text().to_string();
        // Overlap-only chunk: audio [8, 10), fresh_start == end == 10. The model
        // re-hears the committed tail; all word midpoints fall before 10.
        let overlap = vec![
            WordTiming::new("it", 1.5, 1.7),    // abs mid 9.6
            WordTiming::new("today", 1.8, 2.0), // abs mid 9.9
        ];
        let text = a
            .add_chunk(8.0, 10.0, "it today", Some(&overlap), CutKind::Stop)
            .to_string();
        assert_eq!(
            text, before,
            "a zero-fresh chunk must not duplicate the tail"
        );
    }

    #[test]
    fn assembler_reset_clears_state() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(0.0, 0.0, "hello", Some(&words("hello", 0.1)), HC);
        a.reset();
        assert_eq!(a.text(), "");
    }

    // -- canonical formatting contract (integration through add_chunk) -----

    #[test]
    fn hard_cut_output_is_canonical_plain_text() {
        // Formatting from either chunk is removed before assembly.
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "I went to the store.",
            Some(&words("I went to the store.", 8.2)),
            CutKind::HardCut,
        );
        let c2 = vec![
            WordTiming::new("to", 1.2, 1.4),    // abs mid 9.3 → rehearing
            WordTiming::new("the", 1.4, 1.6),   // abs mid 9.5 → rehearing
            WordTiming::new("store", 1.6, 1.8), // abs mid 9.7 → rehearing
            WordTiming::new("and", 2.0, 2.2),   // abs mid 10.1 → fresh
            WordTiming::new("bought", 2.3, 2.5),
            WordTiming::new("milk", 2.6, 2.8),
        ];
        let text = a
            .add_chunk(8.0, 10.0, "to the store and bought milk", Some(&c2), HC)
            .to_string();
        assert_eq!(text, "i went to the store and bought milk");
    }

    #[test]
    fn overlap_witness_punctuation_is_not_preserved() {
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "we packed the boxes.",
            Some(&words("we packed the boxes.", 8.2)),
            CutKind::HardCut,
        );
        let c2 = vec![
            WordTiming::new("the", 1.4, 1.6),    // abs mid 9.5 → rehearing
            WordTiming::new("boxes,", 1.6, 1.8), // abs mid 9.7 → rehearing, has comma
            WordTiming::new("then", 2.0, 2.2),   // fresh
            WordTiming::new("we", 2.3, 2.5),
            WordTiming::new("left", 2.6, 2.8),
        ];
        let text = a
            .add_chunk(8.0, 10.0, "the boxes, then we left", Some(&c2), HC)
            .to_string();
        assert_eq!(text, "we packed the boxes then we left");
    }

    #[test]
    fn silence_cut_does_not_infer_formatting() {
        // Even a real pause does not reintroduce punctuation or capitalization.
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "we should ship it.",
            Some(&words("we should ship it.", 8.2)),
            CutKind::Silence,
        );
        let c2 = vec![
            WordTiming::new("ship", 1.4, 1.6), // abs mid 9.5 → rehearing, bare
            WordTiming::new("it", 1.6, 1.8),   // abs mid 9.7 → rehearing, bare
            WordTiming::new("also", 2.0, 2.2), // fresh, lowercase from the model
            WordTiming::new("we", 2.3, 2.5),
            WordTiming::new("need", 2.6, 2.8),
        ];
        let text = a
            .add_chunk(8.0, 10.0, "ship it also we need", Some(&c2), HC)
            .to_string();
        assert_eq!(text, "we should ship it also we need");
    }

    #[test]
    fn model_capital_after_pause_is_normalized() {
        // Model casing is normalized independently of the boundary reason.
        let mut a = TimelineAssembler::new();
        a.add_chunk(
            0.0,
            0.0,
            "we should ship it",
            Some(&words("we should ship it", 8.2)),
            CutKind::Silence,
        );
        let c2 = vec![
            WordTiming::new("ship", 1.4, 1.6), // rehearing, bare
            WordTiming::new("it", 1.6, 1.8),   // rehearing, bare
            WordTiming::new("Also", 2.0, 2.2), // fresh, capitalized by the model
            WordTiming::new("we", 2.3, 2.5),
            WordTiming::new("need", 2.6, 2.8),
        ];
        let text = a
            .add_chunk(8.0, 10.0, "ship it Also we need", Some(&c2), HC)
            .to_string();
        assert_eq!(text, "we should ship it also we need");
    }

    #[test]
    fn punctuation_only_tokens_are_dropped() {
        // Some models emit punctuation as standalone word rows — they must not
        // produce "milk ." with a stray space.
        let mut a = TimelineAssembler::new();
        let c1 = vec![
            WordTiming::new("bought", 0.1, 0.3),
            WordTiming::new("milk", 0.4, 0.6),
            WordTiming::new(".", 0.6, 0.7),
        ];
        let text = a
            .add_chunk(0.0, 0.0, "bought milk .", Some(&c1), HC)
            .to_string();
        assert_eq!(text, "bought milk");
    }
}
