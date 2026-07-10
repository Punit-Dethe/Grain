# Grain Space: The Obsidian Pivot Roadmap

This document serves as the master roadmap for the transition of Grain Space into an Obsidian-native, zero-friction interface. It captures the complete vision to ensure no feature is forgotten across AI agent handoffs (FableFi / Opus).

## Phase 1: The Foundation (Completed by FableFi)
- [x] **Backend Abstraction:** Dynamic switching between Native JSON Store and Obsidian Store.
- [x] **Basic Markdown Parsing:** `vault.rs` capable of reading `.md` files and extracting YAML frontmatter.
- [x] **Safe Writes:** Captures land as new atomic `.md` files in a `Grain/` folder.
- [x] **Foreign File Safety:** Files created by the user in Obsidian are strictly **Read-Only** in v1.
- [x] **Lazy Reconcile Indexing:** Scanning the vault on-demand to update the SQLite search index (zero idle RAM).

## Phase 2: Format Unification (COMPLETE — 2026-07-11, see EXECUTION-PLAN.md P1)
- [x] **Sunset the JSON Store:** Convert the `Grain Native` store to use the exact same Markdown + YAML frontmatter format as Obsidian.
- [x] **One-Time Data Migration:** Write a script to automatically convert existing user `.json` notes into the new `.md` format. *(Implemented in-app: idempotent lazy migration on first resolve; originals preserved in `notes-json-backup/`.)*
- [x] **Unified Codebase:** Remove all JSON parsing logic from the frontend and backend. Both backends now just point to different folders of `.md` files. *(The frontend never parsed note JSON — the `Note` wire type over Tauri commands is unchanged; JSON survives only as the export/backup format.)*

## Phase 3: The Native UI (Floem Multi-Process Architecture)
*(Note: We deliberately skip updating the legacy Tauri Note UI here to save time, as it will be sunset by this phase).*
- [~] **Architecture Setup:** Create a new separate executable (`crates/grain-editor`) using **Floem**. The background Pill remains purely Native/Slint (~4MB), and the Floem editor process is spawned on demand via IPC. *(Milestone 1 done 2026-07-11: crate scaffolded, floem 0.2.0 pinned, window + text_editor buffer builds and runs on Windows, pulldown-cmark parse step proven. IPC spawn still open.)*
- [ ] **Live Preview Editor:** Drop in `floem::views::text_editor` and integrate `pulldown-cmark` for AST parsing.
- [ ] **Decoration Layer:** Map the Markdown parser's AST to Floem's text-layout attributes (spans) for live syntax highlighting.
- [ ] **Cursor-Gating:** Implement logic to collapse markdown syntax (hide `**` or `#`) unless the user's cursor is on that specific line.
- [ ] **Real-time Two-Way Sync:** Implement the targeted `notify` file watcher for the active note, and integrate a `diff-match-patch` library (like `similar`) to handle exact-millisecond write conflicts.
- [ ] **Pinning & Docking:** Allow users to pin a note overlay to their screen for quick reference without keeping Obsidian open.

## Phase 4: Advanced Editing & Categorization
- [ ] **AI Prompting Upgrade:** Update the AI system prompts so it understands the YAML frontmatter schema and generates perfectly formatted Obsidian notes natively.
- [ ] **Write Access to Foreign Notes:** Implement an AST parser (`pulldown-cmark`) to safely append text (e.g., tasks) into complex, user-created Obsidian notes without breaking their formatting.
- [ ] **AI Auto-Categorization (The Metadata Auto Classifier):** When a user captures a note, inject the vault's existing folder/tag taxonomy into the LLM context. Have the AI automatically generate the correct `tags:` and move the file into the correct subfolder.
- [ ] **AI Formatting Tiers:** Allow the user to select how complex they want AI-generated notes to be (Minimal vs. Full Obsidian Power with Dataview queries/callouts).


