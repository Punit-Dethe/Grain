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

## Phase 3: The Native UI — ~~Floem Multi-Process~~ CANCELLED → enhance the Tauri overlay
**PIVOT (2026-07-11): Floem abandoned — it peaked at ~330 MB RAM, worse than the Tauri webview and against Grain's low-RAM mandate. `crates/grain-editor` removed.** Instead we keep and enhance the EXISTING Tauri Grain Space overlay (already create-on-summon / destroy-on-close = zero idle RAM) into the Mem/Obsidian three-pane workspace. Full plan: **`TAURI-OVERLAY-PLAN.md`**. The legacy Tauri Note UI is NOT sunset — it becomes the product.
- [x] ~~**Architecture Setup:** Floem `crates/grain-editor`.~~ Built and removed (330 MB RAM — the reason Floem existed, gone).
- [ ] **Three-pane shell:** sidebar (Pinned / Notes / Collections) · editor · toggleable chat rail — in the existing Tauri overlay.
- [ ] **Collections:** backend `NoteCard { note, collection }` listing so the sidebar can group notes by their vault subfolder (locked `Note` untouched).
- [ ] **Live Preview Editor:** markdown live-preview in the webview editor (`pulldown-cmark`/a JS markdown lib) — deferred, additive.
- [ ] **Real-time Two-Way Sync:** already shipped for grain-owned notes (`safe_write` + `diffy` 3-way merge, V3). Foreign-note editing is Phase 4.
- [ ] **Chat rail:** scaffold now (slides in/out, non-functional); wire to Recall later.
- [ ] **Pinning & Docking:** pin a note overlay on screen — deferred, additive.

## Phase 4: Advanced Editing & Categorization
- [ ] **AI Prompting Upgrade:** Update the AI system prompts so it understands the YAML frontmatter schema and generates perfectly formatted Obsidian notes natively.
- [ ] **Write Access to Foreign Notes:** Implement an AST parser (`pulldown-cmark`) to safely append text (e.g., tasks) into complex, user-created Obsidian notes without breaking their formatting.
- [ ] **AI Auto-Categorization (The Metadata Auto Classifier):** When a user captures a note, inject the vault's existing folder/tag taxonomy into the LLM context. Have the AI automatically generate the correct `tags:` and move the file into the correct subfolder.
- [ ] **AI Formatting Tiers:** Allow the user to select how complex they want AI-generated notes to be (Minimal vs. Full Obsidian Power with Dataview queries/callouts).


