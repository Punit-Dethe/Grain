# Grain Space Memory System Evaluation Corpus

This directory contains versioned evaluation fixtures for the Grain Space temporal memory system, implementing the requirements specified in [`docs/Grain Space 2.0/MEMORY-SYSTEM-PLAN.md`](../../../../docs/Grain%20Space%202.0/MEMORY-SYSTEM-PLAN.md) §12 and §13 (Phase 0).

## Layout

- `v1/corpus/`: 33 realistic documents across 8 distinct domains and difficult clusters:
  1. Engineering Authentication & Security (near-duplicate PKCE, SSO, SAML, session timeouts)
  2. Recurring Team Meetings (weekly syncs across Weeks 32, 33, 34, 35)
  3. Bi-temporal Events (notes saved recently describing past events)
  4. Daily Notes (midnight crossings, UTC vs PDT vs IST, DST transitions)
  5. Knowledge Updates & Corrections (superseded facts, updated configs)
  6. Aliases, Acronyms & Jargon (K8s, ASR/STT, colloquial phrases)
  7. Provider Snapshots (Linear, GitHub static snapshots vs live state)
  8. Adversarial, Edge & Malformed Notes (prompt injection, Unicode, large/empty documents, malformed YAML)

- `v1/golden.json`: Golden test cases with exact ground truth:
  - `evidence_retrieval`: Queries, target document IDs, temporal constraints, acceptable candidates.
  - `write_target`: Write intents, expected target resolution (`resolved`, `ambiguous`, `not_found`, `unsafe`), minimum margin.
  - `temporal_reasoning`: Spoken time expressions, fixed `now` reference timestamps, IANA timezones, expected `[start, end)` half-open UTC intervals.
  - `abstention`: Out-of-scope or unevidenced queries where retrieval/write-resolution must abstain rather than hallucinate.
  - `adversarial_security`: Payloads testing prompt-injection immunity, path containment, and malformed frontmatter resilience.
  - `mutation_safety`: Scenarios testing idempotent duplicate appends and concurrent external edit preservation.

## Running the Evaluation Harness

Run the evaluation suite in `src-tauri`:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::eval -- --nocapture
```

The harness runs against:
1. Lexical FTS (`search_notes`, `search_notes_natural`)
2. Hybrid without Model (FTS5 + Entity Graph + RRF + Reranker)
3. Hybrid with Model (FTS5 + BGE Embeddings + Entity Graph + RRF + Reranker)

And reports Recall@K, MRR, Precision, Abstention Rate, Latency, and Index Size.
