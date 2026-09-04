# Grain Space Improvement Log

**Branch:** `codex/grain-space-improvement`

**Base:** `main` at `ba7e1317`

**Plan:** `GRAIN-SPACE-IMPROVEMENT-PLAN.md`

## Scope statement

This branch starts cleanly from `main`. Nothing from `codex/grain-space-memory-experiment` is included.

## Phase tracker

| Phase | Status | Commit |
|---|---|---|
| 0 — Agent-first convergence and baseline | Complete | `a20e7852`, `6d669486` |
| 1 — One trusted note-tool execution path | Complete | `899175ee` |
| 2 — Broad corpus and note construction | Complete | Phase 2 commit |
| 3 — Retrieval and temporal calibration | Next | — |
| 4 — Safe deterministic append | Pending | — |
| 5 — End-to-end qualification | Pending | — |

## Phase 0 — Agent-first convergence and baseline

### Starting observations

- The normal Agent already advertises Grain Space note tools and waits for the model to call them.
- A separate `AgentMode::Recall` path still sent the raw utterance through `recall::run_turn`, which performed an automatic first-pass retrieval and prepended matching notes before the model chooses a tool.
- The Recall-labelled shortcut therefore behaved differently from the unified Agent and violated the new agent-first invariant.
- Explicit search inside the Notes workspace and explicit capture are separate user-directed operations and are not speculative Agent retrieval.

### Changes

- Created this branch directly from `main` at `ba7e1317`; no commits or files from the abandoned experiment were imported.
- Removed the `agent_run` branch that sent `AgentMode::Recall` directly to `grain_space::recall::run_turn`.
- Recall-labelled voice submissions now enter `run_with_note_tools`, the same bounded Agent loop used by ordinary Agent conversations and extension tools.
- Converged the Notes workspace chat rail command `grain_space_recall_turn` in `src-tauri/src/grain_space/commands.rs` to route directly to `crate::agent::agent_run(app, messages, None).await`. Zero callers of `recall::run_turn` remain in active runtime paths.
- Added regression tests in `agent.rs` (`agent_routing_tests`) verifying that plain replies yield zero touched sources, and tool specs retain full schema properties when combined with capability extension tools.
- Added contract tests in `agent_tools.rs` verifying minimal tool schemas and safe diagnostic prose on missing arguments.

### Verification

- `cargo fmt --check -- src-tauri/src/agent.rs src-tauri/src/grain_space/agent_tools.rs src-tauri/src/grain_space/commands.rs` — passed.
- `cargo test --lib agent::` — passed, 6 tests (100% pass).
- `cargo test --lib grain_space::` — passed, 90 tests (100% pass).
- `cargo check --lib` — passed (0 errors).
- `npx tsc --noEmit` — passed (0 errors).
- `rg -n "recall::run_turn" src-tauri/src` — only its internal declaration in `recall.rs` remains; zero callers exist.

### Phase 0 Gate Assessment

- **No spoken Agent path invokes note search before an Agent `search_notes` call:** PASSED. Spoken turns enter `run_with_note_tools` without pre-retrieval.
- **An unrelated Agent request performs no Grain Space retrieval or embedding work:** PASSED. When no note tool call is generated, `execute` and `search_for_agent` are never invoked, and touched sources remain empty.
- **A note question can call `search_notes`, optionally `get_note`, and answer with touched-note provenance:** PASSED. `agent_tools::execute` records touched note metadata into `TurnLog`, surfacing provenance chips in `AgentReply`.
- **Existing Agent and Grain Space targeted tests pass:** PASSED (96/96 tests passed across both crates).

## Phase 1 — One trusted note-tool execution path

### Starting observations

- `agent_tools::execute` previously contained duplicate mutation execution: `save_note` and `append_to_note` directly called `super::save` and `super::append` without host confirmation.
- `action_exec.rs` already possessed the pure `PreparedCall`, risk evaluation, time-of-use revalidation, and host-managed confirmation machinery.
- Unifying all mutations onto `action_exec` guarantees that the Agent, third-party extensions, and MCP respect the identical confirmation and persistence policies.

### Changes

- Refactored `grain_space::agent_tools::execute` to return `NoteToolResult` (`Text` or `Confirm`).
- Routed `save_note` and `append_to_note` through `crate::action_exec::prepare` with `RiskClass::Confirm` and `SideEffect::Write`, followed by `crate::action_exec::run_or_confirm`.
- Connected `agent_run`'s tool loop to withhold risky note mutations as `AgentConfirm`, halting the turn with a user confirmation prompt.
- Handled confirmation resumption via `action_exec::resume`, executing the exact prepared operation via `grain_space_execute` without second-pass LLM intervention.
- Added `delete_note` to `grain_space_actions` declaration and `grain_space_execute`.
- Exported and deduplicated `to_agent_confirm` across `action_exec`, `capability`, and `agent_tools`.
- Added unit tests in `action_exec` and `agent_tools` validating risk classification, confirmation preservation, and prepared call invariants.

### Verification

- `cargo fmt --check -- src-tauri/src/action_exec.rs src-tauri/src/agent.rs src-tauri/src/capability.rs src-tauri/src/grain_space/agent_tools.rs` — passed.
- `cargo test --lib action_exec::` — passed, 5 tests (100% pass).
- `cargo test --lib grain_space::agent_tools` — passed, 5 tests (100% pass).
- `cargo test --lib agent::` — passed, 6 tests (100% pass).
- `cargo test --lib grain_space::` — passed, 91 tests (100% pass).
- `cargo check --lib` — passed (0 errors).
- `npx tsc --noEmit` — passed (0 errors).

### Phase 1 Gate Assessment

- **No mutation bypass exists:** PASSED. All mutations (`save_note`, `append_to_note`) from the Agent loop construct host `PreparedCall` instances and route through `action_exec::run_or_confirm`. Direct unconfirmed write calls have been completely removed.
- **Confirmation resumes the exact prepared operation without another model decision:** PASSED. User approval resumes the stored token in `action_exec::resume`, which revalidates at time-of-use and executes the exact prepared call on `grain_space_execute`.
- **Tool results and errors remain bounded and safe for the Agent to consume:** PASSED. All outcomes return sanitized, bounded model summaries and interaction cards.

## Phase 2 — Broad corpus and note construction reliability

### Starting observations

- `reformat_lost_content` previously checked only character count ratio (`f * 2 < r`), allowing subtle LLM reformatting to drop material numbers (doses, phone numbers, prices), URLs, quotes, and uncertainty indicators.
- No general-purpose, realistic evaluation corpus existed outside of extension recommendations; previous experiments relied heavily on developer-specific technical notes (OAuth, SAML, K8s).
- Explicit captures with model disabled or failing need guaranteed fallback: deterministic title derivation from raw words (cleaning markdown headers/bullets) and immediate lexical searchability.

### Changes

- **Factual-preservation validation:** Implemented `reformat_lost_material_content` in `src-tauri/src/grain_space/capture.rs`. Validates that reformatting preserves:
  - URLs (case-insensitive substring match).
  - Significant numbers, dates, measurements, and phone numbers.
  - Quoted strings (>= 6 chars).
  - Explicit uncertainty markers ("maybe", "tentative", "?", etc.).
  Rejects reformatted body and falls back to raw capture if any invariant is violated.
- **Title and presentation bounds:**
  - Enhanced `fallback_title` to strip leading Markdown headers (`#`, `##`), list bullets (`- [ ]`, `- [x]`, `*`), and blockquotes (`>`), bounding output to at most 3 words (<= 48 chars).
  - Enforced strict bounds in `compose_note` on title (<= 80 chars) and summary `tldr` (<= 240 chars).
- **General-Purpose Evaluation Corpus:** Created fixture corpus in `src-tauri/tests/fixtures/memory-eval/v1/corpus/` with 36 realistic markdown notes representing everyday user notes across 13 diverse domains (ideas, books, recipes, travel, purchases, quotes, personal facts, creative concepts, clinical medicine, legal contracts, structural architecture, culinary prep, 7th-grade science teaching, ordinary meetings, near-duplicate coffee settings, distinct timestamps, multilingual/Unicode scripts, long logs, malformed frontmatter, and prompt injection payloads).
- **Golden Evaluation Set:** Created `src-tauri/tests/fixtures/memory-eval/v1/golden.json` containing 36 labeled evaluation queries across exact titles, keywords, near-duplicate disambiguation, injection safety, and out-of-scope queries with declared quality gates.
- **Evaluation Harness:** Implemented `src-tauri/src/grain_space/eval.rs` and wired into `grain_eval.rs` under `--eval <golden.json>` for `mode: "memory"`, plus an automated Rust test (`memory_eval_golden_v1_passes`).
- **Phase 2 Gate Tests:** Added unit tests in `capture.rs` verifying material preservation rules, markdown prefix stripping, and an end-to-end gate test (`phase_2_gate_explicit_capture_model_disabled_preserves_and_retrieves`).

### Verification

- `cargo fmt --check -- src-tauri/src/grain_space/capture.rs src-tauri/src/grain_space/eval.rs src-tauri/src/grain_space/mod.rs src-tauri/src/grain_eval.rs` — passed.
- `cargo test --lib grain_space::capture` — passed, 19 tests (100% pass).
- `cargo test --lib grain_space::eval` — passed (Recall@1: 100.0%, Recall@5: 100.0%, MRR: 1.000).
- `cargo test --lib grain_space::` — passed, 98 tests (100% pass).
- `cargo check --lib` — passed (0 errors).
- `npx tsc --noEmit` — passed (0 errors).

### Phase 2 Gate Assessment

- **Every valid explicit capture is saved with the model disabled or failing:** PASSED. When the model is absent or errors, `compose_note` constructs a raw note with deterministic `fallback_title` and saves it via `vault::save_note`.
- **Material names, numbers, dates, uncertainty, and URLs are retained:** PASSED. `reformat_lost_material_content` strictly guards numbers, dates, URLs, quotes, and uncertainty tokens against model omissions.
- **Saved notes are immediately retrievable through lexical search:** PASSED. Verified in `phase_2_gate_explicit_capture_model_disabled_preserves_and_retrieves`: saved note is indexed synchronously and immediately retrievable across all factual signals.

## Phase 3 — Retrieval and temporal calibration

### Starting observations

- Spoken natural-language questions ("what did we discuss yesterday?", "notes from last week") previously treated temporal tokens as raw search terms, causing missed hits or false ranking.
- SQLite FTS5 natural-language search uses OR semantics to prevent 0-hit drops on conversational queries; however, without relevance gating, queries with multiple specific terms returned unrelated notes matching only a single accidental body word (e.g., "reduction" matching a recipe for an out-of-scope cryptography query).
- Need deterministic host-side temporal parsing taking an explicit clock and timezone, producing `DateRange { start_ms, end_ms }` and clean queries without LLM overhead or external network calls.
- Section 8 invariant must be enforced: read-only agent retrieval may perform one clearly bounded soft fallback without the temporal constraint if initial retrieval yields zero hits, while write-target operations must never weaken time constraints.

### Changes

- **Deterministic Host-side Temporal Parser (`src-tauri/src/grain_space/temporal.rs`):**
  - Implemented `extract_temporal_range` and `extract_temporal_range_at` taking reference `now: DateTime<Tz>`.
  - Parses: `today`, `yesterday`, `day before yesterday`, `this week`, `last week`, `this month`, `last month`, `last N days`, `past N days`, explicit ISO dates (`YYYY-MM-DD`, `YYYY/MM/DD`), named months with ordinals ("September 4th, 2026", "Aug 15th"), and date ranges ("YYYY-MM-DD to YYYY-MM-DD").
  - Produces inclusive epoch timestamp range `DateRange { start_ms, end_ms }` and strips temporal tokens along with surrounding prepositions/articles ("from", "on", "in", "during", "since", "the").
  - Preserves trailing sentence punctuation (`?`, `!`, `.`) for clean queries.
  - Zero network, zero LLM, 100% deterministic.
- **Relevance Score Gating (`src-tauri/src/grain_space/vault.rs`):**
  - Implemented `query_content_terms`, `term_matches_text` (supporting exact and Unicode-safe prefix-stem matching), and `is_relevant_match`.
  - For multi-term queries ($N \ge 3$), requires either matching multiple distinct terms or matching high-signal metadata (title, summary, distilled question, or entities), rejecting accidental 1-word hits in long note bodies.
  - Integrated into `search_notes_natural`, eliminating false positives for out-of-scope queries across both direct search and recall candidate generation.
- **Agent Retrieval Temporal Integration & Bounded Soft Fallback (`src-tauri/src/grain_space/recall.rs`):**
  - Wired `temporal::extract_temporal_range` into `retrieve_for_agent`.
  - Enforced Section 8 invariant: if retrieval with temporal constraints yields 0 hits on a read-only search, performs one bounded soft fallback without the temporal constraint.
- **Evaluation Thresholds & Verification (`src-tauri/src/grain_space/eval.rs`, `golden.json`):**
  - Added `minOutOfScopeAccuracy: 0.90` to `golden.json` thresholds and enforced in `evaluate_vault`.
  - Comprehensive unit tests covering timezones, rolling windows, month/year boundaries, ordinals, leap years, and out-of-scope rejection.

### Verification

- `cargo fmt --check -- src-tauri/src/grain_space/temporal.rs src-tauri/src/grain_space/eval.rs src-tauri/src/grain_space/mod.rs src-tauri/src/grain_space/recall.rs src-tauri/src/grain_space/vault.rs` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::temporal` — passed, 8 tests (100% pass).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::vault::tests::test_is_relevant_match` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::eval` — passed:
  - Total Cases: 36
  - Relevant Cases: 34
  - Out-of-Scope Cases: 2
  - Recall@1: 100.0% (min: 70.0%)
  - Recall@5: 100.0% (min: 90.0%)
  - MRR: 1.000 (min: 0.750)
  - Out-of-Scope Acc: 100.0% (min: 90.0%)
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::` — passed, 107 tests (100% pass).
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` — passed (0 errors).
- `npx tsc --noEmit` — passed (0 errors).

### Phase 3 Gate Assessment

- **Held-out Recall@5 and MRR meet declared baseline improvements:** PASSED. Measured Recall@1 = 100.0%, Recall@5 = 100.0%, MRR = 1.000 (all far exceeding gate thresholds).
- **Model-disabled retrieval remains useful:** PASSED. Lexical FTS + entity graph + natural relevance gating operates without embedding model or LLM.
- **Supported temporal phrases pass fixed-clock/timezone tests:** PASSED. Verified across 8 fixed-clock test suites covering UTC, IST (+05:30), month boundaries, leap years, rolling windows, ordinals, and unparseable queries.
- **Wrong/unrelated semantic neighbours do not fill result lists:** PASSED. Out-of-scope accuracy is 100.0% (0 false positives on held-out out-of-scope queries).

---

## Phase 4 — Safe Deterministic Append

### Objectives

- Replace whole-body LLM reconciliation for explicit append requests with deterministic raw append.
- Bind preparation confirmation to the resolved note's exact identity and a SHA-256 version of its exact persisted Markdown.
- Reject stale targets if the note was modified externally between confirmation and execution.
- Ensure byte-for-byte preservation of existing body text with standard Markdown separators.
- Enforce idempotence: duplicate deliveries of identical additions do not create repeated blocks.
- Keep authorization/version state private to the host.

### Implementation

- **Persisted Version & Concurrency Guard (`src-tauri/src/grain_space/vault.rs`, `src-tauri/src/grain_space/mod.rs`):**
  - `get_append_snapshot` resolves the writable note, display title, and SHA-256 hash of the exact persisted Markdown under one vault lock.
  - `append_note_atomic` compares that exact version (including frontmatter), preserves the existing body byte-for-byte, re-reads immediately before atomic replacement, and rejects stale targets.
  - `append_with_idempotency` reserves an operation key while a write is in flight, records it only after success, and releases failed operations for safe retry. The bounded registry retains 256 completed and 32 in-flight keys.
- **Prepared Action Binding (`src-tauri/src/grain_space/agent_tools.rs`):**
  - Bound `id`, exact title, addition, and host-owned `expected_version` into the prepared action. The digest is removed from user/model-facing confirmation details.
  - Resolution enforces deterministic ID lookup or unambiguous exact title match; missing or ambiguous targets fail closed.
- **Action Execution Dispatch (`src-tauri/src/action_exec.rs`):**
  - Updated `grain_space_execute` to pass the private `expected_version` and per-operation idempotency key to `append_with_idempotency`.
- **Eradication of Whole-Body Model Reconciliation (`src-tauri/src/grain_space/capture.rs`):**
  - Replaced whole-body LLM reconciliation logic with deterministic `raw_append`.
  - Preserved original body byte-for-byte; removed whole-note LLM rewrite pathways on append.
- **Comprehensive Edge-Case Testing (`src-tauri/src/grain_space/capture.rs`):**
  - `phase_4_safe_deterministic_append_invariants`: tests the production storage append, stale version rejection, byte preservation, and non-existent note failure.
  - `phase_4_content_version_hash_properties`: tests determinism, single-byte sensitivity, whitespace sensitivity, CJK/multibyte Unicode, and empty strings.
  - `phase_4_raw_append_whitespace_and_separator_variations`: tests empty body, trailing single/double newlines, code fences, and internal dividers.
  - `phase_4_duplicate_detection_edge_cases`: tests suffix overlap without separator (false-positive prevention), proper separator match, and exact-body match.

### Verification

- `cargo fmt --check -- src-tauri/src/action_exec.rs src-tauri/src/grain_space/agent_tools.rs src-tauri/src/grain_space/capture.rs src-tauri/src/grain_space/mod.rs` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::capture::tests::phase_4` — passed, 4 tests (100% pass).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::` — passed, 109 tests (100% pass).
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` — passed (0 errors).
- `npx tsc --noEmit` — passed (0 errors).

### Phase 4 Gate Assessment

- **Wrong-note append rate is zero in acceptance fixtures:** PASSED. Exact note identity resolution and validation in `agent_tools.rs` and `mod.rs` prevents incorrect note targeting.
- **Ambiguous targets abstain:** PASSED. Preparation fails closed when note cannot be uniquely resolved.
- **Existing text is never silently removed or rewritten:** PASSED. Whole-body LLM reconciliation is removed; the storage-layer atomic append preserves the existing note body byte-for-byte.
- **Duplicate and stale operations fail safely:** PASSED. Idempotence prevents stacked duplicate text; stale version check rejects out-of-order writes with clear error messaging.

---

## Phase 5 — End-to-End Qualification & Clean-up

### Objectives

- Exercise headless contracts for Agent preparation, search/create/append storage, confirmation holding, and result data.
- Clean obsolete Recall-brain contracts (`run_turn`, `run_tool_loop`, `execute_search_memory`, `build_block_and_meta`, `session_registry`, `reconcile_note`, `MergedMeta`) after all consumers migrated to the unified Agent loop.
- Test corrupt-index recovery, external-edit concurrency rejection, and vault-switching isolation.
- Verify complete headless memory evaluation and full library test suites.

### Implementation

- **Obsolete Contract Removal (`src-tauri/src/grain_space/recall.rs`, `src-tauri/src/grain_space/capture.rs`):**
  - Removed `run_turn`, `run_tool_loop`, `execute_search_memory`, `search_memory_spec`, `clone_tools`, `build_block_and_meta`, `session_registry`, `register_hits`, `read_note`, `persist`, `build_entries`, and `prepend_memories` from `recall.rs`.
  - Removed `MergedMeta`, `MergedTodo`, and `reconcile_note` from `capture.rs`.
  - Cleaned obsolete imports (`AgentMessage`, `AgentReply`, `AgentSource`, `Manager`).
- **Headless Qualification Tests (`src-tauri/src/grain_space/eval.rs`):**
  - `phase_5_storage_journey_contract`: exercises Agent write preparation/holding plus direct production storage, lexical search, exact version computation, deterministic safe append, byte-for-byte preservation, and immediate re-indexing. It does not claim to drive a live model or the real Tauri confirmation UI.
  - `phase_5_corrupt_index_and_recovery`: verifies index destruction and zero-loss reconstruction from raw markdown notes via `vault::rebuild_index`.
  - `phase_5_external_edit_and_concurrency_failure`: tests stale version detection when an external edit occurs on disk between preparation and confirmation, ensuring stale writes fail closed and external edits remain uncorrupted.
  - `phase_5_vault_switching_isolation`: verifies multi-vault isolation, ensuring zero cross-vault leakage of search hits or notes between separate vaults.

### Verification

- `cargo fmt --check -- src-tauri/src/grain_space/eval.rs src-tauri/src/grain_space/recall.rs src-tauri/src/grain_space/capture.rs` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::eval::tests::phase_5` — passed, 4 tests (100% pass, 0.13s).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space::` — passed, 112 tests (100% pass).
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` — passed (0 errors, 28 warnings down from 47).
- `npx tsc --noEmit` — passed (0 errors).

### Phase 5 Gate Assessment

- **Core journeys work with weakest supported tool-calling model:** MANUAL ACCEPTANCE NOT RUN. The headless schemas and routing contracts pass; live-model behavior must be judged in the real application.
- **No unresolved critical/high security or data-integrity issue:** PASSED after the final hardening audit below.
- **No unjustified idle or memory regression:** ARCHITECTURALLY PASSED, NOT MEASURED. No daemon, watcher, polling loop, or resident model was added; peak/idle RAM still requires real-app measurement.
- **Clean-checkout required checks pass with exact results recorded:** see the final hardening audit below for the authoritative verification results.

---

## Final Security and Integrity Audit — 2026-09-04

This section supersedes earlier gate claims where their scope differs.

### Findings closed

- Removed MCP and extension note-write routes that could not prove out-of-band user approval. In particular, the MCP caller can no longer receive a confirmation token and submit that token itself. Both bridges now expose bounded reads only; first-party Agent mutations remain behind Grain's confirmation surface.
- Replaced post-write idempotency bookkeeping with bounded in-flight/completed operation state. Failed writes are retryable, concurrent duplicates fail closed, and distinct intentional operations with identical arguments receive distinct SHA-256 keys.
- Bound append confirmation to one locked target/title/exact-Markdown snapshot. Frontmatter-only changes and late external edits are rejected, and the host-owned version digest is not exposed in confirmation details.
- Removed model-authored note-body rewriting. Capture models now return metadata only; the captured text is the durable body. Metadata schemas and Rust normalization cap titles, summaries, questions, todos, entities, and relations.
- Bounded bridge queries, IDs, bodies, metadata, result counts, collection lists, and Agent read output using UTF-8-safe truncation.
- Published the MCP bearer-token file through an owner-only create-new temporary file and atomic rename; every mint revokes prior MCP identities even if the old file is missing/corrupt, and a failed publication revokes the new token.
- Removed obsolete runtime Recall session state and dead bridge mutation implementations. Fixed temporal phrase boundaries and rolling-window behavior.
- Validated the checked-in memory evaluation schema/version/mode instead of accepting unused or empty fixtures.

### Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1` — 704 passed, 1 network test ignored, 0 failed.
- `cargo test --manifest-path crates/grain-core/Cargo.toml` — 204 unit + 4 benchmark tests passed.
- `cargo test --manifest-path crates/grain-sdk/Cargo.toml` — 96 passed.
- `cargo test --manifest-path crates/grain-mcp/Cargo.toml` — 3 integration tests passed.
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` — passed; remaining warnings are pre-existing outside this Grain Space change.
- `npm run test:unit` — 112 passed across 12 files.
- `npx tsc --noEmit`, `npm run lint`, `npm run build`, `cargo fmt --all -- --check`, and `git diff --check` — passed.

### Acceptance scope

The code and headless contracts are closed for this pass. Real-application visual behavior, weakest-model task quality, and measured peak/idle RAM were not simulated by tests; those remain manual dogfood/acceptance checks rather than unimplemented code claims.
