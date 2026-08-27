//! [GRAIN] Frozen benchmark for Capability Index V2 retrieval
//! (`docs/Extensions 2.0/PLAN.md` §7.5).
//!
//! Loads the checked-in corpus, drives the real retriever lexical-only (the
//! always-on floor every user has), and asserts the invariants that must hold at
//! any tuning. Exact production thresholds are deliberately NOT frozen here — the
//! plan defers that until baseline numbers are reviewed — so this asserts loose
//! sanity bounds and prints the full metrics for the record (`--nocapture`).

use grain_core::capability_eval::{evaluate, Corpus};
use grain_core::capability_index::CapabilityIndex;

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
