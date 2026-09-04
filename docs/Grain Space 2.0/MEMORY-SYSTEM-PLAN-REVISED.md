# Grain Space Memory System — Revised Launch Plan

**Status:** Proposed corrected continuation plan; awaiting user handoff

**Branch:** `codex/grain-space-memory-experiment`

**Date:** 2026-09-04

**Based on:** `MEMORY-SYSTEM-PLAN.md`

**Implementation boundary:** Phases 0–3 of the original plan are reported complete; Phase 4 is partially implemented. This revision does not audit that work. It tells the executor what may remain, what must be simplified, and what replaces the unfinished phases.

## 1. Product definition

Grain Space is a local, voice-first **text note memory**:

```text
speak/select text → create a good note → retrieve the note later
                                      ↘ append text to that note safely
```

The user should be able to say:

- “Remember this restaurant recommendation.”
- “Save this link; it is the video about color and perception.”
- “Remember my idea about a quieter onboarding experience.”
- “What was that book Anna recommended?”
- “Add this paragraph to the idea about onboarding.”
- “Add this problem to the Grain authentication issues.”
- “What was the authentication issue I mentioned yesterday?”

Every example produces or updates an ordinary Markdown note. A selected URL, screen excerpt, book, issue, recipe, person, meeting, or idea is **text content**, not a new storage type or capability.

The capture/selection pipeline already provides text to the Agent. This plan begins at that text boundary. It does not add screen understanding, links, images, video, files, attachments, or ingestion systems.

## 2. Governing principles

1. **One memory type: `Note`.** There are no foundational daily, meeting, project, thread, artifact, fact, episode, resource, or relationship objects.
2. **A good note does most of the work.** A concrete title, faithful TLDR, and complete body make lexical and semantic retrieval easier without requiring a clever Agent.
3. **Weak-Agent operability is mandatory.** The Agent identifies a simple user intent and passes text. Rust owns retrieval, temporal parsing, ambiguity, permissions, confirmation, concurrency, and persistence.
4. **No model is required for data safety.** Model failure may produce a less polished note; it may not lose the capture, select a write target, overwrite content, or make the note unsearchable.
5. **Append makes a note evolve.** An idea or issue becomes ongoing simply because later text is appended to the same note. No `Thread` type is necessary.
6. **Time is metadata and a search constraint.** It is not a daily-note container or temporal graph.
7. **Search indexes are derived.** FTS, vectors, snippets/chunks, entity terms, and any existing graph signal are rebuildable implementation details—not user memory.
8. **Launch simplicity wins.** Add an abstraction only after a Grain-specific evaluation proves the existing note model cannot satisfy an important request.

## 3. What the research actually supports

### 3.1 Mem, not Mem0, is the closest reference

Mem by Mem Labs is an unusually close product analogue: its desktop Push-to-Remember shortcut sends a spoken thought to an Agent that can find, create, or edit notes. Its current public storage/API model is centered on ordinary Markdown notes with IDs, creation/update timestamps, versions, and optional collection membership.

Mem's public behavior supports these Grain decisions:

- one general-purpose note rather than a domain ontology;
- raw input is shaped into a useful note;
- title lookup and keyword/filter search happen before semantic Deep Search;
- time-aware retrieval uses note time rather than daily containers;
- an update is based on the exact current note version;
- version history protects against bad edits;
- related snippets are search results inside notes, not canonical memory objects.

Mem 2.0 deliberately removed or narrowed separate People, Task, Daily/Scheduled Note, Inbox, and automatic-all-email concepts to improve simplicity and reliability. That is direct evidence against expanding Grain Space into a personal knowledge graph or collection of specialized memory systems.

Mem's exact internal prompts, thresholds, and database architecture are proprietary. Do not invent them or claim that Mem proves weak-model performance. Borrow only the public, evidence-backed product and API patterns.

### 3.2 Why not Mem0, A-MEM, or Graphiti

- Mem0 is primarily an agent-personalization memory SDK. Its inference path uses a model to extract/manage atomic facts and optional entities. Grain stores user-owned notes and must work with a modest model.
- A-MEM evolves note links and metadata using model reasoning. That adds failure modes without proving a launch-critical Grain benefit.
- Graphiti and related graph systems target temporal facts and multi-hop knowledge. Grain does not need a canonical graph, user-facing graph, or automatic relationship ontology to save and find text notes.
- LongMemEval reports that information compression into atomic facts can lose useful context, while better search keys and time-aware retrieval help. Grain already has title/TLDR/body and a hybrid index; improve those rather than decomposing notes into a fact store.

### 3.3 Research conclusion

Keep Grain's existing note architecture and hybrid search. Preserve the completed safety work. Remove the original plan's new ontology, canonical blocks, event-time system, meeting/daily special cases, and future graph evolution.

## 4. Correction to the original plan

| Original plan element | Revised decision |
|---|---|
| `MemoryDocument` kinds (`note`, `daily`, `meeting`, `project_log`) | Remove/neutralize. There is one ordinary `Note`. |
| Canonical `MemoryBlock` records | Do not use as durable memory. An internal append marker may remain only if required for safe persistence and invisible to the Agent/user. Search chunks are rebuildable projections. |
| `DerivedFact` and evolving links | Cancel for launch. |
| Resource objects/references | Cancel. Selected content, including a URL, is ordinary body text. |
| Event time, validity intervals, supersession graph | Cancel for launch. Use created/updated time and preserved note text. |
| Daily-note identity | Delete. |
| First-class meeting occurrence/series | Delete. A meeting note is an ordinary note. |
| Agent working-memory tier | Do not add. Use current conversation tool results and deterministic recent-note queries. |
| Graph expansion | Cancel. Freeze any existing graph signal as optional, derived, invisible, and benchmark-controlled. |
| Revision/hash, idempotency, confirmation, atomic write, recovery | Retain. These protect ordinary notes without changing the product. |
| Dedicated high-precision append targeting | Retain, but keep the Agent-facing operation simple. |
| Temporal retrieval | Retain in a smaller deterministic created/updated-time form. |
| Block-level evidence as durable architecture | Replace with note-level results and optional derived snippets for long-note search. |

## 5. Minimal durable model

Use the existing `Note` and existing field names wherever possible. Do not create a parallel memory schema.

```text
Note
  id
  title
  tldr
  body                 # ordinary Markdown text
  created_at/timestamp # when Grain saved it
  updated_at           # when its content last changed
  revision             # monotonic content version
  content_hash         # host-computed concurrency check
  collection           # existing optional organization
  source               # existing bounded provenance metadata
  existing optional fields (todos, reminder, pinned, question, entities)
```

Rules:

- Do not add a required `kind` field.
- If Phase 2 already added `kind`, occurrence IDs, series IDs, event intervals, aliases, or block classifications, they must not become required behavior or indexing dependencies. Remove them where safe; otherwise mark them compatibility-only and stop producing them until a later product decision.
- `entities` may remain a bounded search aid because Grain already has it. It is not a relationship graph and must not be required for successful retrieval.
- `title`, `tldr`, and `body` are the searchable content.
- `revision` changes exactly once per committed content write.
- `content_hash` is calculated by Rust from canonical current content.
- Existing Markdown remains the source of truth and remains readable outside Grain.

## 6. Capture and note construction

### 6.1 Narrow model responsibility

The note-construction model receives the transcript plus any text context already supplied by the existing capture/selection pipeline. It has one bounded job:

```text
Input:  text the user wants remembered
Output: descriptive title + faithful TLDR + readable Markdown body
```

It must not decide:

- a memory ontology or note kind;
- relationships or graph edges;
- whether existing memories should be deleted or rewritten;
- temporal validity intervals;
- permissions, paths, IDs, revisions, or confirmation;
- whether an ambiguous existing note is the intended append target.

### 6.2 Quality contract

A good generated note:

- uses concrete names and discriminating terms in the title;
- preserves every material fact, proper noun, number, date, quotation, and URL present in the supplied text;
- does not invent facts or silently turn uncertainty into certainty;
- removes conversational filler only when meaning is unchanged;
- keeps the body understandable without the original conversation;
- uses simple Markdown and bounded length;
- produces a one- or two-line TLDR useful for search.

Do not redesign the existing distillation prompt merely because this plan exists. First freeze examples of current good behavior, then change only demonstrated failures.

### 6.3 Deterministic fallback

If the model is missing, times out, emits invalid structure, returns empty content, exceeds limits, or fails validation:

1. Save the supplied text as the body without semantic rewriting.
2. Derive a bounded title from the first meaningful line/words using deterministic code.
3. Derive a bounded TLDR from the same text or leave the existing optional field empty.
4. Apply the host timestamp and normal ID/revision rules.
5. Index the result immediately.

The user's explicit “remember/save this” request must not disappear because note beautification failed.

## 7. Retrieval

### 7.1 Agent-facing contract

The minimum contract remains conceptually:

```text
search_notes(query)
get_note(id)
```

The weak Agent can pass the user's own description as `query`. It does not construct a retrieval plan, select ranking weights, name collections/types, traverse a graph, or iterate an unbounded search loop.

### 7.2 Host-owned note search

Retain the existing note-level pipeline:

1. exact ID/title matching;
2. title-prefix/title-token matching;
3. SQLite FTS over title, TLDR, body, and existing bounded search metadata;
4. optional local dense similarity over the same note representation;
5. RRF or the existing deterministic fusion;
6. deterministic reranking/boosts for exact title, lexical overlap, time, and recency.

Requirements:

- Models disabled: exact/title/FTS search still works.
- Embeddings unavailable: no request fails solely for that reason.
- Newly saved/updated notes are immediately discoverable through the synchronous lexical path even if semantic indexing is delayed.
- Results are bounded and return `id`, title, TLDR/excerpt, created/updated time, collection/source where already present, and score/match reasons for host diagnostics.
- Do not add a new chunk/block indexing architecture in this launch plan. Existing bounded excerpt/snippet behavior may remain, and FTS may return a matching excerpt from the ordinary note body.

### 7.3 Existing graph signal

Do not add relation types, graph writes, graph traversal APIs, or graph-derived note mutation.

If Grain's existing derived entity/co-occurrence candidate source is already shipped and inexpensive, it may remain behind evaluation. It must be:

- rebuildable from notes;
- invisible to the user and Agent contract;
- non-authoritative;
- removable without losing a note;
- disabled or removed if it does not improve held-out retrieval enough to justify its cost.

No remaining phase performs graph development.

## 8. Temporal retrieval without temporal memory types

Use only note `created_at`/existing capture timestamp and `updated_at` for launch.

Support a bounded deterministic vocabulary in the user's timezone:

- today, yesterday, day before yesterday;
- this week, last week;
- this month, last month;
- last `N` days;
- explicit dates already understood by Grain.

Rules:

1. The parser receives explicit `now` and timezone; tests never depend on wall-clock time.
2. It emits half-open local-time ranges converted to UTC.
3. Search the union of notes created or updated within an explicit range, then rank by textual relevance.
4. Keep the original query for semantic/lexical scoring. A deterministic query variant may remove the recognized time phrase, but both paths remain bounded.
5. If parsing is uncertain, do not invent a filter; search the phrase as text.
6. For read-only recall, a bounded fallback may soften an empty time filter and clearly mark the mismatch.
7. For append targeting, an explicit time constraint must never be silently softened.
8. Do not add event time, valid time, bitemporal facts, daily-note identities, or model-generated temporal plans.

This supports “yesterday's authentication issue” because a note created or appended yesterday is a strong temporal candidate. It does not promise perfect understanding of events described retrospectively; that is a measured future problem, not a launch ontology.

## 9. Safe append for a weak Agent

### 9.1 Simple public operation

The Agent-facing concept should be no more complex than:

```text
append_to_note(target_description, addition)
```

An already known note ID from the current conversation/search may be passed as an optimization, but raw possession of an ID is not write authorization.

The Agent must not be required to:

- call a multi-stage planner correctly;
- compare scores or confidence thresholds;
- manage revision or cryptographic tokens;
- generate a full replacement note body;
- decide how to merge concurrent edits;
- choose a block type or placement ontology.

### 9.2 Host flow

1. If the current interaction has an explicit note ID from a search/get/create result, validate it against the described target.
2. Otherwise run a dedicated high-precision note search using `target_description`.
3. Accept a target only when the top candidate clears a calibrated absolute threshold and top-one/top-two margin.
4. If ambiguous, return two or three compact choices for user clarification.
5. If none match, return `not_found`; do not silently create a new note when the user asked to append.
6. Prepare confirmation showing exact note title, identifying excerpt/TLDR, and exact addition.
7. Internally bind the prepared action to vault, note ID, revision/hash, operation, expiry, and idempotency key. The runtime manages this state; the model should not reason about it.
8. At commit, validate the binding and current revision again.
9. Append the supplied text deterministically using the existing readable separator convention.
10. Preserve all old body text. Do not call an LLM to merge, summarize, reorganize, or rewrite it.
11. Atomically persist and synchronously refresh the lexical index; semantic refresh may follow through the existing bounded lifecycle.
12. Identical retries return the original success. Reusing a key for different content fails closed.
13. If the note changed after confirmation, reject/re-prepare unless the existing proven append-safe rebase can guarantee preservation and unchanged target meaning.

### 9.3 Conversational references

First use normal conversation/tool context: search/create/get results already contain stable note IDs. “That note” immediately after such a result should not require another global search.

Do not add durable hidden Agent memory. If tests with the weakest supported Agent prove ordinary conversation context insufficient, record that failure as a future problem. Do not add a hidden or session-level memory tier in this launch plan.

## 10. Create-versus-append behavior

The Agent still performs one basic conversational distinction:

- “remember/save/write this down” without an existing target → create a note;
- “add/append/include this in that note/idea/list” → resolve and append;
- “what/where/which was…” → retrieve;
- unclear request → ask one clarification rather than mutate.

Do not build automatic Mem0-style `ADD`/`UPDATE`/`DELETE` fact management. The user's requested operation remains the authority. Tool descriptions and examples must be understandable to a small tool-calling model, mutually distinct, and short.

## 11. Security and reliability invariants

The completed policy/mutation work is valuable only if these remain true:

1. Every Agent, MCP, host API, and UI mutation reaches one confirmation/policy and persistence implementation.
2. Retrieved note text and selected screen text are untrusted data, never privileged instructions.
3. Note paths remain inside the canonical vault after symlink/reparse resolution.
4. Write authority is scoped to vault, note, operation, revision/hash, expiry, and idempotency.
5. Ambiguous, stale, tampered, cross-vault, expired, or malformed writes fail closed.
6. Logs contain IDs, hashes, counts, timings, and reason codes—not raw note text, selected text, tokens, secrets, or credentials.
7. Inputs, outputs, candidates, title/TLDR/body, temporal expressions, and model responses have hard size/count limits.
8. Model or embedding failure cannot lose a capture or corrupt an existing note.
9. External edits are preserved; append cannot overwrite unseen content.
10. Duplicate delivery cannot duplicate an append.
11. Markdown remains readable and recoverable if the SQLite projections are deleted.
12. No new daemon, graph server, background memory agent, or always-loaded model is introduced.

## 12. Evaluation corpus correction

Keep useful existing Phase 0 fixtures, but the launch corpus must no longer be dominated by software/authentication and specialized daily/meeting schemas.

Add ordinary, deliberately overlapping examples:

- ideas and later improvements to those ideas;
- books and recommendations from different people;
- recipes with similar ingredients/names;
- films/videos represented only by ordinary note text and URLs;
- travel suggestions, places, and booking details;
- purchases and products being compared;
- quotations and selected paragraphs;
- creative concepts, story/visual ideas, and research thoughts;
- people-related notes without a Person object;
- household/personal reminders recorded as notes;
- work notes from several non-software professions;
- meetings represented as ordinary notes;
- several near-duplicate notes sharing words, entities, and dates;
- notes appended on different days;
- prompt-injection text, Unicode, large notes, malformed Markdown/frontmatter, and URLs as text.

Required evaluations:

| Capability | Gate |
|---|---|
| Note construction | Material facts/names/numbers/dates/URLs preserved; no invented facts |
| Fallback creation | 100% of valid explicit captures saved and retrievable without a model |
| Retrieval | Recall@5 and MRR meet or beat the pre-change baseline on a held-out broad corpus |
| Temporal retrieval | Common phrase/range accuracy measured with fixed timezone/clock |
| Target resolution | Wrong-note append rate is zero; ambiguous cases abstain |
| Append | Old body preserved; duplicate rate zero; stale revision safely rejected |
| Availability | Create/search/append safety tests pass with LLM and embeddings disabled |
| Weak Agent | End-to-end intent/tool tests pass with the weakest supported tool-calling model |
| Resources | No unjustified regression in latency, index size, peak RSS, or idle RSS/CPU |

Developer-only examples may remain as regression cases; they must not define the architecture or dominate acceptance metrics.

## 13. Continuation plan from partial Phase 4

Do not continue the remaining phases of `MEMORY-SYSTEM-PLAN.md` verbatim. Follow these revised phases.

### Revised Phase 4A — Reconcile completed schema/mutation work

This is a focused correction checkpoint, not a general audit.

Deliverables:

- Preserve Phase 1's unified policy/confirmation path.
- Preserve Phase 3's proven revision/hash, idempotency, token binding, atomic-write, stale-write, and recovery machinery.
- Identify Phase 2/3 structures that implement the rejected ontology: required document kinds, daily/meeting identities, event intervals, canonical semantic blocks, or relationship/fact persistence.
- Remove them where not required by safety. If an internal block marker is necessary for already-proven append/recovery behavior, reduce it to a generic persistence detail and keep it out of Agent contracts, note semantics, and retrieval authority.
- Ensure all existing ordinary notes still load without rewrite.
- Do not discard the current uncommitted Phase 4 work. Rebase its useful deterministic temporal parsing onto the simplified contract.

Gate:

- One durable user memory object remains: `Note`.
- No Agent tool accepts/returns note kind, block kind, relationship, event-validity, daily, meeting occurrence, or resource objects.
- Safety regression tests from Phases 1–3 remain green.
- The correction reduces or neutralizes ontology code without weakening write integrity.

### Revised Phase 4B — Finish minimal temporal retrieval

Deliverables:

- Complete the deterministic common-phrase parser with injected clock/timezone.
- Apply ranges to note created/capture and updated timestamps only.
- Simplify the normal Agent search input to natural query plus host-derived time constraints.
- Remove document-kind/event/source planning introduced solely by the old plan.
- Implement bounded read fallback and strict append-time behavior.

Gate:

- Fixed-time tests cover all supported phrases, timezone midnight, DST where applicable, malformed/uncertain phrases, and created-versus-updated matches.
- No LLM is called to interpret supported time phrases.
- Existing no-time queries retain baseline quality and latency.

### Revised Phase 5 — Lock note-construction quality and fallback

Deliverables:

- Freeze a broad set of raw speech/selected-text → expected note invariants.
- Verify the current distillation prompt against those examples with the weakest supported construction model.
- Add strict Rust validation and deterministic fallback without unnecessary prompt/architecture rewriting.
- Ensure the note is synchronously searchable after save.

Gate:

- Every valid explicit capture yields a note with models disabled or failing.
- Required factual tokens/URLs survive construction.
- Invalid model output cannot select IDs, paths, revisions, or tool actions.
- Broad corpus metrics are recorded, not described anecdotally.

### Revised Phase 6 — Note-level target resolution and append handoff

Deliverables:

- Implement/calibrate the high-precision target resolver over ordinary note results.
- Expose the simple append intent/contract while keeping tokens/revisions internal to the runtime.
- Connect ambiguity/not-found outcomes to existing conversational confirmation/clarification.
- Use deterministic append only; remove any whole-body LLM reconciliation from Agent append paths.
- Reuse a known current-conversation note safely.

Gate:

- Zero wrong-note writes across broad and adversarial fixtures.
- Near-duplicate titles/topics abstain when the margin is insufficient.
- Existing body preservation, retry idempotency, external edit, stale confirmation, vault switch, and crash tests pass.
- Weak-Agent tests require no score comparison or multi-step token reasoning.

### Revised Phase 7 — Retrieval calibration, not graph evolution

Deliverables:

- Tune existing title/FTS/dense/RRF/rerank signals on training fixtures.
- Evaluate once on held-out fixtures.
- Preserve existing bounded excerpts/snippets; do not add a new long-note chunk architecture in this plan.
- Measure any pre-existing graph candidate arm independently; keep it only if benefit justifies cost.
- Use the existing note search/list surfaces for recent-note requests; do not add a separate memory feed.

Gate:

- Held-out note retrieval meets the declared baseline gate.
- Model-disabled retrieval remains correct.
- No canonical chunks, facts, relationships, hidden memory tier, or new engine.
- RAM, latency, and index-size changes are measured and accepted explicitly.

### Revised Phase 8 — Contract and documentation cleanup

Deliverables:

- Remove/deprecate active Phase 2–7 schemas and documentation that expose rejected concepts, while preserving the original plan itself as historical context.
- Align Agent, MCP, host API, generated bindings, tests, and docs around ordinary note create/search/get/append behavior.
- Keep one implementation per mutation operation.
- Do not edit or delete `MEMORY-SYSTEM-PLAN.md`; this duplicated revision carries the correction and the original remains historical evidence.

Gate:

- Repository-wide search finds no active daily/meeting/project-log memory behavior, Agent-visible block ontology, relationship API, or unrestricted LLM body merge.
- Generated contracts match Rust/TypeScript.
- No direct mutation bypass remains.

### Revised Phase 9 — Launch qualification

Deliverables:

- Run the broad corpus, weak-model, no-model, model-failure, security, concurrency, crash, rebuild, and resource suites.
- Test fresh vault, existing vault, externally edited Markdown, corrupt/deleted index, vault switch, and interrupted append.
- Review the full diff from the original branch base using the code graph and direct source/tests.
- Publish exact commands/results, metrics, residual risks, and rollback instructions.

Gate:

- No unresolved critical/high security or data-integrity finding.
- Zero wrong-target writes, duplicate appends, and silent old-body loss in acceptance tests.
- Explicit capture remains usable without a model.
- Weakest supported Agent completes the core create → retrieve → append scenarios without hidden reasoning assumptions.
- Clean-checkout required checks pass.

## 14. Explicitly deferred

Do not implement these as part of this plan:

- daily notes or journals as a storage primitive;
- meeting occurrence/series schemas;
- project/thread/artifact/person/book/video/link memory types;
- link previews, attachment storage, media processing, or screen capture changes;
- event-time extraction, temporal facts, validity/supersession graphs;
- canonical facts, blocks, episodes, resources, or relations;
- user-facing backlinks or graph visualization;
- automatic collection creation or ontology evolution;
- persistent hidden Agent working memory or project summaries;
- proactive resurfacing/Heads Up;
- unrestricted Agent full-body replacement;
- a new model, graph database, vector database, daemon, or background memory engine.

Any future proposal must begin with a concrete failure in the launch corpus and show that a smaller ranking, note-quality, or tool-contract change cannot solve it.

## 15. Executor protocol

`AGENTS.md` and this revised plan are binding. When this document conflicts with `MEMORY-SYSTEM-PLAN.md`, this document wins.

### Before resuming

1. Preserve the current dirty Phase 4 work and all unrelated user changes. Do not reset, checkout, or overwrite them.
2. Record branch, base, status, committed phases, and uncommitted files.
3. Read both plans, then work only from Revised Phase 4A onward.
4. Use the code-review graph first to locate the exact implemented ontology and mutation dependencies.
5. State which current changes will be kept, simplified, or removed and why under §§1–4.

### Per revised phase

1. Implement only the current phase's smallest coherent diff.
2. Do not modify `src-tauri/src/handy/` except an unavoidable marked `[GRAIN]` hook.
3. Run targeted tests during development and all affected Rust/TypeScript/contract checks before commit.
4. Review the complete diff for correctness, data preservation, stale/concurrent behavior, cleanup, security, privacy, and resource lifecycle.
5. Add a regression test for every discovered bug.
6. Run important paths with models/embeddings disabled.
7. Record exact commands and honest results; skipped/unavailable checks are not passes.
8. Commit only phase-related files with the existing Git identity and push the branch.
9. Continue to the next phase without waiting unless a stop condition applies.

### Hard rules for the execution model

- Do not create new memory types to improve an example.
- Do not require the Agent to understand internal storage or ranking architecture.
- Do not let an LLM choose or authorize a write target.
- Do not let an LLM rewrite an existing body for append.
- Do not keep complexity merely because an earlier phase already implemented it.
- Do not remove revision/CAS, idempotency, confirmation, atomicity, path safety, or recovery to simplify code.
- Do not weaken tests, limits, abstention, or error handling to complete a phase.
- Do not change unrelated files or stage the pre-existing `src/app/bindings.ts` whitespace-only edits unless regenerated as an intentional phase output.
- Do not claim compatibility, performance, or security without executed evidence.

### Stop conditions

Stop and report rather than improvise if:

- simplifying the Phase 2/3 ontology would break the proven safety machinery and no narrow separation is possible;
- existing ordinary Markdown would require destructive or non-reversible rewriting;
- a required change needs a new external service, credential, model, database, background process, or product capability;
- an overlapping dirty change cannot be preserved;
- a security/data-integrity invariant cannot be satisfied.

Compile errors, test failures, in-scope bugs, and removal of unnecessary complexity are work to resolve, not stop conditions.

### Phase report

```text
Revised phase:
Outcome:
Kept from original implementation:
Removed/simplified from original implementation:
Files/contracts changed:
Tests/checks and exact results:
No-model result:
Weak-model result (when required):
Security/data-integrity review:
RAM/latency/index measurements:
Residual risks:
Commit/push:
Next revised phase:
```

## 16. Independent executor assessment

The later independent audit will judge repository evidence, not the executor's report.

| Category | Weight |
|---|---:|
| Correctness and preservation of user notes | 25 |
| Tests and broad-corpus evaluation | 20 |
| Security, confirmation, concurrency, and privacy | 20 |
| Simplicity and fit with the one-Note architecture | 15 |
| Weak/no-model behavior | 10 |
| Resource lifecycle and change discipline | 10 |

Automatic rejection:

- wrong-note mutation or silent text loss;
- remaining mutation/confirmation bypass;
- an LLM-authorized target or LLM whole-body append merge;
- hidden/skipped/falsified checks or weakened tests;
- secrets/private note contents logged or committed;
- model/network availability required to preserve an explicit capture;
- new canonical graph/ontology/media system;
- Handy boundary violation or unrelated destructive changes.

## 17. Primary references

- [Mem Push-to-Remember](https://get.mem.ai/features/push-to-remember)
- [Mem Chat create-or-update behavior](https://get.mem.ai/features/chat)
- [Mem Search tiers](https://help.mem.ai/features/search)
- [Mem retrieval engineering: keyword + semantic + deterministic](https://get.mem.ai/blog/building-mem-the-ai-notes-app-(feat-pinecone))
- [Mem It raw input, context, instructions, and timestamp](https://docs.mem.ai/guides/use-cases/mem-it)
- [Mem note creation contract](https://docs.mem.ai/api-reference/notes/create-note)
- [Mem exact-version note update contract](https://docs.mem.ai/api-reference/notes/update-note)
- [Mem note search and created/updated filters](https://docs.mem.ai/api-reference/notes/search-notes)
- [Mem version history](https://help.mem.ai/features/version-history)
- [Mem 2.0 simplification decisions](https://get.mem.ai/blog/mem-2-dot-0-transition-guide)
- [Mem Fact Search: matching snippets inside notes](https://newsletter.mem.ai/p/introducing-fact-search)
- [LongMemEval](https://arxiv.org/abs/2410.10813)
- [SQLite FTS5](https://www.sqlite.org/fts5.html)
- [Reciprocal Rank Fusion](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/)
- [Duckling temporal parser reference](https://github.com/facebook/duckling)
- [Mem0 architecture, evaluated as a non-fit](https://github.com/mem0ai/mem0/blob/main/docs/core-concepts/how-it-works.mdx)

## 18. Final execution directive

Resume on `codex/grain-space-memory-experiment` from the current partial Phase 4 state. Do not restart Phases 0–3 and do not continue the original plan's ontology. First complete Revised Phase 4A, preserving all proven write-safety work while returning Grain Space to one ordinary Markdown `Note`. Then execute Revised Phases 4B–9 in order.

The launch objective is intentionally modest: a user speaks or supplies selected text, Grain creates a faithful searchable note, the user can retrieve it naturally, and an explicit addition reaches the correct existing note without overwriting anything. Make that path dependable with the weakest supported Agent before adding any other memory concept.

Do not finish by asserting that the architecture is intelligent. Finish by proving—with broad fixtures, no-model tests, weak-model tests, wrong-target abstention, stale/concurrent write tests, and resource measurements—that the simple note system works.
