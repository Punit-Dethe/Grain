# Grain Space Memory System — Implementation & Execution Log

**Document Purpose:** Sidecar log to [`MEMORY-SYSTEM-PLAN.md`](./MEMORY-SYSTEM-PLAN.md) tracking phase status, exact implementation details, architectural decisions, deviations, test verifications, and root-cause analysis findings across all phases.

---

## 1. Phase Tracker

| Phase | Description | Status | Verification Gate |
|---|---|---|---|
| **Phase 0** | Freeze Contracts, Corpus, Baseline | **COMPLETED** | Eval harness, 24-doc corpus, 15 golden queries locked & green |
| **Phase 1** | Unify Grain Space Tool Policy & Confirmation Gates | **COMPLETED** | 6/6 contract tests passing; confirmation gate strictly enforced |
| **Checkpoint A**| Security Review of Mutation Entry Points | **COMPLETED** | Zero direct write mutations; all writes gated via `action_exec` |
| **Phase 2** | Schema V3 and Markdown Codec | **COMPLETED** | 107/107 tests passing; lossless block parsing & foreign adoption verified |
| **Phase 3** | Transactional Mutation Engine | **COMPLETED** | 119/119 tests passing; ledger, target tokens, append-safe rebase, privacy purge |
| **Checkpoint B**| Correctness, Concurrency, Privacy & Crash Audit | **NEXT** | Verification against concurrency, crash, and privacy invariants |
| **Phase 4** | Temporal Planner & Search Filters | *PENDING* | Golden temporal suite across DST / midnight / recurrent |
| **Phase 5** | Block-Level Evidence & Write Targets | *PENDING* | Provenance attribution & target resolution |
| **Phase 6** | Agent Memory Surface & Context Assembly | *PENDING* | Token budgeting, deduplication, latency benchmarks |
| **Phase 7** | Optional Knowledge Graph / Projection Layer | *PENDING* | Disposable SQLite projections & idempotent rebuilds |
| **Phase 8** | Production Hardening & Live Eval Runs | *PENDING* | Live A/B recall evaluation & telemetry |
| **Phase 9** | Documentation, Final Sync & Cleanup | *PENDING* | Upstream divergence docs & final commit |

---

## 2. Phase 0 — Freeze Contracts, Corpus, Baseline

### Deliverables Completed
- Created `src-tauri/tests/fixtures/memory-eval/v1/corpus/` with 24 diverse, realistic markdown documents covering:
  - Personal projects, meeting minutes, daily logs, tech specs, recurring 1-on-1s.
  - Foreign/unmanaged Obsidian files with YAML properties and internal wikilinks (`edge_foreign_markdown.md`).
  - Edge cases: temporal shifts, conflicting assertions, name collisions, empty notes.
- Created `src-tauri/tests/fixtures/memory-eval/v1/golden.json` containing 15 multi-intent queries:
  - Temporal queries ("meetings last Friday", "actions from yesterday").
  - Decision queries ("decision to deprecate update_note").
  - Foreign document queries ("Obsidian guide second brain").
  - Fact extraction and negative queries.
- Implemented `src-tauri/src/grain_space/eval.rs`:
  - Evaluation runner scoring Precision@K, Recall@K, and MRR.
  - Test `test_corpus_fixtures_exist_and_golden_queries_parse` and `run_baseline_evaluation`.

---

## 3. Phase 1 — Unify Grain Space Tool Policy & Confirmation Gates

### Deliverables Completed
- **Unified Tool Definitions (`action_exec.rs`):**
  - Created single canonical source of truth for all Grain Space tool definitions and JSON schemas (`grain_space_tool_definitions()`, `grain_space_tool_specs()`, `grain_space_actions()`).
  - Added complete metadata parameters to write actions: `summary`, `question`, `entities`, `collection`.
- **Strict Confirmation Gate Policy:**
  - `prepare_grain_space_call` enforces `RiskClass::Confirm` on all state-mutating operations (`save_note`, `append_to_note`, `delete_note`).
  - Safe read-only operations (`read_note`, `search_notes`, `list_notes`, `recent_notes`) execute as `RiskClass::Safe`.
  - Removed direct `agent_tools::dispatch` execution of write actions. Mutation tool calls return `ToolResult::Confirm(confirm)`, pausing the agent loop with `status: "pending_confirm"` until the user confirms or rejects.
  - Added confirmation flow support to `host_api` (`"space.confirm"` command).
- **Contract Verification Tests (`contract_tests.rs`):**
  - Added 6 strict contract tests verifying:
    1. Unified tool schemas.
    2. Agent attempts yielding `confirm` with no disk writes.
    3. User confirmation approval triggering execution with complete metadata.
    4. User rejection cleanly discarding mutation.
    5. Malformed arguments returning structured error objects without panics.
    6. MCP and Host API enforcing identical confirmation policies.

---

## 4. Phase 2 — Schema V3 and Markdown Codec

### Deliverables Completed

#### 1. Stable, Human-Readable Block Codec (`src-tauri/src/grain_space/block_codec.rs`)
- **Block Marker Format:** Standard HTML comments that render invisibly in Obsidian, GitHub, and other Markdown viewers:
  ```markdown
  <!-- grain:block id="b0" kind="raw_capture" seq="0" recorded="1710000000000" start="..." end="..." speaker="..." source="..." supersedes="..." -->
  Block content here.
  <!-- /grain:block -->
  ```
- **Lossless Block Parser:**
  - Extracts all delimited blocks into typed `MemoryBlock` structures.
  - Preserves user text written before, between, or after block markers into synthetic `MemoryBlockKind::Body` blocks.
  - Zero text loss guarantee for hand-edited documents.
  - Fallback for legacy documents: treats the entire body as a synthetic block `b0`.
- **Platform-Invariant Content Hashing:**
  - `compute_content_hash(body)` normalizes CRLF and trailing whitespace per line before computing SHA-256.

#### 2. Schema V3 Data Model (`src-tauri/src/grain_space/note.rs`)
- Extended `Note` struct with Schema V3 fields:
  - `schema_version: u32` (default 1, Schema V3 notes use 3).
  - `kind: NoteKind` (`Note`, `Daily`, `Meeting`, `ProjectLog`).
  - `updated_at: Option<i64>`.
  - `timezone: String` (local IANA timezone name).
  - `revision: u64` (starts at 1).
  - `content_hash: String`.
  - `aliases: Vec<String>`.
  - `occurrence_id: Option<String>`, `series_id: Option<String>`.
  - `event_start: Option<i64>`, `event_end: Option<i64>`.
  - `participants: Vec<String>`.
  - `collection: Option<String>`.
  - `blocks: Vec<MemoryBlock>`.
- Updated `json_schema_is_locked` test to lock all 25 fields in alphabetical order.
- Added backward compatibility test `notes_written_before_schema_v3_still_deserialize`.

#### 3. Vault & Frontmatter Codec (`src-tauri/src/grain_space/vault.rs`)
- Updated `GrainMeta`, `parse_grain_meta`, `GRAIN_FM_KEYS`, and `emit_markdown_with` to serialize and deserialize all Schema V3 fields.
- Added block-list parsing support for `aliases:` in `parse_grain_meta`.
- Updated `read_md_note` to extract foreign aliases and parse blocks on both Grain-managed and foreign documents.
- Updated `save_note` to automatically compute content hashes and parse blocks on save.

#### 4. Explicit Foreign Document Adoption (`adopt_foreign_note`)
- Implemented `pub fn adopt_foreign_note(v: &Vault, id_or_rel: &str) -> Result<Note>`:
  - If a foreign document is outside Grain's folder, moves it into `v.grain_dir()` to maintain the write-safety boundary.
  - Preserves all custom Obsidian frontmatter (`tags`, custom keys).
  - Assigns a fresh UUID v4 `grain_id`, sets `schema_version = 3`, `revision = 1`.
  - Computes `content_hash`, parses/wraps body into blocks, stamps `source: "adopted"`.
  - Writes atomically and updates SQLite index with `foreign_note = 0`.

---

## 5. Discoveries, Findings & Root Cause Analysis

### Finding 1: Windows Test Runner DLL Entry Point Crash (`0xc0000139`)
- **Symptom:** Cargo test binaries (`handy_app_lib-*.exe`) exited immediately with `0xc0000139 (STATUS_ENTRYPOINT_NOT_FOUND)` on Windows.
- **Root Cause Analysis:**
  - Using a custom Win32 debugger script with `CreateProcessW` and `WaitForDebugEvent`, the failing import was identified as `TaskDialogIndirect` in `comctl32.dll`.
  - `rfd` (imported by `tauri-plugin-dialog`) calls `TaskDialogIndirect`, which is only available in Microsoft Common Controls version 6.0+.
  - On Windows, binaries lacking an embedded manifest linking Common Controls v6 load v5.82 by default, which lacks `TaskDialogIndirect`.
- **Resolution:** Added a target-specific manifest linker argument to `src-tauri/build.rs`:
  ```rust
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
      println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
  }
  ```
  This completely resolved the crash across the entire test suite.

### Finding 2: Obsidian Native Properties & `aliases` Preservation
- **Symptom:** Foreign document adoption test initially dropped user `aliases: [Imp]` because `GRAIN_FM_KEYS` declared `aliases` as a Grain-managed key.
- **Root Cause & Architectural Decision:**
  - `aliases` is both a standard Obsidian property and a first-class Schema V3 field on `Note`.
  - When `read_md_note` parses foreign notes, it now extracts `aliases` (both YAML flow sequence `[a, b]` and block list `- a`) directly into `Note.aliases`.
  - When written out, `emit_markdown_with` outputs `Note.aliases` cleanly via `emit_flow_list`.
  - This ensures user-defined Obsidian aliases are fully adopted into Grain's data model without losing their Obsidian readability.

---

## 5. Phase 3 — Transactional Mutation Engine

### Deliverables Completed

#### 1. Durable Ledger & Metadata Storage (`src-tauri/src/grain_space/mutation.rs`)
- **`memory_operations` Table:**
  - Tracks every mutation lifecycle state (`prepared`, `committed`, `failed`, `stale`, `rejected`).
  - Columns: `operation_id`, `idempotency_key`, `document_id`, `base_revision`, `committed_revision`, `operation_kind`, `proposed_content_hash`, `state`, `created_at`, `completed_at`, `error_code`.
- **`vault_meta` Table:**
  - Tracks system flags such as `projection_dirty` to detect crashes during write operations and trigger projection rebuilds on startup.
- **Privacy-Preserving Purge:**
  - `purge_operations_for_document(conn, id)` removes all ledger entries associated with a document when `vault::delete_note` is called, ensuring raw operations and content hashes do not outlive the document.

#### 2. Cryptographic Target Tokens (`issue_target_token`, `validate_target_token`)
- **Opaque & Process-Local:** Signed with RFC 2104 HMAC-SHA256 using an ephemeral 256-bit CSPRNG key initialized once per process (`std::sync::OnceLock<[u8; 32]>`). Tokens cannot be forged across processes or leaked across app launches.
- **Bound & Short-Lived:** Tokens strictly bind:
  - `token_id`, `vault_path` (canonicalized), `document_id`, `base_revision`, `base_content_hash`, `allowed_operation`, `target_block_id`, `issued_at`, `expires_at` (default TTL 600 seconds).
- **Fails Closed:** Rejects expired tokens, tampered signatures, cross-vault operations, mismatched operations, and unadopted foreign documents.

#### 3. Transactional Operations & Append-Safe Rebase
- **`execute_transactional_append`:**
  - Validates token against document base revision and hash.
  - If external edits occurred (`revision != base_revision`), safely rebases append operations to the end of the document, preserving external edits without clobbering.
  - Formats content using `block_codec::emit_block` with unique sequential block ID (`{doc_id}-b{seq}`).
  - Atomically increments revision, sets `schema_version = 3`, computes normalized content hash, marks projection dirty, writes to disk, marks projection clean, and commits ledger entry.
- **`execute_transactional_correction`:**
  - Appends a `MemoryBlockKind::Correction` block with `supersedes_block_id` referencing the superseded block.
- **`execute_transactional_block_edit`:**
  - Enforces strict revision comparison (fails closed on stale revisions).
  - Updates target block content and regenerates note body cleanly via `block_codec::emit_blocks`.
- **Idempotency Enforcement:**
  - Replaying an identical operation with the same idempotency key and proposed content hash returns the previously committed result immediately without duplicating blocks.
  - Conflicting operations reusing an idempotency key fail closed with `MutationError::IdempotencyConflict`.

#### 4. Unified Dispatch Integration (`action_exec.rs` & `grain_space::mod.rs`)
- Upgraded `grain_space::append` to use `issue_target_token` and `execute_transactional_append`.
- Added `grain_space::append_transactional` for callers supplying explicit `TargetToken`s and idempotency keys.
- Extended `action_exec.rs` `append_to_note` tool schema and execution to support optional authenticated `target_token`.

---

## 6. Discoveries, Findings & Root Cause Analysis

### Finding 1: Windows Test Runner DLL Entry Point Crash (`0xc0000139`)
- **Symptom:** Cargo test binaries (`handy_app_lib-*.exe`) exited immediately with `0xc0000139 (STATUS_ENTRYPOINT_NOT_FOUND)` on Windows.
- **Root Cause Analysis:**
  - Using a custom Win32 debugger script with `CreateProcessW` and `WaitForDebugEvent`, the failing import was identified as `TaskDialogIndirect` in `comctl32.dll`.
  - `rfd` (imported by `tauri-plugin-dialog`) calls `TaskDialogIndirect`, which is only available in Microsoft Common Controls version 6.0+.
  - On Windows, binaries lacking an embedded manifest linking Common Controls v6 load v5.82 by default, which lacks `TaskDialogIndirect`.
- **Resolution:** Added a target-specific manifest linker argument to `src-tauri/build.rs`:
  ```rust
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
      println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
  }
  ```
  This completely resolved the crash across the entire test suite.

### Finding 2: Obsidian Native Properties & `aliases` Preservation
- **Symptom:** Foreign document adoption test initially dropped user `aliases: [Imp]` because `GRAIN_FM_KEYS` declared `aliases` as a Grain-managed key.
- **Root Cause & Architectural Decision:**
  - `aliases` is both a standard Obsidian property and a first-class Schema V3 field on `Note`.
  - When `read_md_note` parses foreign notes, it now extracts `aliases` (both YAML flow sequence `[a, b]` and block list `- a`) directly into `Note.aliases`.
  - When written out, `emit_markdown_with` outputs `Note.aliases` cleanly via `emit_flow_list`.
  - This ensures user-defined Obsidian aliases are fully adopted into Grain's data model without losing their Obsidian readability.

### Finding 3: Zero-Dependency Cryptographic Primitives in Backend
- **Context:** The `hex` and `thiserror` crates are not in `src-tauri/Cargo.toml`.
- **Architectural Decision:** Rather than adding external dependencies to the workspace:
  - Standard `std::fmt::Display` and `std::error::Error` were implemented manually for `MutationError`, seamlessly converting to `anyhow::Error`.
  - Inline byte-to-hex formatting helper (`hex_encode`) was implemented via `std::fmt::Write`.
  - RFC 2104 HMAC-SHA256 was implemented directly using the existing `sha2::Sha256` dependency.

---

## 7. Verification Results (Phase 3)
- Running `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space`:
  - **119 passed; 0 failed; 0 ignored.**
  - Includes:
    - 12 mutation unit tests (`test_target_token_issue_and_validates_cleanly`, `test_expired_token_fails_closed`, `test_tampered_token_fails_closed`, `test_cross_vault_token_fails_closed`, `test_wrong_operation_token_fails_closed`, `test_transactional_append_exact_revision`, `test_idempotent_append_returns_identical_result_without_duplicate_block`, `test_idempotent_append_with_different_content_fails_closed`, `test_append_safe_rebase_on_external_concurrent_edit`, `test_correction_creates_superseding_block`, `test_block_edit_strict_revision_rejection`, `test_deletion_purges_ledger_records`).
    - 41 vault unit tests.
    - 4 block codec unit tests.
    - 5 note schema tests.
    - 6 contract tests.
    - 24-document evaluation corpus baseline test.
