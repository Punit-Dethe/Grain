# Grain Recall — Search & Retrieval Overhaul (the plan)

> Builds on `RECALL-PLAN.md` (Recall R1–R3, shipped). Read that + the top
> `TRANSITION-LOG.md` entry first. This adds two capabilities to the SAME
> pipeline (`src-tauri/src/grain_space/recall.rs`); no new surfaces, no new
> windows, no passive/idle cost. Everything here happens only *during an active
> turn*, where extra latency is acceptable.

Where it plugs in today: `recall::run_turn` retrieves once (`retrieve()` = FTS ∪
semantic, RRF-fused, top **6**), builds the memories block (stable M-ids in
`RecallSession`), and makes ONE LLM call (`agent::run_messages`). The reply ends
in a tolerant trailing convention line (`SOURCES:` / `NOT_FOUND` / `ACTION:`).
Both features extend exactly these pieces.

---

## Feature 1 — Dual-stage retrieval ("20 then 6")

**Problem:** raw top-6 by fused cosine/FTS rank isn't always the *relevant* 6.

**Change:** widen the candidate pool, then rerank down to 6 before the block.

- `retrieve()` fuses to a **candidate pool of ~20** (`CANDIDATE_POOL = 20`)
  instead of `TOP_K_PER_TURN = 6`. The semantic leg already pulls `LIMIT 24`
  and FTS is cheap, so this is one pass, ~free.
- New `rerank(query, candidates) -> top 6`. Two ways to implement; ship in this
  order:
  1. **Heuristic first (no new model, do this now).** Re-score the 20 by
     combining the fused RRF score with (a) query-term overlap in title/tldr and
     (b) the existing recency decay (`exp(-λΔt)`, λ from
     `grain_space_decay_half_life_days`). Deterministic, testable, zero latency.
  2. **Cross-encoder rerank (opt-in, later).** A small reranker
     (e.g. `BAAI/bge-reranker-base`) scoring `(query, note)` pairs — best
     quality, but it's a *second* model. Gate it exactly like the embed model:
     opt-in download, `spawn_blocking`, dropped with the engine, silent
     fall-through to the heuristic when absent. Its own `grain_space_rerank`
     setting. **Do not add this until the heuristic is proven insufficient.**
- The "let the LLM narrow it" variant is subsumed by Feature 2 — don't build a
  separate LLM rerank call; if the 6 are wrong, the LLM can `SEARCH:` again.

**Files:** `recall.rs` (`retrieve`, new `rerank`), `store.rs` untouched
(semantic already returns 24). Tests: rerank ordering (recency vs raw score,
term-overlap tie-break).

---

## Feature 2 — Two-way LLM (agentic search mid-conversation)

**Idea:** the LLM isn't stuck with the notes it was handed. If it lacks a fact,
it asks for another search *before* answering — for both reads and writes.

**Mechanism — a `SEARCH:` convention (mirrors the existing trailing lines).**
When the model needs more, it replies with exactly one line `SEARCH: <query>`
and nothing else. `run_turn` detects it (extend `parse_tail` /`ParsedTail`),
runs `retrieve(query)`, folds the hits into the SAME `RecallSession` registry
(union, stable M-ids — already how follow-up turns work), rebuilds the block,
and calls `run_messages` again. Bounded loop: **max 2 search hops per turn**
(`MAX_SEARCH_HOPS = 2`) so latency stays inside the active-turn budget; after the
cap, force a normal answer/clarify. This adds an embedding pass ONLY on the
minority of turns that need it, and never touches idle state.

**Prompt (system_prompt v2):** add a rule — *"If the memories below don't
contain the fact you need, DO NOT guess. Reply with exactly `SEARCH: <a focused
query>` to look again."* Keep it one line, same tolerant-parse discipline as
SOURCES/ACTION.

### Reads
Initial 6 don't cover it → model emits `SEARCH:` → we retrieve + re-ask → it
answers from the widened block (or honestly `NOT_FOUND` if still absent).

### Writes (extends the R3 `ACTION:` conventions)
"change the wifi password" → the model `SEARCH:`es for matching notes, then acts
on match count:

| Matches | Behavior |
|---|---|
| **1** | Resolve directly → `ACTION: update Mn` (forget still confirms in-panel). Answer confirms "done". |
| **0 or 2+** | **Always ask a clarifying question — never a silent guess.** ("Home or office wifi?") |
| Prompt already disambiguates ("change the **home** wifi password") | Resolve directly, no question. |

**Guardrail (non-negotiable, prompt + rule wording):** on WRITES, ambiguous
match counts (0 or 2+) MUST trigger a clarifying question. A wrong silent edit is
worse than a wrong silent answer — the model is told this explicitly. Reads may
answer-then-hedge; writes must disambiguate first. The forget in-panel confirm
(R3) is the final backstop for deletes.

**Files:** `recall.rs` (`run_turn` loop + `parse_tail`/`ParsedTail` gains a
`search: Option<String>`), `system_prompt`. No new commands, no frontend change
(same panel, same string reply). Tests: `parse_tail` SEARCH extraction, hop cap,
union-across-hops keeps M-ids stable.

---

## Phasing & guardrails

- **S1 — Dual-stage (heuristic rerank).** Widen to 20, rerank to 6. Self-contained,
  fully unit-testable. Ship first.
- **S2 — Agentic `SEARCH:` loop (reads).** Convention + bounded loop + prompt v2.
- **S3 — Agentic writes.** Match-count disambiguation on the ACTION path + the
  write guardrail.
- **S4 (optional, later).** Cross-encoder reranker as an opt-in model.

**Invariants to preserve:** one embedding engine, dropped with its surfaces
(never resident idle); M-ids never renumber within a session; every extra LLM/
embedding hop is bounded and active-turn-only; degrade silently to FTS-only when
the model is absent; the model must `SEARCH:` or say `NOT_FOUND`/clarify — never
fabricate. Protocol unchanged: one phase at a time, commit+push per task, update
`TRANSITION-LOG.md` on every stop.
