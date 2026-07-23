# Grain Space — Tauri Overlay Enhancement Plan (roadmap Phase 3, redone)

> **PIVOT (2026-07-11): Floem is dead.** The `grain-editor` Floem process
> peaked at ~330 MB RAM — worse than the Tauri webview it was meant to
> replace, and a direct violation of Grain's edge-device, low-RAM mandate.
> The whole `crates/grain-editor` experiment is removed. Roadmap Phase 3
> ("Native UI, Floem multi-process") is **cancelled as written**. We keep
> and enhance the store we already ship: the **Tauri Grain Space overlay
> window**, which is created-on-summon / destroyed-on-close and therefore
> holds **zero idle RAM** — the property Floem was supposed to give us and
> didn't.
>
> This plan turns that overlay into the Mem/Obsidian-style workspace the
> screenshot inspiration shows. Design language first; the fancy extras
> (timers, calendars, reminders surfaces, templates, "Heads Up") are
> explicitly deferred. **No implementation this session — plan only.**

## Where we're starting from

The overlay today (`src/components/grain-space/GrainSpaceOverlay.tsx` +
`grain-space.css`, window in `src-tauri/src/grain_space/window.rs`):

- **Two panes:** a header (brand + search + exact/semantic toggle + new +
  close) over a body of `list | editor`. The list is date-grouped
  (Pinned → Today → Yesterday → dated); the editor is a real editor
  (title, tldr, growing body textarea, todo checkboxes, and a bottom-right
  reminder · pin · delete action row).
- **Window:** 840×560, frameless, transparent, rounded, resizable,
  taskbar-reachable, NOT always-on-top; created on summon, destroyed on
  close/Esc (zero idle RAM). Loads no other app CSS.
- **Working plumbing we KEEP as-is:** debounced save-on-change (600 ms) +
  flush-on-blur/close/switch, draft-then-create id adoption, FTS + opt-in
  semantic search, the model-download consent banner, pin/reminder/delete,
  live refresh on `grain-space://notes-changed`.

What it is NOT yet: it doesn't look like the inspiration (no sidebar, no
collections, no chat rail), and it can't show collections because the
`Note` wire type carries no folder.

## Target shape (from the inspiration, trimmed to essentials)

A three-column workspace in the same warm-paper language:

```
┌────────────┬───────────────────────────┬──────────────┐
│  SIDEBAR   │        EDITOR             │  CHAT (scaffold) │
│  brand     │  (title / body / todos)  │  toggle to      │
│  + Create  │                          │  slide in/out   │
│  Pinned    │                          │  skeleton only  │
│  Notes     │                          │                 │
│  Collections (expandable, nested)     │                 │
└────────────┴───────────────────────────┴──────────────┘
```

- **Sidebar** replaces the flat date-grouped list: brand/header, a Create
  note button, then **Pinned**, **Notes** (loose notes), and
  **Collections** (each collection expands to its member notes).
- **Editor** is the current editor, cleaned up and given the paper-sheet
  treatment; a slim top strip carries search + the chat toggle.
- **Chat** is a right rail that **slides in/out and does nothing** — pure
  scaffolding (skeleton bubbles, a disabled input, a "scaffold" tag), by
  explicit instruction. It is a placeholder for a future Recall-in-panel.

Everything else in the screenshot (Heads Up, templates/checklist/table
toolbar, tabs, voice, calendars/timers) is **out of scope for now**.

---

## Phase A — Backend: surface a note's collection (the only non-cosmetic change)

The sidebar's Collections section needs to know which folder each note
lives in. The locked `Note` schema (id/title/tldr/body/timestamp/todos/
reminder/pinned) has no folder field and must NOT gain one. So add a
**listing-only wrapper**, leaving `Note` untouched:

```rust
// note.rs (specta type; the frontend's list wire type)
pub struct NoteCard {
    pub note: Note,
    pub collection: Option<String>, // None = loose (shown under "Notes")
}
```

- **New command** `grain_space_list_cards() -> Vec<NoteCard>` (or extend
  `grain_space_list_notes`; a new command is cleaner and leaves the old one
  for callers that only need `Note`). The vault backend already knows each
  note's vault-relative path in `list_notes`; it just also returns the
  derived collection.
- **Collection derivation (recommended default — one decision to confirm):**
  a note's collection = **its immediate parent folder's name, unless that
  parent is the vault root or Grain's writable home folder** (then `None`).
  - obsidian backend (home = `<vault>/Grain`):
    `Grain/Wifi.md` → `None` (Notes); `Grain/Work/Standup.md` → `"Work"`;
    a promoted `Projects/Roadmap.md` → `"Projects"`; a root `Foo.md` → `None`.
  - native backend (flat `…/grain_space/notes/`): always `None` — the
    native store has no subfolders, so everything is under "Notes". (Nested
    collections there are a later, additive feature.)
- Search results stay flat `Note` (a search is not a browse) — the sidebar
  Collections view is a browse-only concept.
- **No new persistence, no schema change, no migration** — collection is
  derived from the path at list time. `NoteCard` is regenerated into
  `bindings.ts`.

Scope: ~1 new type, 1 command, a few lines in `vault.rs::list_notes`'s
caller. This is the whole backend footprint of the redesign.

## Phase B — Layout shell (frontend structure)

- **Window (`window.rs`):** bump `WINDOW_W/H` to ~**1120×740** (min
  ~860×560) so three columns breathe. Keep frameless / transparent /
  rounded / resizable / destroy-on-close. The top strip stays the
  `data-tauri-drag-region`.
- **Split `GrainSpaceOverlay.tsx`** (currently one component) into:
  `GrainSpaceOverlay` (shell + shared state/effects) → `Sidebar`,
  `Editor`, `ChatRail`. All the existing save/select/refresh/delete/pin
  logic moves into the shell and is passed down; **no behavior rewrite**,
  just relocation.
- **Sidebar** consumes `NoteCard[]`:
  - Header: brand + Create note button (reuse `newNote`).
  - **Pinned:** cards with `is_pinned`.
  - **Notes:** cards with `collection === null`, newest first.
  - **Collections:** group by `collection`, each a collapsible row
    (name + count) that expands to its member note rows. Expanded-set in
    component state. Selecting a row reuses `selectNote`.
  - Selection highlight is per-row (no list rebuild on select).
- **Top strip** over the editor+chat area: the search input (FTS +
  exact/semantic toggle, moved out of the old header) on the left/center,
  and the **chat toggle** on the right. Close/new-note move here too.

## Phase C — Chat rail (scaffold only, non-functional)

- A right column, ~300 px, wrapped in an `overflow:hidden` shell whose
  width animates 0 ↔ 300 px on toggle (CSS `transition: width`), so it
  slides in and out. The inner panel is fixed-width so its content never
  reflows mid-slide.
- Contents are inert: a "Chat" header with a small **scaffold** tag,
  a few alternating skeleton message blocks, and a disabled
  "Ask your Grain…" input. **No commands, no state, no Recall wiring** —
  it exists to establish the rail and the motion, nothing more.

## Phase D — Design language (where "looks like the image" lands)

- Consolidate the palette into a warm-paper system (evolve the existing
  `grain-space.css` variables, don't restart): cream app background,
  a slightly deeper sidebar wash, white editor "sheet", `--ink` text,
  the existing `--orange` as the single accent, hairline borders, soft
  radii, generous spacing. Pinned/active rows get the orange-soft wash.
- Section headers, count badges, collection `#name` chips, and the
  create-note button all pull from this system so the three panes read as
  one surface — the Obsidian/Mem "one coherent workspace" feel the user is
  after.
- Keep it self-contained (this window still loads no other app CSS).

## Phase E — Verify

- `tsc` + eslint clean, `bindings.ts` regenerated for `NoteCard` +
  `grain_space_list_cards`, `cargo test --lib` green (the backend change is
  tiny and testable: collection derivation for root/home/subfolder/promoted
  paths on both backends).
- Visual pass in the running app against a real Obsidian vault (the user
  drives this — it can't be verified headlessly): sidebar sections,
  collection expand/collapse, note open/edit/save round-trip still works,
  chat rail slides, search still filters.

---

## Decisions to confirm before implementing

1. **Collection mapping** (Phase A): the "immediate parent folder unless
   it's the vault root or the Grain home folder" rule above — confirm, or
   pick "first path segment from the vault root" (which would lump every
   default capture under a single "Grain" collection — probably not what
   you want).
2. **Window size / feel:** ~1120×740 frameless rounded card, or should it
   feel like a full titled app window? (Frameless keeps the premium
   floating look and the drag region we already have.)
3. **Search placement:** top strip over the editor (Mem-like center), or in
   the sidebar header (Obsidian-like)? Plan assumes the top strip.

## Explicitly deferred (additive, later)

- Timers, calendars, reminder surfaces beyond the existing action row.
- Real chat (wire the rail to the Recall pipeline) and "Heads Up".
- Tabs for multiple open notes; templates/checklist/table toolbar; voice.
- Nested collections on the native backend; drag-to-collection; promote UI.
- The pill-daemon IPC spawn (moot now — the overlay is already a Tauri
  window summoned by the existing `grain_space_open` binding).
