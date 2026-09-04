# Grain Space Improvement Log

**Branch:** `codex/grain-space-improvement`

**Base:** `main` at `ba7e1317`

**Plan:** `GRAIN-SPACE-IMPROVEMENT-PLAN.md`

## Scope statement

This branch starts cleanly from `main`. Nothing from `codex/grain-space-memory-experiment` is included.

## Phase tracker

| Phase | Status | Commit |
|---|---|---|
| 0 — Agent-first convergence and baseline | Complete | `a20e7852`, Phase 0 convergence |
| 1 — One trusted note-tool execution path | Next | — |
| 2 — Broad corpus and note construction | Pending | — |
| 3 — Retrieval and temporal calibration | Pending | — |
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

