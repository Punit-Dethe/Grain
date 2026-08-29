//! [GRAIN] Capability Index V2 retrieval evaluation
//! (`docs/Extensions 2.0/PLAN.md` §7.5, `docs/Extensions 2.0/IMPLEMENTATION-NOTES.md`).
//!
//! Drives the real [`crate::capability_index::CapabilityIndex::retrieve`] over a
//! held-out corpus, headless — no embedder, no Tauri, no filesystem. The
//! successor to [`crate::recommend_eval`], which scored *extension* recommendation;
//! this scores *action* retrieval into the Agent hot set.
//!
//! ## The objective, sharpened
//!
//! The Agent has `search_actions` as a backstop, so a hot-set **miss** costs one
//! round trip while a **flood** costs selection accuracy and latency with no
//! recovery. So the headline is eligible **Recall@K at a small K**, co-reported
//! with the **fallback rate** (miss = a `search_actions` call is needed), the
//! **hot-set size** distribution (the flood signal), and near-neighbour
//! **confusion**. Recall bought by dumping everything is not recall.
//!
//! ## Honest degradation
//!
//! Runs lexical-only when no dense scores are injected — exactly what most users
//! get, since the embedder is an opt-in download. Injected dense scores (aligned
//! one row per case) drive the hybrid path when the host feeds the real embedder,
//! the same split [`crate::recommend_eval`] already uses.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::capability_index::{
    ActionInput, ActionParamInput, CapabilityIndex, Provenance, QuerySource, RetrievalContext,
    RetrievalParams,
};

/// One held-out request. `expected` is the set of acceptable canonical ids; an
/// empty set is a **no-match** case (nothing installed should be called). The
/// per-case eligibility sets model a surface the action is not valid on, or an
/// account the user has not connected — the action is correctly withheld.
#[derive(Clone, Debug, Default)]
pub struct Case {
    pub said: String,
    pub expected: Vec<String>,
    pub slices: Vec<String>,
    pub context_ineligible: Vec<String>,
    pub auth_missing: Vec<String>,
}

impl Case {
    fn expects_none(&self) -> bool {
        self.expected.is_empty()
    }
    fn accepts(&self, id: &str) -> bool {
        self.expected.iter().any(|expected| expected == id)
    }
}

/// A stable, serializable projection of one hot-set entry.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub canonical_id: String,
    pub provenance: ProvenanceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceKind {
    Exact,
    Lexical,
    Dense,
    Hybrid,
}

impl From<Provenance> for ProvenanceKind {
    fn from(value: Provenance) -> Self {
        match value {
            Provenance::Exact => Self::Exact,
            Provenance::Lexical => Self::Lexical,
            Provenance::Dense => Self::Dense,
            Provenance::Hybrid => Self::Hybrid,
        }
    }
}

/// One request's retrieval verdict.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub said: String,
    pub expected: Vec<String>,
    pub slices: Vec<String>,
    pub hot_set: Vec<Entry>,
    /// One-based rank of the first acceptable action in the hot set.
    pub expected_rank: Option<usize>,
    pub top1_correct: bool,
    pub recall: bool,
    /// The expected action was withheld by the eligibility gate — a retrieval
    /// bug when the case says it should be reachable.
    pub false_exclusion: bool,
    pub hot_set_size: usize,
    pub truncated: usize,
    pub retrieval_micros: u64,
}

impl CaseResult {
    fn predicted_none(&self) -> bool {
        self.hot_set.is_empty()
    }
    fn top_provenance(&self) -> Option<ProvenanceKind> {
        self.hot_set.first().map(|entry| entry.provenance)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfusionCell {
    pub expected: String,
    pub got: String,
    pub count: usize,
}

/// Aggregate metrics. Optional ratios are `null` when their denominator is zero,
/// rather than reporting a reassuring but invented value.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub cases: usize,
    pub in_scope_cases: usize,
    pub no_match_cases: usize,
    /// The headline: an acceptable action is anywhere in the hot set.
    pub recall_at_k: Option<f32>,
    /// A `search_actions` round trip is needed. `1 - recall_at_k`, surfaced
    /// explicitly because it is the cost, not the score.
    pub fallback_rate: Option<f32>,
    pub top1_accuracy: Option<f32>,
    pub mean_reciprocal_rank: Option<f32>,
    /// In-scope cases where the expected action was wrongly ruled ineligible.
    pub false_exclusions: usize,
    /// No-match cases that still produced a non-empty hot set (tolerable — the
    /// Agent filters — but tracked).
    pub no_match_nonempty: usize,
    /// No-match cases whose top entry is an Exact hit (a real problem: a spoken
    /// address was manufactured for a request nothing should serve).
    pub no_match_exact_top: usize,
    pub hot_set_median: usize,
    pub hot_set_p95: usize,
    pub exact_tops: usize,
    pub lexical_tops: usize,
    pub dense_tops: usize,
    pub hybrid_tops: usize,
    pub retrieval_p50_micros: u64,
    pub retrieval_p95_micros: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceMetrics {
    pub name: String,
    pub metrics: Metrics,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub metrics: Metrics,
    pub slices: Vec<SliceMetrics>,
    pub confusion: Vec<ConfusionCell>,
    pub results: Vec<CaseResult>,
}

/// Evaluate action retrieval over a corpus at hot-set size `k`.
///
/// `dense`, when present, is aligned one row per case (canonical id -> cosine),
/// exactly as the host would inject it; `None` is honest lexical-only mode.
/// Misaligned dense data is rejected rather than silently scored.
pub fn evaluate(
    index: &CapabilityIndex,
    cases: &[Case],
    k: usize,
    dense: Option<&[HashMap<String, f32>]>,
) -> Result<Report, String> {
    if let Some(rows) = dense {
        if rows.len() != cases.len() {
            return Err(format!(
                "dense score rows ({}) do not match cases ({})",
                rows.len(),
                cases.len()
            ));
        }
    }

    let results: Vec<CaseResult> = cases
        .iter()
        .enumerate()
        .map(|(row, case)| run_case(index, case, k, dense.and_then(|d| d.get(row))))
        .collect();

    let metrics = metrics(results.iter());
    let slices = slice_metrics(&results);
    let confusion = confusion(&results);
    Ok(Report {
        metrics,
        slices,
        confusion,
        results,
    })
}

fn run_case(
    index: &CapabilityIndex,
    case: &Case,
    k: usize,
    dense: Option<&HashMap<String, f32>>,
) -> CaseResult {
    let context_ineligible: HashSet<String> = case.context_ineligible.iter().cloned().collect();
    let auth_missing: HashSet<String> = case.auth_missing.iter().cloned().collect();
    let ctx = RetrievalContext {
        dense,
        context_ineligible: &context_ineligible,
        auth_missing: &auth_missing,
    };

    let started = Instant::now();
    let hot = index.retrieve(
        &case.said,
        &ctx,
        RetrievalParams {
            source: QuerySource::Transcript,
            k,
        },
    );
    let retrieval_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;

    let hot_set: Vec<Entry> = hot
        .entries
        .iter()
        .map(|entry| Entry {
            canonical_id: entry.canonical_id.clone(),
            provenance: entry.provenance.into(),
        })
        .collect();

    let expected_rank = if case.expects_none() {
        None
    } else {
        hot_set
            .iter()
            .position(|entry| case.accepts(&entry.canonical_id))
            .map(|position| position + 1)
    };
    let top1_correct = if case.expects_none() {
        hot_set.is_empty()
    } else {
        hot_set
            .first()
            .is_some_and(|entry| case.accepts(&entry.canonical_id))
    };
    let recall = !case.expects_none() && expected_rank.is_some();
    // A false exclusion is an expected action the eligibility gate withheld —
    // only meaningful when the case expects that action to be reachable.
    let false_exclusion = !case.expects_none()
        && hot
            .excluded
            .iter()
            .any(|excluded| case.accepts(&excluded.canonical_id));

    CaseResult {
        said: case.said.clone(),
        expected: case.expected.clone(),
        slices: case.slices.clone(),
        hot_set_size: hot_set.len(),
        truncated: hot.truncated,
        hot_set,
        expected_rank,
        top1_correct,
        recall,
        false_exclusion,
        retrieval_micros,
    }
}

fn metrics<'a>(results: impl Iterator<Item = &'a CaseResult>) -> Metrics {
    let results: Vec<&CaseResult> = results.collect();
    let cases = results.len();
    let in_scope: Vec<&&CaseResult> = results.iter().filter(|r| !r.expected.is_empty()).collect();
    let no_match: Vec<&&CaseResult> = results.iter().filter(|r| r.expected.is_empty()).collect();
    let in_scope_cases = in_scope.len();
    let no_match_cases = no_match.len();

    let recalled = in_scope.iter().filter(|r| r.recall).count();
    let top1 = in_scope.iter().filter(|r| r.top1_correct).count();
    let reciprocal: f32 = in_scope
        .iter()
        .filter_map(|r| r.expected_rank)
        .map(|rank| 1.0 / rank as f32)
        .sum();
    let false_exclusions = in_scope.iter().filter(|r| r.false_exclusion).count();

    let no_match_nonempty = no_match.iter().filter(|r| !r.predicted_none()).count();
    let no_match_exact_top = no_match
        .iter()
        .filter(|r| r.top_provenance() == Some(ProvenanceKind::Exact))
        .count();

    let mut sizes: Vec<usize> = results.iter().map(|r| r.hot_set_size).collect();
    sizes.sort_unstable();
    let mut micros: Vec<u64> = results.iter().map(|r| r.retrieval_micros).collect();
    micros.sort_unstable();

    let count_top = |kind: ProvenanceKind| {
        results
            .iter()
            .filter(|r| r.top_provenance() == Some(kind))
            .count()
    };

    Metrics {
        cases,
        in_scope_cases,
        no_match_cases,
        recall_at_k: ratio(recalled, in_scope_cases),
        fallback_rate: ratio(in_scope_cases - recalled, in_scope_cases),
        top1_accuracy: ratio(top1, in_scope_cases),
        mean_reciprocal_rank: (in_scope_cases > 0).then_some(reciprocal / in_scope_cases as f32),
        false_exclusions,
        no_match_nonempty,
        no_match_exact_top,
        hot_set_median: percentile(&sizes, 50),
        hot_set_p95: percentile(&sizes, 95),
        exact_tops: count_top(ProvenanceKind::Exact),
        lexical_tops: count_top(ProvenanceKind::Lexical),
        dense_tops: count_top(ProvenanceKind::Dense),
        hybrid_tops: count_top(ProvenanceKind::Hybrid),
        retrieval_p50_micros: percentile(&micros, 50),
        retrieval_p95_micros: percentile(&micros, 95),
    }
}

fn ratio(numerator: usize, denominator: usize) -> Option<f32> {
    (denominator > 0).then_some(numerator as f32 / denominator as f32)
}

/// Nearest-rank percentile — a small deterministic sample should not invent
/// interpolated values that never occurred.
fn percentile<T: Copy + Default>(sorted: &[T], percentile: usize) -> T {
    if sorted.is_empty() {
        return T::default();
    }
    let index = (percentile * sorted.len()).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn slice_metrics(results: &[CaseResult]) -> Vec<SliceMetrics> {
    let mut by_slice: BTreeMap<&str, Vec<&CaseResult>> = BTreeMap::new();
    for result in results {
        for slice in &result.slices {
            by_slice.entry(slice).or_default().push(result);
        }
    }
    by_slice
        .into_iter()
        .map(|(name, results)| SliceMetrics {
            name: name.to_string(),
            metrics: metrics(results.into_iter()),
        })
        .collect()
}

fn confusion(results: &[CaseResult]) -> Vec<ConfusionCell> {
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for result in results
        .iter()
        .filter(|r| !r.expected.is_empty() && !r.top1_correct)
    {
        let expected = result.expected.join("|");
        let got = result
            .hot_set
            .first()
            .map_or_else(|| "none".to_string(), |entry| entry.canonical_id.clone());
        *counts.entry((expected, got)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((expected, got), count)| ConfusionCell {
            expected,
            got,
            count,
        })
        .collect()
}

// ── Corpus loader ────────────────────────────────────────────────────────────
//
// A small JSON schema so a benchmark corpus can carry the V2 target fields
// (examples, whenToUse/whenNotToUse, tags) that today's manifest does not yet
// have. Kept separate from the manifest parser on purpose — the corpus is
// evaluation material, not an installable pack.

#[derive(Clone, Debug, Deserialize)]
pub struct Corpus {
    pub extensions: Vec<CorpusExtension>,
    pub cases: Vec<CorpusCase>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CorpusExtension {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub actions: Vec<CorpusAction>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CorpusAction {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub utterances: Vec<String>,
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, rename = "whenToUse")]
    pub when_to_use: String,
    #[serde(default, rename = "whenNotToUse")]
    pub when_not_to_use: String,
    /// Static ineligibility, so a corpus can include an off-platform or disabled
    /// action as a distractor.
    #[serde(default)]
    pub disabled: bool,
    #[serde(default = "default_true", rename = "platformOk")]
    pub platform_ok: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct CorpusCase {
    pub said: String,
    #[serde(default)]
    pub expect: Vec<String>,
    #[serde(default)]
    pub slices: Vec<String>,
    #[serde(default, rename = "contextIneligible")]
    pub context_ineligible: Vec<String>,
    #[serde(default, rename = "authMissing")]
    pub auth_missing: Vec<String>,
}

/// Last dotted segment of a reverse-dns id — the human-readable namespace.
fn namespace_of(extension_id: &str) -> &str {
    extension_id.rsplit('.').next().unwrap_or(extension_id)
}

impl Corpus {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("corpus parse: {error}"))
    }

    /// Project the corpus into retriever inputs, carrying the richer V2 fields.
    pub fn inputs(&self) -> Vec<ActionInput> {
        let mut inputs = Vec::new();
        for ext in &self.extensions {
            let namespace = namespace_of(&ext.id).to_string();
            let mut aliases = vec![ext.name.clone()];
            aliases.extend(ext.aliases.iter().cloned());
            aliases.retain(|alias| !alias.trim().is_empty());
            aliases.dedup();
            let mut provider_context = Vec::new();
            if !ext.purpose.is_empty() {
                provider_context.push(ext.purpose.clone());
            }

            for action in &ext.actions {
                // Utterance literals become phrasings; params come either from the
                // declared list or the `{param}` names inside the utterances.
                let mut phrases = Vec::new();
                let mut param_names: Vec<String> = action.params.clone();
                for utterance in &action.utterances {
                    if let Ok(parts) = grain_sdk::manifest::parse_utterance(utterance.trim()) {
                        let mut literal = Vec::new();
                        for part in &parts {
                            match part {
                                grain_sdk::manifest::UtterancePart::Literal(text) => {
                                    literal.push(text.clone())
                                }
                                grain_sdk::manifest::UtterancePart::Param(name) => {
                                    if !param_names.contains(name) {
                                        param_names.push(name.clone());
                                    }
                                }
                            }
                        }
                        let phrase = literal.join(" ");
                        if !phrase.trim().is_empty() {
                            phrases.push(phrase);
                        }
                    }
                }
                let mut tags = ext.entities.clone();
                tags.extend(action.tags.iter().cloned());

                inputs.push(ActionInput {
                    canonical_id: format!("{namespace}.{}", action.id.trim()),
                    extension_id: ext.id.clone(),
                    action_id: action.id.trim().to_string(),
                    provider_name: ext.name.clone(),
                    title: action.title.trim().to_string(),
                    aliases: aliases.clone(),
                    namespaces: vec![namespace.clone()],
                    tags,
                    examples: action.examples.clone(),
                    phrases,
                    params: param_names
                        .into_iter()
                        .map(|name| ActionParamInput {
                            name,
                            kind: grain_sdk::manifest::ActionParamKind::Text,
                            required: false,
                        })
                        .collect(),
                    when_to_use: action.when_to_use.clone(),
                    when_not_to_use: action.when_not_to_use.clone(),
                    description: String::new(),
                    provider_context: provider_context.clone(),
                    risk: grain_sdk::manifest::ActionRisk::Safe,
                    enabled: !action.disabled,
                    platform_ok: action.platform_ok,
                    quarantined: false,
                });
            }
        }
        inputs
    }

    pub fn cases(&self) -> Vec<Case> {
        self.cases
            .iter()
            .map(|case| Case {
                said: case.said.clone(),
                expected: case.expect.clone(),
                slices: case.slices.clone(),
                context_ineligible: case.context_ineligible.clone(),
                auth_missing: case.auth_missing.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(said: &str, expect: &[&str], slices: &[&str]) -> Case {
        Case {
            said: said.to_string(),
            expected: expect.iter().map(|s| s.to_string()).collect(),
            slices: slices.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    /// A tiny corpus exercised directly, so the metric plumbing is tested without
    /// depending on the large fixture.
    fn small_corpus() -> Corpus {
        let json = serde_json::json!({
            "extensions": [
                { "id": "com.grain.spotify", "name": "Spotify", "aliases": ["spotify"], "entities": ["artist"],
                  "actions": [
                    { "id": "next", "title": "Skip to the next track", "utterances": ["skip this", "next song"], "examples": ["skip this track"] },
                    { "id": "play", "title": "Play an artist", "utterances": ["play {artist}"], "examples": ["put on some jazz"] }
                  ] },
                { "id": "com.grain.github", "name": "GitHub", "aliases": ["github"], "entities": ["repository"],
                  "actions": [
                    { "id": "create_issue", "title": "Create an issue", "examples": ["file a bug report", "open an issue"] }
                  ] }
            ],
            "cases": []
        });
        Corpus::parse(&json.to_string()).unwrap()
    }

    #[test]
    fn recall_top1_and_fallback_are_computed() {
        let index = CapabilityIndex::build(small_corpus().inputs());
        let cases = [
            case("skip this", &["spotify.next"], &["exact"]),
            case(
                "file a bug report",
                &["github.create_issue"],
                &["paraphrase"],
            ),
            case("tell me a joke", &[], &["no-match"]),
        ];
        let report = evaluate(&index, &cases, 8, None).unwrap();
        assert_eq!(report.metrics.in_scope_cases, 2);
        assert_eq!(report.metrics.no_match_cases, 1);
        assert_eq!(report.metrics.recall_at_k, Some(1.0));
        assert_eq!(report.metrics.fallback_rate, Some(0.0));
        assert_eq!(report.metrics.top1_accuracy, Some(1.0));
        // "tell me a joke" must not manufacture an Exact hit.
        assert_eq!(report.metrics.no_match_exact_top, 0);
    }

    #[test]
    fn a_missed_action_counts_as_fallback_not_recall() {
        let index = CapabilityIndex::build(small_corpus().inputs());
        // Nothing in the corpus is about the weather; the in-scope expectation
        // cannot be met, so it is a fallback (search_actions would be needed).
        let cases = [case(
            "what's the weather like",
            &["weather.today"],
            &["miss"],
        )];
        let report = evaluate(&index, &cases, 8, None).unwrap();
        assert_eq!(report.metrics.recall_at_k, Some(0.0));
        assert_eq!(report.metrics.fallback_rate, Some(1.0));
    }

    #[test]
    fn injected_dense_rows_must_align_with_cases() {
        let index = CapabilityIndex::build(small_corpus().inputs());
        let cases = [case("skip this", &["spotify.next"], &[])];
        assert!(evaluate(&index, &cases, 8, Some(&[])).is_err());
    }

    #[test]
    fn a_false_exclusion_is_flagged_separately_from_a_miss() {
        let index = CapabilityIndex::build(small_corpus().inputs());
        // The action exists and matches, but the case marks it context-ineligible;
        // the retriever correctly withholds it, and because the case still expects
        // it, that is recorded as a false exclusion rather than a plain miss.
        let mut c = case("skip this", &["spotify.next"], &["scoped"]);
        c.context_ineligible = vec!["spotify.next".to_string()];
        let report = evaluate(&index, &[c], 8, None).unwrap();
        assert_eq!(report.metrics.false_exclusions, 1);
        assert_eq!(report.metrics.recall_at_k, Some(0.0));
    }
}
