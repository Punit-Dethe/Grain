//! Cross-extension recommendation mode for the headless eval command.
//!
//! The pure metrics and production ranking decision live in
//! `grain_core::recommend_eval`. This module owns only the versioned golden-file
//! contract, manifest loading, the real on-device embedder, timings, and output.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use grain_core::recommend::{IndexedRecommendation, AUTO_SEND_MIN_MARGIN, TOPICAL_FLOOR};
use grain_core::recommend_eval::{self, Case, Report};
use grain_sdk::{ExtensionManifest, ExtensionProjectManifest};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_EXTENSIONS: usize = 256;
const MAX_CASES: usize = 10_000;
const MAX_UTTERANCE_BYTES: usize = 4_096;
const MAX_SLICES_PER_CASE: usize = 8;
const MAX_SLICE_BYTES: usize = 32;
const DEFAULT_THRESHOLDS: &[f32] = &[0.50, 0.55, 0.60, 0.65, 0.70, 0.80, 0.90];

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Recommendation,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SemanticMode {
    #[default]
    Required,
    Optional,
    Disabled,
}

impl SemanticMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Golden {
    schema_version: u32,
    mode: Mode,
    #[serde(default)]
    corpus: Option<String>,
    extensions: Vec<GoldenExtension>,
    cases: Vec<GoldenCase>,
    #[serde(default)]
    thresholds: Option<Vec<f32>>,
    #[serde(default)]
    semantic_mode: SemanticMode,
    #[serde(default)]
    auto_send_min_margin: Option<f32>,
    #[serde(default)]
    quality_gates: Option<QualityGates>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenExtension {
    manifest: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoldenCase {
    said: String,
    expect: Expected,
    #[serde(default)]
    slices: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Expected {
    One(String),
    Any(Vec<String>),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityGates {
    #[serde(default)]
    min_top1_accuracy: Option<f32>,
    #[serde(default)]
    min_in_scope_top1_accuracy: Option<f32>,
    #[serde(default)]
    min_recall_at_3: Option<f32>,
    #[serde(default)]
    min_no_match_precision: Option<f32>,
    #[serde(default)]
    min_no_match_recall: Option<f32>,
    #[serde(default)]
    max_false_recommendation_rate: Option<f32>,
    #[serde(default)]
    max_wrong_auto_sends: Option<usize>,
}

#[derive(Debug)]
struct LoadedExtension {
    id: String,
    indexed: IndexedRecommendation,
    examples: Vec<String>,
    auto_send_eligible: bool,
}

#[derive(Default)]
struct SemanticRun {
    scores: Option<Vec<HashMap<String, f32>>>,
    index_embedding_millis: u64,
    query_embedding_millis: Vec<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo<'a> {
    repository: &'a str,
    revision: &'a str,
    dimensions: usize,
    query_instruction: &'a str,
    semantic_mode: SemanticMode,
    semantic_ran: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Performance {
    index_embedding_millis: u64,
    query_embedding_p50_millis: u64,
    query_embedding_p95_millis: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QualityStatus {
    configured: bool,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'a> {
    schema_version: u32,
    mode: &'static str,
    corpus: Option<&'a str>,
    extension_count: usize,
    model: ModelInfo<'a>,
    performance: Performance,
    quality: &'a QualityStatus,
    report: &'a Report,
}

/// Parse and run a recommendation golden file. `raw` has already passed the
/// shared eval input-size bound.
pub fn run(golden_path: &Path, raw: &str, json: bool) -> Result<(), String> {
    let golden: Golden = serde_json::from_str(raw).map_err(|error| {
        format!(
            "parse recommendation golden {}: {error}",
            golden_path.display()
        )
    })?;
    validate_golden_shape(&golden)?;
    validate_quality_gates(golden.quality_gates.as_ref())?;
    let corpus = golden.corpus.clone();

    let loaded = load_extensions(golden_path, &golden.extensions)?;
    let known_ids = loaded
        .iter()
        .map(|extension| extension.id.clone())
        .collect::<HashSet<_>>();
    let cases = load_cases(golden.cases, &known_ids)?;
    let thresholds = validate_thresholds(golden.thresholds.as_deref())?;
    let auto_send_min_margin = validate_margin(golden.auto_send_min_margin)?;

    let semantic = match golden.semantic_mode {
        SemanticMode::Disabled => SemanticRun::default(),
        SemanticMode::Required | SemanticMode::Optional => semantic_scores(&loaded, &cases)?,
    };
    if golden.semantic_mode == SemanticMode::Required && semantic.scores.is_none() {
        return Err(format!(
            "recommendation eval requires the semantic model, but {} is not installed; \
             install it, use semanticMode='optional', or use semanticMode='disabled' \
             for an explicit name-only run",
            crate::grain_embed::MODEL_REPO
        ));
    }

    let recommendations = loaded
        .iter()
        .map(|extension| extension.indexed.clone())
        .collect::<Vec<_>>();
    let auto_send_eligible = loaded
        .iter()
        .filter(|extension| extension.auto_send_eligible)
        .map(|extension| extension.id.clone())
        .collect::<HashSet<_>>();
    let report = recommend_eval::evaluate(
        &recommendations,
        &cases,
        semantic.scores.as_deref(),
        &auto_send_eligible,
        auto_send_min_margin,
        &thresholds,
    )?;
    let quality = evaluate_quality_gates(golden.quality_gates.as_ref(), &report.metrics);
    let output = output(
        corpus.as_deref(),
        loaded.len(),
        golden.semantic_mode,
        &semantic,
        &quality,
        &report,
    );

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|error| format!("serialize recommendation report: {error}"))?
        );
    } else {
        print_human(&output);
    }
    if !quality.passed {
        return Err(format!(
            "recommendation quality gates failed: {}",
            quality.failures.join("; ")
        ));
    }
    Ok(())
}

fn validate_golden_shape(golden: &Golden) -> Result<(), String> {
    if golden.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported recommendation eval schemaVersion {}; expected {SCHEMA_VERSION}",
            golden.schema_version
        ));
    }
    if golden.mode != Mode::Recommendation {
        return Err("recommendation eval mode must be 'recommendation'".into());
    }
    if golden.extensions.is_empty() || golden.extensions.len() > MAX_EXTENSIONS {
        return Err(format!(
            "recommendation eval needs 1..={MAX_EXTENSIONS} extensions; got {}",
            golden.extensions.len()
        ));
    }
    if golden.cases.is_empty() || golden.cases.len() > MAX_CASES {
        return Err(format!(
            "recommendation eval needs 1..={MAX_CASES} cases; got {}",
            golden.cases.len()
        ));
    }
    if let Some(corpus) = golden.corpus.as_deref() {
        if corpus.trim().is_empty() || corpus.len() > 120 {
            return Err("corpus must be a non-empty line no longer than 120 bytes".into());
        }
        validate_single_line("corpus", corpus)?;
    }
    Ok(())
}

fn load_extensions(
    golden_path: &Path,
    sources: &[GoldenExtension],
) -> Result<Vec<LoadedExtension>, String> {
    let mut loaded = Vec::with_capacity(sources.len());
    let mut seen = HashSet::with_capacity(sources.len());
    for source in sources {
        let path = resolve_relative(golden_path, &source.manifest);
        let manifest = load_manifest(&path)?;
        if !manifest.kind.is_searchable() {
            return Err(format!(
                "{} declares kind '{}'; recommendation eval accepts searchable extensions only",
                path.display(),
                manifest.kind.as_str()
            ));
        }
        let recommend = manifest.recommend.as_ref().ok_or_else(|| {
            format!(
                "{} is searchable but has no recommend declaration",
                path.display()
            )
        })?;
        validate_recommendation_fields(&manifest, &path)?;
        if !seen.insert(manifest.id.clone()) {
            return Err(format!(
                "duplicate extension id '{}' in recommendation pool",
                manifest.id
            ));
        }

        loaded.push(LoadedExtension {
            id: manifest.id.clone(),
            indexed: IndexedRecommendation::with_name(
                &manifest.id,
                &manifest.name,
                &recommend.aliases,
            ),
            examples: recommend.examples.clone(),
            auto_send_eligible: manifest
                .auto_send
                .as_ref()
                .is_some_and(|declaration| declaration.eligible),
        });
    }
    Ok(loaded)
}

fn load_manifest(path: &Path) -> Result<ExtensionManifest, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("read manifest {}: {error}", path.display()))?;
    serde_json::from_str::<ExtensionProjectManifest>(&raw)
        .map(|project| project.manifest)
        .or_else(|_| serde_json::from_str::<ExtensionManifest>(&raw))
        .map_err(|error| format!("parse manifest {}: {error}", path.display()))
}

fn validate_recommendation_fields(manifest: &ExtensionManifest, path: &Path) -> Result<(), String> {
    use grain_sdk::manifest::{
        validate_extension_id, validate_extension_version, AUTO_SEND_NOTE_MAX_BYTES,
        RECOMMEND_ALIASES_MAX, RECOMMEND_ALIAS_MAX_BYTES, RECOMMEND_ENTITIES_MAX,
        RECOMMEND_ENTITY_MAX_BYTES, RECOMMEND_EXAMPLES_MAX, RECOMMEND_EXAMPLE_MAX_BYTES,
        RECOMMEND_PURPOSE_MAX_BYTES,
    };

    validate_extension_id(&manifest.id).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_extension_version(&manifest.version)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if manifest.id == "none" {
        return Err(format!("{} uses reserved eval id 'none'", path.display()));
    }
    if manifest.name.trim().is_empty() {
        return Err(format!("{} has an empty extension name", path.display()));
    }
    validate_single_line("extension name", &manifest.name)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let recommend = manifest.recommend.as_ref().expect("checked by caller");
    if recommend.purpose.trim().is_empty() || recommend.purpose.len() > RECOMMEND_PURPOSE_MAX_BYTES
    {
        return Err(format!(
            "{} recommend.purpose must be non-empty and at most {RECOMMEND_PURPOSE_MAX_BYTES} bytes",
            path.display()
        ));
    }
    validate_single_line("recommend.purpose", &recommend.purpose)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if recommend.examples.is_empty() || recommend.examples.len() > RECOMMEND_EXAMPLES_MAX {
        return Err(format!(
            "{} recommend.examples must contain 1..={RECOMMEND_EXAMPLES_MAX} phrases",
            path.display()
        ));
    }
    let mut seen_examples = HashSet::with_capacity(recommend.examples.len());
    for example in &recommend.examples {
        if example.trim().is_empty() || example.len() > RECOMMEND_EXAMPLE_MAX_BYTES {
            return Err(format!(
                "{} has a blank or oversized recommend example",
                path.display()
            ));
        }
        validate_single_line("recommend example", example)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !seen_examples.insert(example.trim().to_lowercase()) {
            return Err(format!(
                "{} contains duplicate recommend examples",
                path.display()
            ));
        }
    }
    if recommend.aliases.len() > RECOMMEND_ALIASES_MAX {
        return Err(format!("{} has too many recommend aliases", path.display()));
    }
    let mut seen_aliases = HashSet::with_capacity(recommend.aliases.len());
    for alias in &recommend.aliases {
        if alias.trim().is_empty() || alias.len() > RECOMMEND_ALIAS_MAX_BYTES {
            return Err(format!(
                "{} has a blank or oversized recommend alias",
                path.display()
            ));
        }
        validate_single_line("recommend alias", alias)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !seen_aliases.insert(alias.trim().to_lowercase()) {
            return Err(format!(
                "{} contains duplicate recommend aliases",
                path.display()
            ));
        }
    }
    if recommend.entities.len() > RECOMMEND_ENTITIES_MAX {
        return Err(format!(
            "{} has too many recommend entities",
            path.display()
        ));
    }
    let mut seen_entities = HashSet::with_capacity(recommend.entities.len());
    for entity in &recommend.entities {
        if entity.trim().is_empty() || entity.len() > RECOMMEND_ENTITY_MAX_BYTES {
            return Err(format!(
                "{} has a blank or oversized recommend entity",
                path.display()
            ));
        }
        validate_single_line("recommend entity", entity)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !seen_entities.insert(entity.trim().to_lowercase()) {
            return Err(format!(
                "{} contains duplicate recommend entities",
                path.display()
            ));
        }
    }
    if let Some(auto_send) = manifest.auto_send.as_ref().filter(|value| value.eligible) {
        let note = auto_send.note.as_deref().map(str::trim).unwrap_or_default();
        if note.is_empty() || note.len() > AUTO_SEND_NOTE_MAX_BYTES {
            return Err(format!(
                "{} has Auto-send eligibility without a valid note",
                path.display()
            ));
        }
        validate_single_line("Auto-send note", note)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn load_cases(
    raw_cases: Vec<GoldenCase>,
    known_ids: &HashSet<String>,
) -> Result<Vec<Case>, String> {
    raw_cases
        .into_iter()
        .enumerate()
        .map(|(index, raw)| {
            let said = raw.said.trim();
            if said.is_empty() || said.len() > MAX_UTTERANCE_BYTES {
                return Err(format!(
                    "case {} said must be non-empty and at most {MAX_UTTERANCE_BYTES} bytes",
                    index + 1
                ));
            }
            validate_single_line("case said", said)
                .map_err(|error| format!("case {}: {error}", index + 1))?;
            let expected = normalize_expected(raw.expect, index, known_ids)?;
            let slices = normalize_slices(raw.slices, index)?;
            Ok(Case {
                said: said.to_string(),
                expected,
                slices,
            })
        })
        .collect()
}

fn normalize_expected(
    expected: Expected,
    index: usize,
    known_ids: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let values = match expected {
        Expected::One(value) if value.trim() == "none" => return Ok(Vec::new()),
        Expected::One(value) => vec![value],
        Expected::Any(values) => values,
    };
    if values.is_empty() {
        return Err(format!(
            "case {} expect array is empty; use the string 'none' for no match",
            index + 1
        ));
    }
    let mut out = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        if value == "none" {
            return Err(format!(
                "case {} cannot mix 'none' with extension ids",
                index + 1
            ));
        }
        if !known_ids.contains(value) {
            return Err(format!(
                "case {} expects unknown extension id '{value}'",
                index + 1
            ));
        }
        if seen.insert(value.to_string()) {
            out.push(value.to_string());
        }
    }
    Ok(out)
}

fn normalize_slices(raw: Vec<String>, index: usize) -> Result<Vec<String>, String> {
    if raw.len() > MAX_SLICES_PER_CASE {
        return Err(format!(
            "case {} has more than {MAX_SLICES_PER_CASE} slices",
            index + 1
        ));
    }
    let mut out = Vec::with_capacity(raw.len());
    let mut seen = HashSet::with_capacity(raw.len());
    for slice in raw {
        let slice = slice.trim();
        if slice.is_empty() || slice.len() > MAX_SLICE_BYTES {
            return Err(format!("case {} has a blank or oversized slice", index + 1));
        }
        validate_single_line("case slice", slice)
            .map_err(|error| format!("case {}: {error}", index + 1))?;
        if seen.insert(slice.to_string()) {
            out.push(slice.to_string());
        }
    }
    Ok(out)
}

fn validate_thresholds(raw: Option<&[f32]>) -> Result<Vec<f32>, String> {
    let mut thresholds = raw.unwrap_or(DEFAULT_THRESHOLDS).to_vec();
    if thresholds.is_empty() || thresholds.len() > 32 {
        return Err("thresholds must contain 1..=32 values".into());
    }
    for threshold in &thresholds {
        if !threshold.is_finite() || !(TOPICAL_FLOOR..=1.0).contains(threshold) {
            return Err(format!(
                "recommendation thresholds must be finite values in {TOPICAL_FLOOR}..=1.0"
            ));
        }
    }
    thresholds.sort_by(f32::total_cmp);
    thresholds.dedup_by(|left, right| left.total_cmp(right).is_eq());
    Ok(thresholds)
}

fn validate_margin(raw: Option<f32>) -> Result<f32, String> {
    let margin = raw.unwrap_or(AUTO_SEND_MIN_MARGIN);
    if !margin.is_finite() || !(0.0..=1.0).contains(&margin) {
        return Err("autoSendMinMargin must be a finite value in 0.0..=1.0".into());
    }
    Ok(margin)
}

fn validate_quality_gates(gates: Option<&QualityGates>) -> Result<(), String> {
    let Some(gates) = gates else {
        return Ok(());
    };
    for (name, value) in [
        ("minTop1Accuracy", gates.min_top1_accuracy),
        ("minInScopeTop1Accuracy", gates.min_in_scope_top1_accuracy),
        ("minRecallAt3", gates.min_recall_at_3),
        ("minNoMatchPrecision", gates.min_no_match_precision),
        ("minNoMatchRecall", gates.min_no_match_recall),
        (
            "maxFalseRecommendationRate",
            gates.max_false_recommendation_rate,
        ),
    ] {
        if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
            return Err(format!(
                "qualityGates.{name} must be a finite value in 0.0..=1.0"
            ));
        }
    }
    Ok(())
}

fn evaluate_quality_gates(
    gates: Option<&QualityGates>,
    metrics: &recommend_eval::Metrics,
) -> QualityStatus {
    let Some(gates) = gates else {
        return QualityStatus {
            configured: false,
            passed: true,
            failures: Vec::new(),
        };
    };
    let mut failures = Vec::new();
    check_minimum(
        &mut failures,
        "top1Accuracy",
        Some(metrics.top1_accuracy),
        gates.min_top1_accuracy,
    );
    check_minimum(
        &mut failures,
        "inScopeTop1Accuracy",
        metrics.in_scope_top1_accuracy,
        gates.min_in_scope_top1_accuracy,
    );
    check_minimum(
        &mut failures,
        "recallAt3",
        metrics.recall_at_3,
        gates.min_recall_at_3,
    );
    check_minimum(
        &mut failures,
        "noMatchPrecision",
        metrics.no_match_precision,
        gates.min_no_match_precision,
    );
    check_minimum(
        &mut failures,
        "noMatchRecall",
        metrics.no_match_recall,
        gates.min_no_match_recall,
    );
    if let Some(maximum) = gates.max_false_recommendation_rate {
        match metrics.false_recommendation_rate {
            Some(actual) if actual <= maximum + f32::EPSILON => {}
            Some(actual) => failures.push(format!(
                "falseRecommendationRate {actual:.4} exceeds {maximum:.4}"
            )),
            None => failures.push("falseRecommendationRate is unavailable".into()),
        }
    }
    if let Some(maximum) = gates.max_wrong_auto_sends {
        if metrics.wrong_auto_sends > maximum {
            failures.push(format!(
                "wrongAutoSends {} exceeds {maximum}",
                metrics.wrong_auto_sends
            ));
        }
    }
    QualityStatus {
        configured: true,
        passed: failures.is_empty(),
        failures,
    }
}

fn check_minimum(
    failures: &mut Vec<String>,
    name: &str,
    actual: Option<f32>,
    minimum: Option<f32>,
) {
    let Some(minimum) = minimum else {
        return;
    };
    match actual {
        Some(actual) if actual + f32::EPSILON >= minimum => {}
        Some(actual) => failures.push(format!("{name} {actual:.4} is below {minimum:.4}")),
        None => failures.push(format!("{name} is unavailable")),
    }
}

fn validate_single_line(label: &str, value: &str) -> Result<(), String> {
    if value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    }) {
        return Err(format!(
            "{label} must not contain control or bidirectional formatting characters"
        ));
    }
    Ok(())
}

fn semantic_scores(extensions: &[LoadedExtension], cases: &[Case]) -> Result<SemanticRun, String> {
    if !crate::grain_embed::model_on_disk() {
        return Ok(SemanticRun::default());
    }

    let index_started = Instant::now();
    let mut extension_vectors = Vec::with_capacity(extensions.len());
    for extension in extensions {
        let vectors = crate::grain_embed::embed(extension.examples.clone()).map_err(|error| {
            format!("embed recommend examples for '{}': {error:#}", extension.id)
        })?;
        if vectors.len() != extension.examples.len() {
            return Err(format!(
                "embedder returned {} vectors for {} examples on '{}'",
                vectors.len(),
                extension.examples.len(),
                extension.id
            ));
        }
        extension_vectors.push((extension.id.clone(), vectors));
    }
    let index_embedding_millis = elapsed_millis(index_started);

    let mut scores = Vec::with_capacity(cases.len());
    let mut query_embedding_millis = Vec::with_capacity(cases.len());
    for case in cases {
        let query_started = Instant::now();
        let query = crate::grain_embed::embed_query(case.said.clone())
            .map_err(|error| format!("embed recommendation query: {error:#}"))?;
        query_embedding_millis.push(elapsed_millis(query_started));

        let mut per_extension = HashMap::with_capacity(extension_vectors.len());
        for (id, vectors) in &extension_vectors {
            let best = vectors
                .iter()
                .map(|vector| cosine(&query, vector))
                .reduce(f32::max)
                .unwrap_or(0.0);
            if !best.is_finite() {
                return Err(format!("embedder produced a non-finite score for '{id}'"));
            }
            per_extension.insert(id.clone(), best);
        }
        scores.push(per_extension);
    }
    Ok(SemanticRun {
        scores: Some(scores),
        index_embedding_millis,
        query_embedding_millis,
    })
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() {
        return 0.0;
    }
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn output<'a>(
    corpus: Option<&'a str>,
    extension_count: usize,
    semantic_mode: SemanticMode,
    semantic: &'a SemanticRun,
    quality: &'a QualityStatus,
    report: &'a Report,
) -> Output<'a> {
    let mut query_times = semantic.query_embedding_millis.clone();
    query_times.sort_unstable();
    Output {
        schema_version: SCHEMA_VERSION,
        mode: "recommendation",
        corpus,
        extension_count,
        model: ModelInfo {
            repository: crate::grain_embed::MODEL_REPO,
            revision: crate::grain_embed::MODEL_REVISION,
            dimensions: crate::grain_embed::EMBED_DIM,
            query_instruction: crate::grain_embed::QUERY_INSTRUCTION,
            semantic_mode,
            semantic_ran: semantic.scores.is_some(),
        },
        performance: Performance {
            index_embedding_millis: semantic.index_embedding_millis,
            query_embedding_p50_millis: percentile(&query_times, 50),
            query_embedding_p95_millis: percentile(&query_times, 95),
        },
        quality,
        report,
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (percentile * sorted.len()).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn percent(value: Option<f32>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{:.1}%", value * 100.0))
}

fn print_human(output: &Output<'_>) {
    let metrics = &output.report.metrics;
    let semantic_state = if output.model.semantic_ran {
        "semantic + named"
    } else if output.model.semantic_mode == SemanticMode::Disabled {
        "semantic disabled; named only"
    } else {
        "model unavailable; named only"
    };
    println!(
        "grain-ext eval recommendation — {} extension(s), {} case(s){}",
        output.extension_count,
        metrics.cases,
        output
            .corpus
            .map(|corpus| format!(" — {corpus}"))
            .unwrap_or_default()
    );
    println!(
        "  model               {} ({}, mode {})",
        output.model.repository,
        semantic_state,
        output.model.semantic_mode.as_str()
    );
    println!(
        "  top-1 accuracy      {:.1}%",
        metrics.top1_accuracy * 100.0
    );
    println!(
        "  in-scope top-1     {}",
        percent(metrics.in_scope_top1_accuracy)
    );
    println!("  recall@3           {}", percent(metrics.recall_at_3));
    println!(
        "  MRR                {}",
        metrics
            .mean_reciprocal_rank
            .map_or_else(|| "—".to_string(), |value| format!("{value:.3}"))
    );
    println!(
        "  no-match precision {}",
        percent(metrics.no_match_precision)
    );
    println!("  no-match recall    {}", percent(metrics.no_match_recall));
    println!(
        "  false recommend    {}",
        percent(metrics.false_recommendation_rate)
    );
    println!(
        "  candidate set      median {}, p95 {}, coverage {}",
        metrics.candidate_set_median,
        metrics.candidate_set_p95,
        percent(metrics.candidate_set_coverage)
    );
    println!(
        "  Auto-send          {} correct, {} WRONG",
        metrics.correct_auto_sends, metrics.wrong_auto_sends
    );
    println!(
        "  latency            embed index {} ms, query p50/p95 {}/{} ms, rank p50/p95 {}/{} µs",
        output.performance.index_embedding_millis,
        output.performance.query_embedding_p50_millis,
        output.performance.query_embedding_p95_millis,
        metrics.ranking_p50_micros,
        metrics.ranking_p95_micros
    );
    if output.quality.configured {
        println!(
            "  quality gates      {}",
            if output.quality.passed {
                "PASS"
            } else {
                "FAIL"
            }
        );
    }

    if !output.report.confusion.is_empty() {
        println!("\n  confusion:");
        for cell in &output.report.confusion {
            println!("    {} -> {}   x{}", cell.expected, cell.got, cell.count);
        }
    }
    if !output.report.slices.is_empty() {
        println!("\n  slices:");
        for slice in &output.report.slices {
            println!(
                "    {:<20} n={:<5} top1={:>6.1}% r@3={:>7} false-rec={:>7}",
                slice.name,
                slice.metrics.cases,
                slice.metrics.top1_accuracy * 100.0,
                percent(slice.metrics.recall_at_3),
                percent(slice.metrics.false_recommendation_rate)
            );
        }
    }
    println!("\n  operating points:");
    println!("    floor  coverage  risk      correct  wrong  abstain  auto-wrong");
    for point in &output.report.operating_points {
        println!(
            "    {:>5.2}  {:>7.1}%  {:>8}  {:>7}  {:>5}  {:>7}  {:>10}",
            point.topical_floor,
            point.coverage * 100.0,
            percent(point.selective_risk),
            point.correct_fires,
            point.wrong_fires,
            point.abstains,
            point.wrong_auto_sends
        );
    }

    let misses = output
        .report
        .results
        .iter()
        .filter(|result| !result.top1_correct)
        .collect::<Vec<_>>();
    if misses.is_empty() {
        println!("\n  no top-1 misses.");
    } else {
        println!("\n  top-1 misses ({}):", misses.len());
        for miss in misses {
            let expected = if miss.expected.is_empty() {
                "none".to_string()
            } else {
                miss.expected.join("|")
            };
            let got = miss.ranked.first().map_or_else(
                || "none".to_string(),
                |candidate| {
                    format!(
                        "{} ({:.3}, {:?})",
                        candidate.extension_id, candidate.score, candidate.signal
                    )
                },
            );
            println!("    {:?} expected {} got {}", miss.said, expected, got);
        }
    }
}

fn resolve_relative(golden_path: &Path, value: &str) -> PathBuf {
    let candidate = PathBuf::from(value);
    if candidate.is_absolute() {
        candidate
    } else {
        golden_path
            .parent()
            .map(|directory| directory.join(&candidate))
            .unwrap_or(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> HashSet<String> {
        HashSet::from(["github".to_string(), "linear".to_string()])
    }

    #[test]
    fn recommendation_schema_is_versioned_and_strict() {
        let raw = r#"{
          "schemaVersion": 1,
          "mode": "recommendation",
          "corpus": "hard pool",
          "extensions": [{"manifest": "github.json"}],
          "cases": [{"said": "file an issue", "expect": "github", "slices": ["clean"]}],
          "semanticMode": "disabled"
        }"#;
        let golden: Golden = serde_json::from_str(raw).expect("valid schema");
        assert_eq!(golden.schema_version, 1);
        assert_eq!(golden.mode, Mode::Recommendation);
        assert_eq!(golden.extensions.len(), 1);

        assert_eq!(golden.semantic_mode, SemanticMode::Disabled);

        let unknown = raw.replace("\"semanticMode\": \"disabled\"", "\"surprise\": true");
        assert!(serde_json::from_str::<Golden>(&unknown).is_err());
    }

    #[test]
    fn expectations_support_none_or_an_acceptable_set() {
        assert!(
            normalize_expected(Expected::One("none".into()), 0, &known())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            normalize_expected(
                Expected::Any(vec!["github".into(), "linear".into(), "github".into()]),
                0,
                &known()
            )
            .unwrap(),
            vec!["github", "linear"]
        );
        assert!(normalize_expected(
            Expected::Any(vec!["none".into(), "github".into()]),
            0,
            &known()
        )
        .is_err());
    }

    #[test]
    fn thresholds_are_sorted_deduplicated_and_never_below_production_floor() {
        assert_eq!(
            validate_thresholds(Some(&[0.8, 0.5, 0.8])).unwrap(),
            vec![0.5, 0.8]
        );
        assert!(validate_thresholds(Some(&[0.49])).is_err());
        assert!(validate_thresholds(Some(&[f32::NAN])).is_err());
    }

    #[test]
    fn quality_gates_fail_closed_on_regression_or_missing_slice() {
        let metrics = recommend_eval::Metrics {
            cases: 1,
            in_scope_cases: 0,
            no_match_cases: 1,
            top1_accuracy: 0.0,
            in_scope_top1_accuracy: None,
            recall_at_3: None,
            mean_reciprocal_rank: None,
            no_match_precision: Some(0.0),
            no_match_recall: Some(0.0),
            false_recommendation_rate: Some(1.0),
            candidate_set_coverage: None,
            candidate_set_median: 1,
            candidate_set_p95: 1,
            named_tops: 0,
            topical_tops: 1,
            correct_auto_sends: 0,
            wrong_auto_sends: 1,
            ranking_p50_micros: 1,
            ranking_p95_micros: 1,
        };
        let gates = QualityGates {
            min_top1_accuracy: Some(0.9),
            min_recall_at_3: Some(0.9),
            max_false_recommendation_rate: Some(0.1),
            max_wrong_auto_sends: Some(0),
            ..QualityGates::default()
        };

        let status = evaluate_quality_gates(Some(&gates), &metrics);
        assert!(!status.passed);
        assert_eq!(status.failures.len(), 4);
    }

    #[test]
    fn quality_gate_ratios_are_bounded() {
        let gates = QualityGates {
            min_top1_accuracy: Some(1.01),
            ..QualityGates::default()
        };
        assert!(validate_quality_gates(Some(&gates)).is_err());
    }

    #[test]
    fn report_labels_reject_terminal_and_bidi_controls() {
        assert!(validate_single_line("slice", "clean\u{1b}[2J").is_err());
        assert!(validate_single_line("slice", "clean\u{202e}txt").is_err());
        assert!(validate_single_line("slice", "asr-hard").is_ok());
    }

    #[test]
    fn relative_manifests_follow_the_golden_file() {
        assert_eq!(
            resolve_relative(Path::new("C:/project/eval/golden.json"), "../spotify.json"),
            PathBuf::from("C:/project/eval/../spotify.json")
        );
    }

    #[test]
    fn checked_in_v1_fixture_is_valid_held_out_and_challenging() {
        let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/recommendation-eval/v1/golden.json");
        let raw = std::fs::read_to_string(&golden_path).expect("read checked-in golden");
        let golden: Golden = serde_json::from_str(&raw).expect("parse checked-in golden");
        validate_golden_shape(&golden).expect("valid checked-in shape");
        validate_quality_gates(golden.quality_gates.as_ref())
            .expect("valid checked-in quality gates");
        let loaded =
            load_extensions(&golden_path, &golden.extensions).expect("load checked-in manifests");
        let known_ids = loaded
            .iter()
            .map(|extension| extension.id.clone())
            .collect::<HashSet<_>>();
        let cases = load_cases(golden.cases, &known_ids).expect("validate checked-in cases");

        assert_eq!(loaded.len(), 5);
        assert!(cases.len() >= 25);
        assert!(cases.iter().filter(|case| case.expected.is_empty()).count() >= 5);
        assert!(
            cases
                .iter()
                .filter(|case| case.slices.iter().any(|slice| slice == "hard"))
                .count()
                >= 5
        );
        assert!(
            cases
                .iter()
                .filter(|case| case.slices.iter().any(|slice| slice == "asr"))
                .count()
                >= 3
        );

        let index_phrases = loaded
            .iter()
            .flat_map(|extension| extension.examples.iter())
            .map(|phrase| phrase.trim().to_lowercase())
            .collect::<HashSet<_>>();
        assert!(
            cases
                .iter()
                .all(|case| { !index_phrases.contains(&case.said.trim().to_lowercase()) }),
            "golden cases must be held out from manifest examples"
        );
    }
}
