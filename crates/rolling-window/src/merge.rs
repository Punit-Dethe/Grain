//! Fuzzy, number-aware overlap detection for the seam between chunks.
//!
//! Background: the time-based assembler dedups by POSITION (we own the timeline).
//! With real word timestamps (Parakeet) that is exact. Timestamp-less models use
//! this bounded lexical path directly, and the model may also
//! transcribe the overlap region differently across passes (`O3`/`03`,
//! `3`/`three`). This module is the text-side complement recommended by the
//! research (semi-global fuzzy alignment, §5): within a position-bounded seam
//! zone, detect how many leading new words are a fuzzy repeat of the provisional
//! tail — catching residue that exact matching misses, without touching genuine new speech
//! (which can't fuzzily match
//! the tail).

use strsim::jaro_winkler;

use crate::normalize::comparison_key;

/// Words at/above this Jaro-Winkler similarity (on normalized text) are treated
/// as the same word for seam alignment.
const SIMILARITY_THRESHOLD: f64 = 0.86;
/// Punctuation stripped from word ends — matches the assembler's normalization.
/// Canonicalize a token for comparison: strip punctuation, lowercase, and fold
/// number words to digits so `three` and `3` (or `N three`/`N3`) align.
pub fn canonical(word: &str) -> String {
    let s = comparison_key(word);
    number_word_to_digits(&s).unwrap_or(s)
}

fn number_word_to_digits(w: &str) -> Option<String> {
    let n = match w {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        "twenty" => 20,
        _ => return None,
    };
    Some(n.to_string())
}

/// Whether two words should be treated as the same for seam dedup.
pub fn words_similar(a: &str, b: &str) -> bool {
    let (ca, cb) = (canonical(a), canonical(b));
    if ca == cb {
        return true;
    }
    jaro_winkler(&ca, &cb) >= SIMILARITY_THRESHOLD
}

/// How many leading `head` words are a fuzzy repeat of the END of `tail`.
///
/// Greedy semi-global alignment with a small indel budget (so token-count
/// mismatches like `3`↔`three`, or a dropped article, don't break the match).
/// Only a match that runs to the end of `tail` counts — the overlap sits at the
/// boundary — so unrelated new speech is never stripped.
pub fn seam_overlap_len(tail: &[&str], head: &[&str]) -> usize {
    seam_alignment(tail, head)
        .map(|(_, head_len)| head_len)
        .unwrap_or(0)
}

/// Return `(tail_start, head_len)` for the best fuzzy suffix/prefix seam.
/// `tail_start` lets the lexical escrow replace its still-provisional suffix
/// with the right-hand window's wording instead of merely dropping that wording.
pub fn seam_alignment(tail: &[&str], head: &[&str]) -> Option<(usize, usize)> {
    if tail.is_empty() || head.is_empty() {
        return None;
    }
    let mut best = None;
    // Try every tail start; the overlap is some suffix of tail re-transcribed.
    for start in 0..tail.len() {
        let mut ti = start;
        let mut hi = 0usize;
        let mut matched = 0usize;
        let mut gaps = 0usize;
        let mut first_matched_tail = None;
        while ti < tail.len() && hi < head.len() {
            if words_similar(tail[ti], head[hi]) {
                first_matched_tail.get_or_insert(ti);
                matched += 1;
                ti += 1;
                hi += 1;
            } else if gaps < 2 && ti + 1 < tail.len() && words_similar(tail[ti + 1], head[hi]) {
                ti += 1; // skip a tail word (extra word in the prior pass)
                gaps += 1;
            } else if gaps < 2 && hi + 1 < head.len() && words_similar(tail[ti], head[hi + 1]) {
                hi += 1; // skip a head word (extra word in this pass)
                gaps += 1;
            } else {
                break;
            }
        }
        // Require the run to reach (near) the tail's end — a boundary overlap,
        // tolerating up to 2 trailing junk words the prior pass may have added —
        // and to be mostly matches, then drop the head words it consumed.
        let trailing_tail = tail.len().saturating_sub(ti);
        let consumed_to_end = trailing_tail <= 2;
        // A single common word is enough only when it reaches the actual tail.
        // Replacing one or two trailing "junk" words needs correspondingly more
        // agreement, otherwise phrases such as "the ..." can erase real text.
        let solid = matched >= trailing_tail + 1 && matched * 2 >= hi;
        if consumed_to_end && solid && best.is_none_or(|(_, best_head_len)| hi > best_head_len) {
            best = Some((first_matched_tail.unwrap_or(start), hi));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_folds_number_words() {
        assert_eq!(canonical("Three,"), "3");
        assert_eq!(canonical("3"), "3");
        assert_eq!(canonical("Hello"), "hello");
    }

    #[test]
    fn similar_handles_spelling_and_numbers() {
        assert!(words_similar("three", "3")); // number word ↔ digit
        assert!(words_similar("Mover", "mover,")); // case + punctuation
        assert!(words_similar("color", "colour")); // minor spelling variation
        assert!(!words_similar("platform", "equity"));
    }

    #[test]
    fn exact_overlap_detected() {
        let tail = ["welcome", "to", "the", "store"];
        let head = ["to", "the", "store", "today"];
        // "to the store" repeats → drop 3.
        assert_eq!(seam_overlap_len(&tail, &head), 3);
    }

    #[test]
    fn number_word_mismatch_overlap_detected() {
        // prior pass said "the 3 movers", this pass re-hears "three movers" then
        // continues. "3"↔"three", "movers"↔"movers" → 2 head words overlap.
        let tail = ["the", "3", "movers"];
        let head = ["three", "movers", "are"];
        assert_eq!(seam_overlap_len(&tail, &head), 2);
    }

    #[test]
    fn unrelated_head_not_stripped() {
        let tail = ["finance", "meets", "innovation"];
        let head = ["explore", "diverse", "offerings"];
        assert_eq!(seam_overlap_len(&tail, &head), 0);
    }

    #[test]
    fn one_common_word_cannot_replace_two_unmatched_tail_words() {
        let tail = ["we", "saw", "the", "old", "station"];
        let head = ["the", "new", "route"];
        assert_eq!(seam_alignment(&tail, &head), None);
    }
}
