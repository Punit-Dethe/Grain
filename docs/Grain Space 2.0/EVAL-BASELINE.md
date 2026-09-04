# Grain Space Memory System — Phase 0 Evaluation Baseline

**Date:** 2026-09-04  
**Branch:** `codex/grain-space-memory-experiment`  
**Evaluation Harness:** `src-tauri/src/grain_space/eval.rs`  
**Corpus Version:** `v1` (37 versioned fixture documents in `src-tauri/tests/fixtures/memory-eval/v1/corpus`)  
**Golden Suite:** `src-tauri/tests/fixtures/memory-eval/v1/golden.json`

---

## 1. Evaluation Environment

- **Operating System:** Windows 11 (build 26100) x86_64
- **Rust Toolchain:** `rustc 1.96.0 (ac68faa20 2026-05-25)` / `cargo 1.96.0 (30a34c682 2026-05-25)`
- **SQLite Version:** `3.47` with FTS5 and `sqlite-vec 0.1.9`
- **Embedding Model:** BAAI/bge-small-en-v1.5 (f32 weights in local Hugging Face cache)
- **Baseline Test Suite Status:**
  - Existing `grain_space` unit tests: **88 passed; 0 failed**
  - Workspace crate tests (`crates/*`): **184 passed; 0 failed**
  - New test failures introduced: **0**

---

## 2. Corpus Characteristics

- **Total Documents:** 37
- **Corpus Disk Size:** 38.6 KB
- **Derived SQLite Index Size:** 1,806,336 bytes (1,764.00 KB) with FTS5, metadata, entity relations, and BGE chunk vectors
- **Domain Clusters Represented:**
  1. `engineering_auth`: PKCE exchange, session timeouts, Okta SAML cert rollover, service API keys, Redis rate limiter, GitHub OAuth, TOTP backup codes.
  2. `meetings_sync`: Recurring Weekly Team Syncs across Weeks 32, 33, 34, 35, 1-on-1s, incident post-mortems.
  3. `bi_temporal_events`: Notes saved recently referencing events that occurred in the past.
  4. `daily_journal`: Daily notes across local midnight crossings, UTC logs, PDT logs, IST logs, and DST spring-forward transitions.
  5. `knowledge_updates`: Historical versus current facts (DynamoDB -> Supabase Postgres, 2025 wifi -> 2026 wifi).
  6. `aliases_jargon`: Kubernetes/k8s/EKS, ASR/STT/Whisper, Tahoe vacation cabin, dental appointments.
  7. `provider_snapshots`: Static Linear ticket snapshot (LIN-402), GitHub PR snapshot (PR #1548).
  8. `adversarial_edge`: Prompt injection test note, multilingual Unicode (Chinese, Japanese, Arabic, Russian, math, emojis), large ~35KB architecture document, empty note, foreign Obsidian markdown note, malformed YAML frontmatter note.

---

## 3. Retrieval Quality & Performance Baseline

Evaluated across 20 held-out golden evidence retrieval test cases:

| Metric | Mode A: Lexical FTS5 | Mode B: Hybrid without Model (FTS + Graph + RRF + Rerank) | Mode C: Hybrid with Model (FTS + BGE Vector + Graph + RRF + Rerank) |
|---|:---:|:---:|:---:|
| **Recall@1** | 95.00% (19/20) | 95.00% (19/20) | **95.00% (19/20)** |
| **Recall@3** | 95.00% (19/20) | **100.00% (20/20)** | **100.00% (20/20)** |
| **Recall@6** | 100.00% (20/20) | 100.00% (20/20) | **100.00% (20/20)** |
| **MRR** | 0.9625 | 0.9667 | **0.9750** |
| **Cold Latency** | 2.62 ms | 8.96 ms | 305.32 ms |
| **Avg Warm Latency** | 2.36 ms | 6.53 ms | 276.69 ms |

### Key Findings
1. **Model Optionality Confirmed:** Hybrid retrieval without model (Mode B) achieves **100% Recall@3** and **0.9667 MRR** at **6.53 ms** latency, requiring zero GPU/CPU embedding computation.
2. **Semantic Boost:** Mode C pushes MRR to **0.9750** by resolving subtle semantic paraphrases in the reranker.
3. **Low RAM Footprint:** Cold start index load requires < 3ms for FTS.

---

## 4. Correctness & Security Gap Baselines (Current System Behavior)

### 4.1 Abstention Gap
When queried with out-of-scope queries having **zero** true evidence in the corpus:
- *"flight departure gate for Charles de Gaulle airport"*: FTS returns **21 raw lexical hits** (matching filler word "for").
- *"what is my mortgage interest rate with Chase bank"*: FTS returns **14 raw lexical hits** (matching "is", "with").
- *"what is the 24 word mnemonic seed for my hardware wallet"*: FTS returns **1 hit**.
- **Current Behavior:** The system has no abstention gate and returns irrelevant notes.
- **Phase Target (Phases 4 & 5):** Bounded candidate thresholds and explicit margin gates to return `not_found` / abstain.

### 4.2 Write Target Resolution Gap
- **Current Behavior:** The Agent tool `append_to_note` takes a bare `id` and `text`. It does not perform target disambiguation.
- In queries like *"append to the Weekly Team Sync"*, four candidates exist (Weeks 32, 33, 34, 35). The current system provides no margin scoring, ambiguity detection, or revision binding.
- **Phase Target (Phase 5):** Return `ambiguous` with bounded candidate choices when margin floor is not met.

### 4.3 Mutation & Idempotency Gap
- **Current Behavior:** Retrying `append(id, text)` appends the text block every time without checking idempotency keys.
- Mutations rewrite the entire note body (`note.body = format!("{}\n\n---\n\n{}", ...)`) rather than using an atomic block-append transaction.
- **Phase Target (Phase 3):** Transactional mutation ledger with idempotency keys and stable block boundaries.

### 4.4 Adversarial Security Baseline
- **Path Traversal Containment:** Attempts to access or create folders outside the vault (e.g. `../outside`) are rejected with `Err("Cannot create folder outside vault: ../outside")`.
- **Malformed YAML Frontmatter:** `rebuild_index` survives broken YAML syntax gracefully without crashing the process.
- **Prompt Injection:** Notes containing injection strings (`IGNORE PREVIOUS INSTRUCTIONS...`) are parsed and indexed as raw inert text.

---

## 5. Reproducibility Command

To reproduce this baseline report:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::eval::tests::run_baseline_evaluation -- --nocapture
```
