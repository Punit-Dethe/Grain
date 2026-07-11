//! Right-context seam revision (RCSR) — retro-corrects the committed tail's
//! punctuation and casing at each chunk seam.
//!
//! The problem: chunk N ends BLIND. The model sees its audio stop, so it stamps
//! a sentence-final period on the last word and the next chunk opens with a
//! sentence capital — even mid-phrase. This is the dominant formatting artifact
//! of every chunked long-form pipeline (spurious `.` + wrong capital every
//! window).
//!
//! The insight: chunk N+1 re-hears those same boundary words (the 2 s overlap)
//! WITH right context — it knows whether the speech continued. The assembler
//! already aligns and drops that re-hearing to dedup words; this module harvests
//! its punctuation before it's discarded. Mainstream streaming stacks
//! (whisper_streaming/LocalAgreement, SimulStreaming/AlignAtt) only decide WHAT
//! to commit — none retro-correct committed text with the later evidence.
//!
//! The acoustic prior: the cursor knows WHY it cut ([`CutKind`]):
//! - [`CutKind::HardCut`] — the window filled mid-speech (VAD said voiced,
//!   trailing silence < threshold). A sentence break exactly there is unlikely;
//!   chunk N's terminal period is near-certainly an end-of-audio artifact →
//!   adopt the re-hearing's punctuation verbatim.
//! - [`CutKind::Silence`] — a real ≥0.7 s pause triggered the cut. Pause length
//!   is the single strongest full-stop cue in the prosody literature, so chunk
//!   N's period is PLAUSIBLE → keep it unless the re-hearing actively
//!   substitutes different punctuation; and when BOTH passes left the seam bare
//!   but the model opened the fresh region with a capital, add the period both
//!   signals voted for.
//! - [`CutKind::Stop`] — the user ended the session; nothing follows, so no
//!   seam ever forms on this boundary.
//!
//! Everything here is a handful of string comparisons over ≤ 8 words per seam —
//! compute is flat and allocation-light regardless of session length.

use crate::cursor::CutKind;
use crate::merge::words_similar;

/// Trailing characters treated as adoptable punctuation.
const TRAIL_PUNCT: &[char] = &['.', ',', '!', '?', ';', ':', '…'];
/// Trailing characters that make a word too risky to rewrite (quotes/brackets
/// interleave with punctuation in ways not worth modeling at a seam).
const BAIL_TRAIL: &[char] = &['"', '\'', '\u{2019}', '\u{201D}', ')', ']', '}'];
/// How many committed tail / re-hearing words the alignment may look at.
const SEAM_SPAN: usize = 3;

/// Split a word into (core, trailing punctuation).
fn split_trailing(word: &str) -> (&str, &str) {
    let core_end = word.trim_end_matches(TRAIL_PUNCT).len();
    word.split_at(core_end)
}

/// Whether `text` currently ends with sentence-final punctuation.
pub fn ends_terminal(text: &str) -> bool {
    text.trim_end()
        .ends_with(['.', '!', '?', '…'])
}

/// Words whose leading capital carries no sentence-boundary signal, or that
/// must never be down-cased: "I"-family, acronyms, mixed-case identifiers,
/// anything with digits.
pub fn safe_to_lowercase(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    if word == "I" || word.starts_with("I'") || word.starts_with("I\u{2019}") {
        return false;
    }
    // Pure Titlecase only: any inner capital or digit means acronym/identifier.
    chars.all(|c| c.is_lowercase() || c == '\'' || c == '\u{2019}')
}

/// Uppercase the first letter of `word` (no-op if already upper / non-alpha).
pub fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => c.to_uppercase().chain(chars).collect(),
        _ => word.to_string(),
    }
}

/// Whether a token is nothing but punctuation (some models emit `.` / `,` as
/// standalone word rows) — such tokens glue onto the previous word.
pub fn is_punct_only(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| TRAIL_PUNCT.contains(&c))
}

/// Lowercase the first letter of `word`.
pub fn lowercase_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => c.to_lowercase().chain(chars).collect(),
        _ => word.to_string(),
    }
}

/// Find the re-hearing word that corresponds to the committed text's LAST word.
/// Both sequences end at (nearly) the same audio moment — the fresh boundary —
/// so the match must sit at/near the re-hearing's end. Fuzzy comparison via the
/// same similarity the seam dedup uses.
fn rehearing_of_last(tail_last_core: &str, rehearing: &[&str]) -> Option<usize> {
    let lo = rehearing.len().saturating_sub(SEAM_SPAN);
    (lo..rehearing.len())
        .rev()
        .find(|&j| words_similar(rehearing[j], tail_last_core))
}

/// Retro-correct the committed transcript's final word at a seam, using chunk
/// N+1's re-hearing of the overlap (`rehearing`, in order) as the right-context
/// witness. `first_fresh` is the first word about to be appended (N+1's first
/// accepted fresh word), used only as a capitalization vote.
///
/// Returns `true` if `committed` was modified.
pub fn revise_seam(
    committed: &mut String,
    seam: CutKind,
    rehearing: &[&str],
    first_fresh: Option<&str>,
) -> bool {
    if committed.is_empty() || seam == CutKind::Stop {
        return false;
    }
    let last_start = match committed.rfind(char::is_whitespace) {
        Some(i) => i + 1,
        None => 0,
    };
    let last_word = committed[last_start..].to_string();
    if last_word.is_empty() || last_word.ends_with(BAIL_TRAIL) {
        return false;
    }
    let (core, t_punct) = split_trailing(&last_word);
    if core.is_empty() {
        return false;
    }

    let witness_punct = rehearing_of_last(core, rehearing).map(|j| {
        let (_, o_punct) = split_trailing(rehearing[j]);
        o_punct
    });

    let fresh_votes_sentence = first_fresh.is_some_and(safe_to_lowercase);

    let new_punct: Option<String> = match (seam, witness_punct) {
        // Hard cut mid-speech: N's end punctuation is an end-of-audio artifact.
        // The witness re-heard the seam with right context — adopt verbatim.
        (CutKind::HardCut, Some(o)) if o != t_punct => Some(o.to_string()),
        // Hard cut, no alignment: act only on the unambiguous artifact — a lone
        // period the witness's own casing contradicts (fresh word lowercase).
        (CutKind::HardCut, None)
            if t_punct == "."
                && first_fresh
                    .is_some_and(|w| w.chars().next().is_some_and(char::is_lowercase)) =>
        {
            Some(String::new())
        }
        // Real pause: the period is plausible. Adopt only an active
        // substitution; a bare witness (models under-punctuate mid-stream)
        // never erases it.
        (CutKind::Silence, Some(o)) if !o.is_empty() && o != t_punct => Some(o.to_string()),
        // Real pause + both passes bare + the model opened the fresh region
        // with a sentence capital: acoustic and lexical signals agree — add the
        // period they voted for.
        (CutKind::Silence, Some(o))
            if o.is_empty() && t_punct.is_empty() && fresh_votes_sentence =>
        {
            Some(".".to_string())
        }
        (CutKind::Silence, None) if t_punct.is_empty() && fresh_votes_sentence => {
            Some(".".to_string())
        }
        _ => None,
    };

    match new_punct {
        Some(p) if p != t_punct => {
            committed.truncate(last_start);
            committed.push_str(core);
            committed.push_str(&p);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_seam_strips_spurious_period() {
        // N ended "...went to the store." blind; N+1 re-heard "to the store"
        // (no punct) and continues lowercase → the period was an artifact.
        let mut s = "I went to the store.".to_string();
        let changed = revise_seam(
            &mut s,
            CutKind::HardCut,
            &["to", "the", "store"],
            Some("and"),
        );
        assert!(changed);
        assert_eq!(s, "I went to the store");
    }

    #[test]
    fn hard_seam_adopts_comma() {
        let mut s = "we packed the boxes.".to_string();
        revise_seam(
            &mut s,
            CutKind::HardCut,
            &["packed", "the", "boxes,"],
            Some("and"),
        );
        assert_eq!(s, "we packed the boxes,");
    }

    #[test]
    fn hard_seam_gains_missed_period() {
        // N left the seam bare; the witness, hearing the continuation, stamped
        // a real sentence end.
        let mut s = "ship it today".to_string();
        revise_seam(
            &mut s,
            CutKind::HardCut,
            &["it", "today."],
            Some("Then"),
        );
        assert_eq!(s, "ship it today.");
    }

    #[test]
    fn hard_seam_no_alignment_strips_lone_period_on_lowercase_continuation() {
        let mut s = "totally different words.".to_string();
        revise_seam(&mut s, CutKind::HardCut, &["unrelated", "rehearing"], Some("and"));
        assert_eq!(s, "totally different words");
    }

    #[test]
    fn hard_seam_no_alignment_keeps_period_on_capital_continuation() {
        let mut s = "totally different words.".to_string();
        revise_seam(&mut s, CutKind::HardCut, &[], Some("Then"));
        assert_eq!(s, "totally different words.");
    }

    #[test]
    fn silence_seam_keeps_period_when_witness_is_bare() {
        // Real pause: witness omitting punctuation is under-punctuation, not
        // evidence of continuation.
        let mut s = "we should ship it.".to_string();
        let changed = revise_seam(
            &mut s,
            CutKind::Silence,
            &["should", "ship", "it"],
            Some("also"),
        );
        assert!(!changed);
        assert_eq!(s, "we should ship it.");
    }

    #[test]
    fn silence_seam_adopts_substitution() {
        let mut s = "we should ship it.".to_string();
        revise_seam(
            &mut s,
            CutKind::Silence,
            &["should", "ship", "it,"],
            Some("but"),
        );
        assert_eq!(s, "we should ship it,");
    }

    #[test]
    fn silence_seam_adds_period_on_agreeing_votes() {
        // Both passes bare, but a real pause happened AND the model capitalized
        // the continuation — acoustic + lexical agreement.
        let mut s = "we should ship it".to_string();
        revise_seam(
            &mut s,
            CutKind::Silence,
            &["should", "ship", "it"],
            Some("Also"),
        );
        assert_eq!(s, "we should ship it.");
    }

    #[test]
    fn silence_seam_capital_i_does_not_vote() {
        // "I" is always capitalized — it carries no sentence-boundary signal.
        let mut s = "we should ship it".to_string();
        let changed = revise_seam(
            &mut s,
            CutKind::Silence,
            &["should", "ship", "it"],
            Some("I"),
        );
        assert!(!changed);
    }

    #[test]
    fn stop_seam_never_revises() {
        let mut s = "final words.".to_string();
        assert!(!revise_seam(&mut s, CutKind::Stop, &["final", "words"], Some("and")));
    }

    #[test]
    fn quoted_tail_bails() {
        let mut s = "she said \"stop\"".to_string();
        assert!(!revise_seam(&mut s, CutKind::HardCut, &["said", "stop."], Some("and")));
    }

    #[test]
    fn casing_helpers() {
        assert!(safe_to_lowercase("Hello"));
        assert!(!safe_to_lowercase("I"));
        assert!(!safe_to_lowercase("I'm"));
        assert!(!safe_to_lowercase("NASA"));
        assert!(!safe_to_lowercase("iPhone"));
        assert!(!safe_to_lowercase("O3"));
        assert!(!safe_to_lowercase("hello"));
        assert_eq!(capitalize_first("and"), "And");
        assert_eq!(capitalize_first("And"), "And");
        assert_eq!(lowercase_first("Then"), "then");
        assert_eq!(lowercase_first("then"), "then");
    }

    #[test]
    fn ends_terminal_checks() {
        assert!(ends_terminal("done."));
        assert!(ends_terminal("done!"));
        assert!(ends_terminal("done… "));
        assert!(!ends_terminal("done,"));
        assert!(!ends_terminal("done"));
    }
}
