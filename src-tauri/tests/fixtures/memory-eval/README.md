# Grain Space Memory Evaluation Fixture

`v1/` contains the general-purpose, realistic evaluation corpus and golden test set for Grain Space note creation, retrieval, and append targeting.

## Design Principles

In accordance with `docs/Grain Space 2.0/GRAIN-SPACE-IMPROVEMENT-PLAN.md` §10:

1. **Realistic & General-Purpose:** Covers real user notes across diverse domains:
   - Ideas & iterative improvements
   - Books, articles, video tutorials, and URLs
   - Recipes and similarly named items
   - Purchases, specifications, and hardware comparisons
   - Travel suggestions and itineraries
   - Quotations and selected passages
   - Personal facts, medical allergies, and appointments
   - Creative concepts and world-building notes
   - Work notes across professions (clinical medicine, legal leases, architecture/structural specs, culinary prep, 7th-grade science teaching)
   - Meetings captured as ordinary notes
   - Near-duplicate titles and topics (e.g. coffee grinder dials, cleaning, descaling)
   - Notes with varied creation timestamps (today, yesterday, last week, last month, past years)
   - Unicode/multilingual scripts, long logs (>4KB), malformed frontmatter, and prompt-injection defense.

2. **Held-out Golden Set:** The queries and expected note rankings in `v1/golden.json` represent unseen retrieval and disambiguation challenges. They must not be reverse-engineered or hardcoded into the note titles or bodies.

3. **Multi-mode Operation:**
   - **Lexical / FTS5 (Model-Disabled):** Runs out-of-the-box with zero model or network dependencies, scoring BM25 ranking, exact matches, prefix matches, and entity hits.
   - **Semantic / Hybrid Fusion:** When local embeddings are available, evaluates reciprocal rank fusion (RRF) and similarity reranking.

## Running Evaluation

From `src-tauri`:

```powershell
cargo run -- --eval tests/fixtures/memory-eval/v1/golden.json --json
```

Or run via automated test integration:

```powershell
cargo test --lib grain_space::eval
```
