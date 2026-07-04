//! [GRAIN] Auto-add to dictionary.
//!
//! When the user re-spells a word Grain just pasted — the same way, across a
//! couple of pastes — Grain offers to add that spelling to their dictionary
//! (`custom_words`). This mirrors Wispr Flow / Typeless "auto-add to dictionary".
//!
//! # How we know it's OUR text being edited (the core edge case)
//! We diff the CURRENT field content against exactly what we pasted, at the token
//! level (LCS alignment). A learnable correction is a gap between two matched
//! anchor words where a short run of pasted words (1–2) was replaced by a single,
//! *similar-looking* word (a spelling fix, not an unrelated swap) that is
//! proper-noun / identifier shaped. Pre-existing text around our paste aligns as
//! boundary insertions and is ignored, and if too little of the paste still
//! matches (the user moved to a different field / cleared it) we bail — so edits
//! the user makes elsewhere never trigger a suggestion.
//!
//! # Overhead
//! **Zero when disabled** — [`on_pasted`] returns before spawning anything unless
//! `auto_dictionary_enabled` is on. When on, one short-lived watcher thread polls
//! the focused field for ~10s and then dies (no persistent engine, no listener).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use grain_core::{AppContext, DictCandidate};
use tauri::{AppHandle, Manager};

use crate::settings::get_settings;

/// Observed in this many distinct paste-sessions before we suggest it.
const SUGGEST_THRESHOLD: u32 = 2;
/// Keep the candidate list bounded (prune lowest counts beyond this).
const MAX_CANDIDATES: usize = 60;
/// Total watch window after a paste.
const WATCH_SECS: u64 = 10;
/// Delay before the first sample (let the paste + the user's first edits settle).
const FIRST_SAMPLE_MS: u64 = 1200;
/// Interval between field samples.
const SAMPLE_INTERVAL_MS: u64 = 1200;
/// Minimum fraction of pasted words that must still align for us to trust the
/// field still holds our paste (else the user moved on — don't count anything).
const MIN_ANCHOR_RATIO: f32 = 0.5;

/// Bumped on every paste so a newer watcher supersedes any still-running one
/// (only the latest paste is ever watched — no pile-up).
static GENERATION: AtomicU64 = AtomicU64::new(0);

// ── Pure diff (unit-tested) ─────────────────────────────────────────────────

struct Word {
    /// Punctuation-trimmed surface form (what we'd add to the dictionary).
    raw: String,
    /// Lowercased key for alignment/similarity.
    lc: String,
}

/// Split into word tokens, trimming surrounding punctuation but keeping internal
/// `_`, `'`, and digits (so `snake_case`, `it's`, `v2` stay whole).
fn tokenize(s: &str) -> Vec<Word> {
    s.split_whitespace()
        .filter_map(|t| {
            let raw: String = t
                .trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
                .to_string();
            if raw.is_empty() {
                None
            } else {
                let lc = raw.to_lowercase();
                Some(Word { raw, lc })
            }
        })
        .collect()
}

/// Indices of the longest common subsequence between two token lists, as aligned
/// `(i, j)` anchor pairs. Standard LCS DP; token counts are capped by the caller.
fn lcs_anchors(a: &[Word], b: &[Word]) -> Vec<(usize, usize)> {
    let n = a.len();
    let m = b.len();
    // dp[i][j] = LCS length of a[i..] and b[j..].
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i].lc == b[j].lc {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i].lc == b[j].lc {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Levenshtein edit distance (small strings only).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// True if `new` reads as a spelling correction of `old` (close), not an unrelated
/// replacement. Requires a real change and bounded relative distance.
fn is_spelling_fix(old: &str, new: &str) -> bool {
    if old == new || old.is_empty() || new.is_empty() {
        return false;
    }
    let d = levenshtein(old, new);
    let max_len = old.chars().count().max(new.chars().count());
    d > 0 && d <= 6 && (d as f32) <= (max_len as f32) * 0.5 + 0.5
}

/// True if `word` is worth learning (proper-noun / identifier shaped, not an
/// ordinary English word). Reuses the exact criteria of the nearby-terms
/// extractor so the two features stay consistent.
fn is_learnable(word: &str) -> bool {
    let terms = crate::context_detect::extract_unique_terms(word);
    terms.iter().any(|t| t.eq_ignore_ascii_case(word))
}

/// Detect learnable single-word spelling corrections the user made to `pasted`
/// within `current`. Returns the corrected spellings (deduped, original casing).
/// Empty when too little of the paste still aligns (user moved on).
pub fn detect_corrections(pasted: &str, current: &str) -> Vec<String> {
    const MAX_TOKENS: usize = 400;
    let mut p = tokenize(pasted);
    let mut c = tokenize(current);
    p.truncate(MAX_TOKENS);
    c.truncate(MAX_TOKENS);
    if p.len() < 2 {
        return Vec::new();
    }

    let anchors = lcs_anchors(&p, &c);
    // Guard: the field must still hold most of our paste, else it isn't ours.
    if (anchors.len() as f32) < (p.len() as f32) * MIN_ANCHOR_RATIO {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    // Walk gaps strictly BETWEEN consecutive anchors (never the outer boundaries,
    // where pre-existing surrounding text lives).
    for pair in anchors.windows(2) {
        let (pi0, ci0) = pair[0];
        let (pi1, ci1) = pair[1];
        let old_run = &p[pi0 + 1..pi1]; // pasted words replaced
        let new_run = &c[ci0 + 1..ci1]; // current words in their place
                                        // A correction: 1–2 old words → exactly 1 new word.
        if new_run.len() != 1 || old_run.is_empty() || old_run.len() > 2 {
            continue;
        }
        let old_joined: String = old_run.iter().map(|w| w.lc.as_str()).collect();
        let new = &new_run[0];
        // A 1→1 change must be a close spelling fix (not an unrelated swap). A
        // 2→1 change is a word-merge, which is valid even when the joined letters
        // are identical (e.g. "use effect" → "useEffect").
        let is_merge = old_run.len() >= 2;
        let similar = (is_merge && old_joined == new.lc) || is_spelling_fix(&old_joined, &new.lc);
        if !similar {
            continue;
        }
        if !is_learnable(&new.raw) {
            continue;
        }
        if seen.insert(new.lc.clone()) {
            out.push(new.raw.clone());
        }
    }
    out
}

// ── Watcher + persistence (Windows-effective; no-op elsewhere) ───────────────

fn ctx_of(app: &AppHandle) -> Option<Arc<AppContext>> {
    app.try_state::<Arc<AppContext>>()
        .map(|s| s.inner().clone())
}

/// Called right after a successful paste. Spawns the short watch ONLY when the
/// feature is on — otherwise returns immediately (true zero overhead).
pub fn on_pasted(app: &AppHandle, pasted: &str) {
    let settings = get_settings(app);
    if !settings.auto_dictionary_enabled {
        return;
    }
    let pasted = pasted.trim().to_string();
    // Need at least a couple of words for the alignment to mean anything.
    if pasted.split_whitespace().count() < 2 {
        log::info!("[auto-dict] skip: pasted text has <2 words");
        return;
    }
    let Some(ctx) = ctx_of(app) else { return };
    let app = app.clone();
    let my_gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!(
        "[auto-dict] watching field for edits ({}s) after paste of {} words",
        WATCH_SECS,
        pasted.split_whitespace().count()
    );

    // A dedicated OS thread (UIA/COM is blocking) that dies after WATCH_SECS.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(FIRST_SAMPLE_MS));
        let samples = (WATCH_SECS * 1000 / SAMPLE_INTERVAL_MS).max(1);
        let mut counted: HashSet<String> = HashSet::new();
        for i in 0..samples {
            // A newer paste started its own watch — stand down.
            if GENERATION.load(Ordering::SeqCst) != my_gen {
                return;
            }
            let Some(current) = crate::context_detect::read_focused_text() else {
                log::info!("[auto-dict] sample {i}: could not read focused field (no UIA text)");
                std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
                continue;
            };
            let corrections = detect_corrections(&pasted, &current);
            log::info!(
                "[auto-dict] sample {i}: read {} chars, {} correction(s){}",
                current.len(),
                corrections.len(),
                if corrections.is_empty() {
                    String::new()
                } else {
                    format!(": {corrections:?}")
                }
            );
            for word in corrections {
                let key = word.to_lowercase();
                // Count each correction once per paste-session.
                if !counted.insert(key.clone()) {
                    continue;
                }
                if record_and_maybe_suggest(&app, &ctx, &word) {
                    return; // suggested one; stop this watch.
                }
            }
            std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
        }
        log::info!("[auto-dict] watch window ended");
    });
}

/// Increment the candidate's count; if it reaches the threshold, emit a pill
/// suggestion and drop it from the candidate list. Returns true if a suggestion
/// was emitted. Skips words already in the dictionary.
fn record_and_maybe_suggest(app: &AppHandle, ctx: &AppContext, word: &str) -> bool {
    let key = word.to_lowercase();
    let mut suggest = false;
    let _ = ctx.update_settings(|s| {
        if s.custom_words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
            return; // already known — nothing to learn.
        }
        match s
            .dictionary_candidates
            .iter_mut()
            .find(|c| c.word.eq_ignore_ascii_case(&key))
        {
            Some(c) => c.count += 1,
            None => s.dictionary_candidates.push(DictCandidate {
                word: word.to_string(),
                count: 1,
            }),
        }
        let reached = s
            .dictionary_candidates
            .iter()
            .find(|c| c.word.eq_ignore_ascii_case(&key))
            .map(|c| c.count >= SUGGEST_THRESHOLD)
            .unwrap_or(false);
        if reached {
            s.dictionary_candidates
                .retain(|c| !c.word.eq_ignore_ascii_case(&key));
            suggest = true;
        }
        // Prune: keep the highest-count candidates only.
        if s.dictionary_candidates.len() > MAX_CANDIDATES {
            s.dictionary_candidates
                .sort_by(|a, b| b.count.cmp(&a.count));
            s.dictionary_candidates.truncate(MAX_CANDIDATES);
        }
    });

    if suggest {
        log::info!("[auto-dict] '{word}' reached threshold → suggesting via pill");
        crate::bridge::emit(
            app,
            grain_core::DaemonEvent::DictionarySuggestion {
                word: word.to_string(),
            },
        );
    } else {
        log::info!("[auto-dict] recorded correction '{word}' (below threshold)");
    }
    suggest
}

/// Accept a suggested word: add it to `custom_words` (deduped) and clear the pill
/// prompt. Called from the WS back-channel when the user clicks the pill.
pub fn accept_word(ctx: &AppContext, word: &str) {
    let word = word.trim().to_string();
    if word.is_empty() {
        return;
    }
    let _ = ctx.update_settings(|s| {
        if !s.custom_words.iter().any(|w| w.eq_ignore_ascii_case(&word)) {
            s.custom_words.push(word.clone());
        }
        s.dictionary_candidates
            .retain(|c| !c.word.eq_ignore_ascii_case(&word));
    });
    ctx.emit(grain_core::DaemonEvent::DictionarySuggestionClear);
    log::info!("[GRAIN] auto-dictionary: added '{word}' to custom words");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_proper_noun_respelling() {
        // Pasted "reeta"; user fixed it to "Rita".
        let got = detect_corrections(
            "please tell reeta about the meeting tomorrow",
            "please tell Rita about the meeting tomorrow",
        );
        assert_eq!(got, vec!["Rita"]);
    }

    #[test]
    fn ignores_common_word_edits() {
        // "teh" → "the" is a fix but "the" is not learnable.
        let got = detect_corrections("i saw teh dog run fast", "i saw the dog run fast");
        assert!(got.is_empty());
    }

    #[test]
    fn ignores_unrelated_replacement() {
        // "dog" → "Rembrandt" is not a spelling fix (too dissimilar).
        let got = detect_corrections("i saw the dog run fast", "i saw the Rembrandt run fast");
        assert!(got.is_empty());
    }

    #[test]
    fn handles_two_words_merged_into_one() {
        // "use effect" → "useEffect".
        let got = detect_corrections(
            "call the use effect hook here please",
            "call the useEffect hook here please",
        );
        assert_eq!(got, vec!["useEffect"]);
    }

    #[test]
    fn ignores_edits_when_field_no_longer_holds_paste() {
        // Current shares almost nothing with the paste → user moved on.
        let got = detect_corrections(
            "please tell reeta about the meeting",
            "completely different unrelated Rita text here now",
        );
        assert!(got.is_empty());
    }

    #[test]
    fn ignores_surrounding_pre_existing_text() {
        // Prefix/suffix that wasn't part of our paste must not trip detection.
        let got = detect_corrections(
            "meet Grayson at noon",
            "Hey team, meet Greyson at noon — thanks!",
        );
        assert_eq!(got, vec!["Greyson"]);
    }
}
