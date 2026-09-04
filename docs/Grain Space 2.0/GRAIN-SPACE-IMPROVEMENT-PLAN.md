# Grain Space Improvement Plan

**Status:** Active

**Branch:** `codex/grain-space-improvement`

**Starting point:** `main` at `ba7e1317`

**Date:** 2026-09-04

## 1. Outcome

Grain Space should feel like a dependable, voice-first memory inside the Grain Agent:

```text
speak or type naturally
        |
        v
the Agent understands the request
        |
        +--> search/read notes when needed
        +--> create a useful note when asked
        +--> append to the correct note when asked
        +--> combine notes with extension tools when needed
```

The launch-quality journey is:

```text
capture faithfully -> create a searchable Markdown note
                  -> retrieve it from a vague request
                  -> append without choosing the wrong note or losing old text
```

Mem is a useful product reference, not a compatibility target or an architecture to copy. Grain keeps its local-first, low-overhead design.

## 2. Clean-start rule

This plan begins from `main`. No implementation, schema, codec, mutation engine, fixture corpus, or partial temporal work from `codex/grain-space-memory-experiment` is carried into this branch.

The experiment remains preserved only as historical evidence. If an idea is useful, it must be justified and implemented independently against this plan and the current Grain architecture.

There is no migration requirement because the abandoned experimental schema was never released.

## 3. Product and architecture boundaries

### 3.1 One durable memory object

The durable user object is the existing `Note`:

- ordinary Markdown body;
- descriptive title and faithful summary where already supported;
- existing timestamp and organization metadata;
- existing local/Obsidian storage behavior.

Do not introduce canonical blocks, facts, relationships, episodes, resources, daily-note identities, meeting types, project-log types, event-validity intervals, or a memory graph.

FTS rows, embeddings, long-note chunks, snippets, and ranking signals are derived and rebuildable. They are not memories.

### 3.2 No new engine

Use the existing Grain Space backend, index, Agent loop, provider rotation, confirmation surface, and model lifecycle. Do not add a daemon, database server, graph service, background agent, or always-loaded model.

### 3.3 Weak-Agent operability

Tools must be few, distinct, and described in plain language. Rust owns validation, limits, path safety, confirmation, persistence, and concurrency. The Agent should never need to understand ranking weights, storage paths, hashes, tokens, or Markdown file mechanics.

## 4. Agent-first retrieval

### 4.1 Required conversational flow

Every spoken or typed Agent request follows this flow:

```text
ASR/text -> Agent with note tool descriptions -> Agent chooses whether to call search_notes
                                            -> host executes the requested search
                                            -> Agent reads/follows up/answers
```

The following flow is forbidden:

```text
ASR/text -> automatic search using the raw transcript -> notes injected into prompt -> Agent
```

Reasons:

- the Agent can distinguish a note question from rewriting, extension work, or ordinary conversation;
- the Agent can form a focused search query instead of treating conversational filler as retrieval terms;
- multi-extension tasks stay inside one coherent tool loop;
- every note access is observable as a tool call;
- Grain avoids retrieval and embedding work on turns that do not need notes.

The latency cost of a real search tool hop is accepted for consistency and accuracy. Tool calls and retries remain bounded.

### 4.2 Scope of the invariant

- The normal Agent is the conversational entry point for notes and extensions.
- A legacy Recall-labelled shortcut may remain temporarily as a UI affordance, but it must enter the same unified Agent tool loop and must not prefetch notes.
- The dedicated pre-retrieval Recall brain is retired from spoken Agent turns.
- The Notes workspace's explicit search box may call search directly; it is a search UI, not an Agent turn.
- Explicit quick-add/capture actions may save directly because the user selected a dedicated capture action. They must not perform speculative retrieval.

### 4.3 Retrieval attempts

The Agent may:

1. call `search_notes` with a focused natural-language query;
2. inspect bounded summaries and stable note IDs;
3. call `get_note` for the most relevant full note;
4. make one or two bounded reformulated searches when evidence is insufficient;
5. say that it could not find the note after those attempts.

The host bounds total tool calls, result count, returned text, and query size.

## 5. Minimal Agent tool contract

The conceptual note tools are:

```text
search_notes(query)
get_note(id)
save_note(body, optional_title, optional_collection)
append_to_note(id, text)
list_collections()
```

Rules:

- `search_notes` returns compact title/summary/excerpt/time metadata and stable IDs.
- `get_note` returns one bounded full note.
- Search/read operations are safe reads.
- Save/append/delete operations use the host's normal confirmation policy.
- A known note ID identifies a candidate; it does not bypass confirmation or stale-content validation.
- All Agent, extension, MCP, and UI mutations converge on one host implementation.
- Tool results identify saved notes as historical user context, not live truth from external providers.

The contract stays small until evaluation proves another operation is necessary.

## 6. Note construction

For an explicit capture, the system should produce a useful note without losing the user's material content.

Quality requirements:

- concrete, discriminating title;
- short faithful summary;
- readable Markdown body;
- preserve names, numbers, dates, quotations, uncertainty, and URLs;
- selected text remains intact unless the user explicitly requests transformation;
- no invented facts;
- bounded input and output.

Model responsibility is limited to presentation quality. If the model is absent, times out, or returns invalid content, deterministic Rust fallback saves the supplied text with a bounded derived title. An explicit capture must not disappear because note polishing failed.

## 7. Retrieval

Retain and improve the existing note-level pipeline rather than replacing it:

1. exact ID/title match;
2. title prefix/token match;
3. FTS over title, summary, and body;
4. optional local semantic similarity;
5. existing deterministic fusion/reranking;
6. bounded note-level results with matching excerpts.

Requirements:

- lexical retrieval works with embeddings disabled;
- new and updated notes are synchronously searchable lexically;
- semantic failure cannot fail the request;
- unrelated nearest-neighbour results are rejected rather than returned merely to fill a quota;
- ranking is calibrated on held-out, general-purpose notes rather than developer-only examples;
- no graph or ontology is added to improve ranking.

## 8. Time-aware retrieval

Time is an optional search constraint over existing note creation/capture and modification timestamps.

Initially support deterministic interpretations for:

- today, yesterday, day before yesterday;
- this week, last week;
- this month, last month;
- last `N` days;
- explicit dates and simple date ranges.

The parser receives an explicit clock and timezone. Supported phrases do not require an LLM. An uncertain phrase remains ordinary query text rather than becoming an invented filter.

The Agent may include temporal language in its search query; the host extracts recognized ranges. Read-only retrieval may perform one clearly bounded soft fallback. A write-target search must never silently weaken an explicit time constraint.

## 9. Safe append

Appending is the only launch-critical edit behavior.

Required flow:

1. The Agent searches for the target unless the current conversation already contains a stable note ID.
2. The Agent may read the likely target before requesting append.
3. The host presents the exact note title and exact addition for confirmation.
4. The host re-reads the note and rejects a stale target rather than overwriting unseen edits.
5. The supplied addition is appended deterministically with the existing readable separator.
6. All old body text is preserved byte-for-byte.
7. Persistence remains atomic.
8. A duplicate delivery cannot produce a duplicate append.
9. The lexical index is refreshed synchronously; semantic refresh follows the existing bounded lifecycle.

No LLM rewrites the complete existing note for an append. Ambiguous or missing targets produce clarification/not-found, never a guessed write.

Use the smallest concurrency mechanism compatible with external Markdown edits. Prefer an existing exact version/hash signal and opaque host-managed pending state; do not build a block protocol or custom cryptographic scheme.

## 10. Evaluation corpus

Build a held-out corpus reflecting ordinary use:

- ideas and later improvements;
- books, articles, videos, and URLs represented as text;
- recipes and similarly named items;
- purchases and comparisons;
- travel suggestions;
- quotations and selected passages;
- personal facts and reminders;
- creative concepts;
- work notes across several professions;
- meetings represented as ordinary notes;
- near-duplicate titles and topics;
- notes created or appended on different days;
- Unicode, long notes, malformed Markdown/frontmatter, and prompt-injection text.

Measure retrieval, construction, append targeting, availability, latency, and memory. Developer examples may exist as regression cases but must not dominate the gates.

## 11. Security, privacy, and resource invariants

1. Retrieved notes and selected text are untrusted data, never system instructions.
2. Every mutation uses the same host policy and confirmation implementation.
3. Paths remain inside the configured vault after canonicalization and link/reparse resolution.
4. Stale, malformed, expired, duplicate, or cross-vault writes fail closed.
5. Logs contain IDs, counts, durations, scores, and reason codes—not raw note bodies, selection text, credentials, or tokens.
6. Inputs, candidates, tool hops, full-note reads, and model output have hard limits.
7. Disabling Grain Space tears down its model/index resources according to the existing lifecycle.
8. No idle polling, resident background Agent, or automatic retrieval occurs.

## 12. Execution phases

### Phase 0 — Agent-first convergence and baseline

Deliverables:

- Route spoken Recall-labelled turns through the normal Agent tool loop.
- Remove automatic raw-transcript retrieval from that path.
- Preserve direct Notes UI search and explicit capture paths.
- Confirm note and extension tools can coexist in the same bounded turn.
- Add regression coverage for the routing invariant where feasible.
- Record the current retrieval/no-model/resource baseline without tuning it.

Gate:

- No spoken Agent path invokes note search before an Agent `search_notes` call.
- An unrelated Agent request performs no Grain Space retrieval or embedding work.
- A note question can call `search_notes`, optionally `get_note`, and answer with touched-note provenance.
- Existing Agent and Grain Space targeted tests pass.

### Phase 1 — One trusted note-tool execution path

Deliverables:

- Remove duplicate Agent-only mutation dispatch.
- Route note tools through the shared action/policy executor.
- Ensure save, append, and delete are host-confirmed; reads remain safe.
- Align Agent, built-in capability, MCP, and host contracts without adding tools.

Gate:

- No mutation bypass exists.
- Confirmation resumes the exact prepared operation without another model decision.
- Tool results and errors remain bounded and safe for the Agent to consume.

### Phase 2 — Broad corpus and note-construction reliability

Deliverables:

- Add the general-purpose fixture/evaluation corpus.
- Freeze factual-preservation invariants for title/summary/body construction.
- Verify current note construction before changing prompts.
- Add strict validation and deterministic fallback for demonstrated failures.

Gate:

- Every valid explicit capture is saved with the model disabled or failing.
- Material names, numbers, dates, uncertainty, and URLs are retained.
- Saved notes are immediately retrievable through lexical search.

### Phase 3 — Retrieval and temporal calibration

Deliverables:

- Measure exact/title/FTS/semantic/fusion contributions.
- Tune only signals that improve the training corpus.
- Add deterministic host-side temporal parsing and filters.
- Evaluate once against held-out fixtures.

Gate:

- Held-out Recall@5 and MRR meet declared baseline improvements.
- Model-disabled retrieval remains useful.
- Supported temporal phrases pass fixed-clock/timezone tests.
- Wrong/unrelated semantic neighbours do not fill result lists.

### Phase 4 — Safe deterministic append

Deliverables:

- Replace whole-body model reconciliation for explicit append requests.
- Bind confirmation to the selected note and its current content version.
- Add stale-write, duplicate, external-edit, ambiguity, and wrong-target tests.
- Keep authorization/version state private to the host.

Gate:

- Wrong-note append rate is zero in acceptance fixtures.
- Ambiguous targets abstain.
- Existing text is never silently removed or rewritten.
- Duplicate and stale operations fail safely.

### Phase 5 — End-to-end qualification

Deliverables:

- Exercise speak/type -> Agent -> search/create/append -> confirmation -> result.
- Cover mixed note-plus-extension tasks through the single Agent loop.
- Run model-disabled, embedding-disabled, model-failure, corrupt-index, external-edit, and vault-switch scenarios.
- Measure tool hops, latency, peak/idle RAM, CPU, and index size.
- Clean obsolete Recall-brain contracts only after their consumers have moved.

Gate:

- Core journeys work with the weakest supported tool-calling model.
- No unresolved critical/high security or data-integrity issue.
- No unjustified idle or memory regression.
- Clean-checkout required checks pass with exact results recorded.

## 13. Execution discipline

For every phase:

1. Start from a recorded baseline.
2. Use the code graph before source search.
3. Make the smallest coherent change.
4. Add a regression test for each discovered bug.
5. Run targeted tests, then affected Rust/TypeScript checks.
6. Verify model/embedding-disabled behavior where relevant.
7. Review security, privacy, cleanup, and resource lifetime.
8. Record exact commands and results in `GRAIN-SPACE-IMPROVEMENT-LOG.md`.
9. Commit and push the phase independently.

Stop rather than improvise if a change requires a new service, database, model, daemon, destructive Markdown rewrite, or unrelated architecture expansion.

## 14. Explicitly deferred

- Canonical memory blocks, facts, relationships, or graphs.
- Daily notes, meeting identities, projects, people, links, or media as memory types.
- Persistent hidden Agent memory.
- Proactive resurfacing.
- Automatic organization/collection invention.
- Arbitrary whole-note model rewrites.
- New embedding/vector/graph infrastructure.
- Dynamic UI work.
- Parallel multi-agent orchestration.

## 15. Success definition

The work is complete when a user can speak naturally to the normal Grain Agent, the Agent decides whether notes are relevant, and Grain can faithfully create, accurately find, and safely append to ordinary Markdown notes without speculative retrieval, wrong-note writes, silent text loss, or unnecessary resident infrastructure.

## 16. References

- [Mem Push-to-Remember](https://get.mem.ai/features/push-to-remember)
- [Mem search tiers](https://help.mem.ai/features/search)
- [Mem note creation contract](https://docs.mem.ai/api-reference/notes/create-note)
- [Mem MCP note tools](https://docs.mem.ai/mcp/supported-tools)
- [SQLite FTS5](https://www.sqlite.org/fts5.html)
- [Reciprocal Rank Fusion](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/)
