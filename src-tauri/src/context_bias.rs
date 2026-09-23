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
//! The user's standing dictionary is followed by action vocabulary only when
//! an extension action capture explicitly supplies it.
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
    /// user's `PyTorch` is not silently replaced by another spelling.
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
            let added = term.len()
                + if kept_rev.is_empty() {
                    0
                } else {
                    SEPARATOR.len()
                };
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
/// Kept separate from [`BiasSet::extend`] so action vocabulary can be layered
/// on top without this module knowing about action sessions.
pub fn from_custom_words(custom_words: &[String]) -> BiasSet {
    let mut set = BiasSet::new();
    set.extend(custom_words);
    set
}

// ---------------------------------------------------------------------------
// Per-session action vocabulary
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Action vocabulary waiting for the current action transcription.
static ACTION_TERMS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A late cleanup must not erase vocabulary from a newer action session.
static ACTION_GEN: AtomicU64 = AtomicU64::new(0);

/// Seed this session's bias with action vocabulary (`docs/Extensions V1/PLAN.md` §3).
///
/// The dominant real-world failure in action routing is not the router, it is
/// transcription of the words that identify the action — "skip" arriving as
/// "skit", an extension name mangled entirely. The fix is free: the phrases are
/// already declared in the manifest, already approved by the user, and already
/// in memory in the host's index.
///
/// Static, host-owned, and no privacy cost: it reads nothing about what the
/// user is doing. Biasing from an extension's own data (contacts, playlist names)
/// would be a different question and is not built.
///
/// Synchronous because there is nothing to fetch: the terms are already here.
pub fn arm_action_session(terms: Vec<String>) -> u64 {
    let generation = ACTION_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut guard) = ACTION_TERMS.lock() {
        *guard = terms;
    }
    generation
}

/// Discard action vocabulary when a capture ends before transcription consumes
/// it. Otherwise a cancelled/failed Extension Mode capture can bias the next,
/// unrelated dictation.
pub fn clear_action_session(generation: u64) {
    if let Ok(mut guard) = ACTION_TERMS.lock() {
        // A late cleanup must never erase a newer action session.
        if ACTION_GEN.load(Ordering::SeqCst) == generation {
            guard.clear();
        }
    }
}

/// The decoder prefix for this transcription, or `None` when there is nothing
/// worth biasing with.
///
/// The single entry point the transcription path calls, so the upstream file
/// carries one marked line and none of this module's lifecycle.
///
/// The standing dictionary goes first; explicit action vocabulary follows it.
pub fn for_transcription(settings: &grain_core::AppSettings) -> Option<String> {
    let mut set = from_custom_words(&settings.custom_words);
    let dictionary_terms = set.terms.len();
    if let Ok(mut guard) = ACTION_TERMS.lock() {
        set.extend(std::mem::take(&mut *guard));
    }
    let action_terms = set.terms.len() - dictionary_terms;
    let rendered = set.render();

    // Counts and byte lengths only, never the terms.
    if let Some(prefix) = &rendered {
        let kept = prefix.split(SEPARATOR).count();
        log::info!(
            "[GRAIN] bias: {kept} term(s), {} bytes (from {dictionary_terms} dictionary \
             + {action_terms} action{})",
            prefix.len(),
            if kept < dictionary_terms + action_terms {
                ", budget trimmed the rest"
            } else {
                ""
            },
        );
    } else if dictionary_terms + action_terms > 0 {
        log::info!("[GRAIN] bias: nothing usable from {dictionary_terms} dictionary term(s)");
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    static ACTION_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn empty_renders_to_none() {
        assert!(BiasSet::new().render().is_none());
        assert!(from_custom_words(&[]).render().is_none());
        // Whitespace-only entries contribute nothing.
        assert!(from_custom_words(&["   ".into(), "\n".into()])
            .render()
            .is_none());
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

    /// Explicit action terms follow the standing dictionary.
    #[test]
    fn later_sources_outrank_earlier_ones() {
        let mut set = from_custom_words(&["Standing".into()]);
        set.extend(["ActionName"]);
        let rendered = set.render().unwrap();
        assert_eq!(rendered, "Standing, ActionName");
        assert!(rendered.ends_with("ActionName"));
    }

    /// Consuming action vocabulary must empty it for the next transcription.
    #[test]
    fn action_terms_are_consumed_exactly_once() {
        let _lock = ACTION_TEST_LOCK.lock().unwrap();
        *ACTION_TERMS.lock().unwrap() = vec!["Ephemeral".to_string()];

        let mut first = BiasSet::new();
        first.extend(std::mem::take(&mut *ACTION_TERMS.lock().unwrap()));
        assert_eq!(first.render().unwrap(), "Ephemeral");

        // Second read sees nothing — the stash gave up ownership.
        assert!(ACTION_TERMS.lock().unwrap().is_empty());
    }

    /// A late cleanup must not clear a newer action session.
    #[test]
    fn stale_action_cleanup_preserves_newer_terms() {
        let _lock = ACTION_TEST_LOCK.lock().unwrap();
        let stale_generation = arm_action_session(vec!["Old".into()]);
        let new_generation = arm_action_session(vec!["New".into()]);
        clear_action_session(stale_generation);
        assert_eq!(*ACTION_TERMS.lock().unwrap(), vec!["New".to_string()]);
        clear_action_session(new_generation);
    }
}
