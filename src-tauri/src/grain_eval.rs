//! [GRAIN] `grain-ext eval` — the author test harness, as a headless app
//! subcommand (`docs/Extensions V1/PLAN.md` §10 P3).
//!
//! Runs INSIDE the app process rather than the standalone `grain-ext` CLI,
//! because the embedder lives here — that is the whole point of the eval, so it
//! must use the real one, not a stand-in. It loads an extension's manifest and a
//! labelled test set, scores each case with the real lexical + semantic paths,
//! feeds [`grain_core::eval`], prints the report, and exits.
//!
//! Invoked as `handy --eval <golden.json>` (optionally `--json`). The flag is
//! read straight from the process args in [`requested`] so the upstream
//! `handy/cli.rs` stays byte-identical — eval is a Grain concern and never
//! touches the Handy-derived arg parser.

use std::path::{Path, PathBuf};

use grain_core::eval::{self, Case};
use grain_core::matching::Match;
use serde::Deserialize;
use tauri::AppHandle;

/// The default operating-point sweep when the golden file names none.
const DEFAULT_THRESHOLDS: &[f32] = &[0.45, 0.50, 0.55, 0.60, 0.65, 0.70, 0.80];
/// Headless eval is developer-controlled, but still must not allocate an
/// unbounded file before JSON validation. Large corpora should be split into
/// stable shards and compared independently.
const MAX_GOLDEN_BYTES: u64 = 16 * 1024 * 1024;

/// Detect `--eval <file>` (or `--eval=<file>`) in the process args, without
/// touching the upstream `CliArgs`. Returns the golden-file path when requested.
pub fn requested() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(rest) = arg.strip_prefix("--eval=") {
            return Some(PathBuf::from(rest));
        }
        if arg == "--eval" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// Whether `--json` was passed alongside `--eval`.
fn json_requested() -> bool {
    std::env::args().any(|a| a == "--json")
}

/// A golden test set: the extension under test and its labelled utterances.
#[derive(Deserialize)]
struct CommandGolden {
    /// Path to the extension's `manifest.json`, relative to this file (or
    /// absolute). Its `contributes.actions` are the commands eval ranks.
    manifest: String,
    cases: Vec<GoldenCase>,
    /// Optional operating-point sweep; defaults to [`DEFAULT_THRESHOLDS`].
    #[serde(default)]
    thresholds: Option<Vec<f32>>,
}

#[derive(Deserialize)]
struct GoldenCase {
    said: String,
    /// A command id, or `"none"` (see [`grain_core::eval::EXPECT_NONE`]).
    expect: String,
}

/// Run the eval and return a process exit code (0 ok, 2 bad input).
pub fn run(_app: &AppHandle, golden_path: &Path) -> i32 {
    let raw = match load_raw(golden_path) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("eval: {error}");
            return 2;
        }
    };
    let shape = match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("eval: parse golden file {}: {error}", golden_path.display());
            return 2;
        }
    };
    if let Some(mode) = shape.get("mode") {
        if mode.as_str() != Some("recommendation") {
            eprintln!(
                "eval: unsupported eval mode {}; expected 'recommendation'",
                mode
            );
            return 2;
        }
        return match super::recommend_eval::run(golden_path, &raw, json_requested()) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("eval: {error}");
                2
            }
        };
    }

    let golden = match load_command_golden(golden_path, &raw) {
        Ok(golden) => golden,
        Err(error) => {
            eprintln!("eval: {error}");
            return 2;
        }
    };
    let manifest_path = resolve_manifest(golden_path, &golden.manifest);
    let commands = match load_commands(&manifest_path) {
        Ok(c) => c,
        Err(error) => {
            eprintln!("eval: {error}");
            return 2;
        }
    };
    if commands.is_empty() {
        eprintln!(
            "eval: {} declares no commands (contributes.actions) to evaluate",
            manifest_path.display()
        );
        return 2;
    }

    let cases: Vec<Case> = golden
        .cases
        .iter()
        .map(|c| Case {
            said: c.said.clone(),
            expect: c.expect.clone(),
        })
        .collect();
    let thresholds = golden.thresholds.as_deref().unwrap_or(DEFAULT_THRESHOLDS);

    // The real semantic path, if the model is present. Absent → lexical-only,
    // reported as such rather than pretended around.
    let semantic = semantic_scores(&commands, &cases);
    let report = eval::evaluate(&commands, &cases, semantic.as_deref(), thresholds);

    if json_requested() {
        print_json(&report, semantic.is_some());
    } else {
        print_human(&report, &commands, semantic.is_some());
    }
    0
}

fn load_raw(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("inspect golden file {}: {error}", path.display()))?;
    if metadata.len() > MAX_GOLDEN_BYTES {
        return Err(format!(
            "golden file {} is {} bytes; the limit is {MAX_GOLDEN_BYTES}",
            path.display(),
            metadata.len()
        ));
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("read golden file {}: {error}", path.display()))?;
    if raw.len() as u64 > MAX_GOLDEN_BYTES {
        return Err(format!(
            "golden file {} changed while being read and exceeded {MAX_GOLDEN_BYTES} bytes",
            path.display()
        ));
    }
    Ok(raw)
}

fn load_command_golden(path: &Path, raw: &str) -> Result<CommandGolden, String> {
    serde_json::from_str(raw)
        .map_err(|error| format!("parse golden file {}: {error}", path.display()))
}

/// A relative `manifest` in the golden file is resolved against the golden file's
/// own directory, so a test set is portable with the project it tests.
fn resolve_manifest(golden_path: &Path, manifest: &str) -> PathBuf {
    let candidate = PathBuf::from(manifest);
    if candidate.is_absolute() {
        return candidate;
    }
    golden_path
        .parent()
        .map(|dir| dir.join(&candidate))
        .unwrap_or(candidate)
}

/// The extension's commands as `(id, utterances)` — the same shape `match.*`
/// takes. Parses the authoring project manifest, falling back to a bare
/// `ExtensionManifest` so either on-disk shape works.
fn load_commands(path: &Path) -> Result<Vec<(String, Vec<String>)>, String> {
    use grain_sdk::{ExtensionManifest, ExtensionProjectManifest};
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("read manifest {}: {e}", path.display()))?;
    let manifest = serde_json::from_str::<ExtensionProjectManifest>(&raw)
        .map(|p| p.manifest)
        .or_else(|_| serde_json::from_str::<ExtensionManifest>(&raw))
        .map_err(|e| format!("parse manifest {}: {e}", path.display()))?;
    Ok(manifest
        .contributes
        .actions
        .into_iter()
        .map(|a| (a.id, a.utterances))
        .collect())
}

/// Score every case semantically against the commands with the REAL embedder,
/// or `None` when the model is not installed (lexical-only, honestly).
///
/// Command vectors are embedded once and reused across cases; only the per-case
/// query is embedded in the loop. Any embed failure downgrades the whole run to
/// lexical-only rather than reporting half a picture.
fn semantic_scores(commands: &[(String, Vec<String>)], cases: &[Case]) -> Option<Vec<Vec<Match>>> {
    if !crate::grain_space::embed::model_on_disk() {
        return None;
    }
    let mut command_vectors: Vec<(String, Vec<Vec<f32>>)> = Vec::with_capacity(commands.len());
    for (id, phrases) in commands {
        let phrases: Vec<String> = phrases
            .iter()
            .filter(|p| !p.trim().is_empty())
            .cloned()
            .collect();
        if phrases.is_empty() {
            continue;
        }
        match crate::grain_space::embed::embed(phrases) {
            Ok(vectors) => command_vectors.push((id.clone(), vectors)),
            Err(error) => {
                eprintln!(
                    "eval: embedding commands failed ({error:#}); falling back to lexical only"
                );
                return None;
            }
        }
    }
    let mut per_case = Vec::with_capacity(cases.len());
    for case in cases {
        let query = match crate::grain_space::embed::embed_query(case.said.clone()) {
            Ok(q) => q,
            Err(error) => {
                eprintln!(
                    "eval: embedding a query failed ({error:#}); falling back to lexical only"
                );
                return None;
            }
        };
        let mut matches: Vec<Match> = command_vectors
            .iter()
            .map(|(id, vectors)| Match {
                id: id.clone(),
                score: vectors
                    .iter()
                    .map(|v| cosine(&query, v))
                    .fold(f32::MIN, f32::max),
            })
            .collect();
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        per_case.push(matches);
    }
    Some(per_case)
}

/// Dot product of two L2-normalised vectors. Length-guarded → 0 on a mismatch.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn print_human(report: &eval::Report, commands: &[(String, Vec<String>)], semantic: bool) {
    println!(
        "grain-ext eval — {} command(s), {} case(s)",
        commands.len(),
        report.results.len()
    );
    println!(
        "  lexical accuracy   {:.0}%",
        report.lexical_accuracy * 100.0
    );
    if semantic {
        println!(
            "  semantic accuracy  {:.0}%",
            report.semantic_accuracy * 100.0
        );
    } else {
        println!("  semantic accuracy  — model not installed, lexical only");
    }

    let misses: Vec<&eval::CaseResult> = report.results.iter().filter(|r| !r.correct).collect();
    if misses.is_empty() {
        println!("\n  no misses.");
    } else {
        println!("\n  misses ({}):", misses.len());
        for miss in misses {
            let got = miss
                .semantic_top
                .as_ref()
                .or(miss.lexical_top.as_ref())
                .map(|m| format!("{} ({:.2})", m.id, m.score))
                .unwrap_or_else(|| "nothing".into());
            println!(
                "    \"{}\"  expected {}  got {}",
                miss.said, miss.expect, got
            );
        }
    }

    if semantic && !report.confusion.is_empty() {
        println!("\n  confusion (semantic top):");
        for cell in &report.confusion {
            println!("    {} -> {}   x{}", cell.expected, cell.got, cell.count);
        }
    }

    if semantic {
        println!("\n  operating points (semantic):");
        println!("    minConf  correct  wrong  abstain");
        for point in &report.operating_points {
            println!(
                "    {:>6.2}   {:>7}  {:>5}  {:>7}",
                point.min_confidence, point.correct_fires, point.wrong_fires, point.abstains
            );
        }
        println!(
            "\n  a wrong fire is the number that matters for Auto-send — pick the\n  \
             lowest floor where it reaches 0 without starving correct fires."
        );
    }
}

fn print_json(report: &eval::Report, semantic: bool) {
    let value = serde_json::json!({
        "semanticRan": semantic,
        "lexicalAccuracy": report.lexical_accuracy,
        "semanticAccuracy": report.semantic_accuracy,
        "confusion": report.confusion.iter().map(|c| serde_json::json!({
            "expected": c.expected, "got": c.got, "count": c.count,
        })).collect::<Vec<_>>(),
        "operatingPoints": report.operating_points.iter().map(|p| serde_json::json!({
            "minConfidence": p.min_confidence,
            "correctFires": p.correct_fires,
            "wrongFires": p.wrong_fires,
            "abstains": p.abstains,
        })).collect::<Vec<_>>(),
        "cases": report.results.iter().map(|r| serde_json::json!({
            "said": r.said,
            "expect": r.expect,
            "correct": r.correct,
            "resolver": format!("{:?}", r.resolver),
            "lexicalTop": r.lexical_top.as_ref().map(|m| serde_json::json!({"id": m.id, "score": m.score})),
            "semanticTop": r.semantic_top.as_ref().map(|m| serde_json::json!({"id": m.id, "score": m.score})),
        })).collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_manifest_is_relative_to_the_golden_file() {
        let golden = Path::new("/proj/tests/golden.json");
        assert_eq!(
            resolve_manifest(golden, "manifest.json"),
            PathBuf::from("/proj/tests/manifest.json")
        );
        assert_eq!(
            resolve_manifest(golden, "../manifest.json"),
            PathBuf::from("/proj/tests/../manifest.json")
        );
    }

    #[test]
    fn an_absolute_manifest_path_is_left_alone() {
        let golden = Path::new("/proj/golden.json");
        let abs = if cfg!(windows) {
            "C:/elsewhere/manifest.json"
        } else {
            "/elsewhere/manifest.json"
        };
        assert_eq!(resolve_manifest(golden, abs), PathBuf::from(abs));
    }

    #[test]
    fn oversized_golden_is_rejected_before_allocation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(MAX_GOLDEN_BYTES + 1).unwrap();

        let error = load_raw(file.path()).unwrap_err();
        assert!(error.contains("the limit is"));
    }

    #[test]
    fn legacy_command_golden_remains_supported() {
        let raw = r#"{
            "manifest": "manifest.json",
            "cases": [{"said": "pause", "expect": "pause"}]
        }"#;
        let golden = load_command_golden(Path::new("golden.json"), raw).unwrap();

        assert_eq!(golden.manifest, "manifest.json");
        assert_eq!(golden.cases.len(), 1);
    }
}
