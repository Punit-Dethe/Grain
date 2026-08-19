//! [GRAIN] The action-routing eval harness (`docs/Action Routing/PLAN.md` §12).
//!
//! Drives the **shipped** decision layer — `action_router` for the ranking,
//! `action_decision` for what to do with it — over a golden set, and asserts the
//! numbers the feature actually turns on.
//!
//! **The metric is not top-1 accuracy.** A router that is 95% accurate and knows
//! which 5% it is unsure about is a good product; one that is 98% accurate and
//! confidently wrong the rest of the time is not. So the primary gate is the
//! false-execution rate, and top-1 is reported for information.
//!
//! There is no operating point in this file. It used to hold one, back when the
//! decision layer did not exist; now the thresholds come from
//! `action_decision::calibrate`, which is the thing under test.

use grain_core::action_decision::{
    calibrate, decide, Outcome, Preferences, RefuseReason, DEFAULT_ALPHA,
};
use grain_core::action_router::{equivalence_classes, IndexedAction};
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

/// Coarse shape of an outcome, which is what the golden set labels.
#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Execute(String),
    Choose,
    Escalate,
    None,
}

fn shape(outcome: &Outcome) -> Shape {
    match outcome {
        Outcome::Execute(selection) => Shape::Execute(format!(
            "{}:{}",
            selection.extension_id, selection.action_id
        )),
        Outcome::Choose { .. } => Shape::Choose,
        Outcome::Escalate(_) => Shape::Escalate,
        Outcome::Refuse(_) => Shape::None,
    }
}

struct Fixture {
    actions: Vec<IndexedAction>,
    classes: grain_core::action_router::EquivalenceMap,
    calibration: grain_core::action_decision::Calibration,
    cases: Vec<Case>,
}

fn load() -> Fixture {
    let raw = include_str!("fixtures/action_routing_golden.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden set parses");
    let mut actions = Vec::new();
    for extension in &golden.extensions {
        for decl in &extension.actions {
            actions.push(IndexedAction::from_decl(&extension.id, decl));
        }
    }
    let classes = equivalence_classes(&actions);
    let calibration = calibrate(&actions, DEFAULT_ALPHA);
    Fixture {
        actions,
        classes,
        calibration,
        cases: golden.cases,
    }
}

impl Fixture {
    fn run(&self, said: &str) -> Outcome {
        // No defaults and no foreground app: the hardest configuration, where
        // every provider ambiguity has to be asked about rather than resolved.
        decide(
            said,
            &self.actions,
            &self.classes,
            &self.calibration,
            &Preferences {
                agent_available: true,
                ..Default::default()
            },
        )
    }
}

struct Scores {
    total: usize,
    correct: usize,
    /// Ran something when the correct answer was to ask, to escalate, to say
    /// nothing, or to run something else. **The number that decides whether this
    /// ships** — all four are the same failure to a user.
    false_executions: Vec<String>,
    /// Asked or refused when it could have acted. Costs friction, not trust.
    missed: Vec<String>,
}

fn run() -> Scores {
    let fixture = load();
    let mut scores = Scores {
        total: fixture.cases.len(),
        correct: 0,
        false_executions: Vec::new(),
        missed: Vec::new(),
    };
    for case in &fixture.cases {
        let outcome = fixture.run(&case.said);
        let got = shape(&outcome);
        let expected = match case.expect.as_str() {
            "none" => Shape::None,
            "choose" => Shape::Choose,
            "escalate" => Shape::Escalate,
            id => Shape::Execute(id.to_string()),
        };
        if got == expected {
            scores.correct += 1;
            continue;
        }
        let detail = format!(
            "\"{}\" → {:?}, expected {:?} ({})",
            case.said, got, expected, case.why
        );
        match got {
            Shape::Execute(_) => scores.false_executions.push(detail),
            _ => scores.missed.push(detail),
        }
    }
    scores
}

#[test]
fn the_router_never_executes_when_it_should_have_asked() {
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
fn the_router_reaches_its_baseline_on_the_golden_set() {
    // Tier L alone, no embedder. This is the floor a user with no model
    // downloaded actually experiences, so it is asserted rather than printed.
    let scores = run();
    let rate = scores.correct as f32 / scores.total as f32;
    assert!(
        rate >= 0.95,
        "top-1 {:.1}% over {} cases; {} missed:\n  {}",
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
    let fixture = load();
    assert_eq!(fixture.run(""), Outcome::Refuse(RefuseReason::NothingHeard));
    for said in [
        "...",
        "so anyway I was thinking we should probably rewrite the parser this week",
        "what's the weather",
        "turn the lights off",
        "book a table for two",
    ] {
        assert!(
            matches!(fixture.run(said), Outcome::Refuse(_)),
            "\"{said}\" must not route"
        );
    }
}

#[test]
fn two_providers_of_one_request_ask_rather_than_picking_one() {
    // Without equivalence classes this passes for the wrong reason — every
    // media command becomes a chooser, including the unambiguous ones below.
    let fixture = load();
    assert!(matches!(fixture.run("skip this"), Outcome::Choose { .. }));
    assert!(matches!(fixture.run("next song"), Outcome::Choose { .. }));
    match fixture.run("go back") {
        Outcome::Execute(selection) => {
            assert_eq!(selection.extension_id, "spotify");
            assert_eq!(selection.action_id, "previous");
        }
        other => panic!("a single-provider request must just run: {other:?}"),
    }
}

#[test]
fn a_default_provider_turns_a_chooser_into_an_execution() {
    // The ladder's whole purpose. If this does not hold, installing a second
    // music extension permanently degrades the first.
    let fixture = load();
    let preferences = Preferences {
        default_provider: std::collections::HashMap::from([(
            "media".to_string(),
            "spotify".to_string(),
        )]),
        agent_available: true,
        ..Default::default()
    };
    match decide(
        "skip this",
        &fixture.actions,
        &fixture.classes,
        &fixture.calibration,
        &preferences,
    ) {
        Outcome::Execute(selection) => assert_eq!(selection.extension_id, "spotify"),
        other => panic!("an explicit default must end the ladder: {other:?}"),
    }
}

#[test]
fn the_same_words_in_two_domains_do_not_bleed() {
    let fixture = load();
    match fixture.run("next slide") {
        Outcome::Execute(selection) => assert_eq!(selection.action_id, "next_slide"),
        other => panic!("expected the deck action: {other:?}"),
    }
}

#[test]
fn a_destructive_action_carries_its_read_back_all_the_way_out() {
    let fixture = load();
    match fixture.run("tell Jack that I am running late") {
        Outcome::Execute(selection) => {
            assert_eq!(selection.action_id, "send_dm");
            assert!(
                selection.needs_confirmation(),
                "no score retires the read-back"
            );
        }
        other => panic!("expected Execute pending confirmation: {other:?}"),
    }
}

/// Not an assertion — a printout, so `cargo test -- --nocapture` gives the
/// table the plan asks to be published per phase.
#[test]
fn report() {
    let fixture = load();
    let scores = run();
    println!(
        "\naction routing — Tier L, no embedder, no user defaults\
         \n  cases            {}\n  top-1            {:.1}%\n  false executions {}\
         \n  missed           {}\n  alpha            {}\n  calibration      {} samples\n",
        scores.total,
        scores.correct as f32 / scores.total as f32 * 100.0,
        scores.false_executions.len(),
        scores.missed.len(),
        fixture.calibration.alpha,
        fixture.calibration.samples,
    );
    for miss in &scores.missed {
        println!("  missed: {miss}");
    }
}
