//! [GRAIN] Headless evaluation harness for Grain Space memory retrieval.
//!
//! Loads a corpus of markdown notes into an isolated test vault, indexes them
//! via synchronous FTS5 / reconcile, and evaluates retrieval quality
//! (Recall@1, Recall@5, MRR, out-of-scope rejection) against a labeled golden set.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::vault::{self, Vault};

#[derive(Deserialize, Debug)]
pub struct MemoryGolden {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub mode: String,
    pub corpus: String,
    #[serde(rename = "corpusDir")]
    pub corpus_dir: String,
    #[serde(default)]
    pub thresholds: Option<EvalThresholds>,
    pub cases: Vec<MemoryCase>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EvalThresholds {
    #[serde(rename = "minRecallAt1")]
    pub min_recall_at_1: f64,
    #[serde(rename = "minRecallAt5")]
    pub min_recall_at_5: f64,
    #[serde(rename = "minMrr")]
    pub min_mrr: f64,
    #[serde(default, rename = "minOutOfScopeAccuracy")]
    pub min_out_of_scope_accuracy: Option<f64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MemoryCase {
    pub query: String,
    pub expect: String,
    #[serde(default)]
    pub acceptable: Vec<String>,
    pub category: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct CaseResult {
    pub query: String,
    pub expect: String,
    pub category: String,
    pub hit_at_1: bool,
    pub hit_at_5: bool,
    pub mrr: f64,
    pub top_hits: Vec<String>,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct EvalMetrics {
    pub total_cases: usize,
    pub relevant_cases: usize,
    pub out_of_scope_cases: usize,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub mrr: f64,
    pub out_of_scope_accuracy: f64,
}

#[derive(Serialize, Debug, Clone)]
pub struct EvalReport {
    pub corpus: String,
    pub total_notes: usize,
    pub metrics: EvalMetrics,
    pub cases: Vec<CaseResult>,
    pub passed_thresholds: bool,
}

/// Set up an isolated test vault and copy all markdown files from `corpus_dir` into it.
pub fn setup_eval_vault(corpus_dir: &Path) -> Result<(Vault, PathBuf, usize)> {
    let unique_id = uuid::Uuid::new_v4().to_string();
    let temp_base = std::env::temp_dir().join(format!("grain_mem_eval_{unique_id}"));
    let vault_root = temp_base.join("vault");
    let grain_dir = vault_root.join("Grain");
    let index_base = temp_base.join("appdata");

    fs::create_dir_all(&grain_dir)
        .with_context(|| format!("failed creating grain dir at {}", grain_dir.display()))?;
    fs::create_dir_all(&index_base)
        .with_context(|| format!("failed creating index dir at {}", index_base.display()))?;

    let mut note_count = 0;
    if corpus_dir.is_dir() {
        for entry in fs::read_dir(corpus_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let dest = grain_dir.join(entry.file_name());
                fs::copy(&path, &dest)?;
                note_count += 1;
            }
        }
    } else {
        return Err(anyhow!(
            "corpus directory not found: {}",
            corpus_dir.display()
        ));
    }

    let vault = Vault {
        root: vault_root,
        folder: "Grain".to_string(),
        index_base,
        native: false,
    };

    Ok((vault, temp_base, note_count))
}

/// Run evaluation over a prepared vault.
pub fn evaluate_vault(
    golden: &MemoryGolden,
    vault: &Vault,
    total_notes: usize,
) -> Result<EvalReport> {
    let mut case_results = Vec::new();
    let mut relevant_count = 0;
    let mut out_of_scope_count = 0;
    let mut out_of_scope_correct = 0;
    let mut hits_at_1 = 0;
    let mut hits_at_5 = 0;
    let mut total_mrr = 0.0;

    for case in &golden.cases {
        let is_negative = case.expect.eq_ignore_ascii_case("none");
        let search_hits = vault::search_notes_natural(vault, &case.query, None).unwrap_or_default();

        let top_ids: Vec<String> = search_hits.iter().map(|n| n.id.clone()).collect();

        if is_negative {
            out_of_scope_count += 1;
            let correct = search_hits.is_empty();
            if correct {
                out_of_scope_correct += 1;
            }
            case_results.push(CaseResult {
                query: case.query.clone(),
                expect: case.expect.clone(),
                category: case.category.clone(),
                hit_at_1: correct,
                hit_at_5: correct,
                mrr: if correct { 1.0 } else { 0.0 },
                top_hits: top_ids,
            });
        } else {
            relevant_count += 1;
            let hit_1 = !top_ids.is_empty()
                && (top_ids[0] == case.expect || case.acceptable.contains(&top_ids[0]));
            let hit_5 = top_ids
                .iter()
                .take(5)
                .any(|id| id == &case.expect || case.acceptable.contains(id));
            let mrr = top_ids
                .iter()
                .position(|id| id == &case.expect || case.acceptable.contains(id))
                .map(|pos| 1.0 / (pos + 1) as f64)
                .unwrap_or(0.0);

            if hit_1 {
                hits_at_1 += 1;
            }
            if hit_5 {
                hits_at_5 += 1;
            }
            total_mrr += mrr;

            case_results.push(CaseResult {
                query: case.query.clone(),
                expect: case.expect.clone(),
                category: case.category.clone(),
                hit_at_1: hit_1,
                hit_at_5: hit_5,
                mrr,
                top_hits: top_ids,
            });
        }
    }

    let recall_at_1 = if relevant_count > 0 {
        hits_at_1 as f64 / relevant_count as f64
    } else {
        0.0
    };
    let recall_at_5 = if relevant_count > 0 {
        hits_at_5 as f64 / relevant_count as f64
    } else {
        0.0
    };
    let mrr = if relevant_count > 0 {
        total_mrr / relevant_count as f64
    } else {
        0.0
    };
    let out_of_scope_accuracy = if out_of_scope_count > 0 {
        out_of_scope_correct as f64 / out_of_scope_count as f64
    } else {
        1.0
    };

    let mut passed = true;
    if let Some(th) = &golden.thresholds {
        if recall_at_1 < th.min_recall_at_1 {
            passed = false;
        }
        if recall_at_5 < th.min_recall_at_5 {
            passed = false;
        }
        if mrr < th.min_mrr {
            passed = false;
        }
        if let Some(min_oos) = th.min_out_of_scope_accuracy {
            if out_of_scope_accuracy < min_oos {
                passed = false;
            }
        }
    }

    let metrics = EvalMetrics {
        total_cases: golden.cases.len(),
        relevant_cases: relevant_count,
        out_of_scope_cases: out_of_scope_count,
        recall_at_1,
        recall_at_5,
        mrr,
        out_of_scope_accuracy,
    };

    Ok(EvalReport {
        corpus: golden.corpus.clone(),
        total_notes,
        metrics,
        cases: case_results,
        passed_thresholds: passed,
    })
}

/// Execute headless memory evaluation.
pub fn run(golden_path: &Path, raw: &str, json: bool) -> Result<()> {
    let golden: MemoryGolden =
        serde_json::from_str(raw).context("failed to parse memory golden JSON")?;
    if golden.schema_version != 1 {
        return Err(anyhow!(
            "unsupported memory evaluation schema version {}",
            golden.schema_version
        ));
    }
    if golden.mode != "memory" {
        return Err(anyhow!(
            "memory evaluator cannot run mode {:?}",
            golden.mode
        ));
    }
    if golden.cases.is_empty() {
        return Err(anyhow!("memory evaluation must contain at least one case"));
    }

    let corpus_dir = golden_path
        .parent()
        .ok_or_else(|| anyhow!("invalid golden path"))?
        .join(&golden.corpus_dir);

    let (vault, temp_dir, note_count) = setup_eval_vault(&corpus_dir)?;
    let report = evaluate_vault(&golden, &vault, note_count)?;

    // Clean up temporary evaluation directory
    let _ = fs::remove_dir_all(&temp_dir);

    if json {
        let json_out = serde_json::to_string_pretty(&report)?;
        println!("{json_out}");
    } else {
        println!("============================================================");
        println!(
            "Grain Space Memory Eval: {} ({} notes indexed)",
            report.corpus, report.total_notes
        );
        println!("============================================================");
        println!("Total Cases:       {}", report.metrics.total_cases);
        println!("Relevant Cases:    {}", report.metrics.relevant_cases);
        println!("Out-of-Scope:      {}", report.metrics.out_of_scope_cases);
        println!("------------------------------------------------------------");
        println!(
            "Recall@1:          {:.1}% (min: {:.1}%)",
            report.metrics.recall_at_1 * 100.0,
            golden
                .thresholds
                .as_ref()
                .map(|t| t.min_recall_at_1 * 100.0)
                .unwrap_or(0.0)
        );
        println!(
            "Recall@5:          {:.1}% (min: {:.1}%)",
            report.metrics.recall_at_5 * 100.0,
            golden
                .thresholds
                .as_ref()
                .map(|t| t.min_recall_at_5 * 100.0)
                .unwrap_or(0.0)
        );
        println!(
            "MRR:               {:.3} (min: {:.3})",
            report.metrics.mrr,
            golden.thresholds.as_ref().map(|t| t.min_mrr).unwrap_or(0.0)
        );
        println!(
            "Out-of-Scope Acc:  {:.1}%",
            report.metrics.out_of_scope_accuracy * 100.0
        );
        println!("============================================================");
        println!(
            "Status: {}",
            if report.passed_thresholds {
                "PASSED"
            } else {
                "FAILED (Quality Gates Not Met)"
            }
        );
    }

    if !report.passed_thresholds {
        return Err(anyhow!("Grain Space memory eval quality gates not met"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_eval_golden_v1_passes() {
        let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/memory-eval/v1/golden.json");
        assert!(
            golden_path.exists(),
            "golden.json must exist at {}",
            golden_path.display()
        );
        let raw = fs::read_to_string(&golden_path).expect("read golden.json");
        let result = run(&golden_path, &raw, false);
        assert!(result.is_ok(), "memory eval must pass: {:?}", result.err());
    }

    #[tokio::test]
    async fn phase_5_storage_journey_contract() {
        use crate::grain_llm_client::ToolCallOut;
        use crate::grain_space::agent_tools::{self, NoteToolResult, TurnLog};
        use crate::grain_space::note::Note;

        let unique = uuid::Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("grain_p5_storage_{unique}"));
        let vault = Vault {
            root: base.join("vault"),
            folder: "Grain".to_string(),
            index_base: base.join("appdata"),
            native: false,
        };
        fs::create_dir_all(&vault.root).unwrap();
        fs::create_dir_all(&vault.index_base).unwrap();

        // 1. A synthetic Agent tool call enters the production preparation path.
        let raw_user_body = "Packing list:\n- Passport\n- Camera";
        let raw_user_title = "Trip to Kyoto";
        let call = ToolCallOut {
            id: "call_save_kyoto".to_string(),
            name: "save_note".to_string(),
            arguments: serde_json::json!({
                "title": raw_user_title,
                "body": raw_user_body,
            })
            .to_string(),
        };
        let mut log = TurnLog::new();

        let tool_result = agent_tools::execute_opt(None, &call, &mut log).await;

        // 2. The write is withheld as an Interaction::Confirm.
        let confirm = match tool_result {
            NoteToolResult::Confirm(c) => c,
            other => panic!("Expected NoteToolResult::Confirm, got {:?}", other),
        };
        assert!(confirm.token.starts_with("pc_"));
        assert_eq!(confirm.title, "Save Note");
        assert_eq!(confirm.summary, "Save note \"Trip to Kyoto\"");
        assert!(confirm.markdown.contains("Trip to Kyoto"));
        assert!(confirm.markdown.contains("Passport"));

        // 3. Inspect the held action without pretending this headless test drove
        // the real confirmation UI or a live model.
        let prepared = crate::action_exec::take_pending(&confirm.token)
            .expect("held confirmation must be retrieved from pending calls");
        assert_eq!(prepared.action_id, "save_note");
        assert_eq!(prepared.risk, grain_core::execution::RiskClass::Confirm);
        assert_eq!(
            prepared.side_effect,
            grain_core::execution::SideEffect::Write
        );

        // 4. Exercise the storage contract with the exact confirmed arguments.
        let confirmed_title = prepared
            .arguments
            .get("title")
            .and_then(|v| v.as_str())
            .expect("title in prepared call");
        let confirmed_body = prepared
            .arguments
            .get("body")
            .and_then(|v| v.as_str())
            .expect("body in prepared call");
        assert_eq!(confirmed_title, raw_user_title);
        assert_eq!(confirmed_body, raw_user_body);

        let mut note = Note::raw(confirmed_body.to_string());
        note.title = confirmed_title.to_string();
        vault::save_note(&vault, &note).expect("save confirmed note byte-for-byte");

        // 5. Exercise search and read against the persisted note.
        let hits = vault::search_notes_natural(&vault, "Kyoto packing", None).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Trip to Kyoto");
        assert_eq!(hits[0].id, note.id);

        let read_note = vault::get_note(&vault, &note.id).expect("get note");
        assert_eq!(read_note.body, raw_user_body);
        let expected_version =
            vault::note_storage_version(&vault, &note.id).expect("read exact persisted version");

        // 6. Exercise the production atomic append primitive.
        let addition = "Hotel reservation confirmed: Ryokan Sakura";
        vault::append_note_atomic(&vault, &note.id, addition, Some(&expected_version))
            .expect("production atomic append must succeed with valid version");

        // 7. Verification of byte-for-byte preservation and updated search
        let post_append = vault::get_note(&vault, &note.id).expect("read post-append");
        assert!(post_append.body.starts_with(raw_user_body));
        assert!(post_append.body.contains("Ryokan Sakura"));
        let post_hits =
            vault::search_notes_natural(&vault, "Ryokan Sakura", None).expect("search appended");
        assert_eq!(post_hits.len(), 1);
        assert_eq!(post_hits[0].id, note.id);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn phase_5_corrupt_index_and_recovery() {
        use crate::grain_space::note::Note;

        let unique = uuid::Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("grain_p5_recovery_{unique}"));
        let vault = Vault {
            root: base.join("vault"),
            folder: "Grain".to_string(),
            index_base: base.join("appdata"),
            native: false,
        };
        fs::create_dir_all(&vault.root).unwrap();
        fs::create_dir_all(&vault.index_base).unwrap();

        for i in 1..=3 {
            let mut note = Note::raw(format!("Content of document {i} with key term alpha{i}"));
            note.title = format!("Document {i}");
            vault::save_note(&vault, &note).expect("save note");
        }

        assert_eq!(
            vault::search_notes_natural(&vault, "alpha1", None)
                .unwrap()
                .len(),
            1
        );

        // Corrupt / delete index files
        let _ = fs::remove_dir_all(&vault.index_base);
        fs::create_dir_all(&vault.index_base).unwrap();

        // Rebuild index from disk markdown files
        let reindexed = vault::rebuild_index(&vault).expect("rebuild index");
        assert_eq!(reindexed, 3);

        // Verify all documents are retrievable again
        assert_eq!(
            vault::search_notes_natural(&vault, "alpha1", None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            vault::search_notes_natural(&vault, "alpha2", None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            vault::search_notes_natural(&vault, "alpha3", None)
                .unwrap()
                .len(),
            1
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn phase_5_external_edit_and_concurrency_failure() {
        use crate::grain_space::note::Note;

        let unique = uuid::Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("grain_p5_concurrency_{unique}"));
        let vault = Vault {
            root: base.join("vault"),
            folder: "Grain".to_string(),
            index_base: base.join("appdata"),
            native: false,
        };
        fs::create_dir_all(&vault.root).unwrap();
        fs::create_dir_all(&vault.index_base).unwrap();

        let mut note = Note::raw("Initial draft of architecture".to_string());
        note.title = "Architecture".to_string();
        vault::save_note(&vault, &note).expect("save initial note");

        // Agent prepares append and binds version hash v1
        let v1 =
            vault::note_storage_version(&vault, &note.id).expect("read exact persisted version");

        // External edit occurs before confirmation
        let mut external_edit = note.clone();
        external_edit.body = "Overhauled draft by user in Obsidian".to_string();
        vault::save_note(&vault, &external_edit).expect("save external edit");

        // Invoke real production append implementation with stale version hash v1
        let append_res = vault::append_note_atomic(
            &vault,
            &note.id,
            "Conflicting agent addition that must be rejected",
            Some(&v1),
        );

        // Assert failure closed: stale version rejected
        assert!(append_res.is_err(), "Must fail closed on stale version");
        let err_msg = append_res.unwrap_err().to_string();
        assert!(
            err_msg.contains("modified externally") || err_msg.contains("Stale"),
            "Error message must indicate external modification: {err_msg}"
        );

        // Note on disk remains exactly the user's external edit without 3-way merge
        let current = vault::get_note(&vault, &note.id).expect("get note");
        assert_eq!(current.body, "Overhauled draft by user in Obsidian");
        assert!(!current.body.contains("Conflicting agent addition"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn phase_5_operation_identity_idempotency() {
        use crate::grain_space::{
            begin_idempotent_write, finish_idempotent_write, IdempotencyStart,
        };

        let key = format!("op_key_{}", uuid::Uuid::new_v4());
        assert!(matches!(
            begin_idempotent_write(&key).unwrap(),
            IdempotencyStart::Started
        ));
        assert!(
            begin_idempotent_write(&key).is_err(),
            "a concurrent duplicate must not execute"
        );

        // A failed write releases its reservation so a genuine retry can run.
        finish_idempotent_write(&key, false);
        assert!(matches!(
            begin_idempotent_write(&key).unwrap(),
            IdempotencyStart::Started
        ));
        finish_idempotent_write(&key, true);
        assert!(matches!(
            begin_idempotent_write(&key).unwrap(),
            IdempotencyStart::Duplicate
        ));
    }

    #[tokio::test]
    async fn phase_5_host_argument_bounds_enforcement() {
        use crate::grain_llm_client::ToolCallOut;
        use crate::grain_space::agent_tools::{self, NoteToolResult, TurnLog};

        let mut log = TurnLog::new();

        // 1. Oversized query (> 4096 bytes)
        let huge_query = "a".repeat(5000);
        let call_query = ToolCallOut {
            id: "call_q".to_string(),
            name: "search_notes".to_string(),
            arguments: serde_json::json!({ "query": huge_query }).to_string(),
        };
        let res_q = agent_tools::execute_opt(None, &call_query, &mut log).await;
        match res_q {
            NoteToolResult::Text(msg) => {
                assert!(msg.contains("exceeds maximum allowed size"));
            }
            _ => panic!("Expected text rejection for oversized query"),
        }

        // 2. Oversized body (> 65536 bytes)
        let huge_body = "b".repeat(70000);
        let call_body = ToolCallOut {
            id: "call_b".to_string(),
            name: "save_note".to_string(),
            arguments: serde_json::json!({ "body": huge_body }).to_string(),
        };
        let res_b = agent_tools::execute_opt(None, &call_body, &mut log).await;
        match res_b {
            NoteToolResult::Text(msg) => {
                assert!(msg.contains("exceeds maximum allowed size"));
            }
            _ => panic!("Expected text rejection for oversized body"),
        }

        // 3. Oversized title (> 80 bytes)
        let huge_title = "t".repeat(350);
        let call_title = ToolCallOut {
            id: "call_t".to_string(),
            name: "save_note".to_string(),
            arguments: serde_json::json!({ "title": huge_title, "body": "valid body" }).to_string(),
        };
        let res_t = agent_tools::execute_opt(None, &call_title, &mut log).await;
        match res_t {
            NoteToolResult::Text(msg) => {
                assert!(msg.contains("exceeds maximum allowed size"));
            }
            _ => panic!("Expected text rejection for oversized title"),
        }
    }

    #[test]
    fn phase_5_temporal_search_without_lexical_fallback() {
        use crate::grain_space::note::Note;
        use chrono::{Datelike, Local, TimeZone};

        let unique = uuid::Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("grain_p5_temporal_{unique}"));
        let vault = Vault {
            root: base.join("vault"),
            folder: "Grain".to_string(),
            index_base: base.join("appdata"),
            native: false,
        };
        fs::create_dir_all(&vault.root).unwrap();
        fs::create_dir_all(&vault.index_base).unwrap();

        let now = Local::now();
        let today_start = Local
            .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let today_end = today_start + 86_400_000 - 1;

        // Note 1: Created today, body contains NO temporal words like "today"
        let mut note_today = Note::raw("Grocery shopping list: milk, eggs, bread".to_string());
        note_today.title = "Groceries".to_string();
        note_today.timestamp = today_start + 1000;
        vault::save_note(&vault, &note_today).expect("save today note");

        // Note 2: Created a week ago
        let mut note_old = Note::raw("Car maintenance records: oil change".to_string());
        note_old.title = "Car".to_string();
        note_old.timestamp = today_start - 7 * 86_400_000;
        vault::save_note(&vault, &note_old).expect("save old note");

        // Temporal query with today's range and EMPTY query text
        let hits = vault::search_notes_natural(&vault, "", Some((today_start, today_end)))
            .expect("search");
        assert_eq!(hits.len(), 1, "Must find exactly 1 note from today");
        assert_eq!(hits[0].id, note_today.id);
        assert_eq!(hits[0].title, "Groceries");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn phase_5_vault_switching_isolation() {
        use crate::grain_space::note::Note;

        let unique = uuid::Uuid::new_v4().to_string();
        let base_a = std::env::temp_dir().join(format!("grain_p5_va_{unique}"));
        let vault_a = Vault {
            root: base_a.join("vault"),
            folder: "Grain".to_string(),
            index_base: base_a.join("appdata"),
            native: false,
        };
        let base_b = std::env::temp_dir().join(format!("grain_p5_vb_{unique}"));
        let vault_b = Vault {
            root: base_b.join("vault"),
            folder: "Grain".to_string(),
            index_base: base_b.join("appdata"),
            native: false,
        };
        fs::create_dir_all(&vault_a.root).unwrap();
        fs::create_dir_all(&vault_a.index_base).unwrap();
        fs::create_dir_all(&vault_b.root).unwrap();
        fs::create_dir_all(&vault_b.index_base).unwrap();

        let mut note_a = Note::raw("Classified project codename Apollo".to_string());
        note_a.title = "Vault A Note".to_string();
        vault::save_note(&vault_a, &note_a).expect("save in A");

        let mut note_b = Note::raw("Classified project codename Artemis".to_string());
        note_b.title = "Vault B Note".to_string();
        vault::save_note(&vault_b, &note_b).expect("save in B");

        // Search in vault A
        let hits_a = vault::search_notes_natural(&vault_a, "Classified project", None).unwrap();
        assert_eq!(hits_a.len(), 1);
        assert_eq!(hits_a[0].title, "Vault A Note");

        // Search in vault B
        let hits_b = vault::search_notes_natural(&vault_b, "Classified project", None).unwrap();
        assert_eq!(hits_b.len(), 1);
        assert_eq!(hits_b[0].title, "Vault B Note");

        // Zero cross-contamination
        assert!(vault::search_notes_natural(&vault_a, "Artemis", None)
            .unwrap()
            .is_empty());
        assert!(vault::search_notes_natural(&vault_b, "Apollo", None)
            .unwrap()
            .is_empty());

        let _ = fs::remove_dir_all(&base_a);
        let _ = fs::remove_dir_all(&base_b);
    }
}
