# ADR 0001: Temporal Document Memory with Safe Append Transactions

**Status:** Accepted  
**Date:** 2026-09-04  
**Context & Plan Reference:** Links and implements the architecture declared in [`docs/Grain Space 2.0/MEMORY-SYSTEM-PLAN.md`](./MEMORY-SYSTEM-PLAN.md).  
**Supersedes:** The mutation and temporal-data portions of `KNOWLEDGE-ARCHITECTURE-PLAN.md` (hybrid-retrieval foundation remains valid).

---

## 1. Context and Problem Statement

Grain Space enables users to capture, retrieve, and append to personal notes, meetings, and thoughts via voice, shortcuts, MCP, and an Agent assistant. 

Under the previous design:
1. **Agent write-policy bypass:** The Agent tool dispatcher executed `save_note` and `append_to_note` directly in `agent_tools.rs`, bypassing the risk classification and user confirmation flow in `action_exec.rs`.
2. **Unbound mutation targets:** Appends accepted a bare note ID string without binding the target to a specific document revision, vault scope, allowed operation, or expiration window.
3. **Whole-document rewriting:** Appends and captures read the note, reconstructed the entire body in memory, and rewrote the full document file. This created severe risks of content clobbering, race conditions with external tools (such as Obsidian), and silent data loss.
4. **No idempotency:** Retried agent calls or network/transport replays appended the same block repeatedly.
5. **Coarse temporal representation:** Documents possessed only a single lifecycle timestamp (`timestamp`), making it impossible to distinguish between "when this note was captured" and "when the event occurred" (e.g., a note saved today about a meeting that occurred last Tuesday).
6. **Agent filter loss:** Normal Agent search lacked the temporal, entity, document-kind, and source filters present in Recall.

We require a local-first, low-RAM, cross-platform architecture that guarantees user document integrity, safe append transactions, bi-temporal indexing, and strict confirmation gating.

---

## 2. Decision: The Three-Layer Architecture

We declare a strict three-layer architecture for Grain Space:

```
┌────────────────────────────────────────────────────────────────────────┐
│  Layer 1: Durable User Content (System of Record)                      │
│  - User-owned Markdown files with YAML frontmatter                     │
│  - Stable block markers (HTML comments) for Grain-structured documents  │
│  - Human-readable and editable outside Grain (e.g. in Obsidian / IDE)  │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
┌────────────────────────────────────▼───────────────────────────────────┐
│  Layer 2: Durable Mutation Metadata (SQLite Ledger)                   │
│  - Monotonic document revisions and content hashes                     │
│  - Idempotency records and operation states (prepared, committed, etc.)│
│  - Never duplicates raw sensitive user text                            │
│  - Enables safe append transactions, rebase, and TOCTOU protection    │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │
┌────────────────────────────────────▼───────────────────────────────────┐
│  Layer 3: Rebuildable Projections (Derived SQLite Index)               │
│  - FTS5 full-text index (documents and stable blocks)                  │
│  - Optional BGE-small embeddings (sqlite-vec)                          │
│  - Entity adjacency graph, aliases, and temporal intervals             │
│  - Disposable and 100% reconstructible from Layer 1                    │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Layer 1: Durable User Content
- Markdown files with YAML frontmatter remain the durable system of record.
- Structured Grain documents (meetings, daily notes, captured logs) use stable, human-readable block markers (`<!-- grain:block id="..." kind="..." -->`).
- Legacy notes without block markers remain readable and searchable as whole-document blocks without eager rewriting.
- Foreign Markdown notes in an Obsidian vault remain read-only for agent mutations until explicitly adopted.

### 2.2 Layer 2: Durable Mutation Metadata
- A dedicated SQLite mutation ledger manages document revisions, content hashes, and idempotency keys.
- Every mutation must compare its `base_revision` against the current document revision.
- Pure appends to non-conflicting sections can be rebased safely; conflicting edits or stale targets fail closed with `stale`.
- Deletions sever content from both Markdown and ledger records. No raw note bodies are retained in ledger logs.

### 2.3 Layer 3: Rebuildable Projections
- SQLite FTS5, entity relations, aliases, and optional vector embeddings exist purely as derived projections.
- If the index is corrupted or deleted, startup rebuilds all projections deterministically from Markdown files.
- If the mutation ledger is lost, content survives and the latest revision is reconstructed from frontmatter and content hashes.

---

## 3. Tool Policy, Safety, and Authority

1. **Unified Action Policy:** All Agent, MCP, and host mutations must route through `action_exec.rs`. The direct bypass in `agent_tools.rs` is eliminated.
2. **Opaque Revision-Bound Tokens:** Write targets must first be resolved via `resolve_memory_target`. A successful resolution yields an opaque, short-lived `target_token` binding `(vault_id, doc_id, base_revision, content_hash, operation, expiry)`. Writes presenting an invalid, stale, cross-vault, or expired token fail closed.
3. **Idempotency:** All mutations require an `idempotency_key`. Repeating a write with the same key and identical content returns success without duplicating blocks. A repeated key with conflicting content fails closed.
4. **Historical Memory vs. Live Provider Authority:** Grain Space notes are historical records. Live, mutable external state (e.g. GitHub issues, Linear tickets) must be fetched from the corresponding provider.

---

## 4. Invariants and Non-Goals

1. **No Silent Loss:** A user's original capture wording must never be overwritten, summarized away, or clobbered by an agent.
2. **No Autonomous Daemons:** No background sync daemon, resident worker thread, or continuous consolidation loop is permitted. All indexes are updated on mutation or lazily during retrieval.
3. **Model Optionality:** Local semantic embeddings (BGE-small) and LLM distillation are strictly optional. Creation, FTS search, temporal queries, and safe appends must work with 100% correctness on a machine with no models or network.
4. **Path Containment:** Every vault path is strictly canonicalized and contained within the configured vault directory. All directory traversal and symlink escapes are rejected.

---

## 5. Consequences

### Positive
- Eliminates silent clobbering and race conditions during note appends.
- Prevents duplicate writes from retried agent tool calls.
- Enforces user confirmation and host time-of-use revalidation on all agent writes.
- Enables precise temporal queries ("notes from last Tuesday", "meetings last week").
- Keeps memory footprint at zero when idle.

### Negative / Trade-offs
- Writing to a note requires resolving a target token before proposing an append.
- The schema is expanded to track bi-temporal metadata (`created_at`, `updated_at`, `event_start`, `event_end`, `timezone`).
- Migration requires maintaining backward compatibility with legacy un-blocked Markdown files.
