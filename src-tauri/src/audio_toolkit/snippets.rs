//! [GRAIN] Voice snippets: replace a spoken trigger phrase with saved text.
//!
//! Runs ONCE per dictation on the fully assembled transcript (all engines —
//! Batch, Rolling, Live — paste a single final string, so this is the one
//! choke point). Matching must survive rolling-window chunk artifacts:
//! capitalization drift ("Grain GitHub Repo"), punctuation injected at chunk
//! boundaries ("grain, github repo."), and word splits/merges ("git hub" vs
//! "github"). No LLM, no allocation-heavy regex — a single linear scan.
//!
//! Algorithm: each trigger is flattened to its normalized characters
//! (lowercased, alphanumeric-only: "grain github repo" → "graingithubrepo").
//! The transcript is tokenized on whitespace; a match greedily consumes
//! consecutive tokens while the concatenation of their normalized forms is a
//! prefix of the flattened trigger, succeeding on exact equality. Punctuation
//! *inside* the matched span is thereby ignored, while punctuation attached to
//! the span's edges (an opening quote, a closing period) is preserved around
//! the replacement.

use crate::settings::Snippet;

/// A trigger pre-flattened for matching. Kept sorted longest-first so a more
/// specific trigger ("grain github repo docs") wins over a shorter prefix
/// ("grain github repo") starting at the same token.
struct CompiledSnippet<'a> {
    flat: String,
    replacement: &'a str,
}

/// A whitespace token of the transcript with its byte span and normalized form.
/// `pub(crate)` so voice actions can reuse the same tolerant matcher instead of
/// reimplementing normalization.
pub(crate) struct Token<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    norm: String,
    raw: &'a str,
}

/// Lowercased alphanumeric characters only. Unicode-aware so non-Latin
/// transcripts normalize consistently with their triggers.
pub(crate) fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    for part in text.split_whitespace() {
        // split_whitespace loses offsets; recover them with a forward find.
        // Safe because parts appear in order and are non-overlapping.
        let start = text[offset..]
            .find(part)
            .map(|i| offset + i)
            .unwrap_or(offset);
        let end = start + part.len();
        tokens.push(Token {
            start,
            end,
            norm: normalize(part),
            raw: part,
        });
        offset = end;
    }
    tokens
}

/// Byte length of the leading non-alphanumeric run (e.g. an opening quote).
fn punct_prefix_len(raw: &str) -> usize {
    raw.char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, _)| i)
        .unwrap_or(raw.len())
}

/// Byte offset where the trailing non-alphanumeric run starts (e.g. ".", ",").
fn punct_suffix_start(raw: &str) -> usize {
    raw.char_indices()
        .rev()
        .take_while(|(_, c)| !c.is_alphanumeric())
        .last()
        .map(|(i, _)| i)
        .unwrap_or(raw.len())
}

/// Try to match `flat` starting at token `i`. Returns the index one past the
/// last consumed token on success. `pub(crate)` for reuse by voice actions.
pub(crate) fn match_at(tokens: &[Token], i: usize, flat: &str) -> Option<usize> {
    // The anchor token must contribute characters — a bare "-" or "…" token
    // can't start a match (it would widen the replaced span for no reason).
    if tokens[i].norm.is_empty() {
        return None;
    }
    let mut consumed = 0usize;
    let mut empty_run = 0usize;
    let mut j = i;
    while j < tokens.len() {
        let norm = &tokens[j].norm;
        if norm.is_empty() {
            // Bridge pure-punctuation tokens ("grain - github repo"), but a
            // long punctuation run means we've left the phrase.
            empty_run += 1;
            if empty_run > 2 {
                return None;
            }
            j += 1;
            continue;
        }
        empty_run = 0;
        if flat[consumed..].starts_with(norm.as_str()) {
            consumed += norm.len();
            j += 1;
            if consumed == flat.len() {
                return Some(j);
            }
        } else {
            return None;
        }
    }
    None
}

/// Expand all enabled snippets in `text`. Single pass, left-to-right, longest
/// trigger wins; replacements are inserted verbatim and never re-scanned.
pub fn apply_snippets(text: &str, snippets: &[Snippet]) -> String {
    let mut compiled: Vec<CompiledSnippet> = snippets
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| {
            let flat = normalize(&s.trigger);
            if flat.is_empty() {
                None
            } else {
                Some(CompiledSnippet {
                    flat,
                    replacement: s.replacement.as_str(),
                })
            }
        })
        .collect();
    if compiled.is_empty() {
        return text.to_string();
    }
    compiled.sort_by(|a, b| b.flat.len().cmp(&a.flat.len()));

    let tokens = tokenize(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize; // byte position in `text` copied so far
    let mut i = 0usize;
    while i < tokens.len() {
        let hit = compiled
            .iter()
            .find_map(|s| match_at(&tokens, i, &s.flat).map(|end| (s, end)));
        match hit {
            Some((snippet, end)) => {
                let first = &tokens[i];
                let last = &tokens[end - 1];
                out.push_str(&text[cursor..first.start]);
                // Keep edge punctuation: '"grain github repo." → '"<url>."
                out.push_str(&first.raw[..punct_prefix_len(first.raw)]);
                out.push_str(snippet.replacement);
                out.push_str(&last.raw[punct_suffix_start(last.raw)..]);
                cursor = last.end;
                i = end;
            }
            None => i += 1,
        }
    }
    out.push_str(&text[cursor..]);
    out
}

// ── [GRAIN] "Scrap that" voice reset ────────────────────────────────────────
//
// Reuses this module's tolerant matcher (the same `tokenize` + `match_at` that
// power snippets) so it survives capitalization drift, chunk-boundary
// punctuation, and word split/merge exactly like a snippet trigger — no second
// scanner, no extra allocation beyond the tokenized transcript.

/// The reset phrase flattened to its normalized characters ("scrap that" →
/// "scrapthat"), matching how `normalize` treats a trigger.
const SCRAP_FLAT: &str = "scrapthat";

/// Byte offset in `text` just past the LAST spoken "scrap that", or `None` when
/// the phrase never appears. Scans left-to-right keeping the latest match so a
/// user who says it twice still resets to the final one.
fn last_scrap_end(text: &str) -> Option<usize> {
    let tokens = tokenize(text);
    let mut end = None;
    let mut i = 0;
    while i < tokens.len() {
        match match_at(&tokens, i, SCRAP_FLAT) {
            Some(j) => {
                end = Some(tokens[j - 1].end);
                i = j;
            }
            None => i += 1,
        }
    }
    end
}

/// Final-transcript reset: drop everything up to and including the last spoken
/// "scrap that", returning only what follows (leading whitespace trimmed).
/// Unchanged text when the phrase never appears. Callers gate on the setting.
pub fn strip_scrapped(text: &str) -> String {
    match last_scrap_end(text) {
        Some(end) => text[end..].trim_start().to_string(),
        None => text.to_string(),
    }
}

/// Live-preview reset for the streaming Studio pill: scrub the committed/tentative
/// snapshot past the last "scrap that" so the expanded pill visibly restarts (and
/// collapses to the compact capsule when nothing yet follows the phrase).
/// Idempotent — safe to run on every emit. The phrase is almost always fully
/// within `committed` (stable text commits quickly); the combined fallback only
/// covers the brief instant it straddles the commit boundary.
pub fn scrub_stream_preview(committed: &str, tentative: &str) -> (String, String) {
    if let Some(end) = last_scrap_end(committed) {
        return (
            committed[end..].trim_start().to_string(),
            tentative.to_string(),
        );
    }
    if tentative.is_empty() {
        return (committed.to_string(), tentative.to_string());
    }
    let combined = if committed.is_empty() {
        tentative.to_string()
    } else {
        format!("{committed} {tentative}")
    };
    match last_scrap_end(&combined) {
        Some(end) => (String::new(), combined[end..].trim_start().to_string()),
        None => (committed.to_string(), tentative.to_string()),
    }
}

#[cfg(test)]
mod scrap_tests {
    use super::*;

    #[test]
    fn keeps_text_after_phrase() {
        assert_eq!(
            strip_scrapped("hello world scrap that new start"),
            "new start"
        );
    }

    #[test]
    fn no_phrase_is_identity() {
        assert_eq!(
            strip_scrapped("just a normal sentence"),
            "just a normal sentence"
        );
    }

    #[test]
    fn tolerant_to_case_and_punctuation() {
        // Chunk-boundary comma / period + capitalization, like a snippet trigger.
        assert_eq!(strip_scrapped("blah blah Scrap, that. fresh"), "fresh");
        assert_eq!(strip_scrapped("one two SCRAP THAT three"), "three");
    }

    #[test]
    fn resets_to_last_occurrence() {
        assert_eq!(strip_scrapped("a scrap that b c scrap that d"), "d");
    }

    #[test]
    fn phrase_at_end_yields_empty() {
        assert_eq!(strip_scrapped("everything before scrap that"), "");
        assert_eq!(strip_scrapped("scrap that"), "");
    }

    #[test]
    fn no_partial_word_match() {
        // "scrapped that" / "scrap thatch" must NOT trigger (whole-token match).
        assert_eq!(
            strip_scrapped("we scrapped that idea"),
            "we scrapped that idea"
        );
    }

    #[test]
    fn stream_scrub_phrase_in_committed() {
        let (c, t) = scrub_stream_preview("hello scrap that world", "tail");
        assert_eq!(c, "world");
        assert_eq!(t, "tail");
    }

    #[test]
    fn stream_scrub_phrase_spans_boundary() {
        // "scrap" committed, "that" still in the volatile tail.
        let (c, t) = scrub_stream_preview("hello scrap", "that fresh");
        assert_eq!(c, "");
        assert_eq!(t, "fresh");
    }

    #[test]
    fn stream_scrub_empty_when_phrase_is_tail() {
        let (c, t) = scrub_stream_preview("everything scrap that", "");
        assert_eq!(c, "");
        assert_eq!(t, "");
    }

    #[test]
    fn stream_scrub_no_phrase_passthrough() {
        let (c, t) = scrub_stream_preview("normal committed", "tentative tail");
        assert_eq!(c, "normal committed");
        assert_eq!(t, "tentative tail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snip(trigger: &str, replacement: &str) -> Snippet {
        Snippet {
            id: trigger.to_string(),
            trigger: trigger.to_string(),
            replacement: replacement.to_string(),
            enabled: true,
        }
    }

    const URL: &str = "https://github.com/grain/grain";

    #[test]
    fn exact_match() {
        let s = [snip("grain github repo", URL)];
        assert_eq!(
            apply_snippets("check out grain github repo today", &s),
            format!("check out {URL} today")
        );
    }

    #[test]
    fn case_insensitive() {
        let s = [snip("grain github repo", URL)];
        assert_eq!(apply_snippets("Grain GitHub Repo", &s), URL.to_string());
    }

    #[test]
    fn rolling_window_punctuation_mid_phrase() {
        // Chunk boundary artifacts: comma / period injected inside the phrase.
        let s = [snip("grain github repo", URL)];
        assert_eq!(apply_snippets("grain, github repo", &s), URL.to_string());
        assert_eq!(apply_snippets("Grain. GitHub repo", &s), URL.to_string());
        assert_eq!(
            apply_snippets("see Grain, GitHub — repo, thanks", &s),
            format!("see {URL}, thanks")
        );
    }

    #[test]
    fn preserves_edge_punctuation() {
        let s = [snip("grain github repo", URL)];
        assert_eq!(
            apply_snippets("Check grain github repo.", &s),
            format!("Check {URL}.")
        );
        assert_eq!(
            apply_snippets("(grain github repo)", &s),
            format!("({URL})")
        );
    }

    #[test]
    fn word_split_and_merge() {
        let s = [snip("grain github repo", URL)];
        // ASR split "github" into two words / hyphenated the pair.
        assert_eq!(apply_snippets("grain git hub repo", &s), URL.to_string());
        assert_eq!(apply_snippets("grain github-repo", &s), URL.to_string());
    }

    #[test]
    fn no_partial_word_match() {
        let s = [snip("grain", "GRAIN")];
        assert_eq!(apply_snippets("grainy texture", &s), "grainy texture");
        assert_eq!(apply_snippets("the grain app", &s), "the GRAIN app");
    }

    #[test]
    fn longest_trigger_wins() {
        let s = [
            snip("grain github repo", "SHORT"),
            snip("grain github repo docs", "LONG"),
        ];
        assert_eq!(
            apply_snippets("open grain github repo docs now", &s),
            "open LONG now"
        );
        assert_eq!(
            apply_snippets("open grain github repo now", &s),
            "open SHORT now"
        );
    }

    #[test]
    fn multiple_occurrences_and_snippets() {
        let s = [snip("my email", "a@b.c"), snip("my address", "1 Main St")];
        assert_eq!(
            apply_snippets("send to my email or my address or my email", &s),
            "send to a@b.c or 1 Main St or a@b.c"
        );
    }

    #[test]
    fn replacement_not_rescanned() {
        // Expansion containing another trigger must not recurse.
        let s = [snip("alpha", "beta"), snip("beta", "gamma")];
        assert_eq!(apply_snippets("alpha beta", &s), "beta gamma");
    }

    #[test]
    fn disabled_and_empty_ignored() {
        let mut off = snip("grain github repo", URL);
        off.enabled = false;
        let empty = snip("   ...   ", "x");
        assert_eq!(
            apply_snippets("grain github repo", &[off, empty]),
            "grain github repo"
        );
    }

    #[test]
    fn multiline_replacement() {
        let s = [snip("my signature", "Punit\nGrain Team")];
        assert_eq!(
            apply_snippets("bye my signature", &s),
            "bye Punit\nGrain Team"
        );
    }

    #[test]
    fn punctuation_bridge_is_bounded() {
        // Three+ pure-punctuation tokens break the phrase — that's a real pause,
        // not a chunk artifact.
        let s = [snip("grain github repo", URL)];
        assert_eq!(
            apply_snippets("grain - - - github repo", &s),
            "grain - - - github repo"
        );
    }

    #[test]
    fn no_snippets_is_identity() {
        assert_eq!(apply_snippets("hello world", &[]), "hello world");
    }

    #[test]
    fn unicode_trigger() {
        let s = [snip("größe test", "OK")];
        assert_eq!(apply_snippets("Größe, Test!", &s), "OK!");
    }
}
