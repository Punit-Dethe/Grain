//! [GRAIN] The `match.*` primitives an extension calls to interpret a handed-off
//! request (`docs/Extensions V1/PLAN.md` §4). The extension's own command
//! ranking, one level below Grain's extension ranking — same machinery, exposed
//! as callable pieces so an author never rebuilds a scorer or a confidence model.
//!
//! Two of the three live here because they are **pure**: `lexical_rank` (over the
//! extension's own phrases) and `decide` (a confidence policy over any scored
//! candidates). The third, `match.semantic`, needs the embedder and so lives on
//! the host side — but it produces the same `(id, score)` shape `decide` consumes,
//! which is the point: the primitives compose in any order (§4), they are not a
//! fixed pipeline.

use crate::action_router::{ActionIndex, IndexedAction};
use grain_sdk::manifest::{parse_utterance, ActionRisk};

/// One scored candidate — an extension command id and how well the request
/// matched it. The common currency the primitives pass between each other.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub id: String,
    pub score: f32,
}

/// Rank caller-supplied candidates lexically against `text`.
///
/// Each candidate is `(id, phrasings)` — the id the extension will act on, and
/// the ways a user might ask for it. Returns `(id, score)` best-first, using the
/// **same** scorer, IDF specificity and ASR edit tolerance Grain uses for its own
/// lexical leg (`action_router`). Corpus statistics come from the supplied set,
/// so specificity is measured across exactly the commands the extension is
/// choosing between.
///
/// Lexical only — no model, no capability, and (per §5) reliable for
/// *name/verb* hits, not paraphrase. An extension that needs paraphrase reaches
/// for `match.semantic`.
pub fn lexical_rank(text: &str, candidates: &[(String, Vec<String>)]) -> Vec<Match> {
    let actions: Vec<IndexedAction> = candidates
        .iter()
        .map(|(id, phrases)| IndexedAction {
            extension_id: String::new(),
            action_id: id.clone(),
            title: String::new(),
            // Irrelevant to scoring; the primitive ranks, it never executes, so
            // the safety axis a real action carries has no meaning here.
            risk: ActionRisk::Safe,
            required_params: Vec::new(),
            templates: phrases
                .iter()
                .filter_map(|p| parse_utterance(p.trim()).ok())
                .collect(),
        })
        .collect();
    ActionIndex::build(actions)
        .rank(text)
        .into_iter()
        .map(|c| Match {
            id: c.action_id,
            score: c.score,
        })
        .collect()
}

/// What a confidence policy concluded from a ranked candidate set.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// One candidate is clearly best — run it.
    Pick(String),
    /// Several are too close to separate — ask the user. Carries the tied ids,
    /// best-first, so the caller can render a chooser without re-ranking.
    Ambiguous(Vec<String>),
    /// Nothing cleared the bar — decline rather than guess.
    None,
}

/// The knobs a `decide` call turns. Deliberately the two an author can reason
/// about — "how sure, and how much clearer than the next" — not an opaque score.
///
/// There is **no universal threshold** (the research is explicit, §8 G4); the
/// author measures on their own test set with `grain-ext eval` and sets these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecidePolicy {
    /// The floor the best candidate must clear to be acted on at all.
    pub min_confidence: f32,
    /// How far the best must lead the runner-up to be picked outright; within
    /// this, the two are ambiguous.
    pub margin: f32,
}

/// Turn a ranked candidate set into an action: pick, ask, or decline
/// (`grain.match.decide`, §4).
///
/// Defensive about order — it sorts rather than trusting the caller — because a
/// `decide` fed an unsorted list that silently picked the wrong "top" is the
/// exact bug that makes a confidence model untrustworthy. Ties break on id so the
/// decision is deterministic.
pub fn decide(candidates: &[(String, f32)], policy: &DecidePolicy) -> Decision {
    let mut ranked: Vec<&(String, f32)> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let eligible: Vec<&(String, f32)> = ranked
        .into_iter()
        .filter(|(_, score)| *score >= policy.min_confidence)
        .collect();

    let Some((top_id, top_score)) = eligible.first().map(|(id, s)| (id.clone(), *s)) else {
        return Decision::None;
    };
    let runner_up = eligible.get(1).map(|(_, s)| *s);
    match runner_up {
        // Clear of the runner-up, or alone above the floor: pick it.
        Some(second) if top_score - second >= policy.margin => Decision::Pick(top_id),
        None => Decision::Pick(top_id),
        // Too close to separate: everything within `margin` of the top is tied.
        Some(_) => Decision::Ambiguous(
            eligible
                .iter()
                .filter(|(_, s)| top_score - s < policy.margin)
                .map(|(id, _)| id.clone())
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(pairs: &[(&str, &str)]) -> Vec<(String, Vec<String>)> {
        // One phrasing per id, which is all these lexical tests need.
        let mut by_id: Vec<(String, Vec<String>)> = Vec::new();
        for (id, phrase) in pairs {
            by_id.push((id.to_string(), vec![phrase.to_string()]));
        }
        by_id
    }

    #[test]
    fn lexical_rank_puts_the_matching_command_first() {
        let commands = cand(&[
            ("next", "skip to the next track"),
            ("pause", "pause the music"),
            ("volume", "set the volume"),
        ]);
        let ranked = lexical_rank("skip this track", &commands);
        assert_eq!(ranked[0].id, "next", "the closest declared phrasing wins");
        assert!(ranked[0].score > 0.0);
    }

    #[test]
    fn lexical_rank_returns_nothing_for_an_unrelated_request() {
        let commands = cand(&[("next", "skip to the next track")]);
        assert!(
            lexical_rank("what's the weather in tokyo", &commands).is_empty(),
            "no shared language is no match, not a weak one"
        );
    }

    #[test]
    fn decide_picks_a_clear_leader() {
        let policy = DecidePolicy {
            min_confidence: 0.4,
            margin: 0.15,
        };
        let out = decide(&[("a".into(), 0.9), ("b".into(), 0.5)], &policy);
        assert_eq!(out, Decision::Pick("a".into()));
    }

    #[test]
    fn decide_asks_when_two_are_too_close() {
        let policy = DecidePolicy {
            min_confidence: 0.4,
            margin: 0.15,
        };
        let out = decide(
            &[("a".into(), 0.72), ("b".into(), 0.70), ("c".into(), 0.3)],
            &policy,
        );
        assert_eq!(
            out,
            Decision::Ambiguous(vec!["a".into(), "b".into()]),
            "c is below the floor and not part of the tie"
        );
    }

    #[test]
    fn decide_declines_when_nothing_clears_the_floor() {
        let policy = DecidePolicy {
            min_confidence: 0.6,
            margin: 0.1,
        };
        assert_eq!(
            decide(&[("a".into(), 0.55), ("b".into(), 0.4)], &policy),
            Decision::None
        );
    }

    #[test]
    fn decide_is_defensive_about_order() {
        // Fed worst-first, it must still find the real leader, not trust index 0.
        let policy = DecidePolicy {
            min_confidence: 0.4,
            margin: 0.15,
        };
        let out = decide(&[("low".into(), 0.45), ("high".into(), 0.95)], &policy);
        assert_eq!(out, Decision::Pick("high".into()));
    }

    #[test]
    fn decide_on_nothing_is_none() {
        let policy = DecidePolicy {
            min_confidence: 0.4,
            margin: 0.15,
        };
        assert_eq!(decide(&[], &policy), Decision::None);
    }
}
