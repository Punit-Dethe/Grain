//! [GRAIN] Frozen benchmark for Capability Index V2 retrieval
//! (`docs/Extensions 2.0/PLAN.md` §7.5).
//!
//! Loads the checked-in corpus, drives the real retriever lexical-only (the
//! always-on floor every user has), and asserts the invariants that must hold at
//! any tuning. Exact production thresholds are deliberately NOT frozen here — the
//! plan defers that until baseline numbers are reviewed — so this asserts loose
//! sanity bounds and prints the full metrics for the record (`--nocapture`).

use grain_core::capability_eval::{evaluate, Corpus, CorpusAction, CorpusCase, CorpusExtension};
use grain_core::capability_index::{CapabilityIndex, QuerySource, RetrievalContext, RetrievalParams};
use std::collections::HashSet;

const CORPUS: &str = include_str!("fixtures/capability_corpus.json");
const K: usize = 8;

#[test]
fn benchmark_corpus_holds_the_retrieval_invariants() {
    let corpus = Corpus::parse(CORPUS).expect("corpus parses");
    let index = CapabilityIndex::build(corpus.inputs());
    let cases = corpus.cases();
    assert!(index.len() >= 35, "corpus should be mid-scale, got {} actions", index.len());

    let report = evaluate(&index, &cases, K, None).expect("evaluate");
    let m = &report.metrics;

    eprintln!(
        "\n== Capability retrieval benchmark (lexical-only, K={K}) ==\n\
         cases={} in_scope={} no_match={}\n\
         recall@{K}={:?} fallback={:?} top1={:?} mrr={:?}\n\
         false_exclusions={} no_match_nonempty={} no_match_exact_top={}\n\
         hot_set median={} p95={}  latency p50={}us p95={}us\n\
         tops: exact={} lexical={} dense={} hybrid={}",
        m.cases, m.in_scope_cases, m.no_match_cases,
        m.recall_at_k, m.fallback_rate, m.top1_accuracy, m.mean_reciprocal_rank,
        m.false_exclusions, m.no_match_nonempty, m.no_match_exact_top,
        m.hot_set_median, m.hot_set_p95, m.retrieval_p50_micros, m.retrieval_p95_micros,
        m.exact_tops, m.lexical_tops, m.dense_tops, m.hybrid_tops,
    );
    eprintln!("\nper-slice recall@{K}:");
    for slice in &report.slices {
        eprintln!(
            "  {:<20} recall={:?} top1={:?} cases={}",
            slice.name, slice.metrics.recall_at_k, slice.metrics.top1_accuracy, slice.metrics.cases
        );
    }
    if !report.confusion.is_empty() {
        eprintln!("\nconfusion (top1 wrong):");
        for cell in &report.confusion {
            eprintln!("  expected {:<40} got {:<24} x{}", cell.expected, cell.got, cell.count);
        }
    }

    // Invariants that must hold at any tuning:

    // A spoken address is never manufactured for a request nothing should serve.
    assert_eq!(m.no_match_exact_top, 0, "a no-match request produced an Exact top");

    // The eligibility gate never withholds an action a case says is reachable.
    assert_eq!(m.false_exclusions, 0, "an expected action was wrongly ruled ineligible");

    // The hot set is bounded — the flood guard holds.
    assert!(m.hot_set_p95 <= K, "hot set p95 {} exceeded K={K}", m.hot_set_p95);

    // The always-on lexical floor must clear a loose recall bar. If this ever
    // fails, inspect the printed per-slice table before touching thresholds.
    let recall = m.recall_at_k.expect("in-scope cases exist");
    assert!(recall >= 0.70, "lexical-only recall {recall} below the 0.70 floor");
}

// ── Scale + conflict + multi-intent stress (PLAN §7.5) ───────────────────────

fn action(id: &str, title: &str, examples: &[&str]) -> CorpusAction {
    CorpusAction {
        id: id.to_string(),
        title: title.to_string(),
        examples: examples.iter().map(|s| s.to_string()).collect(),
        utterances: Vec::new(),
        params: Vec::new(),
        tags: Vec::new(),
        when_to_use: String::new(),
        when_not_to_use: String::new(),
        disabled: false,
        platform_ok: true,
    }
}

fn case(said: &str, expect: Vec<String>, slice: &str) -> CorpusCase {
    CorpusCase {
        said: said.to_string(),
        expect,
        slices: vec![slice.to_string()],
        context_ineligible: Vec::new(),
        auth_missing: Vec::new(),
    }
}

/// Six domains × five competing providers = 30 extensions, ~80 actions, with
/// heavy near-neighbour conflict (five providers of every action). Providers in a
/// family declare identical example phrasings on purpose — the point is that a
/// topical request must recall the family without one provider owning the words,
/// and a *named* request must still pick the right provider. Provider names are
/// distinct brandables (as real extensions are), not the domain word, so naming
/// one is unambiguous.
struct Family {
    /// Domain label, for readability of the table above.
    #[allow(dead_code)]
    key: &'static str,
    /// Five distinct provider names — the searched-for topical example that
    /// follows each in a named query, and the action canonical prefix.
    names: [&'static str; 5],
    /// (action_id, title, [examples])
    actions: &'static [(&'static str, &'static str, &'static [&'static str])],
}

fn families() -> Vec<Family> {
    vec![
        Family {
            key: "music",
            names: ["tunely", "harmonix", "beatstream", "echoplay", "melodex"],
            actions: &[
                ("play", "Play music", &["put on some music", "play a song"]),
                ("pause", "Pause playback", &["pause the music"]),
                ("next", "Skip to the next track", &["skip this song", "next track"]),
            ],
        },
        Family {
            key: "issues",
            names: ["tracktor", "bugzone", "issuely", "tickethub", "flowtrack"],
            actions: &[
                ("create_issue", "Create an issue", &["file a bug", "open an issue"]),
                ("close_issue", "Close an issue", &["close this issue"]),
                ("list_issues", "List open issues", &["show open issues"]),
            ],
        },
        Family {
            key: "calendar",
            names: ["daybook", "timegrid", "meetwise", "calendra", "slotly"],
            actions: &[
                ("create_event", "Create a calendar event", &["schedule a meeting", "book a meeting"]),
                ("list_events", "List calendar events", &["what is on my calendar"]),
                ("delete_event", "Delete a calendar event", &["cancel my meeting"]),
            ],
        },
        Family {
            key: "mail",
            names: ["postbox", "mailwise", "inboxly", "letterly", "dispatchr"],
            actions: &[
                ("send_email", "Send an email", &["send an email", "email a colleague"]),
                ("archive", "Archive an email", &["archive this thread"]),
                ("search_mail", "Search your mail", &["find emails from someone"]),
            ],
        },
        Family {
            key: "notes",
            names: ["noteworthy", "scribbly", "jotbox", "memoire", "penmark"],
            actions: &[
                ("create_note", "Create a note", &["make a note", "jot something down"]),
                ("search_notes", "Search notes", &["find my notes"]),
            ],
        },
        Family {
            key: "weather",
            names: ["skycast", "climaview", "forecly", "raincheck", "temply"],
            actions: &[
                ("today", "Current weather", &["what is the weather"]),
                ("forecast", "Weather forecast", &["will it rain tomorrow"]),
            ],
        },
    ]
}

fn stress_corpus() -> Corpus {
    let families = families();
    let mut extensions = Vec::new();
    for family in &families {
        for name in family.names {
            extensions.push(CorpusExtension {
                id: format!("com.grain.{name}"),
                name: name.to_string(),
                aliases: vec![name.to_string()],
                purpose: String::new(),
                entities: Vec::new(),
                actions: family.actions.iter().map(|(id, title, ex)| action(id, title, ex)).collect(),
            });
        }
    }

    // Singleton, unique-purpose extensions mixed in with the conflict clusters —
    // the non-conflict path, and enough actions to clear the plan's ≥100 bar.
    let singletons: &[(&str, &str, &str, &[&str])] = &[
        ("stopwatch", "start", "Start a timer", &["set a timer for ten minutes"]),
        ("stopwatch2", "stop", "Stop the timer", &["stop the countdown"]),
        ("calcpad", "compute", "Compute an expression", &["add these numbers up"]),
        ("unitwise", "convert", "Convert units", &["convert miles to kilometers"]),
        ("clipwell", "copy", "Copy to clipboard", &["copy this to my clipboard"]),
        ("snapshot", "capture", "Take a screenshot", &["grab a screenshot"]),
        ("keyforge", "generate", "Generate a password", &["generate a strong password"]),
        ("qrmaker", "make", "Make a QR code", &["make a qr code for this link"]),
        ("huepick", "pick", "Pick a colour", &["pick a colour from the screen"]),
        ("lexica", "define", "Define a word", &["define the word serendipity"]),
        ("tickr", "quote", "Get a stock quote", &["look up the apple stock price"]),
        ("headliner", "top", "Show the news", &["show me the latest headlines"]),
        ("remindly", "set", "Set a reminder", &["remind me to call the dentist"]),
        ("wakeup", "alarm", "Set an alarm", &["set an alarm for seven am"]),
        ("flightfox", "search", "Search flights", &["find flights to tokyo"]),
        ("cheftime", "recipe", "Find a recipe", &["find a recipe for carbonara"]),
        ("mapster", "route", "Get directions", &["directions to the airport"]),
        ("fittrack", "log", "Log a workout", &["log a five mile run"]),
        ("moneybags", "balance", "Check balance", &["what is my account balance"]),
        ("habitly", "checkin", "Check in a habit", &["mark my habit done for today"]),
        ("readlater", "save", "Save an article", &["save this article for later"]),
        ("focuszen", "start", "Start a focus session", &["start a focus session"]),
    ];
    for (name, aid, title, ex) in singletons {
        extensions.push(CorpusExtension {
            id: format!("com.grain.{name}"),
            name: name.to_string(),
            aliases: vec![name.to_string()],
            purpose: String::new(),
            entities: Vec::new(),
            actions: vec![action(aid, title, ex)],
        });
    }

    let mut cases = Vec::new();
    // Topical: every provider of the family is acceptable (recall = any present).
    for family in &families {
        let (act, _, ex) = family.actions[0];
        let expect = family.names.iter().map(|nm| format!("{nm}.{act}")).collect();
        cases.push(case(ex[0], expect, "scale-topical"));
    }
    // A unique singleton, retrieved topically — the non-conflict case.
    for (name, aid, _, ex) in &singletons[..8] {
        cases.push(case(ex[0], vec![format!("{name}.{aid}")], "scale-singleton"));
    }
    // Named: a specific provider is named — must be the top1, and Exact.
    for family in &families {
        let (aid, _, ex) = family.actions[0];
        let name = family.names[2];
        cases.push(case(&format!("{name} {}", ex[0]), vec![format!("{name}.{aid}")], "scale-named"));
    }
    // Nothing installed serves these.
    for said in ["tell me a joke", "what is the meaning of life", "translate this sentence"] {
        cases.push(case(said, Vec::new(), "scale-nomatch"));
    }

    Corpus { extensions, cases }
}

#[test]
fn scale_conflict_and_multi_intent_hold() {
    let corpus = stress_corpus();
    let index = CapabilityIndex::build(corpus.inputs());
    assert!(index.len() >= 100, "plan §7.5 wants ≥100 actions, got {}", index.len());

    let cases = corpus.cases();
    let report = evaluate(&index, &cases, K, None).expect("evaluate");
    let m = &report.metrics;
    eprintln!(
        "\n== Scale stress ({} actions, K={K}) ==\n\
         recall@{K}={:?} top1={:?}  no_match_exact_top={}  hot_set p95={}  latency p50={}us p95={}us",
        index.len(), m.recall_at_k, m.top1_accuracy, m.no_match_exact_top, m.hot_set_p95,
        m.retrieval_p50_micros, m.retrieval_p95_micros,
    );
    for slice in &report.slices {
        eprintln!("  {:<16} recall={:?} top1={:?} cases={}", slice.name, slice.metrics.recall_at_k, slice.metrics.top1_accuracy, slice.metrics.cases);
    }

    let slice = |name: &str| report.slices.iter().find(|s| s.name == name).expect("slice");
    // A named provider is recalled and is the top1 Exact hit, even buried among
    // five identical competitors.
    assert_eq!(slice("scale-named").metrics.top1_accuracy, Some(1.0), "naming must beat the near-neighbours");
    // A topical request recalls its family without a provider owning the words.
    assert_eq!(slice("scale-topical").metrics.recall_at_k, Some(1.0), "topical family recall");
    // A unique singleton is retrieved cleanly amid the conflict clusters.
    assert_eq!(slice("scale-singleton").metrics.recall_at_k, Some(1.0), "singleton recall");
    // Nothing is manufactured for out-of-domain requests.
    assert_eq!(m.no_match_exact_top, 0);
    // The flood guard holds and retrieval stays fast even at scale.
    assert!(m.hot_set_p95 <= K);
    assert!(m.retrieval_p95_micros < 20_000, "retrieval p95 {}us too slow at scale", m.retrieval_p95_micros);

    // Multi-intent: one request naming two different capabilities must surface an
    // action for BOTH — the retriever does not collapse to a single intent.
    let empty: HashSet<String> = HashSet::new();
    let ctx = RetrievalContext { dense: None, context_ineligible: &empty, auth_missing: &empty };
    let hot = index.retrieve(
        "file a bug and schedule a meeting",
        &ctx,
        RetrievalParams { source: QuerySource::Transcript, k: K },
    );
    let ids: Vec<&str> = hot.entries.iter().map(|e| e.canonical_id.as_str()).collect();
    assert!(ids.iter().any(|id| id.contains("create_issue")), "multi-intent lost the issue: {ids:?}");
    assert!(ids.iter().any(|id| id.contains("create_event")), "multi-intent lost the event: {ids:?}");

    // Determinism at scale.
    let again = evaluate(&index, &cases, K, None).expect("evaluate");
    for (a, b) in report.results.iter().zip(again.results.iter()) {
        let ia: Vec<&str> = a.hot_set.iter().map(|e| e.canonical_id.as_str()).collect();
        let ib: Vec<&str> = b.hot_set.iter().map(|e| e.canonical_id.as_str()).collect();
        assert_eq!(ia, ib, "non-deterministic at scale for: {}", a.said);
    }
}

#[test]
fn injected_dense_lifts_ranking_on_vocab_mismatch() {
    // Validates the hybrid seam with the same per-case injection interface the
    // host uses for recommend's semantic scores. Lexical alone ranks the right
    // near-neighbour below a competitor on vocab-mismatch cases; a dense signal on
    // exactly those cases must fuse (RRF) and lift it to top1 — proving dense is
    // wired correctly and recovers precisely the gap the benchmark identified.
    let corpus = Corpus::parse(CORPUS).expect("corpus parses");
    let index = CapabilityIndex::build(corpus.inputs());
    let cases = corpus.cases();

    let lexical = evaluate(&index, &cases, K, None).expect("evaluate");

    // Simulate the embedder: give the intended action a strong cosine on exactly
    // the vocabulary-mismatch cases (where lexical ranking is weakest).
    let dense: Vec<std::collections::HashMap<String, f32>> = cases
        .iter()
        .map(|c| {
            let mut row = std::collections::HashMap::new();
            if c.slices.iter().any(|s| s == "vocab-mismatch") {
                if let Some(id) = c.expected.first() {
                    row.insert(id.clone(), 0.8);
                }
            }
            row
        })
        .collect();
    let hybrid = evaluate(&index, &cases, K, Some(&dense)).expect("evaluate");

    let vm_top1 = |report: &grain_core::capability_eval::Report| {
        report
            .slices
            .iter()
            .find(|s| s.name == "vocab-mismatch")
            .and_then(|s| s.metrics.top1_accuracy)
    };
    assert!(vm_top1(&lexical) < Some(1.0), "precondition: lexical leaves ranking room");
    assert_eq!(vm_top1(&hybrid), Some(1.0), "dense must lift vocab-mismatch to top1");
    assert!(
        hybrid.metrics.top1_accuracy >= lexical.metrics.top1_accuracy,
        "the hybrid must never rank worse overall than lexical alone"
    );
}

#[test]
fn retrieval_is_deterministic_over_the_corpus() {
    let corpus = Corpus::parse(CORPUS).expect("corpus parses");
    let index = CapabilityIndex::build(corpus.inputs());
    let cases = corpus.cases();

    let first = evaluate(&index, &cases, K, None).expect("evaluate");
    let second = evaluate(&index, &cases, K, None).expect("evaluate");
    for (a, b) in first.results.iter().zip(second.results.iter()) {
        let ids_a: Vec<&str> = a.hot_set.iter().map(|e| e.canonical_id.as_str()).collect();
        let ids_b: Vec<&str> = b.hot_set.iter().map(|e| e.canonical_id.as_str()).collect();
        assert_eq!(ids_a, ids_b, "retrieval differed across runs for: {}", a.said);
    }
}
