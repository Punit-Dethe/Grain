//! Cross-extension recommendation evaluation.
//!
//! This is deliberately separate from [`crate::eval`], which evaluates one
//! extension's private command catalogue. Recommendation is an open-set ranking
//! problem over the whole installed pool. The host injects semantic scores from
//! the production embedder; this module drives the exact production
//! [`crate::recommend::rank`] and Auto-send decision without depending on a
//! model, Tauri, or filesystem state.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::recommend::{
    auto_send_target, rank, IndexedRecommendation, Recommendation, Signal, TOPICAL_FLOOR,
};

/// Maximum number of candidates the recommendation surface presents as a
/// clarification set. The full ranking remains in each case result for analysis.
pub const CLARIFICATION_LIMIT: usize = 3;

/// One held-out request. An empty `expected` set means that no installed
/// extension should be recommended. Multiple ids are accepted for genuinely
/// ambiguous requests without forcing the corpus author to invent one truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub said: String,
    pub expected: Vec<String>,
    pub slices: Vec<String>,
}

impl Case {
    fn expects_none(&self) -> bool {
        self.expected.is_empty()
    }

    fn accepts(&self, id: &str) -> bool {
        self.expected.iter().any(|expected| expected == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalKind {
    Named,
    Topical,
}

impl From<Signal> for SignalKind {
    fn from(value: Signal) -> Self {
        match value {
            Signal::Named => Self::Named,
            Signal::Topical => Self::Topical,
        }
    }
}

/// Stable, serializable projection of the production recommendation type.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub extension_id: String,
    pub score: f32,
    pub margin: f32,
    pub signal: SignalKind,
}

impl From<&Recommendation> for Candidate {
    fn from(value: &Recommendation) -> Self {
        Self {
            extension_id: value.extension_id.clone(),
            score: value.score,
            margin: value.margin,
            signal: value.signal.into(),
        }
    }
}

/// One request's production verdict.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub said: String,
    /// Empty means `none`.
    pub expected: Vec<String>,
    pub slices: Vec<String>,
    pub ranked: Vec<Candidate>,
    /// One-based rank of the first acceptable extension.
    pub expected_rank: Option<usize>,
    pub top1_correct: bool,
    pub recall_at_3: bool,
    pub auto_sent: Option<String>,
    pub auto_send_correct: bool,
    pub ranking_micros: u64,
}

impl CaseResult {
    fn predicted_none(&self) -> bool {
        self.ranked.is_empty()
    }

    fn expects_none(&self) -> bool {
        self.expected.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfusionCell {
    pub expected: String,
    pub got: String,
    pub count: usize,
}

/// Aggregate metrics. Optional ratios are `null` when their denominator does
/// not exist, rather than reporting a reassuring but invented zero or one.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub cases: usize,
    pub in_scope_cases: usize,
    pub no_match_cases: usize,
    pub top1_accuracy: f32,
    pub in_scope_top1_accuracy: Option<f32>,
    pub recall_at_3: Option<f32>,
    pub mean_reciprocal_rank: Option<f32>,
    pub no_match_precision: Option<f32>,
    pub no_match_recall: Option<f32>,
    pub false_recommendation_rate: Option<f32>,
    pub candidate_set_coverage: Option<f32>,
    pub candidate_set_median: usize,
    pub candidate_set_p95: usize,
    pub named_tops: usize,
    pub topical_tops: usize,
    pub correct_auto_sends: usize,
    pub wrong_auto_sends: usize,
    pub ranking_p50_micros: u64,
    pub ranking_p95_micros: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceMetrics {
    pub name: String,
    pub metrics: Metrics,
}

/// A hypothetical topical floor. Named routes remain structural and therefore
/// unaffected, exactly as in production.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingPoint {
    pub topical_floor: f32,
    pub coverage: f32,
    pub selective_risk: Option<f32>,
    pub correct_fires: usize,
    pub wrong_fires: usize,
    pub abstains: usize,
    pub correct_auto_sends: usize,
    pub wrong_auto_sends: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub metrics: Metrics,
    pub slices: Vec<SliceMetrics>,
    pub confusion: Vec<ConfusionCell>,
    pub operating_points: Vec<OperatingPoint>,
    pub results: Vec<CaseResult>,
}

/// Evaluate the production ranker over an installed extension pool.
///
/// `semantic` is aligned with `cases`: one extension-id-to-score map per case,
/// or `None` when the model is not installed and the run is honestly name-only.
/// Each extension score must already use the same example aggregation as the
/// live host. Invalid or misaligned injected data is rejected rather than
/// silently producing a plausible-looking report.
pub fn evaluate(
    recommendations: &[IndexedRecommendation],
    cases: &[Case],
    semantic: Option<&[HashMap<String, f32>]>,
    auto_send_eligible: &HashSet<String>,
    auto_send_min_margin: f32,
    thresholds: &[f32],
) -> Result<Report, String> {
    validate_inputs(
        recommendations,
        cases,
        semantic,
        auto_send_eligible,
        auto_send_min_margin,
        thresholds,
    )?;
    let results = rank_cases(
        recommendations,
        cases,
        semantic,
        auto_send_eligible,
        auto_send_min_margin,
        None,
        true,
    );
    let metrics = metrics(results.iter());
    let slices = slice_metrics(&results);
    let confusion = confusion(&results);
    let operating_points = thresholds
        .iter()
        .map(|&threshold| {
            let at_threshold = rank_cases(
                recommendations,
                cases,
                semantic,
                auto_send_eligible,
                auto_send_min_margin,
                Some(threshold),
                false,
            );
            operating_point(threshold, &at_threshold)
        })
        .collect();

    Ok(Report {
        metrics,
        slices,
        confusion,
        operating_points,
        results,
    })
}

fn validate_inputs(
    recommendations: &[IndexedRecommendation],
    cases: &[Case],
    semantic: Option<&[HashMap<String, f32>]>,
    auto_send_eligible: &HashSet<String>,
    auto_send_min_margin: f32,
    thresholds: &[f32],
) -> Result<(), String> {
    let ids = recommendations
        .iter()
        .map(|recommendation| recommendation.extension_id.as_str())
        .collect::<HashSet<_>>();
    if ids.len() != recommendations.len() {
        return Err("recommendation pool contains duplicate extension ids".into());
    }
    if let Some(scores) = semantic {
        if scores.len() != cases.len() {
            return Err(format!(
                "semantic score rows ({}) do not match cases ({})",
                scores.len(),
                cases.len()
            ));
        }
        for row in scores {
            for (id, score) in row {
                if !ids.contains(id.as_str()) {
                    return Err(format!(
                        "semantic scores contain unknown extension id '{id}'"
                    ));
                }
                if !score.is_finite() {
                    return Err(format!("semantic score for '{id}' is not finite"));
                }
            }
        }
    }
    for case in cases {
        if let Some(id) = case.expected.iter().find(|id| !ids.contains(id.as_str())) {
            return Err(format!("case expects unknown extension id '{id}'"));
        }
    }
    if let Some(id) = auto_send_eligible
        .iter()
        .find(|id| !ids.contains(id.as_str()))
    {
        return Err(format!(
            "Auto-send set contains unknown extension id '{id}'"
        ));
    }
    if !auto_send_min_margin.is_finite() || !(0.0..=1.0).contains(&auto_send_min_margin) {
        return Err("Auto-send margin must be a finite value in 0.0..=1.0".into());
    }
    for threshold in thresholds {
        if !threshold.is_finite() || !(TOPICAL_FLOOR..=1.0).contains(threshold) {
            return Err(format!(
                "topical floors must be finite values in {TOPICAL_FLOOR}..=1.0"
            ));
        }
    }
    Ok(())
}

fn rank_cases(
    recommendations: &[IndexedRecommendation],
    cases: &[Case],
    semantic: Option<&[HashMap<String, f32>]>,
    auto_send_eligible: &HashSet<String>,
    auto_send_min_margin: f32,
    threshold: Option<f32>,
    measure: bool,
) -> Vec<CaseResult> {
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let scores = semantic.and_then(|all| all.get(index));
            let filtered;
            let scores = if let (Some(scores), Some(threshold)) = (scores, threshold) {
                filtered = scores
                    .iter()
                    .filter(|(_, score)| score.is_finite() && **score >= threshold)
                    .map(|(id, score)| (id.clone(), *score))
                    .collect::<HashMap<_, _>>();
                Some(&filtered)
            } else {
                scores
            };

            let started = Instant::now();
            let ranked = rank(recommendations, &case.said, scores, &[]);
            let ranking_micros = if measure {
                started.elapsed().as_micros().min(u64::MAX as u128) as u64
            } else {
                0
            };
            let expected_rank = if case.expects_none() {
                None
            } else {
                ranked
                    .iter()
                    .position(|candidate| case.accepts(&candidate.extension_id))
                    .map(|position| position + 1)
            };
            let top1_correct = if case.expects_none() {
                ranked.is_empty()
            } else {
                ranked
                    .first()
                    .is_some_and(|candidate| case.accepts(&candidate.extension_id))
            };
            let recall_at_3 = !case.expects_none()
                && expected_rank.is_some_and(|position| position <= CLARIFICATION_LIMIT);
            let auto_sent = auto_send_target(&ranked, auto_send_eligible, auto_send_min_margin);
            let auto_send_correct = auto_sent
                .as_deref()
                .is_some_and(|id| !case.expects_none() && case.accepts(id));

            CaseResult {
                said: case.said.clone(),
                expected: case.expected.clone(),
                slices: case.slices.clone(),
                ranked: ranked.iter().map(Candidate::from).collect(),
                expected_rank,
                top1_correct,
                recall_at_3,
                auto_sent,
                auto_send_correct,
                ranking_micros,
            }
        })
        .collect()
}

fn metrics<'a>(results: impl Iterator<Item = &'a CaseResult>) -> Metrics {
    let results = results.collect::<Vec<_>>();
    let cases = results.len();
    let in_scope_cases = results
        .iter()
        .filter(|result| !result.expects_none())
        .count();
    let no_match_cases = cases - in_scope_cases;
    let top1_correct = results.iter().filter(|result| result.top1_correct).count();
    let in_scope_top1 = results
        .iter()
        .filter(|result| !result.expects_none() && result.top1_correct)
        .count();
    let recall_at_3_count = results.iter().filter(|result| result.recall_at_3).count();
    let reciprocal_rank_sum = results
        .iter()
        .filter(|result| !result.expects_none())
        .filter_map(|result| result.expected_rank)
        .map(|rank| 1.0 / rank as f32)
        .sum::<f32>();

    let predicted_none = results
        .iter()
        .filter(|result| result.predicted_none())
        .count();
    let correct_none = results
        .iter()
        .filter(|result| result.expects_none() && result.predicted_none())
        .count();
    let false_recommendations = results
        .iter()
        .filter(|result| result.expects_none() && !result.predicted_none())
        .count();
    let correct_auto_sends = results
        .iter()
        .filter(|result| result.auto_send_correct)
        .count();
    let wrong_auto_sends = results
        .iter()
        .filter(|result| result.auto_sent.is_some() && !result.auto_send_correct)
        .count();
    let named_tops = results
        .iter()
        .filter(|result| {
            result
                .ranked
                .first()
                .is_some_and(|candidate| candidate.signal == SignalKind::Named)
        })
        .count();
    let topical_tops = results
        .iter()
        .filter(|result| {
            result
                .ranked
                .first()
                .is_some_and(|candidate| candidate.signal == SignalKind::Topical)
        })
        .count();

    let mut candidate_sizes = results
        .iter()
        .map(|result| result.ranked.len().min(CLARIFICATION_LIMIT))
        .collect::<Vec<_>>();
    candidate_sizes.sort_unstable();
    let mut ranking_micros = results
        .iter()
        .map(|result| result.ranking_micros)
        .collect::<Vec<_>>();
    ranking_micros.sort_unstable();

    Metrics {
        cases,
        in_scope_cases,
        no_match_cases,
        top1_accuracy: ratio(top1_correct, cases).unwrap_or(0.0),
        in_scope_top1_accuracy: ratio(in_scope_top1, in_scope_cases),
        recall_at_3: ratio(recall_at_3_count, in_scope_cases),
        mean_reciprocal_rank: (in_scope_cases > 0)
            .then_some(reciprocal_rank_sum / in_scope_cases as f32),
        no_match_precision: ratio(correct_none, predicted_none),
        no_match_recall: ratio(correct_none, no_match_cases),
        false_recommendation_rate: ratio(false_recommendations, no_match_cases),
        candidate_set_coverage: ratio(recall_at_3_count, in_scope_cases),
        candidate_set_median: percentile(&candidate_sizes, 50),
        candidate_set_p95: percentile(&candidate_sizes, 95),
        named_tops,
        topical_tops,
        correct_auto_sends,
        wrong_auto_sends,
        ranking_p50_micros: percentile(&ranking_micros, 50),
        ranking_p95_micros: percentile(&ranking_micros, 95),
    }
}

fn ratio(numerator: usize, denominator: usize) -> Option<f32> {
    (denominator > 0).then_some(numerator as f32 / denominator as f32)
}

/// Nearest-rank percentile. Small deterministic eval samples should not invent
/// interpolated latencies or candidate counts that never occurred.
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
    for result in results.iter().filter(|result| !result.top1_correct) {
        let expected = if result.expects_none() {
            "none".to_string()
        } else {
            result.expected.join("|")
        };
        let got = result
            .ranked
            .first()
            .map_or_else(|| "none".to_string(), |top| top.extension_id.clone());
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

fn operating_point(threshold: f32, results: &[CaseResult]) -> OperatingPoint {
    let correct_fires = results
        .iter()
        .filter(|result| !result.predicted_none() && result.top1_correct)
        .count();
    let wrong_fires = results
        .iter()
        .filter(|result| !result.predicted_none() && !result.top1_correct)
        .count();
    let abstains = results
        .iter()
        .filter(|result| result.predicted_none())
        .count();
    let fires = correct_fires + wrong_fires;
    OperatingPoint {
        topical_floor: threshold.max(TOPICAL_FLOOR),
        coverage: ratio(fires, results.len()).unwrap_or(0.0),
        selective_risk: ratio(wrong_fires, fires),
        correct_fires,
        wrong_fires,
        abstains,
        correct_auto_sends: results
            .iter()
            .filter(|result| result.auto_send_correct)
            .count(),
        wrong_auto_sends: results
            .iter()
            .filter(|result| result.auto_sent.is_some() && !result.auto_send_correct)
            .count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, aliases: &[&str]) -> IndexedRecommendation {
        IndexedRecommendation::new(
            id,
            &aliases
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    }

    fn case(said: &str, expected: &[&str], slices: &[&str]) -> Case {
        Case {
            said: said.to_string(),
            expected: expected.iter().map(|value| value.to_string()).collect(),
            slices: slices.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn score(rows: &[(&str, f32)]) -> HashMap<String, f32> {
        rows.iter()
            .map(|(id, score)| (id.to_string(), *score))
            .collect()
    }

    #[test]
    fn recommendation_metrics_cover_rank_none_confusion_and_slices() {
        let recommendations = [rec("spotify", &["spotify"]), rec("github", &["github"])];
        let cases = [
            case("play something", &["spotify"], &["clean"]),
            case("open github", &["github"], &["clean", "named"]),
            case("what is the weather", &[], &["none"]),
            case("file an issue", &["github"], &["hard"]),
        ];
        let semantic = [
            score(&[("spotify", 0.80), ("github", 0.54)]),
            score(&[("spotify", 0.70), ("github", 0.60)]),
            score(&[("spotify", 0.30), ("github", 0.20)]),
            score(&[("spotify", 0.71), ("github", 0.69)]),
        ];
        let eligible = HashSet::from(["spotify".to_string()]);
        let report = evaluate(
            &recommendations,
            &cases,
            Some(&semantic),
            &eligible,
            0.15,
            &[0.50, 0.75],
        )
        .unwrap();

        assert_eq!(report.metrics.cases, 4);
        assert_eq!(report.metrics.in_scope_cases, 3);
        assert_eq!(report.metrics.no_match_cases, 1);
        assert!((report.metrics.top1_accuracy - 0.75).abs() < 1e-6);
        assert_eq!(report.metrics.wrong_auto_sends, 0);
        assert_eq!(report.confusion[0].expected, "github");
        assert_eq!(report.confusion[0].got, "spotify");
        assert_eq!(
            report
                .slices
                .iter()
                .map(|slice| slice.name.as_str())
                .collect::<Vec<_>>(),
            vec!["clean", "hard", "named", "none"]
        );
        assert_eq!(report.results[1].ranked[0].signal, SignalKind::Named);
        assert!(report.results[2].ranked.is_empty());
    }

    #[test]
    fn multiple_expected_extensions_credit_the_first_acceptable_rank() {
        let recommendations = [rec("spotify", &[]), rec("youtube", &[])];
        let cases = [case("play music", &["spotify", "youtube"], &[])];
        let semantic = [score(&[("spotify", 0.70), ("youtube", 0.80)])];
        let report = evaluate(
            &recommendations,
            &cases,
            Some(&semantic),
            &HashSet::new(),
            0.15,
            &[],
        )
        .unwrap();
        assert!(report.results[0].top1_correct);
        assert_eq!(report.results[0].expected_rank, Some(1));
        assert!(report.confusion.is_empty());
    }

    #[test]
    fn name_only_run_is_honest_and_never_auto_sends() {
        let recommendations = [rec("github", &["github"])];
        let cases = [
            case("open github", &["github"], &[]),
            case("file an issue", &["github"], &[]),
        ];
        let report = evaluate(
            &recommendations,
            &cases,
            None,
            &HashSet::from(["github".to_string()]),
            0.15,
            &[],
        )
        .unwrap();
        assert!(report.results[0].top1_correct);
        assert!(report.results[0].auto_sent.is_none());
        assert!(report.results[1].ranked.is_empty());
    }

    #[test]
    fn a_higher_operating_floor_reduces_wrong_fires() {
        let recommendations = [rec("a", &[]), rec("b", &[])];
        let cases = [
            case("right", &["a"], &[]),
            case("wrong", &["b"], &[]),
            case("none", &[], &[]),
        ];
        let semantic = [
            score(&[("a", 0.82), ("b", 0.55)]),
            score(&[("a", 0.62), ("b", 0.60)]),
            score(&[("a", 0.56), ("b", 0.55)]),
        ];
        let report = evaluate(
            &recommendations,
            &cases,
            Some(&semantic),
            &HashSet::new(),
            0.15,
            &[0.50, 0.70],
        )
        .unwrap();
        assert_eq!(report.operating_points[0].wrong_fires, 2);
        assert_eq!(report.operating_points[1].wrong_fires, 0);
        assert_eq!(report.operating_points[1].abstains, 2);
    }

    #[test]
    fn malformed_injected_scores_are_rejected() {
        let recommendations = [rec("github", &[])];
        let cases = [case("file an issue", &["github"], &[])];
        assert!(evaluate(
            &recommendations,
            &cases,
            Some(&[]),
            &HashSet::new(),
            0.15,
            &[TOPICAL_FLOOR],
        )
        .is_err());
        assert!(evaluate(
            &recommendations,
            &cases,
            Some(&[score(&[("unknown", 0.8)])]),
            &HashSet::new(),
            0.15,
            &[TOPICAL_FLOOR],
        )
        .is_err());
    }
}
