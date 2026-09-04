# Grain Space Improvement Log

**Branch:** `codex/grain-space-improvement`

**Base:** `main` at `ba7e1317`

**Plan:** `GRAIN-SPACE-IMPROVEMENT-PLAN.md`

## Scope statement

This branch starts cleanly from `main`. Nothing from `codex/grain-space-memory-experiment` is included.

## Phase tracker

| Phase | Status | Commit |
|---|---|---|
| 0 — Agent-first convergence and baseline | In progress | — |
| 1 — One trusted note-tool execution path | Pending | — |
| 2 — Broad corpus and note construction | Pending | — |
| 3 — Retrieval and temporal calibration | Pending | — |
| 4 — Safe deterministic append | Pending | — |
| 5 — End-to-end qualification | Pending | — |

## Phase 0 — Agent-first convergence and baseline

### Starting observations

- The normal Agent already advertises Grain Space note tools and waits for the model to call them.
- A separate `AgentMode::Recall` path still sends the raw utterance through `recall::run_turn`, which performs an automatic first-pass retrieval and prepends matching notes before the model chooses a tool.
- The Recall-labelled shortcut therefore behaves differently from the unified Agent and violates the new agent-first invariant.
- Explicit search inside the Notes workspace and explicit capture are separate user-directed operations and are not speculative Agent retrieval.

### Changes

- Created this branch directly from `main` at `ba7e1317`; no commits or files from the abandoned experiment were imported.
- Removed the `agent_run` branch that sent `AgentMode::Recall` directly to `grain_space::recall::run_turn`.
- Recall-labelled voice submissions now enter `run_with_note_tools`, the same bounded Agent loop used by ordinary Agent conversations and extension tools.
- Kept Recall's no-selection/no-field capture behavior as a presentation/input distinction.
- Kept the explicit Notes workspace recall command unchanged for this first slice; it is the remaining consumer of the legacy pre-retrieval implementation and will be converged before Phase 0 is complete.
- Updated stale comments that described Recall as a separate conversational brain.

### Verification

- `cargo fmt --all -- --check` — passed.
- `cargo test --lib agent::` — passed, 3 tests.
- `cargo test --lib grain_space::agent_tools` — passed, 2 tests.
- `cargo check --lib` — passed with three pre-existing warnings outside this change.
- `rg -n "recall::run_turn" src-tauri/src` — only the explicit Notes workspace command remains; the spoken Agent command has no direct legacy Recall call.

### Remaining before Phase 0 gate

- Converge the Notes workspace conversational rail onto the unified Agent tool loop, or explicitly remove it if the product surface no longer needs a second conversation entry point.
- Add an observable regression test around tool selection so an unrelated Agent request proves it performs zero Grain Space searches.
- Record the current retrieval, model-disabled, latency, and resource baseline before tuning.
