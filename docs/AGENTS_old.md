# AGENTS.md

> **For AI coding assistants only.** This file defines your identity, constraints, operating rules, and reference knowledge for working in this repository. Read it top-to-bottom on first load. On subsequent tasks, constraints and architecture are your most important anchors — consult them before acting.

---

## Table of Contents

1. [Agent Identity & Operating Mode](#1-agent-identity--operating-mode)
2. [Project Constraints — Read Before Touching Any Code](#2-project-constraints--read-before-touching-any-code)
3. [Architecture Overview](#3-architecture-overview)
4. [Review Workflow](#4-review-workflow)
5. [Code Style](#5-code-style)
6. [GitHub Workflow — Mandatory Before Any PR/Issue](#6-github-workflow--mandatory-before-any-prissue)
7. [Reference: Development Commands](#7-reference-development-commands)
8. [Reference: CLI Parameters](#8-reference-cli-parameters)
9. [Reference: Internationalization (i18n)](#9-reference-internationalization-i18n)
10. [Reference: Platform Notes & Debug Mode](#10-reference-platform-notes--debug-mode)

---

## 1. Agent Identity & Operating Mode

You are an AI coding assistant working on **Grain** — a cross-platform desktop speech-to-text application (Tauri 2.x, Rust backend, React/TypeScript frontend), forked from the open-source **Handy** project.

Your default operating mode when given a domain to work on:

1. **Understand** the domain (read relevant files, infer intent).
2. **Plan** changes that respect project constraints.
3. **Execute** targeted, minimal, high-value edits.
4. **Log** what you changed and why.

If the user gives a more specific goal, follow that. Otherwise, look for: bugs, efficiency gains, architectural improvements, and maintainability improvements — in that priority order.

---

## 2. Project Constraints — Read Before Touching Any Code

These constraints govern every decision. A technically correct change that violates these is still a wrong change.

| Constraint | What it means in practice |
| --- | --- |
| **Upstream compatibility** | Keep compatibility with upstream Handy as much as reasonably possible. Avoid unnecessary divergence. Prefer extending over modifying shared code. |
| **Frontend/backend decoupling** | Frontend and backend must remain independently operable. Do not blur the command/event boundary. |
| **Destroy if not in use** | Do not hold resources, listeners, state, or services alive beyond their required lifetime. Explicit cleanup is mandatory. |
| **Low RAM / low overhead** | Prefer solutions with minimal background footprint. Reject approaches that trade memory for marginal convenience. |
| **Unusual code is probably intentional** | Before changing anything that looks odd, infer why it exists. Ask yourself what problem it was solving before removing or rewriting it. |

**Optimization priority order:** Correctness → Efficiency → Maintainability.

---

## 3. Architecture Overview

### What Grain Is

Grain records audio via a global keyboard shortcut, filters it with VAD, transcribes it locally using Whisper/Parakeet, and pastes the result into the active application.

### Application Flow

```
Keypress → Audio recording (cpal) → VAD filter (Silero) → Whisper/Parakeet → Clipboard/Paste
```

1. **Initialization** — App starts minimized to tray, loads settings, initializes managers.
2. **Model Setup** — First-run downloads preferred Whisper model (Small/Medium/Turbo/Large).
3. **Recording** — Global shortcut triggers audio capture with VAD filtering.
4. **Processing** — Audio sent to Whisper model for transcription.
5. **Output** — Text pasted to active application via system clipboard.

### Key Architecture Patterns

**Manager Pattern** — Core functionality split into three managers (Audio, Model, Transcription), initialized at startup and held in Tauri state. This is the primary unit of organization.

**Command-Event Architecture** — All frontend→backend communication goes through Tauri commands. All backend→frontend communication goes through events. Never bypass this boundary.

**State Flow**
```
Zustand (frontend) → Tauri Command → Rust State → Persistence (tauri-plugin-store)
```

**Single Instance** — Launching when already running brings the settings window to front. Remote control flags work by launching a second instance that sends args via `tauri_plugin_single_instance`, then exits immediately.

### Backend File Map (`src-tauri/src/`)

| Path | Purpose |
| --- | --- |
| `lib.rs` | Main entry point, Tauri setup, manager initialization |
| `managers/audio.rs` | Audio recording and device management |
| `managers/model.rs` | Model downloading and management |
| `managers/transcription.rs` | Speech-to-text processing pipeline |
| `managers/history.rs` | Transcription history storage |
| `audio_toolkit/audio/` | Device enumeration, recording, resampling |
| `audio_toolkit/vad/` | Voice Activity Detection (Silero VAD) |
| `commands/` | Tauri command handlers for frontend communication |
| `cli.rs` | CLI argument definitions (clap derive) |
| `shortcut.rs` | Global keyboard shortcut handling |
| `settings.rs` | Application settings management |
| `overlay.rs` | Recording overlay window (platform-specific) |
| `signal_handle.rs` | `send_transcription_input()` — shared between CLI and signal handlers |
| `utils.rs` | Platform detection helpers |

### Frontend File Map (`src/`)

| Path | Purpose |
| --- | --- |
| `App.tsx` | Main component with onboarding flow |
| `components/settings/` | Settings UI |
| `components/model-selector/` | Model management interface |
| `components/onboarding/` | First-run experience |
| `components/overlay/` | Recording overlay UI |
| `components/update-checker/` | App update notifications |
| `components/shared/`, `ui/`, `icons/`, `footer/` | Shared components |
| `hooks/useSettings.ts` | Settings state management hook |
| `stores/settingsStore.ts` | Zustand store for settings |
| `bindings.ts` | Auto-generated Tauri type bindings (via tauri-specta) — do not edit manually |
| `overlay/` | Recording overlay window entry point |
| `lib/types.ts` | Shared TypeScript type definitions |

### Core Libraries

| Library | Role |
| --- | --- |
| `whisper-rs` | Local Whisper inference with GPU acceleration |
| `cpal` | Cross-platform audio I/O |
| `vad-rs` | Voice Activity Detection |
| `rdev` | Global keyboard shortcuts |
| `rubato` | Audio resampling |
| `rodio` | Audio playback for feedback sounds |

### Settings System

Persisted via Tauri's store plugin with reactive updates. Covers: keyboard shortcuts (push-to-talk supported), audio device selection, model preferences (Small/Medium/Turbo/Large), audio feedback, and translation options.

---

## 4. Review Workflow

### Step 1 — Understand First

Before proposing or making any change:

- Read the relevant files in full.
- Infer why the current implementation exists.
- Identify what problem it was solving before assuming it's wrong.

### Step 2 — Identify What to Fix

Look for, in priority order:

1. **Correctness issues** — bugs, wrong behavior, edge cases that break.
2. **Lifecycle leaks** — resources, listeners, or state that outlive their scope.
3. **Unnecessary retention** — anything held alive when it could be destroyed.
4. **Weak boundaries** — logic that crosses the frontend/backend separation or bleeds between managers.
5. **Efficiency waste** — redundant work, unnecessary allocations, avoidable overhead.
6. **Hard-to-maintain logic** — code that's correct but structurally fragile.

### Step 3 — Plan Before Acting

After understanding the domain, form a plan. For each change, ask:

- Does this respect the project constraints in §2?
- Is this targeted, or am I broadening scope unnecessarily?
- Would this harm upstream compatibility?
- Does this introduce long-lived state or coupling that shouldn't exist?

If a better approach would harm upstream compatibility or lifecycle discipline, prefer the safer approach.

### Step 4 — Execute

- Make minimal, reviewable, high-value edits.
- Prefer explicit cleanup and predictable lifecycle behavior.
- Do not introduce long-lived state, listeners, services, or coupling unless the domain genuinely requires it.

### Step 5 — Log

If a log file already exists in the project, update it. If not, create one in an appropriate location (e.g. `CHANGELOG.md` or a `logs/` directory).

Keep entries short:

```
Date: YYYY-MM-DD
Domain: <what you reviewed>
Files changed: <list>
Changes: <what and why — one line per change>
```

No long explanations unless the user asks.

---

## 5. Code Style

### Rust

- Run `cargo fmt` and `cargo clippy` before committing.
- Handle errors explicitly — avoid `unwrap` in production paths.
- Use descriptive names. Add doc comments for public APIs.

### TypeScript / React

- Strict TypeScript. Avoid `any`.
- Functional components with hooks only.
- Tailwind CSS for styling.
- Path alias: `@/` → `./src/`

### Commits

Use conventional commit prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`.
Focus the message on **why**, not what.

---

## 6. GitHub Workflow — Mandatory Before Any PR/Issue

**MANDATORY. Before opening any PR, issue, or discussion: read the relevant template and follow it strictly.** Sections that look ceremonial (checklists, AI Assistance disclosures, Human Written Description) are all required.

| Action | Rule |
| --- | --- |
| **Opening a PR** | Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section is mandatory. If a section requires a human-written paragraph, leave a clear TODO — do not invent their voice. |
| **Opening an issue** | Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled. Use `bug_report.md` for bugs. Feature requests go to Discussions, not issues. |
| **Proposing a feature** | Grain is under a **feature freeze**. New features require community support in [Discussions](https://github.com/Punit-Dethe/Grain/discussions) before any PR is opened. |
| **Translations** | Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md). |
| **Full contributor workflow** | See [CONTRIBUTING.md](CONTRIBUTING.md). |

---

## 7. Reference: Development Commands

> Look up as needed. Not required reading for every task.

**Prerequisites:** [Rust](https://rustup.rs/) (latest stable), [Bun](https://bun.sh/)

```bash
# Dependencies
bun install

# Development
bun run tauri dev
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev  # macOS cmake workaround

# Production build
bun run tauri build

# Frontend only
bun run dev       # Vite dev server
bun run build     # TypeScript + Vite build
bun run preview   # Preview built frontend

# Lint & format (run before committing)
bun run lint
bun run lint:fix
bun run format            # Prettier + cargo fmt
bun run format:check
bun run format:frontend
bun run format:backend
```

**Model setup (required for dev):**

```bash
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.Grain.computer/silero_vad_v4.onnx
```

For platform-specific build setup, see [BUILD.md](BUILD.md).
For troubleshooting, see [README.md — Troubleshooting](README.md#troubleshooting).

---

## 8. Reference: CLI Parameters

> Look up as needed.

**Implementation files:** `cli.rs` (definitions) → `main.rs` (parsing) → `lib.rs` (applying) → `signal_handle.rs` (shared logic)

| Flag | Description |
| --- | --- |
| `--toggle-transcription` | Toggle recording on/off on a running instance |
| `--toggle-post-process` | Toggle recording with post-processing on/off |
| `--cancel` | Cancel the current operation on a running instance |
| `--start-hidden` | Launch without showing the main window (tray icon visible) |
| `--no-tray` | Launch without system tray (closing window quits the app) |
| `--debug` | Enable debug mode with verbose (Trace) logging |

**Design decisions:**
- CLI flags are runtime-only overrides — they do **not** modify persisted settings.
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits.
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI.

---

## 9. Reference: Internationalization (i18n)

> Only relevant when adding or modifying user-facing strings.

All user-facing strings must use i18next. ESLint enforces this — no hardcoded strings in JSX.

**Adding new text:**
1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

```
src/i18n/
├── index.ts
├── languages.ts
└── locales/
    ├── en/translation.json   ← source of truth
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
```

For translation contribution, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

---

## 10. Reference: Platform Notes & Debug Mode

> Only relevant when behavior is platform-specific.

| Platform | Notes |
| --- | --- |
| **macOS** | Metal acceleration. Accessibility permissions required for keyboard shortcuts. |
| **Windows** | Vulkan acceleration. Code signing required. |
| **Linux** | OpenBLAS + Vulkan. Limited Wayland support. Overlay uses GTK layer shell — disable with `Grain_NO_GTK_LAYER_SHELL=1`. |

**Debug mode:** `Cmd+Shift+D` (macOS) / `Ctrl+Shift+D` (Windows/Linux)

<!-- code-review-graph MCP tools -->
## MCP Tools: code-review-graph

**IMPORTANT: This project has a knowledge graph. ALWAYS use the
code-review-graph MCP tools BEFORE using Grep/Glob/Read to explore
the codebase.** The graph is faster, cheaper (fewer tokens), and gives
you structural context (callers, dependents, test coverage) that file
scanning cannot.

### When to use graph tools FIRST

- **Exploring code**: `semantic_search_nodes` or `query_graph` instead of Grep
- **Understanding impact**: `get_impact_radius` instead of manually tracing imports
- **Code review**: `detect_changes` + `get_review_context` instead of reading entire files
- **Finding relationships**: `query_graph` with callers_of/callees_of/imports_of/tests_for
- **Architecture questions**: `get_architecture_overview` + `list_communities`

Fall back to Grep/Glob/Read **only** when the graph doesn't cover what you need.

### Key Tools

| Tool | Use when |
| ------ | ---------- |
| `detect_changes` | Reviewing code changes — gives risk-scored analysis |
| `get_review_context` | Need source snippets for review — token-efficient |
| `get_impact_radius` | Understanding blast radius of a change |
| `get_affected_flows` | Finding which execution paths are impacted |
| `query_graph` | Tracing callers, callees, imports, tests, dependencies |
| `semantic_search_nodes` | Finding functions/classes by name or keyword |
| `get_architecture_overview` | Understanding high-level codebase structure |
| `refactor_tool` | Planning renames, finding dead code |

### Workflow

1. The graph auto-updates on file changes (via hooks).
2. Use `detect_changes` for code review.
3. Use `get_affected_flows` to understand impact.
4. Use `query_graph` pattern="tests_for" to check coverage.
