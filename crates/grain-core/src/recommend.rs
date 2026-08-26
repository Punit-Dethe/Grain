//! [GRAIN] Recommendation ranking, headless (`docs/Extensions V1/PLAN.md` §3.1,
//! §5). Given what the user said, decide **which searchable extension** should
//! be offered the request — never which command, and never executing anything.
//!
//! Pure over its inputs, exactly like [`crate::action_router`], and for the same
//! reason: the eval harness (V1-P3) drives it without a running app, and the
//! host feeds it. In particular this module holds **no model**. Semantic scores
//! are injected by the host, which owns the embedder; here they are just numbers
//! keyed by extension id. That keeps `grain-core` free of candle and the ~130 MB
//! weights, and keeps the one interesting decision — how a topical score and a
//! name hit combine — in one testable place.
//!
//! Two signals, deliberately not blended into one number (§5):
//!
//! - **Named** — the user said the extension's name or a declared alias. Lexical,
//!   decisive, and cheap. Its whole job is exactly this; it is weak at topical
//!   ranking and strong here.
//! - **Topical** — the request is *about* what the extension does, in words that
//!   share nothing with its declared phrases. This is what semantic matching is
//!   for, and it is the reason Extension Mode needs the embedding model at all.
//!
//! The two are kept apart on the result, not summed, because a downstream
//! consumer must be able to tell them apart: Auto-send (V1-P4) may fire on a
//! clear Topical match but **never** on a Named one — a lexical hit on speech
//! proves a phrasing occurred, not that intent is certain (§5).

use std::collections::HashMap;

use crate::action_router::{normalise, same_word, tokens};

/// One searchable extension's recommendation surface, prepared for ranking.
///
/// Only what the *lexical* leg needs lives here — the aliases, normalised once.
/// The examples that feed the semantic leg are embedded host-side and never
/// reach this module; it receives their verdict as a score, not their text.
#[derive(Clone, Debug)]
pub struct IndexedRecommendation {
    pub extension_id: String,
    /// The manifest name plus declared spoken aliases, normalised and tokenised
    /// at build. Names are part of the contract's implicit address surface;
    /// aliases add reviewed pronunciations or nicknames.
    aliases: Vec<Vec<String>>,
}

impl IndexedRecommendation {
    /// Build from a declared alias list. Empty and whitespace aliases are
    /// dropped rather than matched (an empty alias would match every request).
    pub fn new(extension_id: impl Into<String>, aliases: &[String]) -> Self {
        let mut normalised_aliases = Vec::with_capacity(aliases.len());
        for alias in aliases {
            let words = normalise(alias)
                .split(' ')
                .filter(|token| !token.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !words.is_empty() && !normalised_aliases.contains(&words) {
                normalised_aliases.push(words);
            }
        }
        IndexedRecommendation {
            extension_id: extension_id.into(),
            aliases: normalised_aliases,
        }
    }

    /// Build the production name-detection surface. `manifest.name` is an
    /// implicit alias by contract, so the host and the eval harness must never
    /// construct recommendation indexes differently here.
    pub fn with_name(
        extension_id: impl Into<String>,
        name: &str,
        declared_aliases: &[String],
    ) -> Self {
        let mut aliases = Vec::with_capacity(declared_aliases.len() + 1);
        aliases.push(name.to_string());
        aliases.extend(declared_aliases.iter().cloned());
        Self::new(extension_id, &aliases)
    }
}

/// Which signal put an extension on the list. Carried out, never collapsed into
/// the score, because Auto-send eligibility depends on it (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// The user named the extension. Decisive for ordering, and — precisely
    /// because a lexical hit on speech is not evidence of certain intent —
    /// **ineligible** for Auto-send however high the score.
    Named,
    /// The request was topically close to what the extension is for. The only
    /// signal Auto-send may ever act on without asking.
    Topical,
}

/// One extension Grain would offer, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub extension_id: String,
    /// 0.0–1.0. For [`Signal::Named`] it is a fixed high confidence, not a
    /// cosine — a name either was said or was not. For [`Signal::Topical`] it is
    /// the injected similarity.
    pub score: f32,
    /// Gap to the next candidate below this one, `0.0` for the last. The clarity
    /// Auto-send reads (§5): a top Topical match that is not clear of the
    /// runner-up is exactly the ambiguous case that must be asked about.
    pub margin: f32,
    pub signal: Signal,
}

/// The floor a Topical score must clear to be offered at all.
///
/// Matches the Grain Space recall floor for the same BGE model: below this the
/// asymmetric query geometry no longer separates related from unrelated, so an
/// entry here would be noise dressed as a recommendation. A Named hit ignores
/// this — being named is not a similarity claim.
pub const TOPICAL_FLOOR: f32 = 0.50;

/// The confidence a clean name hit is reported at. Above any real cosine, so a
/// Named extension always sorts above a Topical one, which is the intended
/// precedence: if the user said the name, that is what they meant.
const NAMED_SCORE: f32 = 1.0;

/// The clarity margin a Topical top must lead the runner-up by to Auto-send
/// (§5). A beta starting point — tune against `grain-ext eval`'s operating
/// points, which report exactly the wrong-fire rate this guards.
pub const AUTO_SEND_MIN_MARGIN: f32 = 0.15;

/// Decide whether a ranking may **Auto-send**, and to whom (§5).
///
/// `eligible` is the set the host has already reduced to author-eligible ∩
/// user-not-disabled with the global toggle on — the *configuration* half of the
/// four conditions. This function enforces the two rules that are about the
/// MATCH, and both are structural, not tuning:
///
/// - **Never on a lexical / named hit.** A lexical hit on speech proves a
///   phrasing occurred, not that the intent is certain — so a named top is never
///   auto-sent, however high it scored.
/// - **Clear of the runner-up.** A top that does not lead by `min_margin` is the
///   ambiguous case Auto-send must ask about, not fire on.
///
/// Above-the-floor is already guaranteed — [`rank`] only returns above-floor
/// candidates. Returns the extension to hand to, or `None` to fall back to the
/// chooser.
pub fn auto_send_target(
    ranked: &[Recommendation],
    eligible: &std::collections::HashSet<String>,
    min_margin: f32,
) -> Option<String> {
    let top = ranked.first()?;
    if top.signal != Signal::Topical {
        return None;
    }
    if !eligible.contains(&top.extension_id) {
        return None;
    }
    if top.margin < min_margin {
        return None;
    }
    Some(top.extension_id.clone())
}

/// Rank the searchable extensions for one spoken request.
///
/// `semantic` is the host's verdict from the embedder: best similarity per
/// extension id, or `None` when the model is not available. `None` is
/// **name-only mode** — not an error and not empty-by-fiat, but honestly
/// degraded: only Named hits can be offered, which is why first use of Extension
/// Mode offers the download (§5). An empty result is the real "nothing matched"
/// state (§1), and the caller renders it as such rather than as a failure.
///
/// `excluded` is the decline-and-reopen set (§8 G2): extensions the user already
/// turned down for *this* request, dropped from the ballot so the reopened
/// chooser cannot offer the same wrong pick again.
pub fn rank(
    recs: &[IndexedRecommendation],
    spoken: &str,
    semantic: Option<&HashMap<String, f32>>,
    excluded: &[String],
) -> Vec<Recommendation> {
    let request = normalise(spoken);
    let request_tokens = tokens(&request);

    let mut named: Vec<Recommendation> = Vec::new();
    let mut topical: Vec<Recommendation> = Vec::new();

    for rec in recs {
        // Decline-and-reopen (G2): an extension the user has already turned down
        // for this request is gone from the ballot entirely, not merely demoted —
        // re-offering it as the runner-up is the loop the recovery exists to
        // break.
        if excluded.iter().any(|id| *id == rec.extension_id) {
            continue;
        }
        if rec
            .aliases
            .iter()
            .any(|alias| contains_run(&request_tokens, alias))
        {
            named.push(Recommendation {
                extension_id: rec.extension_id.clone(),
                score: NAMED_SCORE,
                margin: 0.0,
                signal: Signal::Named,
            });
            // A named extension is never *also* offered as a topical guess: it is
            // already the strongest possible answer for this request.
            continue;
        }
        if let Some(scores) = semantic {
            if let Some(&score) = scores.get(&rec.extension_id) {
                if score >= TOPICAL_FLOOR {
                    topical.push(Recommendation {
                        extension_id: rec.extension_id.clone(),
                        score,
                        margin: 0.0,
                        signal: Signal::Topical,
                    });
                }
            }
        }
    }

    // Named first, each kind by descending score. Ties break on extension id so
    // the order never depends on registry iteration — the same ballot twice must
    // read the same, or a user learns the recommendation is arbitrary.
    let by_score = |a: &Recommendation, b: &Recommendation| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.extension_id.cmp(&b.extension_id))
    };
    named.sort_by(by_score);
    topical.sort_by(by_score);

    let mut out = named;
    out.append(&mut topical);

    // Margin is the gap to the next one down, filled after the final order is
    // known. It is meaningful within a signal, so a Named-then-Topical seam
    // reports the Named leader's margin against the next Named entry (0.0 if it
    // is alone in its kind), never against a topical score on a different scale.
    for i in 0..out.len() {
        let next_same_signal = out[i + 1..].iter().find(|r| r.signal == out[i].signal);
        out[i].margin = next_same_signal.map_or(out[i].score, |n| out[i].score - n.score);
    }
    out
}

/// Does `needle` appear as a contiguous run inside `haystack`, each token equal
/// under ASR tolerance ([`same_word`])?
///
/// Contiguous and in order on purpose: "spotify" naming Spotify is a run of one;
/// a two-word alias like "apple music" must appear as those two words together,
/// not scattered across a sentence, or every alias with a common word in it
/// would fire on unrelated speech.
fn contains_run(haystack: &[&str], needle: &[String]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(spoken, alias)| same_word(spoken, alias))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, aliases: &[&str]) -> IndexedRecommendation {
        IndexedRecommendation::new(
            id,
            &aliases.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
    }

    fn scores(pairs: &[(&str, f32)]) -> HashMap<String, f32> {
        pairs.iter().map(|(id, s)| (id.to_string(), *s)).collect()
    }

    #[test]
    fn naming_an_extension_puts_it_first_and_marks_it_named() {
        let recs = [rec("spotify", &["spotify"]), rec("linear", &["linear"])];
        let out = rank(
            &recs,
            "spotify next song",
            Some(&scores(&[("linear", 0.9)])),
            &[],
        );
        assert_eq!(out[0].extension_id, "spotify");
        assert_eq!(out[0].signal, Signal::Named);
        assert!(
            out[0].score > out[1].score,
            "a named extension outranks a topical one however strong the topic"
        );
    }

    #[test]
    fn manifest_name_is_an_implicit_alias() {
        let recs = [IndexedRecommendation::with_name(
            "github",
            "GitHub",
            &["github".to_string()],
        )];
        assert_eq!(recs[0].aliases.len(), 1);
        let out = rank(&recs, "could you open github", None, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].extension_id, "github");
        assert_eq!(out[0].signal, Signal::Named);
    }

    #[test]
    fn a_name_hit_survives_what_asr_does_to_it() {
        // "linear" heard with a dropped vowel — one deletion on a long word.
        let recs = [rec("linear", &["linear"])];
        let out = rank(&recs, "add a linar ticket", None, &[]);
        assert_eq!(out.len(), 1, "one edit must not lose the name");
        assert_eq!(out[0].signal, Signal::Named);
    }

    #[test]
    fn a_multi_word_alias_must_appear_together() {
        let recs = [rec("apple", &["apple music"])];
        // The two words are present but not adjacent — not a naming.
        assert!(rank(&recs, "the apple on the table plays music", None, &[]).is_empty());
        assert_eq!(
            rank(&recs, "apple music play something", None, &[])[0].extension_id,
            "apple"
        );
    }

    #[test]
    fn topical_ranking_orders_by_similarity_above_the_floor() {
        let recs = [rec("spotify", &["spotify"]), rec("notes", &["notes"])];
        let out = rank(
            &recs,
            "put on some jazz",
            Some(&scores(&[("spotify", 0.71), ("notes", 0.54)])),
            &[],
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].extension_id, "spotify");
        assert_eq!(out[0].signal, Signal::Topical);
        assert!(out[1].extension_id == "notes");
    }

    #[test]
    fn a_score_below_the_floor_is_not_a_recommendation() {
        let recs = [rec("spotify", &["spotify"])];
        assert!(rank(
            &recs,
            "what's the weather",
            Some(&scores(&[("spotify", 0.42)])),
            &[]
        )
        .is_empty());
    }

    #[test]
    fn name_only_mode_offers_only_named_hits() {
        // The model is not on disk: `None`. A topical request that named nothing
        // returns nothing, which is honest — not silently ranking on lexical.
        let recs = [rec("spotify", &["spotify"])];
        assert!(rank(&recs, "put on some jazz", None, &[]).is_empty());
        assert_eq!(
            rank(&recs, "open spotify", None, &[])[0].extension_id,
            "spotify"
        );
    }

    #[test]
    fn nothing_matched_is_an_empty_list_not_an_error() {
        let recs = [rec("spotify", &["spotify"])];
        let out = rank(
            &recs,
            "so anyway we should rewrite the parser",
            Some(&scores(&[])),
            &[],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_named_extension_is_not_also_offered_topically() {
        // It named itself AND scores high on topic; it appears once, as Named.
        let recs = [rec("spotify", &["spotify"])];
        let out = rank(
            &recs,
            "spotify put on some jazz",
            Some(&scores(&[("spotify", 0.8)])),
            &[],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].signal, Signal::Named);
    }

    #[test]
    fn the_ballot_does_not_depend_on_registry_order() {
        let forwards = [rec("a", &["aaa"]), rec("b", &["bbb"])];
        let backwards = [rec("b", &["bbb"]), rec("a", &["aaa"])];
        let s = scores(&[("a", 0.6), ("b", 0.6)]);
        let ids = |recs: &[IndexedRecommendation]| {
            rank(recs, "something topical", Some(&s), &[])
                .into_iter()
                .map(|r| r.extension_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&forwards),
            ids(&backwards),
            "a tie must resolve the same way twice"
        );
    }

    #[test]
    fn margin_is_the_gap_to_the_runner_up_within_a_signal() {
        let recs = [rec("a", &["aaa"]), rec("b", &["bbb"]), rec("c", &["ccc"])];
        let out = rank(
            &recs,
            "topical request",
            Some(&scores(&[("a", 0.9), ("b", 0.6), ("c", 0.55)])),
            &[],
        );
        assert!((out[0].margin - 0.30).abs() < 1e-6, "0.9 leads 0.6 by 0.30");
        assert!(
            (out[1].margin - 0.05).abs() < 1e-6,
            "0.6 leads 0.55 by 0.05"
        );
        assert!(
            (out[2].margin - 0.55).abs() < 1e-6,
            "the last reports its own score"
        );
    }

    #[test]
    fn a_declined_extension_leaves_the_ballot_entirely() {
        // Decline-and-reopen (G2): after the user turns spotify down, reranking
        // must not offer it again as the runner-up — it is gone, and the next
        // best takes the top.
        let recs = [rec("spotify", &["spotify"]), rec("apple", &["apple"])];
        let s = scores(&[("spotify", 0.8), ("apple", 0.7)]);
        let first = rank(&recs, "put on some music", Some(&s), &[]);
        assert_eq!(first[0].extension_id, "spotify");

        let reopened = rank(
            &recs,
            "put on some music",
            Some(&s),
            &["spotify".to_string()],
        );
        assert!(
            reopened.iter().all(|r| r.extension_id != "spotify"),
            "a declined extension must not reappear"
        );
        assert_eq!(reopened[0].extension_id, "apple");
    }

    fn eligible(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_send_fires_on_a_clear_eligible_topical_top() {
        let recs = [rec("spotify", &["spotify"]), rec("notes", &["notes"])];
        let ranked = rank(
            &recs,
            "put on some jazz",
            Some(&scores(&[("spotify", 0.82), ("notes", 0.55)])),
            &[],
        );
        assert_eq!(
            auto_send_target(&ranked, &eligible(&["spotify"]), AUTO_SEND_MIN_MARGIN),
            Some("spotify".to_string())
        );
    }

    #[test]
    fn auto_send_never_fires_on_a_named_hit() {
        // The whole §5 rule: a lexical hit on speech is not evidence of certain
        // intent, so however cleanly the name won, Auto-send declines.
        let recs = [rec("spotify", &["spotify"])];
        let ranked = rank(&recs, "spotify play something", None, &[]);
        assert_eq!(ranked[0].signal, Signal::Named);
        assert_eq!(
            auto_send_target(&ranked, &eligible(&["spotify"]), AUTO_SEND_MIN_MARGIN),
            None
        );
    }

    #[test]
    fn auto_send_declines_an_ineligible_extension() {
        let recs = [rec("spotify", &["spotify"])];
        let ranked = rank(
            &recs,
            "put on some jazz",
            Some(&scores(&[("spotify", 0.82)])),
            &[],
        );
        assert_eq!(
            auto_send_target(&ranked, &eligible(&[]), AUTO_SEND_MIN_MARGIN),
            None,
            "eligible is empty — nothing may auto-send"
        );
    }

    #[test]
    fn auto_send_declines_an_ambiguous_top() {
        let recs = [rec("spotify", &["spotify"]), rec("notes", &["notes"])];
        let ranked = rank(
            &recs,
            "put on some jazz",
            Some(&scores(&[("spotify", 0.72), ("notes", 0.68)])),
            &[],
        );
        assert_eq!(
            auto_send_target(&ranked, &eligible(&["spotify"]), AUTO_SEND_MIN_MARGIN),
            None,
            "a 0.04 lead is exactly the case to ask about"
        );
    }

    #[test]
    fn an_empty_alias_matches_nothing() {
        // A blank alias would otherwise match every request as a zero-length run.
        let recs = [rec("x", &["", "  "])];
        assert!(rank(&recs, "anything at all", None, &[]).is_empty());
    }
}
