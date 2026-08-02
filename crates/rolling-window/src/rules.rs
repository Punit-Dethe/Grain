//! Deterministic text-repair rules for rolling-window output.
//!
//! The rolling window transcribes overlapping chunks and stitches them, so its
//! text picks up defects a single-shot transcription never has: tokens that the
//! model emitted glued together, punctuation landing at a chunk edge, spacing
//! that only looks wrong once two chunks sit side by side. Those are *mechanical*
//! defects with mechanical fixes — no model, no heuristics over meaning.
//!
//! This module is that fix layer, and it is deliberately a **registry**, not a
//! function: each defect becomes one [`Rule`] appended to [`RULES`], with its own
//! guards and its own tests. Adding the next one touches nothing that already
//! works.
//!
//! # Scope
//!
//! Rules run **only** on rolling-window (Flow) text. The batch path is a single
//! decode with no seams and does not share this layer.
//!
//! # Contract every rule must honour
//!
//! 1. **Idempotent** — `apply(apply(x)) == apply(x)`. Text flows through this
//!    layer once per chunk, and a rule that keeps changing its own output would
//!    compound across a long dictation.
//! 2. **Conservative** — when a guard is ambiguous, do nothing. A missed repair
//!    is invisible; a wrong one corrupts the user's words.
//! 3. **Allocation-free when clean** — the `matches` pre-check runs on every
//!    chunk, so it must be a cheap scan that returns `false` without touching the
//!    heap. Only `rewrite` may allocate.

use std::borrow::Cow;

/// One deterministic repair.
///
/// Split into `matches`/`rewrite` so the common case (nothing to fix) costs a
/// single scan and no allocation.
pub struct Rule {
    /// Stable identifier — used in tests and diagnostics, never shown to users.
    pub name: &'static str,
    /// Cheap pre-check: does this rule have anything to do to `text`?
    matches: fn(&str) -> bool,
    /// Write the repaired form of `text` into `out`. Only called when `matches`
    /// returned `true`.
    rewrite: fn(&str, &mut String),
}

/// The active rule set, applied in order.
///
/// Order matters only if two rules can touch the same span; today there is one
/// rule, so the ordering is trivially stable.
pub static RULES: &[Rule] = &[SENTENCE_SPACE];

/// Apply every rule in [`RULES`].
///
/// Returns [`Cow::Borrowed`] when the text is already clean, which is the
/// overwhelmingly common case — no allocation on the hot path.
pub fn apply(text: &str) -> Cow<'_, str> {
    let mut out: Cow<'_, str> = Cow::Borrowed(text);
    for rule in RULES {
        if !(rule.matches)(&out) {
            continue;
        }
        // +8 covers a handful of inserted spaces without a second realloc; a
        // chunk needing more than that is not a case worth pre-sizing for.
        let mut buf = String::with_capacity(out.len() + 8);
        (rule.rewrite)(&out, &mut buf);
        out = Cow::Owned(buf);
    }
    out
}

/// [`apply`], reusing the caller's `String` when nothing changed.
pub fn apply_in_place(text: &mut String) {
    let repaired = match apply(text) {
        Cow::Owned(fixed) => Some(fixed),
        Cow::Borrowed(_) => None,
    };
    if let Some(fixed) = repaired {
        *text = fixed;
    }
}

// ---------------------------------------------------------------------------
// Rule: sentence-space
// ---------------------------------------------------------------------------

/// Insert the missing space in `end of sentence.Next sentence`.
///
/// Whisper-family models emit the sentence break as part of a single token often
/// enough that a chunk arrives already glued, and the assembler's join logic only
/// governs space *between* tokens — it cannot see inside one. So the defect
/// survives to the final transcript.
///
/// See [`needs_break_after_stop`] for the guards that keep decimals, initialisms
/// and domains intact.
const SENTENCE_SPACE: Rule = Rule {
    name: "sentence-space",
    matches: sentence_space_matches,
    rewrite: sentence_space_rewrite,
};

/// Terminators treated as end-of-sentence.
///
/// Only the full stop today. `?` and `!` glue the same way and belong here, but
/// they are a separate observation and get added with their own tests rather
/// than assumed — widening this list is the entire change when they do.
const TERMINATORS: [char; 1] = ['.'];

fn is_terminator(ch: char) -> bool {
    TERMINATORS.contains(&ch)
}

/// A character that can begin a new sentence.
///
/// Requiring *uppercase* is the load-bearing guard: transcript sentences are
/// capitalised (the assembler enforces this at seams), while the false-positive
/// cases this rule must never touch — `example.com`, `file.txt`, `e.g.` — are
/// lowercase. Cased-script only by construction: in CJK and other caseless
/// scripts `is_uppercase()` is false, so the rule correctly never fires there.
fn is_sentence_start(ch: char) -> bool {
    ch.is_alphabetic() && ch.is_uppercase()
}

/// Walk `text`, reporting each byte offset where a space must be inserted.
///
/// The offset is the position of the sentence-starting character, i.e. the space
/// goes immediately *before* it. Returns whether any break was found, so the
/// same walk backs both `matches` (with a no-op sink) and `rewrite`.
fn scan_breaks(text: &str, mut on_break: impl FnMut(usize)) -> bool {
    let mut found = false;
    // The character before the current one.
    let mut prev: Option<char> = None;
    // Length of the run of alphabetic characters ending at `prev`.
    let mut alpha_run: usize = 0;
    // Length of the alphabetic run immediately preceding the terminator, when
    // `prev` is that terminator. Distinguishes `Dr.Smith` (run 2, a real word)
    // from `U.S.` (run 1, an initialism).
    let mut run_before_stop: usize = 0;

    for (idx, ch) in text.char_indices() {
        let after_stop = prev.is_some_and(is_terminator);
        // A one-letter run before the stop means an initial (`J.R.R.`, `U.S.A.`)
        // — those dots are internal to the token, not sentence ends.
        if after_stop && run_before_stop != 1 && is_sentence_start(ch) {
            found = true;
            on_break(idx);
        }

        if is_terminator(ch) {
            run_before_stop = alpha_run;
        }
        alpha_run = if ch.is_alphabetic() { alpha_run + 1 } else { 0 };
        prev = Some(ch);
    }

    found
}

fn sentence_space_matches(text: &str) -> bool {
    scan_breaks(text, |_| {})
}

fn sentence_space_rewrite(text: &str, out: &mut String) {
    let mut copied = 0usize;
    scan_breaks(text, |idx| {
        out.push_str(&text[copied..idx]);
        out.push(' ');
        copied = idx;
    });
    out.push_str(&text[copied..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str) -> String {
        apply(text).into_owned()
    }

    // -- the reported defect ------------------------------------------------

    #[test]
    fn inserts_space_after_sentence_stop() {
        assert_eq!(
            fixed("I finished the draft.It needs review."),
            "I finished the draft. It needs review."
        );
    }

    #[test]
    fn repairs_every_occurrence_in_one_pass() {
        assert_eq!(
            fixed("One.Two.Three thing.Four"),
            "One. Two. Three thing. Four"
        );
    }

    #[test]
    fn leaves_correctly_spaced_text_untouched() {
        let clean = "I finished the draft. It needs review.";
        assert_eq!(fixed(clean), clean);
        // And does so without allocating.
        assert!(matches!(apply(clean), Cow::Borrowed(_)));
    }

    // -- guards -------------------------------------------------------------

    #[test]
    fn keeps_decimals_intact() {
        assert_eq!(fixed("It costs 3.14 dollars"), "It costs 3.14 dollars");
    }

    #[test]
    fn keeps_initialisms_intact() {
        assert_eq!(fixed("the U.S.A. team"), "the U.S.A. team");
        assert_eq!(fixed("J.R.R.Tolkien"), "J.R.R.Tolkien");
    }

    #[test]
    fn keeps_lowercase_domains_and_filenames_intact() {
        assert_eq!(fixed("visit example.com today"), "visit example.com today");
        assert_eq!(fixed("open README.md now"), "open README.md now");
    }

    #[test]
    fn splits_after_a_real_abbreviation() {
        // Two-letter run before the stop is a word, not an initial.
        assert_eq!(fixed("Ask Dr.Smith about it"), "Ask Dr. Smith about it");
    }

    #[test]
    fn splits_after_an_ellipsis() {
        assert_eq!(fixed("Wait...Okay then"), "Wait... Okay then");
    }

    #[test]
    fn ignores_a_stop_followed_by_punctuation() {
        assert_eq!(fixed("He said \"go\".\"Now\""), "He said \"go\".\"Now\"");
    }

    // -- contract -----------------------------------------------------------

    #[test]
    fn is_idempotent() {
        for input in [
            "One.Two",
            "the U.S.A. team",
            "3.14",
            "Wait...Okay",
            "plain text with no stops",
            "",
        ] {
            let once = fixed(input);
            assert_eq!(fixed(&once), once, "not idempotent for {input:?}");
        }
    }

    #[test]
    fn handles_multibyte_text_without_panicking() {
        // Byte offsets must land on char boundaries.
        assert_eq!(fixed("café.Après midi"), "café. Après midi");
        // Caseless scripts have no uppercase, so the rule correctly abstains.
        assert_eq!(fixed("これです。次の文"), "これです。次の文");
        assert_eq!(fixed("完了.次"), "完了.次");
    }

    #[test]
    fn empty_and_edge_inputs() {
        assert_eq!(fixed(""), "");
        assert_eq!(fixed("."), ".");
        assert_eq!(fixed(".A"), ". A");
        assert_eq!(fixed("A."), "A.");
    }

    #[test]
    fn apply_in_place_matches_apply() {
        let mut owned = String::from("One.Two");
        apply_in_place(&mut owned);
        assert_eq!(owned, "One. Two");

        let mut clean = String::from("One. Two");
        apply_in_place(&mut clean);
        assert_eq!(clean, "One. Two");
    }

    #[test]
    fn every_rule_has_a_unique_name() {
        let mut names: Vec<&str> = RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate rule name in RULES");
    }
}
