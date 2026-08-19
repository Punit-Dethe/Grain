//! [GRAIN] The action-routing eval harness (`docs/Action Routing/PLAN.md` §12).
//!
//! P0's exit condition is "the harness can score a router that does not exist
//! yet". This is that harness: it drives the pure Tier-L functions over a golden
//! set and reports the numbers the decision actually turns on.
//!
//! **The metric is not top-1 accuracy.** A router that is 95% accurate and knows
//! which 5% it is unsure about is a good product; one that is 98% accurate and
//! confidently wrong the rest of the time is not. So the assertions here are on
//! the false-execution rate and the reject rate, and top-1 is reported for
//! information.
//!
//! The operating point below is **provisional and lives in the harness, not the
//! router** — deliberately. P1 owns the decision table; keeping thresholds out
//! of `action_router` is what makes it measurable before that exists.

use grain_core::action_router::{equivalence_classes, rank, Candidate, IndexedAction};
use grain_sdk::manifest::ActionDecl;
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    extensions: Vec<FixtureExtension>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct FixtureExtension {
    id: String,
    actions: Vec<ActionDecl>,
}

#[derive(Deserialize)]
struct Case {
    said: String,
    expect: String,
    why: String,
}

/// What the harness decided to do, given the ranking.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Execute(String),
    Choose,
    None,
}

/// Provisional Tier-L operating point.
///
/// Two knobs only, because Tier L has two kinds of evidence: an exact declared
/// phrase, and a template whose literals were found in order. The margin is
/// taken **between equivalence classes**, never between action ids — otherwise
/// two providers of one request tie forever and every media command becomes a
/// chooser (REVIEW-PASS2 D1).
const MIN_SCORE: f32 = 0.7;
const MIN_MARGIN: f32 = 0.15;

fn decide(
    candidates: &[Candidate],
    classes: &grain_core::action_router::EquivalenceMap,
) -> Outcome {
    let Some(best) = candidates.first() else {
        return Outcome::None;
    };
    if best.score < MIN_SCORE {
        return Outcome::None;
    }
    let best_class = classes.class_of(&best.extension_id, &best.action_id);
    // The runner-up that matters is the best candidate from a DIFFERENT class.
    let rival = candidates
        .iter()
        .find(|c| classes.class_of(&c.extension_id, &c.action_id) != best_class);

    // Within the winning class, more than one provider means the action is
    // decided and the PROVIDER is not — which is the provider ladder's job, and
    // with no defaults configured it ends at the chooser.
    let providers = candidates
        .iter()
        .filter(|c| classes.class_of(&c.extension_id, &c.action_id) == best_class)
        .filter(|c| c.score >= MIN_SCORE)
        .map(|c| c.extension_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    if let Some(rival) = rival {
        if best.kind == rival.kind && best.score - rival.score < MIN_MARGIN {
            return Outcome::Choose;
        }
    }
    if providers.len() > 1 {
        return Outcome::Choose;
    }
    Outcome::Execute(format!("{}:{}", best.extension_id, best.action_id))
}

fn load() -> (Vec<IndexedAction>, Vec<Case>) {
    let raw = include_str!("fixtures/action_routing_golden.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden set parses");
    let mut actions = Vec::new();
    for extension in &golden.extensions {
        for decl in &extension.actions {
            actions.push(IndexedAction::from_decl(&extension.id, decl));
        }
    }
    (actions, golden.cases)
}

struct Scores {
    total: usize,
    correct: usize,
    /// Ran something when the correct answer was "ask" or "nothing". **The
    /// number that decides whether this ships.**
    false_executions: Vec<String>,
    /// Asked or refused when it could have acted. Costs friction, not trust.
    missed: Vec<String>,
}

fn run() -> Scores {
    let (actions, cases) = load();
    let classes = equivalence_classes(&actions);
    let mut scores = Scores {
        total: cases.len(),
        correct: 0,
        false_executions: Vec::new(),
        missed: Vec::new(),
    };

    for case in &cases {
        let candidates = rank(&case.said, &actions);
        let outcome = decide(&candidates, &classes);
        let expected = match case.expect.as_str() {
            "none" => Outcome::None,
            "choose" => Outcome::Choose,
            id => Outcome::Execute(id.to_string()),
        };
        if outcome == expected {
            scores.correct += 1;
            continue;
        }
        let detail = format!(
            "\"{}\" → {:?}, expected {:?} ({})",
            case.said, outcome, expected, case.why
        );
        match (&outcome, &expected) {
            // Ran something when the right answer was to ask, to say nothing, or
            // to run something else. All three are the same failure to a user.
            (Outcome::Execute(_), _) => scores.false_executions.push(detail),
            _ => scores.missed.push(detail),
        }
    }
    scores
}

#[test]
fn the_lexical_router_never_executes_when_it_should_have_asked() {
    // The primary gate. A wrong execution is unrecoverable from the user's
    // side; a chooser they did not need is an annoyance.
    let scores = run();
    assert!(
        scores.false_executions.is_empty(),
        "{} false execution(s) of {} cases:\n  {}",
        scores.false_executions.len(),
        scores.total,
        scores.false_executions.join("\n  ")
    );
}

#[test]
fn the_lexical_router_reaches_its_baseline_on_the_golden_set() {
    // Tier L alone, no embedder. This number is the floor a user with no model
    // downloaded actually experiences, so it is asserted rather than printed.
    let scores = run();
    let rate = scores.correct as f32 / scores.total as f32;
    // 27/27 at the provisional operating point as of P0. Held one case below
    // that so a genuinely marginal new case can be added without a red build,
    // while a regression in the router still trips it.
    assert!(
        rate >= 0.95,
        "lexical top-1 {:.1}% over {} cases; {} missed:\n  {}",
        rate * 100.0,
        scores.total,
        scores.missed.len(),
        scores.missed.join("\n  ")
    );
}

#[test]
fn ordinary_dictation_and_empty_capture_never_route() {
    // The out-of-scope class that actually matters once the trigger is its own
    // key: not "that was really dictation" but "nothing installed can do this".
    let (actions, _) = load();
    let classes = equivalence_classes(&actions);
    for said in [
        "",
        "...",
        "so anyway I was thinking we should probably rewrite the parser this week",
        "what's the weather",
        "turn the lights off",
    ] {
        let outcome = decide(&rank(said, &actions), &classes);
        assert_eq!(outcome, Outcome::None, "\"{said}\" must not route");
    }
}

#[test]
fn two_providers_of_one_request_ask_rather_than_picking_one() {
    // Without equivalence classes this test passes for the wrong reason (zero
    // margin), and every unambiguous media command fails alongside it.
    let (actions, _) = load();
    let classes = equivalence_classes(&actions);
    assert_eq!(
        decide(&rank("skip this", &actions), &classes),
        Outcome::Choose
    );
    assert_eq!(
        decide(&rank("next song", &actions), &classes),
        Outcome::Choose
    );
    // …while a request only one of them declares still runs.
    assert_eq!(
        decide(&rank("go back", &actions), &classes),
        Outcome::Execute("spotify:previous".into())
    );
}

#[test]
fn the_same_words_in_two_domains_do_not_bleed() {
    let (actions, _) = load();
    let classes = equivalence_classes(&actions);
    assert_eq!(
        decide(&rank("next slide", &actions), &classes),
        Outcome::Execute("deck:next_slide".into())
    );
}

/// Not an assertion — a printout, so `cargo test -- --nocapture` gives the
/// operating-point table the plan asks to be published per phase.
#[test]
fn report() {
    let scores = run();
    println!(
        "\naction routing — Tier L, no embedder\n  cases            {}\n  top-1            {:.1}%\
         \n  false executions {}\n  missed           {}\n  operating point  score>={MIN_SCORE} \
         margin>={MIN_MARGIN}\n",
        scores.total,
        scores.correct as f32 / scores.total as f32 * 100.0,
        scores.false_executions.len(),
        scores.missed.len(),
    );
    for miss in &scores.missed {
        println!("  missed: {miss}");
    }
}
