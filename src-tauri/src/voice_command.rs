//! [GRAIN] Voice commands — an inline wake-phrase interceptor on the LIVE
//! streaming transcript (Rolling + Native ASR). It is NOT an engine: no thread,
//! no acoustic model, no per-frame work. It reads the transcript the streaming
//! worker already produces and drives a tiny state machine, exactly like the
//! `scrap that` reset reuses the snippet matcher.
//!
//! ## The wake gesture
//!
//! The user says an anchor + keyword ("hey grain") mid-dictation. That arms a
//! short listening window (the pill goes yellow). The NEXT word(s) resolve it:
//!
//! - `{switch,change} {prompt,profile}` → **Switch**: bring up the prompt
//!   switcher; arrow keys cycle it. The "hey grain switch prompt" span is
//!   scrubbed from the transcript, and speech before AND after it is kept.
//! - anything else → **Record**: everything spoken after the wake phrase is an
//!   AI instruction (the existing Prompt Record path, but voice-triggered). The
//!   split is done in TEXT space at the wake phrase — no audio re-slice, no
//!   second STT pass — because we detected the phrase in the text to begin with.
//!
//! ## Two-layer design (why the split is recomputed at the end)
//!
//! The live detector's job is only to decide the *kind* (Switch vs Record) and
//! drive the pill's colour transitions as text streams in. The actual text
//! surgery runs ONCE on the FINAL assembled transcript via [`apply_to_final`],
//! because the rolling assembler retro-edits committed punctuation (so a byte
//! offset captured mid-stream can drift). Re-finding the phrase on the final
//! text is robust to that.
//!
//! ## Why phonetic
//!
//! The dictionary can't help (it runs on the FINAL transcript, post-hoc), and
//! low-parameter ASR models drift a short keyword toward common words: "grain"
//! → green / grin / grane. So the keyword is matched on a consonant-skeleton
//! phonetic key (grain / green / grin / grane all fold to `grn`; brain → `brn`,
//! correctly excluded), backstopped by onset-guarded Jaro-Winkler similarity.
//! The anchor ("hey") is transcribed reliably and gates the whole match, so
//! false positives on a bare "green" in normal speech stay near-zero.

use crate::audio_toolkit::snippets::{normalize, tokenize, Token};

/// Command verbs that (with a noun) mean "open the prompt switcher".
const SWITCH_VERBS: &[&str] = &["switch", "change", "next", "previous"];
/// Command nouns that pair with a verb to mean "prompt switcher".
const SWITCH_NOUNS: &[&str] = &["prompt", "profile", "preset", "mode"];

/// Jaro-Winkler similarity at or above which two normalized tokens are treated
/// as the same word. The phonetic key is the primary signal; this is the
/// onset-guarded backstop for wobble it misses.
const FUZZY_THRESHOLD: f64 = 0.86;

/// Once this many meaningful tokens follow the wake phrase without forming a
/// switch command, the gesture is committed to Record. Two is enough to rule out
/// a `verb noun` command while keeping the yellow→blue transition snappy.
const RESOLVE_AFTER_TOKENS: usize = 2;

// ── Phonetic matching ───────────────────────────────────────────────────────

/// Consonant-skeleton phonetic key: lowercase alphanumerics, a single leading
/// vowel folded to `a` (so "okay" keeps its onset while "grain" does not sprout
/// one), interior vowels dropped, doubled letters collapsed, silent `h` after
/// the onset removed. Cheap, allocation-light, deterministic.
///
/// Examples: grain/green/grin/grane → `grn`; brain → `brn`; hey → `ha`.
fn phonetic_key(word: &str) -> String {
    let norm = normalize(word);
    let mut out = String::with_capacity(norm.len());
    let mut prev = '\0';
    for (i, c) in norm.chars().enumerate() {
        let is_vowel = matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        let mapped = if is_vowel {
            if i == 0 {
                'a'
            } else {
                prev = c;
                continue;
            }
        } else if c == 'h' && i != 0 {
            prev = c;
            continue;
        } else {
            c
        };
        if mapped != prev {
            out.push(mapped);
        }
        prev = mapped;
    }
    out
}

/// True when `token` sounds like `target` (same phonetic key, or onset-guarded
/// high edit similarity as a fallback). Both compared on their normalized forms.
fn sounds_like(token: &str, target: &str) -> bool {
    let t = normalize(token);
    let g = normalize(target);
    if t.is_empty() || g.is_empty() {
        return false;
    }
    if t == g {
        return true;
    }
    if phonetic_key(&t) == phonetic_key(&g) {
        return true;
    }
    // Edit-distance backstop for wobble the skeleton misses, but only when the
    // onset agrees — the first sound is the most reliably transcribed, and
    // requiring it keeps near-rhymes ("brain" vs "grain") apart even though only
    // one letter differs.
    t.chars().next() == g.chars().next() && strsim::jaro_winkler(&t, &g) >= FUZZY_THRESHOLD
}

/// True when `token` matches any word in `set`.
fn matches_any(token: &str, set: &[&str]) -> bool {
    set.iter().any(|w| sounds_like(token, w))
}

// ── Wake phrase + reusable matching primitives ──────────────────────────────

/// The wake phrase, pre-split into its anchor word and keyword word(s). Built
/// once from the configured phrase; cheap to clone into a per-session detector.
#[derive(Clone, Debug)]
pub struct WakePhrase {
    anchor: String,
    keywords: Vec<String>,
}

impl WakePhrase {
    /// Parse a configured phrase ("hey grain") into anchor ("hey") + keywords
    /// (["grain"]). A single-word phrase is treated as a keyword with the
    /// default "hey" anchor, so a bare keyword still needs the anchor to fire.
    pub fn parse(phrase: &str) -> Self {
        let mut words: Vec<String> = phrase.split_whitespace().map(|w| w.to_string()).collect();
        if words.len() >= 2 {
            let anchor = words.remove(0);
            WakePhrase {
                anchor,
                keywords: words,
            }
        } else {
            WakePhrase {
                anchor: "hey".to_string(),
                keywords: if words.is_empty() {
                    vec!["grain".to_string()]
                } else {
                    words
                },
            }
        }
    }
}

impl Default for WakePhrase {
    fn default() -> Self {
        WakePhrase::parse("hey grain")
    }
}

/// Index of the next token at or after `from` that carries characters.
fn next_meaningful(tokens: &[Token], from: usize) -> Option<usize> {
    (from..tokens.len()).find(|&k| !tokens[k].norm.is_empty())
}

/// A located wake-phrase occurrence.
struct WakeHit {
    /// Byte offset where the phrase begins (the anchor token's start).
    start: usize,
    /// Byte offset just past the last keyword token.
    kw_end: usize,
    /// Token index just past the phrase (where the follow-up begins).
    after_token: usize,
}

/// Try to match anchor + all keyword tokens starting at token `i`, skipping
/// interior pure-punctuation tokens the way the snippet matcher does.
fn match_wake_at(tokens: &[Token], i: usize, phrase: &WakePhrase) -> Option<WakeHit> {
    let start = tokens.get(i)?.start;
    let mut j = next_meaningful(tokens, i)?;
    if j != i || !sounds_like(&tokens[j].norm, &phrase.anchor) {
        return None;
    }
    let mut last_end = tokens[j].end;
    j += 1;
    for kw in &phrase.keywords {
        let k = next_meaningful(tokens, j)?;
        if !sounds_like(&tokens[k].norm, kw) {
            return None;
        }
        last_end = tokens[k].end;
        j = k + 1;
    }
    Some(WakeHit {
        start,
        kw_end: last_end,
        after_token: j,
    })
}

/// Locate the LAST wake-phrase occurrence in `tokens` (a user may re-issue the
/// gesture later in the same utterance).
fn find_last_wake(tokens: &[Token], phrase: &WakePhrase) -> Option<WakeHit> {
    let mut hit = None;
    let mut i = 0;
    while i < tokens.len() {
        if let Some(h) = match_wake_at(tokens, i, phrase) {
            i = h.after_token;
            hit = Some(h);
        } else {
            i += 1;
        }
    }
    hit
}

// ── Live state machine ──────────────────────────────────────────────────────

/// The machine's externally observable phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Armed,
    Resolved,
}

/// How an armed gesture resolved. Stored by the caller so the FINAL transcript
/// can be processed the matching way at session end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Prompt switcher: scrub the "hey grain switch prompt" span, keep the rest.
    Switch,
    /// Prompt Record: split content (before) from the spoken instruction (after).
    Record,
}

/// What the caller should do in response to feeding the latest transcript. The
/// byte offsets are best-effort hints for the LIVE preview only; the paste-time
/// split is recomputed on the final text by [`apply_to_final`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WakeEvent {
    None,
    /// Wake phrase just heard → yellow listening state. `scrub_from` marks where
    /// the phrase begins.
    Armed {
        scrub_from: usize,
    },
    /// Resolved to a prompt-switch command.
    Switch,
    /// Resolved to Prompt Record.
    Record,
}

/// Per-session wake detector. Fed the growing transcript on each streaming
/// update; holds only a few bytes of state between calls.
pub struct WakeDetector {
    phrase: WakePhrase,
    phase: Phase,
    resolution: Option<Resolution>,
    /// Token index just past the wake phrase, so we inspect only what follows.
    after_token: usize,
}

impl WakeDetector {
    pub fn new(phrase: WakePhrase) -> Self {
        WakeDetector {
            phrase,
            phase: Phase::Idle,
            resolution: None,
            after_token: 0,
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.phase == Phase::Resolved
    }

    pub fn is_armed(&self) -> bool {
        self.phase == Phase::Armed
    }

    /// The resolution kind, once the gesture has resolved.
    pub fn resolution(&self) -> Option<Resolution> {
        self.resolution
    }

    /// Feed the current full transcript (committed text, optionally plus the
    /// tentative tail). Returns the transition to act on, if any.
    pub fn observe(&mut self, text: &str) -> WakeEvent {
        match self.phase {
            Phase::Resolved => WakeEvent::None,
            Phase::Idle => self.scan_for_wake(text),
            Phase::Armed => self.resolve(text),
        }
    }

    /// Called when the session ends while still armed (the user said the wake
    /// phrase but nothing decisive followed). Commit to Record so a trailing
    /// instruction, if any, is honored; an empty instruction degrades to a
    /// normal paste downstream.
    pub fn resolve_on_stop(&mut self) -> WakeEvent {
        if self.phase == Phase::Armed {
            self.phase = Phase::Resolved;
            self.resolution = Some(Resolution::Record);
            WakeEvent::Record
        } else {
            WakeEvent::None
        }
    }

    fn scan_for_wake(&mut self, text: &str) -> WakeEvent {
        let tokens = tokenize(text);
        match find_last_wake(&tokens, &self.phrase) {
            Some(hit) => {
                self.phase = Phase::Armed;
                self.after_token = hit.after_token;
                WakeEvent::Armed {
                    scrub_from: hit.start,
                }
            }
            None => WakeEvent::None,
        }
    }

    fn resolve(&mut self, text: &str) -> WakeEvent {
        let tokens = tokenize(text);
        let after: Vec<&str> = tokens
            .iter()
            .skip(self.after_token)
            .filter(|t| !t.norm.is_empty())
            .map(|t| t.norm.as_str())
            .collect();

        if let Some(first) = after.first() {
            if matches_any(first, SWITCH_VERBS) {
                match after.get(1) {
                    Some(second) if matches_any(second, SWITCH_NOUNS) => {
                        return self.commit(Resolution::Switch, WakeEvent::Switch);
                    }
                    // Verb heard but the second word isn't a command noun → not a
                    // switch; it's dictation.
                    Some(_) => return self.commit(Resolution::Record, WakeEvent::Record),
                    // Only the verb so far — keep waiting for the noun.
                    None => return WakeEvent::None,
                }
            }
            // First follow-up isn't a command verb → dictation. Wait for
            // RESOLVE_AFTER_TOKENS words to avoid a one-token flicker, then Record.
            if after.len() >= RESOLVE_AFTER_TOKENS {
                return self.commit(Resolution::Record, WakeEvent::Record);
            }
        }
        WakeEvent::None
    }

    fn commit(&mut self, res: Resolution, event: WakeEvent) -> WakeEvent {
        self.phase = Phase::Resolved;
        self.resolution = Some(res);
        event
    }
}

// ── Final-transcript surgery ────────────────────────────────────────────────

/// The cleaned final transcript for a resolved gesture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleaned {
    /// Text to paste (wake phrase and any command words removed).
    pub content: String,
    /// The spoken AI instruction for Prompt Record, if any.
    pub instruction: Option<String>,
}

/// Process the FINAL assembled transcript for a resolved gesture. Re-finds the
/// wake phrase on the final text (robust to mid-stream retro-edits) and applies
/// the resolution:
///
/// - `Record` → content is everything before the phrase; instruction is
///   everything after it.
/// - `Switch` → the "hey grain switch prompt" span is removed and the text
///   before and after it is rejoined (dictation continues normally after a
///   switch).
///
/// If the phrase can't be found on the final text (a rare retro-edit erased it),
/// the transcript is returned unchanged as content — never silently dropped.
pub fn apply_to_final(text: &str, phrase: &WakePhrase, resolution: Resolution) -> Cleaned {
    let tokens = tokenize(text);
    let hit = match find_last_wake(&tokens, phrase) {
        Some(h) => h,
        None => {
            return Cleaned {
                content: text.trim().to_string(),
                instruction: None,
            }
        }
    };

    match resolution {
        Resolution::Record => {
            let content = text[..hit.start].trim().to_string();
            let instruction = text[hit.kw_end..].trim();
            Cleaned {
                content,
                instruction: (!instruction.is_empty()).then(|| instruction.to_string()),
            }
        }
        Resolution::Switch => {
            // Extend the scrub span across the command words (verb, then noun).
            let mut scrub_end = hit.kw_end;
            if let Some(v) = next_meaningful(&tokens, hit.after_token) {
                if matches_any(&tokens[v].norm, SWITCH_VERBS) {
                    scrub_end = tokens[v].end;
                    if let Some(n) = next_meaningful(&tokens, v + 1) {
                        if matches_any(&tokens[n].norm, SWITCH_NOUNS) {
                            scrub_end = tokens[n].end;
                        }
                    }
                }
            }
            let before = text[..hit.start].trim();
            let after = text[scrub_end..].trim();
            let content = match (before.is_empty(), after.is_empty()) {
                (true, _) => after.to_string(),
                (_, true) => before.to_string(),
                _ => format!("{before} {after}"),
            };
            Cleaned {
                content,
                instruction: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonetic_folds_grain_variants() {
        let k = phonetic_key("grain");
        assert_eq!(phonetic_key("green"), k);
        assert_eq!(phonetic_key("grin"), k);
        assert_eq!(phonetic_key("grane"), k);
        assert_ne!(phonetic_key("brain"), k);
    }

    #[test]
    fn sounds_like_matches_mistranscriptions() {
        assert!(sounds_like("green", "grain"));
        assert!(sounds_like("grane", "grain"));
        assert!(sounds_like("Grain,", "grain"));
        assert!(!sounds_like("brain", "grain"));
        assert!(!sounds_like("train", "grain"));
    }

    fn detector() -> WakeDetector {
        WakeDetector::new(WakePhrase::parse("hey grain"))
    }

    #[test]
    fn arms_on_wake_phrase() {
        let mut d = detector();
        assert_eq!(d.observe("write this down"), WakeEvent::None);
        match d.observe("write this down hey grain") {
            WakeEvent::Armed { scrub_from } => {
                assert_eq!(&"write this down hey grain"[scrub_from..], "hey grain");
            }
            e => panic!("expected Armed, got {e:?}"),
        }
        assert!(d.is_armed());
    }

    #[test]
    fn arms_through_mistranscribed_keyword() {
        let mut d = detector();
        assert!(matches!(
            d.observe("some notes hey green"),
            WakeEvent::Armed { .. }
        ));
    }

    #[test]
    fn resolves_switch_command() {
        let mut d = detector();
        assert!(matches!(
            d.observe("notes hey grain"),
            WakeEvent::Armed { .. }
        ));
        assert_eq!(
            d.observe("notes hey grain switch prompt"),
            WakeEvent::Switch
        );
        assert_eq!(d.resolution(), Some(Resolution::Switch));
    }

    #[test]
    fn resolves_switch_change_profile() {
        let mut d = detector();
        d.observe("a hey grain");
        assert_eq!(d.observe("a hey grain change profile"), WakeEvent::Switch);
    }

    #[test]
    fn resolves_record_on_free_speech() {
        let mut d = detector();
        d.observe("draft hey grain");
        assert_eq!(
            d.observe("draft hey grain make this formal"),
            WakeEvent::Record
        );
        assert_eq!(d.resolution(), Some(Resolution::Record));
    }

    #[test]
    fn switch_verb_then_noncommand_is_record() {
        let mut d = detector();
        d.observe("x hey grain");
        assert_eq!(
            d.observe("x hey grain change everything here"),
            WakeEvent::Record
        );
    }

    #[test]
    fn resolve_on_stop_when_only_wake_heard() {
        let mut d = detector();
        d.observe("hello hey grain");
        assert_eq!(d.resolve_on_stop(), WakeEvent::Record);
    }

    #[test]
    fn no_false_trigger_without_anchor() {
        let mut d = detector();
        assert_eq!(d.observe("the grain of the wood"), WakeEvent::None);
        assert_eq!(d.observe("green fields forever"), WakeEvent::None);
    }

    // ── apply_to_final ──────────────────────────────────────────────────────

    fn phrase() -> WakePhrase {
        WakePhrase::parse("hey grain")
    }

    #[test]
    fn final_record_splits_content_and_instruction() {
        let c = apply_to_final(
            "draft the memo hey grain make this formal and short",
            &phrase(),
            Resolution::Record,
        );
        assert_eq!(c.content, "draft the memo");
        assert_eq!(c.instruction.as_deref(), Some("make this formal and short"));
    }

    #[test]
    fn final_record_empty_instruction_is_none() {
        let c = apply_to_final("all my notes hey grain", &phrase(), Resolution::Record);
        assert_eq!(c.content, "all my notes");
        assert_eq!(c.instruction, None);
    }

    #[test]
    fn final_record_survives_boundary_punctuation() {
        // Rolling injected a comma at the seam: "grain," still matches.
        let c = apply_to_final(
            "some text hey grain, rewrite in past tense",
            &phrase(),
            Resolution::Record,
        );
        assert_eq!(c.content, "some text");
        assert_eq!(c.instruction.as_deref(), Some("rewrite in past tense"));
    }

    #[test]
    fn final_switch_scrubs_command_keeps_both_sides() {
        let c = apply_to_final(
            "first part hey grain switch prompt second part",
            &phrase(),
            Resolution::Switch,
        );
        assert_eq!(c.content, "first part second part");
        assert_eq!(c.instruction, None);
    }

    #[test]
    fn final_switch_at_start_keeps_tail() {
        let c = apply_to_final(
            "hey grain change profile now write this",
            &phrase(),
            Resolution::Switch,
        );
        assert_eq!(c.content, "now write this");
    }

    #[test]
    fn final_uses_last_occurrence() {
        let c = apply_to_final(
            "hey grain make it bold hey grain make it italic",
            &phrase(),
            Resolution::Record,
        );
        // The last wake wins: content is everything before the 2nd phrase.
        assert_eq!(c.content, "hey grain make it bold");
        assert_eq!(c.instruction.as_deref(), Some("make it italic"));
    }

    #[test]
    fn final_missing_phrase_returns_text() {
        let c = apply_to_final("no wake phrase here", &phrase(), Resolution::Record);
        assert_eq!(c.content, "no wake phrase here");
        assert_eq!(c.instruction, None);
    }
}
