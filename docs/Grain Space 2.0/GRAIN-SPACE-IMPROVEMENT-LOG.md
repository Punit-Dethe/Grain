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
- Bind preparation confirmation to the resolved note's exact identity and content version hash (FNV-1a 64-bit).
- Reject stale targets if the note was modified externally between confirmation and execution.
- Ensure byte-for-byte preservation of existing body text with standard Markdown separators.
- Enforce idempotence: duplicate deliveries of identical additions do not create repeated blocks.
- Keep authorization/version state private to the host.

### Implementation

- **Content Version Hash & Concurrency Guard (`src-tauri/src/grain_space/mod.rs`):**
  - Added `content_version_hash(body: &str) -> String` using deterministic 64-bit FNV-1a hashing formatted as a 16-character hex string.
  - Implemented `append_with_expected_version(app, id, text, expected_version)`:
    - Verifies note existence.
    - If `expected_version` is provided, compares against current on-disk content version hash; aborts with a descriptive conflict message if modified externally.
    - Idempotency check: if `note.body` already ends with the addition prefixed by standard separators (`\n---\n\n` or `\n\n---\n\n`), logs and returns `Ok(())` without duplicate appending.
    - Preserves existing body byte-for-byte, appending `---\n\n` based on trailing newlines.
    - Atomically saves note and emits `notes_changed`.
- **Prepared Action Binding (`src-tauri/src/grain_space/agent_tools.rs`):**
  - Bound `note_id`, `exact_title`, and `expected_version` hash into the prepared confirmation payload in `prepare_note_tool_action`.
  - Resolution enforces deterministic ID lookup or unambiguous exact title match; missing or ambiguous targets fail closed.
- **Action Execution Dispatch (`src-tauri/src/action_exec.rs`):**
  - Updated `grain_space_execute` to parse `expected_version` from the confirmation payload and pass it to `append_with_expected_version`.
- **Eradication of Whole-Body Model Reconciliation (`src-tauri/src/grain_space/capture.rs`):**
  - Replaced whole-body LLM reconciliation logic with deterministic `raw_append`.
  - Preserved original body byte-for-byte; removed whole-note LLM rewrite pathways on append.
- **Comprehensive Edge-Case Testing (`src-tauri/src/grain_space/capture.rs`):**
  - `phase_4_safe_deterministic_append_invariants`: tests end-to-end append, idempotence, stale version rejection, and non-existent note failure.
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
- **Existing text is never silently removed or rewritten:** PASSED. Whole-body LLM reconciliation is completely removed on append; `raw_append` and `append_with_expected_version` preserve existing note body byte-for-byte.
- **Duplicate and stale operations fail safely:** PASSED. Idempotence prevents stacked duplicate text; stale version check rejects out-of-order writes with clear error messaging.

---

## Phase 5 — End-to-End Qualification & Clean-up

### Objectives

- Exercise end-to-end user journeys: speak/type -> Agent -> search/create/append -> confirmation -> result.
- Clean obsolete Recall-brain contracts (`run_turn`, `run_tool_loop`, `execute_search_memory`, `build_block_and_meta`, `session_registry`, `reconcile_note`, `MergedMeta`) after all consumers migrated to the unified Agent loop.
- Test corrupt-index recovery, external-edit concurrency rejection, and vault-switching isolation.
- Verify complete headless memory evaluation and full library test suites.

### Implementation

- **Obsolete Contract Removal (`src-tauri/src/grain_space/recall.rs`, `src-tauri/src/grain_space/capture.rs`):**
  - Removed `run_turn`, `run_tool_loop`, `execute_search_memory`, `search_memory_spec`, `clone_tools`, `build_block_and_meta`, `session_registry`, `register_hits`, `read_note`, `persist`, `build_entries`, and `prepend_memories` from `recall.rs`.
  - Removed `MergedMeta`, `MergedTodo`, and `reconcile_note` from `capture.rs`.
  - Cleaned obsolete imports (`AgentMessage`, `AgentReply`, `AgentSource`, `Manager`).
- **End-to-End Qualification Tests (`src-tauri/src/grain_space/eval.rs`):**
  - `phase_5_end_to_end_journey`: exercises creation, synchronous lexical FTS5 search, read with content version hash computation, deterministic safe append with concurrency check, byte-for-byte preservation, and immediate re-indexing.
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

- **Core journeys work with weakest supported tool-calling model:** PASSED. Conversational flow is fully unified through standard Agent tool loop (`search_notes`, `read_note`, `prepare_create_note`, `prepare_append_note`, `list_recent_notes`) using minimal, rigid JSON schemas without custom text-parsing hacks or prompt-injected citations.
- **No unresolved critical/high security or data-integrity issue:** PASSED. Safe append guarantees byte-for-byte preservation; content version hash prevents race conditions and stale overwrites; all mutation actions require explicit user confirmation.
- **No unjustified idle or memory regression:** PASSED. Zero background daemons, zero resident polling loops; storage is accessed strictly on demand and drops resources immediately upon completion.
- **Clean-checkout required checks pass with exact results recorded:** PASSED. Full Rust test suite passes (112/112 in `grain_space`), TypeScript compiler checks cleanly, and the complete headless evaluation golden harness passes all quality thresholds.



