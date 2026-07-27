//! [GRAIN] Recognizer biasing — the hotword list handed to Whisper's decoder
//! prefix (`initial_prompt`).
//!
//! # Why this is not just `join(", ")`
//!
//! Whisper conditions decoding on a text prefix, and that prefix is **hard
//! capped at `n_text_ctx / 2` ≈ 224 tokens**. Past the cap whisper.cpp does not
//! error — it silently drops tokens from the *front*. So a user with a large
//! dictionary was already losing terms with no signal that it happened, and had
//! no way to know which ones survived.
//!
//! Three properties follow from how the prefix is consumed, and this module
//! exists to guarantee them:
//!
//! 1. **The tail is privileged.** Attention weights the end of the prefix most
//!    heavily, and truncation eats the front. Whatever matters most goes last.
//! 2. **Truncate on term boundaries, never mid-word.** Slicing bytes to fit
//!    would feed the decoder a fragment (`torch` out of `PyTorch`), which biases
//!    toward a word the user never has in their dictionary. Whole terms are
//!    dropped instead.
//! 3. **Whitespace must be normalized.** Irregular whitespace in the prefix
//!    makes the multilingual tokenizer drift, in the well-known failure where an
//!    English utterance starts emitting CJK.
//!
//! # Ordering contract
//!
//! Terms are held **least- to most-important**, because that is the order the
//! budget consumes them in: the front is what gets dropped. Concretely the
//! user's standing dictionary goes in first, and anything derived from the
//! current surface (which is far more likely to be about *this* utterance) is
//! appended after it, so it is the last thing sacrificed.
//!
//! # Cost
//!
//! Pure string work over a list already in memory, run once per transcription.
//! No allocation is held past `render`, and an empty bias set returns `None` so
//! the caller omits the decoder extension entirely.

/// Byte budget for the rendered prefix.
///
/// Whisper's own cap is `n_text_ctx / 2` ≈ 224 tokens, which is ~896 bytes of
/// ASCII; cloud whisper endpoints that accept a prompt enforce 896 bytes
/// directly (counting UTF-8 bytes even where the error says "characters"). 800
/// leaves headroom for multi-byte scripts, where a token can be 3–4 bytes and a
/// byte-based budget would otherwise overshoot the token cap.
const MAX_PROMPT_BYTES: usize = 800;

/// Separator between terms. The trailing space matters: it keeps the tokenizer
/// from gluing two terms into one piece.
const SEPARATOR: &str = ", ";

/// An ordered, de-duplicated hotword list, held least- to most-important.
#[derive(Debug, Default, Clone)]
pub struct BiasSet {
    terms: Vec<String>,
}

impl BiasSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append terms at the current top of priority. Later calls outrank earlier
    /// ones, so callers add from least to most specific.
    ///
    /// De-duplicates case-insensitively, keeping the FIRST spelling seen: a
    /// user's `PyTorch` is not silently replaced by a screen-scraped `pytorch`.
    pub fn extend<I, S>(&mut self, terms: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for term in terms {
            let term = term.as_ref().trim();
            if term.is_empty() {
                continue;
            }
            if self
                .terms
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(term))
            {
                continue;
            }
            self.terms.push(term.to_string());
        }
    }

    /// Render to a decoder prefix, or `None` when there is nothing to bias with.
    ///
    /// `None` rather than an empty string is what lets the caller omit the
    /// decoder extension entirely, so there is no separate `is_empty` to keep in
    /// sync with it.
    ///
    /// Drops whole terms from the FRONT until the result fits [`MAX_PROMPT_BYTES`]
    /// — see the module docs for why the front is what gives way.
    pub fn render(&self) -> Option<String> {
        if self.terms.is_empty() {
            return None;
        }

        // Normalize first: a term carrying a newline or a tab would otherwise
        // put irregular whitespace into the prefix.
        let normalized: Vec<String> = self
            .terms
            .iter()
            .map(|t| collapse_whitespace(t))
            .filter(|t| !t.is_empty())
            .collect();
        if normalized.is_empty() {
            return None;
        }

        // Walk from the END (most important) and keep what fits, so the terms
        // that survive are the ones the decoder attends to hardest.
        let mut kept_rev: Vec<&str> = Vec::new();
        let mut bytes = 0usize;
        for term in normalized.iter().rev() {
            let added = term.len() + if kept_rev.is_empty() { 0 } else { SEPARATOR.len() };
            if bytes + added > MAX_PROMPT_BYTES {
                // A single term longer than the whole budget is unusable; skip
                // it and keep trying shorter, lower-priority ones rather than
                // giving up on the list.
                continue;
            }
            bytes += added;
            kept_rev.push(term.as_str());
        }
        if kept_rev.is_empty() {
            return None;
        }

        kept_rev.reverse();
        Some(kept_rev.join(SEPARATOR))
    }
}

/// Collapse every run of whitespace to a single space and trim.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.trim().chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Build the bias set for a transcription from the user's standing dictionary.
///
/// Kept separate from [`BiasSet::extend`] so the surface-derived sources can be
/// layered on top by the caller without this module knowing about them.
pub fn from_custom_words(custom_words: &[String]) -> BiasSet {
    let mut set = BiasSet::new();
    set.extend(custom_words);
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_renders_to_none() {
        assert!(BiasSet::new().render().is_none());
        assert!(from_custom_words(&[]).render().is_none());
        // Whitespace-only entries contribute nothing.
        assert!(from_custom_words(&["   ".into(), "\n".into()]).render().is_none());
    }

    #[test]
    fn renders_in_order_and_dedupes_case_insensitively() {
        let set = from_custom_words(&[
            "PyTorch".into(),
            "pytorch".into(),
            "Tauri".into(),
            "PYTORCH".into(),
        ]);
        assert_eq!(set.render().unwrap(), "PyTorch, Tauri");
    }

    #[test]
    fn whitespace_is_collapsed() {
        // Irregular whitespace in the prefix makes the multilingual tokenizer
        // drift, so it must never reach the decoder.
        let set = from_custom_words(&["Grain\n\tSpace".into(), "  useGrain   Store ".into()]);
        let rendered = set.render().unwrap();
        assert_eq!(rendered, "Grain Space, useGrain Store");
        assert!(!rendered.contains('\n'));
        assert!(!rendered.contains('\t'));
    }

    /// The budget must hold, and it must give way at the FRONT — the tail is
    /// what whisper attends to and what survives its own truncation.
    #[test]
    fn budget_drops_from_the_front_keeping_the_tail() {
        let mut words: Vec<String> = (0..400).map(|i| format!("Term{i:04}")).collect();
        words.push("MostImportant".into());
        let rendered = from_custom_words(&words).render().unwrap();

        assert!(
            rendered.len() <= MAX_PROMPT_BYTES,
            "rendered {} bytes, over budget",
            rendered.len()
        );
        // The last term in, being the highest priority, must survive.
        assert!(rendered.ends_with("MostImportant"));
        // The earliest terms are the ones sacrificed.
        assert!(!rendered.contains("Term0000"));
    }

    /// Truncation must never emit a partial word: a fragment biases toward a
    /// term the user does not actually have.
    #[test]
    fn truncation_never_splits_a_term() {
        let words: Vec<String> = (0..400).map(|i| format!("Identifier{i:04}")).collect();
        let rendered = from_custom_words(&words).render().unwrap();
        for term in rendered.split(SEPARATOR) {
            assert!(
                words.iter().any(|w| w == term),
                "emitted a fragment, not a whole term: {term:?}"
            );
        }
    }

    /// Multi-byte terms must not blow the byte budget or be cut mid-character.
    #[test]
    fn multibyte_terms_respect_the_byte_budget() {
        let words: Vec<String> = (0..200).map(|i| format!("日本語テスト{i}")).collect();
        let rendered = from_custom_words(&words).render().unwrap();
        assert!(rendered.len() <= MAX_PROMPT_BYTES);
        // Still valid UTF-8 with whole terms (String guarantees the former;
        // this asserts the latter).
        for term in rendered.split(SEPARATOR) {
            assert!(words.iter().any(|w| w == term), "split a multi-byte term");
        }
    }

    /// A term longer than the entire budget cannot be included, but it must not
    /// take the rest of the list down with it.
    #[test]
    fn oversized_term_is_skipped_not_fatal() {
        let huge = "X".repeat(MAX_PROMPT_BYTES + 50);
        let set = from_custom_words(&["Tauri".into(), huge, "Grain".into()]);
        let rendered = set.render().unwrap();
        assert!(rendered.contains("Grain"));
        assert!(rendered.contains("Tauri"));
        assert!(rendered.len() <= MAX_PROMPT_BYTES);
    }

    /// Later sources outrank earlier ones — the ordering contract the whole
    /// budget depends on.
    #[test]
    fn later_sources_outrank_earlier_ones() {
        let mut set = from_custom_words(&["Standing".into()]);
        set.extend(["FromScreen"]);
        let rendered = set.render().unwrap();
        assert_eq!(rendered, "Standing, FromScreen");
        assert!(rendered.ends_with("FromScreen"));
    }
}
