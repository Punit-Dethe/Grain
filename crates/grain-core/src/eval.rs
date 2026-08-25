//! [GRAIN] The author-facing evaluation core (`docs/Extensions V1/PLAN.md` §8
//! G4, §10 P3). "Moving interpretation into extensions moves failure into
//! extensions" — so an author needs to *measure* their command matching, because
//! there is no universal confidence threshold (Rasa's warning): the only honest
//! way to pick one is on a test set.
//!
//! Pure and model-free, the same split the rest of the platform uses. Lexical is
//! computed here (it reuses `matching::lexical_rank`); the semantic ranking per
//! case is injected by the caller, which owns the embedder. So `grain-ext eval`
//! runs this over the real embedder (a headless app subcommand), and the unit
//! tests run it with hand-supplied scores.
//!
//! It measures the **primitives' capability**, not the author's exact `onRequest`
//! order — Grain cannot know how an author chained lexical/semantic/llm. Reporting
//! each primitive's reach separately is more useful anyway: it tells the author
//! which one to lean on and where to set the floor.

use crate::matching::{lexical_rank, Match};

/// One labelled test utterance. `expect` is a command id, or `"none"` when
/// nothing should match (ordinary speech the extension must not act on).
#[derive(Clone, Debug)]
pub struct Case {
    pub said: String,
    pub expect: String,
}

/// The sentinel `expect` for "nothing should match".
pub const EXPECT_NONE: &str = "none";

/// Which primitive's top pick matched the label, at [`MATCH_FLOOR`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolver {
    /// Only lexical got it — a name/verb hit.
    Lexical,
    /// Only semantic got it — paraphrase the lexical leg could not see.
    Semantic,
    /// Both would have.
    Both,
    /// Neither. For an `expect: none` case this is the CORRECT outcome; for a
    /// real command it is the miss the author has to fix.
    Neither,
}

/// One case's verdict.
#[derive(Clone, Debug)]
pub struct CaseResult {
    pub said: String,
    pub expect: String,
    pub lexical_top: Option<Match>,
    pub semantic_top: Option<Match>,
    pub resolver: Resolver,
    /// True when the case landed as it should: the right command by some
    /// primitive, or nothing for an `expect: none`.
    pub correct: bool,
}

/// One (expected → got) pair and how often it occurred, over the SEMANTIC top —
/// the leg that carries paraphrase, so the one whose confusions matter most.
/// `got` is [`EXPECT_NONE`] when semantic produced nothing above the floor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfusionCell {
    pub expected: String,
    pub got: String,
    pub count: usize,
}

/// Accuracy and mis-fires if the author set `minConfidence` here — the sweep that
/// answers "where do I put the floor", since there is no universal one (G4).
#[derive(Clone, Copy, Debug)]
pub struct OperatingPoint {
    pub min_confidence: f32,
    /// Real commands the semantic top got right AND cleared the floor.
    pub correct_fires: usize,
    /// Fires that were wrong — a wrong command, or firing on an `expect: none`.
    /// The number Auto-send cares about most (§5): a wrong fire is unrecoverable
    /// from the user's side.
    pub wrong_fires: usize,
    /// Cases that stayed below the floor. Right for `expect: none`, friction for
    /// a real command.
    pub abstains: usize,
}

/// The floor a top score must clear to count as a match when tallying accuracy
/// and the resolver. The operating-point sweep varies its own threshold; this is
/// only the baseline the headline accuracy is reported at.
pub const MATCH_FLOOR: f32 = crate::recommend::TOPICAL_FLOOR;

/// The full report for one extension's test set.
#[derive(Clone, Debug)]
pub struct Report {
    pub results: Vec<CaseResult>,
    /// Fraction of cases the lexical top resolved correctly (a real command it
    /// picked right, or a `none` it stayed silent on).
    pub lexical_accuracy: f32,
    /// Same for the semantic top. Compare the two to see what the model buys.
    pub semantic_accuracy: f32,
    pub confusion: Vec<ConfusionCell>,
    pub operating_points: Vec<OperatingPoint>,
}

/// Evaluate one extension's command matching against a labelled set.
///
/// `commands` is `(id, phrasings)` — the extension's declared commands. `semantic`
/// is the injected per-case semantic ranking (best-first), aligned by index with
/// `cases`, or `None` for a lexical-only run (no model, honestly reported rather
/// than pretended). `thresholds` is the operating-point sweep to report.
pub fn evaluate(
    commands: &[(String, Vec<String>)],
    cases: &[Case],
    semantic: Option<&[Vec<Match>]>,
    thresholds: &[f32],
) -> Report {
    let mut results = Vec::with_capacity(cases.len());
    let mut lexical_hits = 0usize;
    let mut semantic_hits = 0usize;
    let mut confusion: Vec<ConfusionCell> = Vec::new();

    for (index, case) in cases.iter().enumerate() {
        // A semantic ranking was FED for this case (possibly empty) vs. not run
        // at all. The distinction matters: an unfed case must not credit semantic
        // for "abstaining", or a lexical-only run would report phantom semantic
        // accuracy.
        let semantic_ran = semantic.and_then(|s| s.get(index)).is_some();
        let lexical_top = top_above_floor(lexical_rank(&case.said, commands));
        let semantic_top = semantic
            .and_then(|s| s.get(index))
            .and_then(|ranked| top_above_floor(ranked.clone()));

        let expects_none = case.expect == EXPECT_NONE;
        let (lexical_ok, semantic_ok, resolver, correct) = if expects_none {
            // Correct = every primitive that ran stayed silent. Nothing "picked"
            // it, so the resolver is Neither even though the case passed.
            let lexical_ok = lexical_top.is_none();
            let semantic_ok = semantic_ran && semantic_top.is_none();
            (
                lexical_ok,
                semantic_ok,
                Resolver::Neither,
                lexical_ok && (!semantic_ran || semantic_top.is_none()),
            )
        } else {
            let lexical_ok = lexical_top.as_ref().is_some_and(|m| m.id == case.expect);
            let semantic_ok =
                semantic_ran && semantic_top.as_ref().is_some_and(|m| m.id == case.expect);
            let resolver = match (lexical_ok, semantic_ok) {
                (true, true) => Resolver::Both,
                (true, false) => Resolver::Lexical,
                (false, true) => Resolver::Semantic,
                (false, false) => Resolver::Neither,
            };
            // Credits either primitive: an author leans on whichever reaches a
            // case, so a case some primitive resolves is not a failure.
            (lexical_ok, semantic_ok, resolver, lexical_ok || semantic_ok)
        };

        if lexical_ok {
            lexical_hits += 1;
        }
        if semantic_ok {
            semantic_hits += 1;
        }

        // Confusion is tallied over the semantic top when semantic ran: expected
        // vs what it picked (or `none`). A correct `none` is not a confusion.
        if semantic_ran {
            let got = semantic_top
                .as_ref()
                .map_or(EXPECT_NONE.to_string(), |m| m.id.clone());
            if !(expects_none && got == EXPECT_NONE) {
                bump_confusion(&mut confusion, &case.expect, &got);
            }
        }

        results.push(CaseResult {
            said: case.said.clone(),
            expect: case.expect.clone(),
            lexical_top,
            semantic_top,
            correct,
            resolver,
        });
    }

    let total = cases.len().max(1) as f32;
    let operating_points = thresholds
        .iter()
        .map(|&min_confidence| operating_point(cases, semantic, min_confidence))
        .collect();

    Report {
        results,
        lexical_accuracy: lexical_hits as f32 / total,
        semantic_accuracy: semantic_hits as f32 / total,
        confusion,
        operating_points,
    }
}

/// The top match, but only if it clears [`MATCH_FLOOR`]. A weak top is treated as
/// no match — the same floor recommendation uses, so eval and the live path agree
/// on what "matched" means.
fn top_above_floor(ranked: Vec<Match>) -> Option<Match> {
    ranked.into_iter().next().filter(|m| m.score >= MATCH_FLOOR)
}

fn bump_confusion(confusion: &mut Vec<ConfusionCell>, expected: &str, got: &str) {
    if let Some(cell) = confusion
        .iter_mut()
        .find(|c| c.expected == expected && c.got == got)
    {
        cell.count += 1;
    } else {
        confusion.push(ConfusionCell {
            expected: expected.to_string(),
            got: got.to_string(),
            count: 1,
        });
    }
}

/// One row of the operating-point sweep, over the semantic leg (the one a floor
/// is set on). A fire is the semantic top clearing `min_confidence`.
fn operating_point(
    cases: &[Case],
    semantic: Option<&[Vec<Match>]>,
    min_confidence: f32,
) -> OperatingPoint {
    let mut correct_fires = 0;
    let mut wrong_fires = 0;
    let mut abstains = 0;
    for (index, case) in cases.iter().enumerate() {
        let top = semantic
            .and_then(|s| s.get(index))
            .and_then(|ranked| ranked.first());
        match top {
            Some(m) if m.score >= min_confidence => {
                if case.expect != EXPECT_NONE && m.id == case.expect {
                    correct_fires += 1;
                } else {
                    wrong_fires += 1;
                }
            }
            _ => abstains += 1,
        }
    }
    OperatingPoint {
        min_confidence,
        correct_fires,
        wrong_fires,
        abstains,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, score: f32) -> Match {
        Match {
            id: id.to_string(),
            score,
        }
    }
    fn commands() -> Vec<(String, Vec<String>)> {
        vec![
            ("next".into(), vec!["skip to the next track".into()]),
            ("pause".into(), vec!["pause the music".into()]),
        ]
    }
    fn case(said: &str, expect: &str) -> Case {
        Case {
            said: said.into(),
            expect: expect.into(),
        }
    }

    #[test]
    fn lexical_only_run_reports_what_names_reach() {
        let cases = [
            case("skip to the next track", "next"),
            case("put on some jazz", "none"),
        ];
        let report = evaluate(&commands(), &cases, None, &[]);
        assert!(
            (report.lexical_accuracy - 1.0).abs() < 1e-6,
            "verbatim + a clean none"
        );
        assert_eq!(
            report.semantic_accuracy, 0.0,
            "no model fed, no semantic hits"
        );
        assert_eq!(report.results[0].resolver, Resolver::Lexical);
        assert_eq!(
            report.results[1].resolver,
            Resolver::Neither,
            "a correct none is Neither"
        );
        assert!(report.results[1].correct, "and it is still correct");
    }

    #[test]
    fn semantic_catches_a_paraphrase_lexical_misses() {
        let cases = [case("put on some music", "pause")];
        // Lexical shares nothing with "pause the music"? It shares "music"; but
        // one shared token is noise, not a match — so lexical misses, semantic
        // (fed high) catches it.
        let semantic = vec![vec![m("pause", 0.72), m("next", 0.40)]];
        let report = evaluate(&commands(), &cases, Some(&semantic), &[]);
        assert_eq!(report.results[0].resolver, Resolver::Semantic);
        assert!((report.semantic_accuracy - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_wrong_semantic_top_is_a_confusion() {
        let cases = [case("skip this", "next")];
        let semantic = vec![vec![m("pause", 0.80), m("next", 0.55)]];
        let report = evaluate(&commands(), &cases, Some(&semantic), &[]);
        assert_eq!(
            report.confusion,
            vec![ConfusionCell {
                expected: "next".into(),
                got: "pause".into(),
                count: 1,
            }]
        );
    }

    #[test]
    fn a_correct_none_is_not_a_confusion() {
        let cases = [case("what's the weather", "none")];
        let semantic = vec![vec![m("next", 0.20)]]; // below the floor
        let report = evaluate(&commands(), &cases, Some(&semantic), &[]);
        assert!(
            report.confusion.is_empty(),
            "staying silent is not a mistake"
        );
        assert!(report.results[0].correct);
    }

    #[test]
    fn the_operating_point_sweep_trades_fires_for_mistakes() {
        let cases = [
            case("a request", "next"),
            case("another", "pause"),
            case("ordinary speech", "none"),
        ];
        let semantic = vec![
            vec![m("next", 0.80)],  // correct, strong
            vec![m("next", 0.62)],  // WRONG (expected pause), medium
            vec![m("pause", 0.55)], // should abstain (none), medium
        ];
        let report = evaluate(&commands(), &cases, Some(&semantic), &[0.5, 0.7]);
        let low = report.operating_points[0];
        assert_eq!(low.min_confidence, 0.5);
        assert_eq!(low.correct_fires, 1);
        assert_eq!(
            low.wrong_fires, 2,
            "the misroute and the none both fire at 0.5"
        );
        assert_eq!(low.abstains, 0);

        let high = report.operating_points[1];
        assert_eq!(high.correct_fires, 1, "the good one still fires at 0.7");
        assert_eq!(
            high.wrong_fires, 0,
            "raising the floor silenced both mistakes"
        );
        assert_eq!(high.abstains, 2);
    }
}
