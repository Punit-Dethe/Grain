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
| 1 — One trusted note-tool execution path | Complete | Phase 1 commit |
| 2 — Broad corpus and note construction | Next | — |
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


