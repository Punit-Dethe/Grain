//! [GRAIN] The ASR-text substrate: normalisation, tokenisation, and the fuzzy
//! matching that absorbs what the acoustic model does to speech.
//!
//! Extracted from `action_router` so it can outlive it. The V1 template matcher
//! in `action_router` is one consumer; the V2 capability retriever
//! (`capability_index`) is another, and both must agree — byte-for-byte — about
//! what counts as "the same word heard slightly wrong", or a token filed one way
//! and matched another silently drops candidates. Keeping the definition in one
//! place is the only way that agreement is enforced rather than hoped for.
//!
//! Everything here is pure, allocation-light, and model-free. It runs on the
//! felt path and inside the eval harness with no running app.

/// Lowercase, strip punctuation, collapse whitespace.
///
/// Deliberately crude. This runs on ASR output, which arrives without reliable
/// punctuation anyway, and every transformation here is one more place a declared
/// phrase and the spoken one can disagree.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

/// Split normalised text into non-empty tokens.
pub fn tokens(text: &str) -> Vec<&str> {
    text.split(' ').filter(|t| !t.is_empty()).collect()
}

/// Do two tokens count as the same word, allowing for what the acoustic model
/// does to short common words?
///
/// ASR substitutes rather than omits — "skip" arrives as "skit", "next" as
/// "nex" — and those substitutions pass every grammar check, which is what makes
/// them dangerous. Exact token equality throws the whole utterance away for one
/// mangled character; production voice systems all sit somewhere on this
/// spectrum, with Apple's phonetically-augmented rescoring at the far end.
///
/// This is the cheap end deliberately: bounded edit distance, no phonetic table,
/// no dependency. It is a **recall** aid only. Phonetic keying (Double Metaphone)
/// is the next rung and is named, not built.
pub fn same_word(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Short words are where a one-character edit changes the meaning entirely
    // ("on"/"in", "to"/"do"), so they get no tolerance at all.
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.len() < 4 || long.len() - short.len() > 1 {
        return false;
    }
    edit_distance_at_most_one(short, long)
}

/// Keys a token is filed under, so an inverted-index lookup can find every token
/// within [`same_word`]'s tolerance without scanning.
///
/// The **deletion neighbourhood** (SymSpell's construction): file a token under
/// itself and under each of its single-character deletions. Two strings within
/// one edit always share a key, so one hash lookup per key finds every fuzzy
/// match — no scan, and no enumerating 26 substitutions per position.
///
/// Tokens shorter than four characters get no tolerance in [`same_word`], so they
/// are filed under themselves alone. The two functions must agree, or the index
/// prunes away candidates the matcher would have accepted.
pub fn fuzzy_keys(token: &str) -> Vec<String> {
    let mut keys = vec![token.to_string()];
    if token.len() >= 4 && token.is_ascii() {
        for skip in 0..token.len() {
            let mut variant = String::with_capacity(token.len() - 1);
            variant.push_str(&token[..skip]);
            variant.push_str(&token[skip + 1..]);
            keys.push(variant);
        }
    }
    keys
}

/// True when `a` becomes `b` with at most one insertion, deletion or
/// substitution. Bounded at one on purpose — two edits on a four-letter word is
/// a different word.
fn edit_distance_at_most_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() == b.len() {
        let mut differences = 0;
        for i in 0..a.len() {
            if a[i] != b[i] {
                differences += 1;
                if differences > 1 {
                    return false;
                }
            }
        }
        return differences == 1;
    }
    // Exactly one insertion: walk both, allowing a single skip in the longer.
    let (mut i, mut j, mut skipped) = (0usize, 0usize, false);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
            j += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_survives_what_asr_actually_produces() {
        // No reliable punctuation, inconsistent case, stray spacing.
        assert_eq!(normalise("Skip this, please."), "skip this please");
        assert_eq!(normalise("  NEXT   song  "), "next song");
        assert_eq!(normalise("!!!"), "");
    }

    #[test]
    fn short_words_get_no_spelling_tolerance() {
        // One edit turns "on" into "in" and "to" into "do" — different words,
        // not misheard ones. Tolerance there would be a false-execution source.
        assert!(!same_word("on", "in"));
        assert!(!same_word("to", "do"));
        assert!(same_word("skip", "skit"));
        assert!(same_word("playlist", "playlst"));
        // Two edits is a different word even when it is long.
        assert!(!same_word("playlist", "plarlsst"));
    }

    #[test]
    fn a_token_and_its_single_deletions_share_a_fuzzy_key() {
        // The index/matcher contract: any two tokens within one edit must share
        // at least one key, or the postings prune a candidate the matcher accepts.
        let keys = fuzzy_keys("playlist");
        assert!(keys.contains(&"playlist".to_string()));
        // "playlst" is "playlist" with the 'i' deleted; both list "playlst".
        assert!(keys.contains(&"playlst".to_string()));
        // Short tokens are filed under themselves alone.
        assert_eq!(fuzzy_keys("on"), vec!["on".to_string()]);
    }
}
