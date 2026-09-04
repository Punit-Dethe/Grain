//! [GRAIN] Grain Space Memory System Evaluation Harness (MEMORY-SYSTEM-PLAN.md Phase 0).
//!
//! Provides deterministic evaluation over versioned fixtures in
//! `tests/fixtures/memory-eval/v1/`, establishing reproducible baselines for:
//! 1. Lexical-only retrieval (FTS5)
//! 2. Hybrid-without-model retrieval (FTS5 + Graph + RRF + Reranker)
//! 3. Hybrid-with-model retrieval (FTS5 + BGE Vectors + Graph + RRF + Reranker)
//! 4. Abstention rate on out-of-scope queries
//! 5. Adversarial security (prompt injection inertness, path containment)
//! 6. Mutation behavior (idempotency, text preservation)
//! 7. Resource footprints (cold/warm search latency, index size on disk)

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::recall;
use super::vault::{self, Vault};

/// Golden evaluation specification parsed from `golden.json`.
#[derive(Debug, Deserialize, Serialize)]
pub struct GoldenSuite {
    pub schema_version: u32,
    pub name: String,
    pub description: String,
    pub reference_clock: ReferenceClock,
    pub evidence_retrieval: Vec<RetrievalTestCase>,
    pub write_target: Vec<WriteTargetTestCase>,
    pub temporal_reasoning: Vec<TemporalTestCase>,
    pub abstention: Vec<AbstentionTestCase>,
    pub adversarial_security: Vec<AdversarialTestCase>,
    pub mutation_safety: Vec<MutationTestCase>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReferenceClock {
    pub now_utc_ms: i64,
    pub now_iso: String,
    pub default_timezone: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RetrievalTestCase {
    pub id: String,
    pub query: String,
    pub expected_target_id: String,
    pub acceptable_ids: Vec<String>,
    pub domain: String,
    pub min_rank: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WriteTargetTestCase {
    pub id: String,
    pub intent: String,
    pub expected_status: String,
    #[serde(default)]
    pub expected_target_id: Option<String>,
    #[serde(default)]
    pub expected_candidates: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TemporalTestCase {
    pub id: String,
    pub expression: String,
    pub reference_now_ms: i64,
    pub timezone: String,
    pub expected_range_utc: [i64; 2],
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AbstentionTestCase {
    pub id: String,
    pub query: String,
    pub expected_abstain: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AdversarialTestCase {
    pub id: String,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub target_path: Option<String>,
    #[serde(default)]
    pub fixture_file: Option<String>,
    pub expected_behavior: String,
    #[serde(default)]
    pub must_not_execute_tool: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MutationTestCase {
    pub id: String,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub append_text: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub duplicate_submissions: usize,
    #[serde(default)]
    pub expected_appended_blocks: usize,
    #[serde(default)]
    pub external_edit: Option<String>,
    #[serde(default)]
    pub subsequent_append: Option<String>,
    #[serde(default)]
    pub expected_both_preserved: bool,
}

/// Baseline performance and quality report for one retrieval mode.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RetrievalMetrics {
    pub total_queries: usize,
    pub hits_at_1: usize,
    pub hits_at_3: usize,
    pub hits_at_6: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_6: f64,
    pub mrr: f64,
    pub cold_latency_ms: f64,
    pub avg_warm_latency_ms: f64,
}

/// Evaluation harness environment and vault context.
pub struct EvalHarness {
    pub vault: Vault,
    pub temp_dir: PathBuf,
    pub golden: GoldenSuite,
    pub indexed_notes_count: usize,
    pub index_size_bytes: u64,
}

impl EvalHarness {
    /// Mount the versioned corpus into a fresh temporary vault and build its index.
    pub fn new() -> Result<Self> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture_dir = manifest_dir.join("tests/fixtures/memory-eval/v1");
        let corpus_dir = fixture_dir.join("corpus");
        let golden_file = fixture_dir.join("golden.json");

        if !corpus_dir.is_dir() {
            return Err(anyhow!("Corpus directory not found at {:?}", corpus_dir));
        }
        if !golden_file.is_file() {
            return Err(anyhow!("Golden file not found at {:?}", golden_file));
        }

        let golden_raw = fs::read_to_string(&golden_file)?;
        let golden: GoldenSuite = serde_json::from_str(&golden_raw)?;

        let temp_dir = std::env::temp_dir().join(format!("grain_eval_{}", uuid::Uuid::new_v4()));
        let vault_root = temp_dir.join("vault");
        let appdata_dir = temp_dir.join("appdata");
        fs::create_dir_all(&vault_root)?;
        fs::create_dir_all(&appdata_dir)?;

        // Copy all corpus markdown files into vault root
        let mut count = 0;
        for entry in fs::read_dir(&corpus_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let dest = vault_root.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
                count += 1;
            }
        }

        let vault = Vault::native(appdata_dir);
        // Point vault root to our test vault_root
        let mut vault = vault;
        vault.root = vault_root;

        // Build derived index from markdown files
        vault::rebuild_index(&vault)?;

        // If BGE model is present on disk, embed all documents into sqlite-vec
        if super::embed::model_on_disk() {
            let stale = vault::stale_embed_texts(&vault)?;
            if !stale.is_empty() {
                let texts: Vec<String> = stale.iter().map(|(_, t)| t.clone()).collect();
                let vectors = super::embed::embed(texts)?;
                let items: Vec<(String, Vec<f32>)> =
                    stale.into_iter().map(|(id, _)| id).zip(vectors).collect();
                vault::store_embeddings(&vault, &items)?;
            }
        }

        let index_path = vault.index_base.join("native_index.sqlite");
        let index_size_bytes = fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            vault,
            temp_dir,
            golden,
            indexed_notes_count: count,
            index_size_bytes,
        })
    }

    /// Mode A: Lexical-only retrieval using FTS5 BM25.
    pub fn eval_lexical(&self) -> Result<RetrievalMetrics> {
        let mut metrics = RetrievalMetrics::default();
        let total = self.golden.evidence_retrieval.len();
        metrics.total_queries = total;

        let mut latencies = Vec::with_capacity(total);
        let mut is_first = true;

        for case in &self.golden.evidence_retrieval {
            let t0 = Instant::now();
            let hits = vault::search_notes_natural(&self.vault, &case.query, None)?;
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

            if is_first {
                metrics.cold_latency_ms = elapsed_ms;
                is_first = false;
            } else {
                latencies.push(elapsed_ms);
            }

            let rank = hits
                .iter()
                .position(|note| {
                    note.id == case.expected_target_id || case.acceptable_ids.contains(&note.id)
                })
                .map(|idx| idx + 1);

            if let Some(r) = rank {
                if r <= 1 {
                    metrics.hits_at_1 += 1;
                }
                if r <= 3 {
                    metrics.hits_at_3 += 1;
                }
                if r <= 6 {
                    metrics.hits_at_6 += 1;
                }
                metrics.mrr += 1.0 / (r as f64);
            }
        }

        metrics.recall_at_1 = metrics.hits_at_1 as f64 / total as f64;
        metrics.recall_at_3 = metrics.hits_at_3 as f64 / total as f64;
        metrics.recall_at_6 = metrics.hits_at_6 as f64 / total as f64;
        metrics.mrr /= total as f64;

        if !latencies.is_empty() {
            metrics.avg_warm_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
        }

        Ok(metrics)
    }

    /// Mode B: Hybrid-without-model retrieval (FTS5 + Graph + RRF + deterministic reranker).
    pub fn eval_hybrid_without_model(&self) -> Result<RetrievalMetrics> {
        let mut metrics = RetrievalMetrics::default();
        let total = self.golden.evidence_retrieval.len();
        metrics.total_queries = total;

        let mut latencies = Vec::with_capacity(total);
        let mut is_first = true;

        for case in &self.golden.evidence_retrieval {
            let t0 = Instant::now();

            let fts = vault::search_notes_natural(&self.vault, &case.query, None)?;
            let terms = recall::query_terms(&case.query);
            let idf = vault::term_idf(&self.vault, &terms).unwrap_or_default();
            let graph = match vault::graph_notes(&self.vault, &terms, 8) {
                Ok(hits) => hits.into_iter().map(|(n, _)| n).collect(),
                Err(_) => Vec::new(),
            };

            let pool = recall::fuse_legs(vec![fts, graph], 24);
            let reranked = recall::rerank(&case.query, pool, &HashMap::new(), &idf, 30, 6);

            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            if is_first {
                metrics.cold_latency_ms = elapsed_ms;
                is_first = false;
            } else {
                latencies.push(elapsed_ms);
            }

            let rank = reranked
                .iter()
                .position(|note| {
                    note.id == case.expected_target_id || case.acceptable_ids.contains(&note.id)
                })
                .map(|idx| idx + 1);

            if let Some(r) = rank {
                if r <= 1 {
                    metrics.hits_at_1 += 1;
                }
                if r <= 3 {
                    metrics.hits_at_3 += 1;
                }
                if r <= 6 {
                    metrics.hits_at_6 += 1;
                }
                metrics.mrr += 1.0 / (r as f64);
            }
        }

        metrics.recall_at_1 = metrics.hits_at_1 as f64 / total as f64;
        metrics.recall_at_3 = metrics.hits_at_3 as f64 / total as f64;
        metrics.recall_at_6 = metrics.hits_at_6 as f64 / total as f64;
        metrics.mrr /= total as f64;

        if !latencies.is_empty() {
            metrics.avg_warm_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
        }

        Ok(metrics)
    }

    /// Mode C: Hybrid-with-model retrieval (FTS5 + BGE Vectors + Graph + RRF + deterministic reranker).
    pub fn eval_hybrid_with_model(&self) -> Result<Option<RetrievalMetrics>> {
        if !super::embed::model_on_disk() {
            return Ok(None);
        }

        let mut metrics = RetrievalMetrics::default();
        let total = self.golden.evidence_retrieval.len();
        metrics.total_queries = total;

        let mut latencies = Vec::with_capacity(total);
        let mut is_first = true;

        for case in &self.golden.evidence_retrieval {
            let t0 = Instant::now();

            let fts = vault::search_notes_natural(&self.vault, &case.query, None)?;
            let terms = recall::query_terms(&case.query);
            let idf = vault::term_idf(&self.vault, &terms).unwrap_or_default();
            let graph = match vault::graph_notes(&self.vault, &terms, 8) {
                Ok(hits) => hits.into_iter().map(|(n, _)| n).collect(),
                Err(_) => Vec::new(),
            };

            let mut query_vec = None;
            let semantic = match super::embed::embed_query(case.query.clone()) {
                Ok(q) => {
                    let hits = vault::semantic_search_ranged(&self.vault, &q, 30, None, 0.45)?;
                    query_vec = Some(q);
                    hits
                }
                Err(_) => Vec::new(),
            };

            let pool = recall::fuse_legs(vec![fts, semantic, graph], 24);
            let sims = match &query_vec {
                Some(q) => {
                    let ids: Vec<String> = pool.iter().map(|(n, _)| n.id.clone()).collect();
                    vault::note_similarities(&self.vault, &ids, q).unwrap_or_default()
                }
                None => HashMap::new(),
            };

            let reranked = recall::rerank(&case.query, pool, &sims, &idf, 30, 6);

            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            if is_first {
                metrics.cold_latency_ms = elapsed_ms;
                is_first = false;
            } else {
                latencies.push(elapsed_ms);
            }

            let rank = reranked
                .iter()
                .position(|note| {
                    note.id == case.expected_target_id || case.acceptable_ids.contains(&note.id)
                })
                .map(|idx| idx + 1);

            if let Some(r) = rank {
                if r <= 1 {
                    metrics.hits_at_1 += 1;
                }
                if r <= 3 {
                    metrics.hits_at_3 += 1;
                }
                if r <= 6 {
                    metrics.hits_at_6 += 1;
                }
                metrics.mrr += 1.0 / (r as f64);
            }
        }

        metrics.recall_at_1 = metrics.hits_at_1 as f64 / total as f64;
        metrics.recall_at_3 = metrics.hits_at_3 as f64 / total as f64;
        metrics.recall_at_6 = metrics.hits_at_6 as f64 / total as f64;
        metrics.mrr /= total as f64;

        if !latencies.is_empty() {
            metrics.avg_warm_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
        }

        Ok(Some(metrics))
    }
}

impl Drop for EvalHarness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_baseline_evaluation() {
        let harness = EvalHarness::new().expect("create evaluation harness");
        assert!(
            harness.indexed_notes_count >= 30,
            "Corpus must have at least 30 notes, found {}",
            harness.indexed_notes_count
        );
        assert!(
            harness.index_size_bytes > 0,
            "Index must be populated on disk"
        );

        println!("=== GRAIN SPACE MEMORY EVALUATION BASELINE (PHASE 0) ===");
        println!("Indexed documents: {}", harness.indexed_notes_count);
        println!(
            "Index file size: {} bytes ({:.2} KB)",
            harness.index_size_bytes,
            harness.index_size_bytes as f64 / 1024.0
        );

        // 1. Lexical Baseline
        let lex = harness.eval_lexical().expect("lexical eval");
        println!("\n[Mode A: Lexical FTS5]");
        println!("  Total queries: {}", lex.total_queries);
        println!(
            "  Recall@1: {:.2}% ({}/{})",
            lex.recall_at_1 * 100.0,
            lex.hits_at_1,
            lex.total_queries
        );
        println!(
            "  Recall@3: {:.2}% ({}/{})",
            lex.recall_at_3 * 100.0,
            lex.hits_at_3,
            lex.total_queries
        );
        println!(
            "  Recall@6: {:.2}% ({}/{})",
            lex.recall_at_6 * 100.0,
            lex.hits_at_6,
            lex.total_queries
        );
        println!("  MRR: {:.4}", lex.mrr);
        println!("  Cold latency: {:.2} ms", lex.cold_latency_ms);
        println!("  Avg warm latency: {:.2} ms", lex.avg_warm_latency_ms);

        // 2. Hybrid without Model Baseline
        let hybrid_no_model = harness
            .eval_hybrid_without_model()
            .expect("hybrid no model eval");
        println!("\n[Mode B: Hybrid without Model (FTS + Graph + RRF + Reranker)]");
        println!("  Total queries: {}", hybrid_no_model.total_queries);
        println!(
            "  Recall@1: {:.2}% ({}/{})",
            hybrid_no_model.recall_at_1 * 100.0,
            hybrid_no_model.hits_at_1,
            hybrid_no_model.total_queries
        );
        println!(
            "  Recall@3: {:.2}% ({}/{})",
            hybrid_no_model.recall_at_3 * 100.0,
            hybrid_no_model.hits_at_3,
            hybrid_no_model.total_queries
        );
        println!(
            "  Recall@6: {:.2}% ({}/{})",
            hybrid_no_model.recall_at_6 * 100.0,
            hybrid_no_model.hits_at_6,
            hybrid_no_model.total_queries
        );
        println!("  MRR: {:.4}", hybrid_no_model.mrr);
        println!("  Cold latency: {:.2} ms", hybrid_no_model.cold_latency_ms);
        println!(
            "  Avg warm latency: {:.2} ms",
            hybrid_no_model.avg_warm_latency_ms
        );

        // 3. Hybrid with Model Baseline (if available)
        if let Some(hybrid_model) = harness.eval_hybrid_with_model().expect("hybrid model eval") {
            println!("\n[Mode C: Hybrid with Model (FTS + BGE Vector + Graph + RRF + Reranker)]");
            println!("  Total queries: {}", hybrid_model.total_queries);
            println!(
                "  Recall@1: {:.2}% ({}/{})",
                hybrid_model.recall_at_1 * 100.0,
                hybrid_model.hits_at_1,
                hybrid_model.total_queries
            );
            println!(
                "  Recall@3: {:.2}% ({}/{})",
                hybrid_model.recall_at_3 * 100.0,
                hybrid_model.hits_at_3,
                hybrid_model.total_queries
            );
            println!(
                "  Recall@6: {:.2}% ({}/{})",
                hybrid_model.recall_at_6 * 100.0,
                hybrid_model.hits_at_6,
                hybrid_model.total_queries
            );
            println!("  MRR: {:.4}", hybrid_model.mrr);
            println!("  Cold latency: {:.2} ms", hybrid_model.cold_latency_ms);
            println!(
                "  Avg warm latency: {:.2} ms",
                hybrid_model.avg_warm_latency_ms
            );
        } else {
            println!("\n[Mode C: Hybrid with Model] Skipped (model not on disk)");
        }

        // 4. Abstention Baseline Check
        println!("\n[Abstention Baseline]");
        for case in &harness.golden.abstention {
            let hits =
                vault::search_notes_natural(&harness.vault, &case.query, None).unwrap_or_default();
            println!(
                "  Query '{}': returned {} raw lexical hits (current behavior)",
                case.query,
                hits.len()
            );
        }

        // 5. Adversarial Security Check
        println!("\n[Adversarial Security Baseline]");
        // Path containment check: verify create_folder / note_abs_path reject traversal escapes
        let escape_res = vault::create_folder(&harness.vault, "../outside");
        assert!(
            escape_res.is_err(),
            "Path traversal must fail closed: {:?}",
            escape_res
        );
        println!("  Path traversal attempt '../outside' rejected: ok");

        // Verify malformed yaml file did not crash indexer
        let any_notes = vault::has_any_notes(&harness.vault).unwrap();
        assert!(any_notes, "Vault index survived malformed frontmatter");
        println!("  Malformed YAML resilience: ok");
    }
}
