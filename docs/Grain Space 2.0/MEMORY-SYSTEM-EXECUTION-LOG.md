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
| **Phase 3** | Transactional Mutation Engine | **NEXT** | Mutation ledger, target tokens, revision checks, safe rebase |
| **Checkpoint B**| Correctness, Concurrency, Privacy & Crash Audit | *PENDING* | - |
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

## 6. Verification Results (Phase 2)
- Running `cargo test --manifest-path src-tauri/Cargo.toml --lib grain_space`:
  - **107 passed; 0 failed; 0 ignored.**
  - Includes:
    - 41 vault unit tests (including `schema_v3_frontmatter_full_roundtrip`, `adopt_foreign_note_moves_and_promotes_cleanly`, `legacy_notes_without_schema_v3_load_cleanly`).
    - 4 block codec unit tests (`test_emit_and_parse_blocks_roundtrip`, `test_mixed_manual_edits_preserved_without_loss`, `test_content_hash_deterministic_crlf_lf`, `test_legacy_body_without_markers_parses_to_single_block`).
    - 5 note schema tests (`json_schema_is_locked`, `notes_written_before_schema_v3_still_deserialize`, etc.).
    - 6 contract tests (Phase 1 confirmation policy).
    - 24-document evaluation corpus baseline test (`run_baseline_evaluation`).
