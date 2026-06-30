//! SAPrefix transcript stabilizer — the partial/commit policy layer.
//!
//! A streaming backend rewrites its hypothesis many times a second. The UI needs
//! the opposite: a growing prefix of text that is *guaranteed never to change*
//! (so it can be pasted/saved), plus a volatile tail that may still move. This
//! module is that bridge. It is a pure policy — no backend, no audio, no I/O.
//!
//! ## Why SAPrefix (and not plain LocalAgreement)
//!
//! LocalAgreement commits the longest *exactly* common prefix of the last two
//! hypotheses. That's brittle for ASR: a backend that flips `O3`↔`03`,
//! `three`↔`3`, casing, or trailing punctuation between revisions would keep
//! re-stating the boundary and stall the commit. SAPrefix is the softer policy
//! the plan locks in: compare words with *normalized Levenshtein similarity*
//! (punctuation/case tolerant) so cosmetic drift still counts as agreement, and
//! only genuine word changes block the commit.
//!
//! ## Commitment rule
//!
//! For each new hypothesis we compute the similarity-agreed prefix against the
//! previous hypothesis, then commit the agreed words that lie beyond what is
//! already committed (optionally holding back the last [`StabilizerConfig::commit_lag`]
//! agreed words as still-volatile). Committed words are immutable forever — the
//! agreed prefix can only grow. A `BackendFinal` (when the model promises
//! immutability) or an `Endpoint`/`finish` closes the segment and commits the
//! remaining tail, since the audio for it is fully consumed.

use crate::events::{AsrEvent, AsrRawEvent, AsrWord, Stability};
use crate::model::AsrCapabilities;

/// Punctuation stripped from word ends for comparison — matches the
/// rolling-window assembler's normalization so the two stay consistent.
const PUNCTUATION: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// Tuning for the stabilizer.
#[derive(Clone, Copy, Debug)]
pub struct StabilizerConfig {
    /// Normalized-Levenshtein similarity (0..=1) at/above which two words are
    /// treated as "the same" for prefix agreement. Lower = commits faster but
    /// risks committing a word that was about to change.
    pub word_similarity: f64,
    /// How many trailing agreed words to hold back as volatile instead of
    /// committing immediately. `0` = classic LocalAgreement-2 (commit the whole
    /// agreed prefix). `1` adds a one-word safety margin at the boundary.
    pub commit_lag: usize,
}

impl Default for StabilizerConfig {
    fn default() -> Self {
        Self {
            word_similarity: 0.8,
            commit_lag: 0,
        }
    }
}

/// Normalize a token for comparison: strip surrounding punctuation, lowercase.
fn norm(word: &str) -> String {
    word.trim_matches(|c| PUNCTUATION.contains(c)).to_lowercase()
}

/// Whether two raw tokens should count as the same word for prefix agreement.
fn words_similar(a: &str, b: &str, threshold: f64) -> bool {
    let (na, nb) = (norm(a), norm(b));
    if na == nb {
        return true;
    }
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    strsim::normalized_levenshtein(&na, &nb) >= threshold
}

/// Length of the leading run where `a` and `b` agree word-for-word (similarity).
fn agreed_prefix_len(a: &[String], b: &[String], threshold: f64) -> usize {
    let mut i = 0;
    while i < a.len() && i < b.len() && words_similar(&a[i], &b[i], threshold) {
        i += 1;
    }
    i
}

fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// Turns a per-session stream of [`AsrRawEvent`]s into stabilized [`AsrEvent`]s.
///
/// Drive it with [`ingest`](Self::ingest) per raw event, then call
/// [`session_final`](Self::session_final) exactly once when the session ends.
pub struct TranscriptStabilizer {
    session_id: u64,
    caps: AsrCapabilities,
    config: StabilizerConfig,

    /// Segment currently being built. Set on the first event of a segment.
    cur_segment: Option<u64>,
    /// Immutable, already-committed words of the current segment.
    committed: Vec<String>,
    /// The previous hypothesis's full word list (current segment), for agreement.
    prev: Vec<String>,

    /// Finalized segment texts in order, for [`session_final`](Self::session_final).
    finals: Vec<String>,
}

impl TranscriptStabilizer {
    pub fn new(session_id: u64, caps: AsrCapabilities, config: StabilizerConfig) -> Self {
        Self {
            session_id,
            caps,
            config,
            cur_segment: None,
            committed: Vec::new(),
            prev: Vec::new(),
            finals: Vec::new(),
        }
    }

    /// Process one raw event, returning zero or more UI-safe events.
    pub fn ingest(&mut self, raw: AsrRawEvent) -> Vec<AsrEvent> {
        match raw {
            AsrRawEvent::Partial {
                segment_id,
                revision,
                text,
                ..
            } => self.on_partial(segment_id, revision, text),
            AsrRawEvent::BackendFinal {
                segment_id,
                text,
                words,
            } => self.on_backend_final(segment_id, text, words),
            AsrRawEvent::Endpoint { segment_id, .. } => self.on_endpoint(segment_id),
            AsrRawEvent::Error {
                recoverable,
                message,
            } => vec![AsrEvent::Error {
                session_id: self.session_id,
                recoverable,
                message,
            }],
        }
    }

    /// Reset per-segment state when a new segment id appears.
    fn begin_segment_if_new(&mut self, segment_id: u64) {
        if self.cur_segment != Some(segment_id) {
            self.cur_segment = Some(segment_id);
            self.committed.clear();
            self.prev.clear();
        }
    }

    fn on_partial(&mut self, segment_id: u64, revision: u64, text: String) -> Vec<AsrEvent> {
        self.begin_segment_if_new(segment_id);
        let words = tokenize(&text);
        let mut out = Vec::new();

        // Similarity-agreed prefix between the previous and current hypotheses.
        let agreed = agreed_prefix_len(&self.prev, &words, self.config.word_similarity);

        // Commit agreed words beyond what is already committed, holding back
        // `commit_lag` trailing agreed words. Never un-commit (prefix only grows).
        let commit_upto = agreed.saturating_sub(self.config.commit_lag);
        if commit_upto > self.committed.len() {
            let new_words: Vec<String> = words[self.committed.len()..commit_upto].to_vec();
            self.committed.extend(new_words.iter().cloned());
            out.push(AsrEvent::Commit {
                session_id: self.session_id,
                segment_id,
                text: new_words.join(" "),
                // Partial revisions carry no trustworthy per-word timing; the
                // authoritative timed words land on SegmentFinal.
                words: Vec::new(),
            });
        }

        // The volatile tail is everything past the committed prefix. It is
        // `Stable` only when the whole hypothesis is within the agreed region
        // (i.e. nothing beyond agreement) and we are merely holding by lag.
        let tail = if self.committed.len() < words.len() {
            words[self.committed.len()..].join(" ")
        } else {
            String::new()
        };
        let stability = if words.len() <= agreed {
            Stability::Stable
        } else {
            Stability::Volatile
        };
        out.push(AsrEvent::Partial {
            session_id: self.session_id,
            segment_id,
            revision,
            text: tail,
            stability,
        });

        self.prev = words;
        out
    }

    fn on_backend_final(
        &mut self,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    ) -> Vec<AsrEvent> {
        self.begin_segment_if_new(segment_id);
        let toks = tokenize(&text);
        let mut out = Vec::new();

        // Trust an immutable final verbatim; otherwise the final still wins over
        // partials because its audio is fully consumed — either way commit the
        // remaining tail beyond what's already committed.
        if toks.len() > self.committed.len() {
            let new_words: Vec<String> = toks[self.committed.len()..].to_vec();
            out.push(AsrEvent::Commit {
                session_id: self.session_id,
                segment_id,
                text: new_words.join(" "),
                words: Vec::new(),
            });
        }
        // Note: caps.immutable_final currently gates nothing extra because both
        // branches commit the tail; it exists so a future non-immutable backend
        // can instead re-run agreement here without touching the call sites.
        let _ = self.caps.immutable_final;

        out.push(self.finalize_segment(segment_id, text, words));
        out
    }

    fn on_endpoint(&mut self, segment_id: u64) -> Vec<AsrEvent> {
        self.begin_segment_if_new(segment_id);
        // No explicit final: finalize from the last hypothesis. Audio for the
        // segment is done, so the whole last hypothesis is safe to commit.
        let text = self.prev.join(" ");
        let mut out = Vec::new();
        if self.prev.len() > self.committed.len() {
            let new_words: Vec<String> = self.prev[self.committed.len()..].to_vec();
            out.push(AsrEvent::Commit {
                session_id: self.session_id,
                segment_id,
                text: new_words.join(" "),
                words: Vec::new(),
            });
        }
        out.push(self.finalize_segment(segment_id, text, Vec::new()));
        out
    }

    /// Close the current segment: record its text for the session final and
    /// reset per-segment state. Returns the `SegmentFinal` event.
    fn finalize_segment(
        &mut self,
        segment_id: u64,
        text: String,
        words: Vec<AsrWord>,
    ) -> AsrEvent {
        if !text.trim().is_empty() {
            self.finals.push(text.clone());
        }
        self.committed.clear();
        self.prev.clear();
        self.cur_segment = None;
        AsrEvent::SegmentFinal {
            session_id: self.session_id,
            segment_id,
            text,
            words,
        }
    }

    /// Close the session. Finalizes any segment still open (no trailing
    /// `BackendFinal`/`Endpoint` was seen), then emits the concatenated final.
    pub fn session_final(&mut self) -> AsrEvent {
        if let Some(seg) = self.cur_segment {
            let text = self.prev.join(" ");
            let _ = self.finalize_segment(seg, text, Vec::new());
        }
        AsrEvent::SessionFinal {
            session_id: self.session_id,
            text: self.finals.join(" "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> AsrCapabilities {
        AsrCapabilities::streaming_minimal()
    }

    fn partial(seg: u64, rev: u64, text: &str) -> AsrRawEvent {
        AsrRawEvent::Partial {
            segment_id: seg,
            revision: rev,
            text: text.to_string(),
            words: Vec::new(),
        }
    }

    /// Pull the committed text deltas out of a batch of events, in order.
    fn commits(events: &[AsrEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                AsrEvent::Commit { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn last_partial(events: &[AsrEvent]) -> Option<(String, Stability)> {
        events.iter().rev().find_map(|e| match e {
            AsrEvent::Partial {
                text, stability, ..
            } => Some((text.clone(), *stability)),
            _ => None,
        })
    }

    #[test]
    fn commits_growing_agreed_prefix() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        // First hypothesis: nothing to agree with yet → no commit, all volatile.
        let e0 = s.ingest(partial(0, 0, "hello"));
        assert!(commits(&e0).is_empty());
        assert_eq!(last_partial(&e0), Some(("hello".into(), Stability::Volatile)));

        // "hello" now agrees with the previous hypothesis → commits.
        let e1 = s.ingest(partial(0, 1, "hello world"));
        assert_eq!(commits(&e1), vec!["hello".to_string()]);
        // "world" is new/volatile (not yet agreed).
        assert_eq!(last_partial(&e1), Some(("world".into(), Stability::Volatile)));

        // "world" now agrees → commits; tail empty and fully agreed → Stable.
        let e2 = s.ingest(partial(0, 2, "hello world"));
        assert_eq!(commits(&e2), vec!["world".to_string()]);
        assert_eq!(last_partial(&e2), Some((String::new(), Stability::Stable)));
    }

    #[test]
    fn tolerates_punctuation_and_case_drift() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "Hello, world"));
        // Casing/punctuation differ but the words are the "same" → both commit.
        // We commit from the freshest hypothesis, so the latest spelling wins.
        let e = s.ingest(partial(0, 1, "hello world."));
        assert_eq!(commits(&e), vec!["hello world.".to_string()]);
    }

    #[test]
    fn unstable_substitution_blocks_commit_past_divergence() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "the quick brown fox"));
        // "brown"→"bright" diverges at index 2: only "the quick" may commit.
        let e = s.ingest(partial(0, 1, "the quick bright fox"));
        assert_eq!(commits(&e), vec!["the quick".to_string()]);
        let (tail, stab) = last_partial(&e).unwrap();
        assert_eq!(tail, "bright fox");
        assert_eq!(stab, Stability::Volatile);
    }

    #[test]
    fn repeated_words_commit_in_order() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "go go go"));
        let e = s.ingest(partial(0, 1, "go go go now"));
        // All three "go"s agree → committed; "now" stays volatile.
        assert_eq!(commits(&e).join(" "), "go go go");
        assert_eq!(last_partial(&e), Some(("now".into(), Stability::Volatile)));
    }

    #[test]
    fn backend_final_after_partials_commits_tail_and_finalizes() {
        let mut s = TranscriptStabilizer::new(7, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "hello"));
        s.ingest(partial(0, 1, "hello there")); // commits "hello"
        let words = vec![AsrWord {
            text: "friend".into(),
            start_ms: 0,
            end_ms: 0,
            confidence: None,
        }];
        let e = s.ingest(AsrRawEvent::BackendFinal {
            segment_id: 0,
            text: "hello there friend".into(),
            words: words.clone(),
        });
        // "there friend" was uncommitted → committed by the final.
        assert_eq!(commits(&e), vec!["there friend".to_string()]);
        match e.last().unwrap() {
            AsrEvent::SegmentFinal { text, words: w, .. } => {
                assert_eq!(text, "hello there friend");
                assert_eq!(w, &words);
            }
            other => panic!("expected SegmentFinal, got {other:?}"),
        }
    }

    #[test]
    fn endpoint_without_final_finalizes_from_last_hypothesis() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "good morning"));
        s.ingest(partial(0, 1, "good morning")); // commits both
        let e = s.ingest(AsrRawEvent::Endpoint {
            segment_id: 0,
            reason: crate::events::EndpointReason::Backend,
            audio_end_ms: Some(1234),
        });
        match e.last().unwrap() {
            AsrEvent::SegmentFinal { text, .. } => assert_eq!(text, "good morning"),
            other => panic!("expected SegmentFinal, got {other:?}"),
        }
    }

    #[test]
    fn session_final_concatenates_segments() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        // Segment 0.
        s.ingest(partial(0, 0, "first segment"));
        s.ingest(AsrRawEvent::BackendFinal {
            segment_id: 0,
            text: "first segment".into(),
            words: Vec::new(),
        });
        // Segment 1.
        s.ingest(partial(1, 0, "second segment"));
        s.ingest(AsrRawEvent::BackendFinal {
            segment_id: 1,
            text: "second segment".into(),
            words: Vec::new(),
        });
        match s.session_final() {
            AsrEvent::SessionFinal { text, .. } => {
                assert_eq!(text, "first segment second segment")
            }
            other => panic!("expected SessionFinal, got {other:?}"),
        }
    }

    #[test]
    fn session_final_flushes_open_segment() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        s.ingest(partial(0, 0, "no explicit final here"));
        s.ingest(partial(0, 1, "no explicit final here"));
        // No BackendFinal/Endpoint — session_final must still flush it.
        match s.session_final() {
            AsrEvent::SessionFinal { text, .. } => assert_eq!(text, "no explicit final here"),
            other => panic!("expected SessionFinal, got {other:?}"),
        }
    }

    // -- M8 hardening ------------------------------------------------------

    /// Committed text is immutable: across an adversarial stream where the
    /// hypothesis grows, shrinks, and substitutes words, the running committed
    /// string only ever GROWS by appended deltas — it never changes or shrinks.
    #[test]
    fn committed_text_never_flickers() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        let hyps = [
            "the",
            "the quick",
            "the quick brown",
            "the quick brown",       // stable → commits
            "the quick brown fox",
            "the quick green",       // tail rewritten (brown→green): already-committed prefix must stand
            "the quick brown fox jumps",
            "the quick brown fox jumps over",
            "the quick brown fox jumps over",
        ];
        let mut committed = String::new();
        for (rev, h) in hyps.iter().enumerate() {
            let evs = s.ingest(partial(0, rev as u64, h));
            for c in commits(&evs) {
                let prev = committed.clone();
                if !committed.is_empty() {
                    committed.push(' ');
                }
                committed.push_str(&c);
                // The invariant: every commit only EXTENDS the committed text.
                assert!(
                    committed.starts_with(&prev),
                    "commit must extend, not rewrite: {prev:?} -> {committed:?}"
                );
            }
        }
        // And the committed text is a real prefix of the last stable hypothesis.
        assert!("the quick brown fox jumps over".starts_with(&committed));
    }

    /// Silence / empty hypotheses never commit and yield an empty session final.
    #[test]
    fn no_speech_produces_no_commit_and_empty_final() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        for rev in 0..6 {
            let e = s.ingest(partial(0, rev, "   "));
            assert!(commits(&e).is_empty(), "silence must not commit");
        }
        match s.session_final() {
            AsrEvent::SessionFinal { text, .. } => assert!(text.is_empty()),
            other => panic!("expected empty SessionFinal, got {other:?}"),
        }
    }

    /// A long multi-segment session stays correct and bounded: per-segment state
    /// resets on each finalize, so memory does not grow with utterance count.
    #[test]
    fn long_multi_segment_session_is_correct() {
        let mut s = TranscriptStabilizer::new(1, caps(), StabilizerConfig::default());
        const SEGMENTS: u64 = 500;
        for seg in 0..SEGMENTS {
            s.ingest(partial(seg, 0, "word one"));
            s.ingest(partial(seg, 1, "word one")); // agree → commit
            s.ingest(AsrRawEvent::BackendFinal {
                segment_id: seg,
                text: "word one".into(),
                words: Vec::new(),
            });
        }
        match s.session_final() {
            AsrEvent::SessionFinal { text, .. } => {
                assert_eq!(text.split_whitespace().count() as u64, SEGMENTS * 2);
            }
            other => panic!("expected SessionFinal, got {other:?}"),
        }
    }

    #[test]
    fn commit_lag_holds_back_trailing_word() {
        let cfg = StabilizerConfig {
            commit_lag: 1,
            ..Default::default()
        };
        let mut s = TranscriptStabilizer::new(1, caps(), cfg);
        s.ingest(partial(0, 0, "alpha beta"));
        // Agreed = [alpha, beta] (len 2); lag 1 → commit only "alpha".
        let e = s.ingest(partial(0, 1, "alpha beta"));
        assert_eq!(commits(&e), vec!["alpha".to_string()]);
        // Whole hypothesis is within agreement → held tail is Stable.
        assert_eq!(last_partial(&e), Some(("beta".into(), Stability::Stable)));
    }
}
