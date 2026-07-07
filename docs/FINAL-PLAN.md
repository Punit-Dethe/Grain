# Grain Space — Final Implementation Plan

> **This is the authoritative plan.** `basicplan.md` was the seed (it is now empty —
> its content lives in section 2 as "directives"). `modelinfo.md` documents the
> chosen embedding model. `TRANSITION-LOG.md` tracks cross-session progress —
> **read it right after this file if you are a fresh session.**

---

## 1. What Grain Space is

A local, zero-idle-RAM "essential space" for notes inside Grain (the Tauri ASR
app). Users capture notes by voice (dictation), by hotkey from selected text, or
by typing in an overlay. Notes get an optional AI title/TLDR/reminder extraction
(only if the user has a BYOM post-process provider configured). Retrieval is
fuzzy/FTS by default, semantic (local embedding model) as an opt-in, and
voice-first as the end game.

**UI reference:** the Raycast Notes window (screenshot shared 2026-07-06):
a floating window with a search field on top, a left column of notes grouped by
date, and a right pane. **Deviations from Raycast:** the right pane is the note
itself (a full editor) — NOT a metadata/"information" panel. Any extra
interactions (pin, delete, reminder, append-via-voice) live in a compact action
row at the bottom-right of the note pane.
**Do NOT use `temp/essential_space_prototype.html` as a UI reference — the user
explicitly rejected it.** The old "3D carousel" idea from the basic plan is
**superseded** by this Raycast-style two-pane layout.

---

## 2. Strict directives (non-negotiable, from the user)

1. **Zero idle RAM.** 0 MB when the feature's surfaces are closed. Overlay is a
   create/destroy Tauri window. SQLite connections open per operation and drop.
   No resident background services for Grain Space.
2. **Feature-disabled = nothing loads.** If `grain_space_enabled == false`:
   no DB opens, no models load, no listeners register, no shortcuts bind.
   Physical data files are NEVER deleted by disabling.
3. **Storage:** flat JSON files on disk are the source of truth; `sqlite-vec`
   holds vectors; SQLite is a *derived, rebuildable* index.
4. **Concurrency:** NO WAL. `PRAGMA journal_mode=TRUNCATE;`. One application-wide
   `Mutex` serializes every store read/write.
5. **Schema lock (JSON note file):** exactly
   `id, title, tldr, body, timestamp, todo_tags, reminder_state, is_pinned`.
   **Never store embeddings in JSON.**
6. **Model is opt-in, never shipped.** Enabling semantic search prompts a
   consent dialog, then downloads BGE-small-en-v1.5 (see `modelinfo.md`). People
   who don't need it never download it.
7. **Model lifecycle (overrides modelinfo.md's "never unload"):** load lazily on
   the first semantic search of a session, keep warm while the overlay window is
   open, **drop the instant the window is destroyed**. No idle timers.
8. **Phase by phase.** Do not code future phases before the current is complete.
9. **Transition logs.** On every stop, update `TRANSITION-LOG.md` assuming the
   next session has zero context. Also log via SQLite MCP (domain: `space`).
10. **Git:** commit + push when a task completes. No AI attribution, no
    Co-authored-by. Never touch git identity.
11. **UI Component Reuse (Agent Pill):** For voice recording and text input states, explicitly REUSE the existing frontend Agent Pill workflow components (which perfectly handle the transitions between recording, transcribing, and writing). Adapt and reuse these components instead of building new voice UI states from scratch.
12. **Phase 3 & 4 SCRAP LIST:** When building the manual Overlay Browser, completely SCRAP the "Append via Voice" feature and SCRAP the "Ask AI" feature from the search box. The manual overlay is strictly for fuzzy/semantic searching and manual note editing.

---

## 3. Architecture

### 3.1 On-disk layout

```
{app_data_dir}/grain_space/
  notes/{uuid}.json        ← source of truth, one file per note (locked schema)
  index.sqlite             ← derived: FTS5 + sqlite-vec + staleness flags
```

`index.sqlite` schema:

```sql
PRAGMA journal_mode=TRUNCATE;

CREATE TABLE IF NOT EXISTS notes_meta (
  id            TEXT PRIMARY KEY,   -- uuid, matches JSON filename
  timestamp     INTEGER NOT NULL,   -- epoch ms (mirror for sorting w/o file reads)
  is_pinned     INTEGER NOT NULL DEFAULT 0,
  embed_stale   INTEGER NOT NULL DEFAULT 1  -- 1 = needs (re-)embedding
);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
  id UNINDEXED, title, tldr, body
);

-- Created only once semantic search has been enabled (Phase 4):
CREATE VIRTUAL TABLE IF NOT EXISTS notes_vec USING vec0(
  id TEXT PRIMARY KEY,
  embedding float[384]
);
```

Rebuild rule: `rebuild_index()` wipes the SQLite tables and re-derives them from
the JSON files (embeddings marked stale). This is the recovery path for any
index corruption — JSON is never derived from SQLite.

### 3.2 Note JSON schema (LOCKED)

```json
{
  "id": "uuid-v4",
  "title": "3-word title or \"\"",
  "tldr": "1-sentence summary or \"\"",
  "body": "full text; may contain <todo>…</todo> markup",
  "timestamp": 1751800000000,
  "todo_tags": [{ "text": "buy milk", "done": false }],
  "reminder_state": { "status": "none|pending|armed|fired|dismissed", "fire_at": null },
  "is_pinned": false
}
```

### 3.3 Rust modules (all inside the tauri crate)

```
src-tauri/src/grain_space/
  mod.rs        ← gate helpers (enabled checks), init-nothing-by-default
  store.rs      ← JSON I/O + SQLite index + global Mutex + rebuild
  capture.rs    ← Inputs A/B/C glue (selection grab, transcript intake, LLM extract)
  commands.rs   ← #[tauri::command] surface (specta-registered in lib.rs)
  reminders.rs  ← single-timer scheduler (Phase 2/3)
  embed.rs      ← Candle engine thread (Phase 4 ONLY — do not create earlier)
```

Concurrency primitive: `static STORE_LOCK: Mutex<()>` in `store.rs`; every
public store fn takes the guard, opens `Connection`, does its work, drops both.

### 3.4 Existing infrastructure to REUSE (do not reinvent)

| Need | Reuse |
|---|---|
| Settings schema/defaults/migration | `crates/grain-core/src/settings.rs` (`AppSettings`, `merge_missing_bindings`-style migration at `~line 1146`) |
| Shortcut → action dispatch | `ShortcutAction` trait + `ACTION_MAP` in `src-tauri/src/actions.rs` (~line 1672); handler in `src-tauri/src/shortcut/handler.rs` |
| Selected-text capture (Input C) | the Agent's selection-capture path in `src-tauri/src/agent.rs` |
| Transcription pipeline (Inputs A/B) | `TranscribeAction` flow in `actions.rs` — add a *destination interceptor*, not a new engine (quality standard #4) |
| LLM structured call (Input A) | existing post-process/LLM client (`llm_client.rs`, structured-output path in `actions.rs`, provider selection via `post_process_router.rs`) |
| rusqlite patterns | `src-tauri/src/managers/history.rs` (open-per-op connections, migrations, tests with in-memory conns) |
| Model downloads with progress/cancel | hf-hub fork already in `src-tauri/Cargo.toml` + `managers/model.rs` download/progress-event pattern |
| Window create/destroy | `agent.rs` webview panel lifecycle; **window create/resize commands must stay async** (tauri#3990 freeze) |
| Settings UI tab registry | `SECTIONS_CONFIG` in `src/components/Sidebar.tsx`; tab components in `src/components/settings/*` |

### 3.5 New settings (grain-core `AppSettings`)

```rust
#[serde(default)] pub grain_space_enabled: bool,            // master gate, default false
#[serde(default)] pub grain_space_semantic: bool,           // semantic search gate, default false
#[serde(default = "default_true")] pub grain_space_auto_reminders: bool,
#[serde(default)] pub grain_space_retrieval_mode: GrainSpaceRetrievalMode, // AiQa | Carousel(list)
#[serde(default = "default_grain_space_decay_half_life_days")] pub grain_space_decay_half_life_days: u32, // 30
```

New bindings (merged via the existing missing-bindings migration):
- `grain_space_quick_add` — `ctrl+shift+c` (macOS: `cmd+shift+c`) — Input C, tap.
- `grain_space_capture` — voice-note dictation, push-to-talk/toggle like transcribe.
- `grain_space_open` — open/close the overlay window, tap.
- `grain_space_voice_search` — Phase 5, tap. **Register in Phase 5, not before.**

Gating rule: shortcut registration for these ids is skipped entirely when
`grain_space_enabled == false` (and re-evaluated on settings change, same as
other dynamic bindings).

### 3.6 New dependencies

| Phase | Crate | Notes |
|---|---|---|
| 1 | `sqlite-vec` | registers via `sqlite3_auto_extension` against rusqlite's bundled ffi ([guide](https://alexgarcia.xyz/sqlite-vec/rust.html)). Add in Phase 1 so the extension is wired once, table created in Phase 4. |
| 1 | `zerocopy` | pass `&[f32]` to sqlite-vec without copies |
| 1 | `uuid` (v4) | check if already transitively present; add feature `v4` |
| 4 | `candle-core`, `candle-nn`, `candle-transformers` | BERT support is first-class; CPU only |
| 4 | `tokenizers` | HF tokenizer for BGE |

FTS5: verify rusqlite `bundled` compiles FTS5 in (add feature flag if needed);
fallback if unavailable = `LIKE`-based fuzzy match (the Phase 2 "no-semantic"
path must work regardless).

---

## 4. Phases (importance-ordered; each ends with commit + transition log)

### Phase 1 — Core storage + capture (THE core; everything depends on it)

1. **Settings & gating** — fields + bindings above; sidebar tab placeholder NOT
   yet; verify disabled ⇒ no shortcut registration, no directory creation.
2. **Store** — `store.rs`: create dirs lazily on first *write*; note CRUD
   (create/read/update/delete/toggle-pin/list-by-date); FTS index kept in sync
   on every write; TRUNCATE pragma; global Mutex; `rebuild_index()`;
   unit tests following `managers/history.rs` test style.
3. **Input C (quick add)** — tap action: capture current selection (reuse agent
   mechanism), save silently as raw note (blank title/tldr), body = selection.
   Debounce double-fires (~500 ms). Empty selection ⇒ no-op (no empty notes).
4. **Inputs A/B (voice note)** — `grain_space_capture` records + transcribes via
   the existing pipeline, but the transcript is *intercepted* to the store
   instead of pasted.
   - **A (BYOM configured & post-process enabled):** one structured LLM call —
     system prompt: *"Generate 3-word title, 1-sentence TLDR, and extract
     reminders/timers."* Parse strict JSON `{title, tldr, todos:[…],
     reminders:[…]}`. **On any LLM failure, degrade to Input B** (raw save) —
     capture must never lose audio-derived text.
   - **B (no LLM):** raw body, `title = ""`, `tldr = ""`.
5. **Commands** — specta-registered: `grain_space_list_notes`,
   `get_note`, `save_note`, `delete_note`, `toggle_pin`, `rebuild_index`.
   Every command early-returns if the feature is disabled.

**Acceptance:** JSON files appear/round-trip; app restart preserves; disabling
the feature makes every entry point inert; `cargo test` green.

### Phase 2 — Settings UI ("Grain Space" tab)

1. New sidebar section (always visible, like Experimentations) with the master
   enable toggle at top; everything below disabled/hidden until on.
2. Top section: Reminders/Timers (list of notes with `reminder_state.status ∈
   {pending, armed}`, quick dismiss/complete). Bottom: notes grouped by date
   (Today / Yesterday / date), click opens the overlay focused on that note.
3. **Global semantic toggle** (`grain_space_semantic`) with the model-consent
   copy. In this phase the toggle only flips the setting and shows "model will
   download on first use" copy — the download flow itself is Phase 4. Semantic
   OFF ⇒ fuzzy/FTS only, embedding model must NEVER load.
4. `grain_space_auto_reminders` toggle + shortcut rows for the new bindings.
5. Reminder scheduler (`reminders.rs`): one timer armed to the earliest
   `fire_at`; re-armed on any reminder mutation and on app start *only if*
   feature enabled AND a pending reminder exists; OS notification on fire.
   No polling loop, no persistent thread when idle.
6. i18n: English keys only; other locales fall back.

### Phase 3 — Overlay window (Raycast-style)

1. **Lifecycle:** `grain_space_open` creates a webview window (route
   `/grain-space` in the same React bundle), destroy on close/Esc/blur-optional.
   All window ops async. Feature toggled off while open ⇒ close immediately.
2. **Layout:** search input top-left with back button; left column = results/
   date-grouped list; right pane = the full note (editable title + body). No
   metadata panel. Bottom-right action row: pin, reminder, delete.
3. **Editing:** debounced save-on-change to JSON via commands; `<todo>` spans
   render as live checkboxes; toggling writes `todo_tags` + body back to disk.
4. **Blank-vs-list UX rule:** no notes at all ⇒ open straight into a new blank
   note; otherwise open the list with the newest note selected. Explicit "New
   note" action (button + `ctrl+n`) always available.
5. **Reminder auto-toggle:** `grain_space_auto_reminders` ON ⇒ Input A arms
   extracted reminders automatically; OFF ⇒ note pane shows an "arm reminder"
   button instead.
7. Search box in this phase = FTS/fuzzy only.
8. Edits refresh the index synchronously (same mutex) so search stays truthful.

### Phase 4 — Semantic search (opt-in model)

1. **Install flow:** first enable of the semantic toggle (or first semantic
   search with no model on disk) ⇒ consent dialog ("downloads BGE-small-en-v1.5,
   ~34 MB, MIT") → hf-hub download (`model.safetensors`, `tokenizer.json`,
   `config.json`) into the shared HF cache with progress events + cancel.
   Never bundled with the app.
2. **Engine (`embed.rs`):** dedicated OS thread owning tokenizer+model
   (candle-transformers `BertModel`), mpsc request channel, mean-pool + L2
   normalize. Start f32 (accept ~60–130 MB transiently); INT8/quantized is a
   Phase 6 optimization. 100% concurrent with audio transcription (separate
   thread, zero shared state).
3. **Lifecycle:** lazily spawned on the first semantic query while the overlay
   is open; engine handle owned by the overlay window state; **dropped
   (thread joined, weights freed) on window-destroyed event.** No timers.
4. **Indexing:** embed text = `Title: {title}\n\nSummary: {tldr}\n\nBody:
   {body}` (blank fields omitted; truncate to 512 tokens). On save: if engine
   resident ⇒ embed inline; else set `embed_stale=1`. On engine spawn ⇒ batch
   re-embed all stale rows before serving the first query.
5. **Search modes:** UI toggle exact (FTS5) vs semantic; semantic = embed query
   → sqlite-vec KNN (cosine; vectors pre-normalized so inner product works).
6. **Ranking:** `S_final = S_semantic * exp(-λ·Δt)`, λ from
   `grain_space_decay_half_life_days` (λ = ln2/half_life); `is_pinned ⇒ Δt = 0`.

### Phase 5 — Voice-first retrieval

1. Register `grain_space_voice_search` binding; records + transcribes the query.
2. **Mode A (AI Q&A):** retrieve top-K (semantic if enabled+model else FTS),
   RAG call with system prompt *"Prioritize the most recent, relevant note for
   direct answers. Use older notes only as fallback."*; show the answer in a
   lightweight destroyable surface (reuse agent reply-panel pattern).
3. **Mode B (open list):** run the search, open the overlay with results in the
   left column, newest/best match selected in the right pane.
4. Mode chosen by `grain_space_retrieval_mode` (settings + overlay quick toggle).

### Phase 6 — Hardening & polish (additive, only after 1–5)

Quantized INT8 model; corrupted-JSON quarantine (`notes/corrupt/`); export all
notes; decay-λ setting UI; overlay multi-monitor placement; model uninstall
button; append while another capture is running; long-note chunked embedding.

---

## 5. Edge-case catalog (check against every phase)

- LLM fails / returns malformed JSON ⇒ save raw (A degrades to B). Never lose text.
- Quick-add with empty selection ⇒ silent no-op. Rapid double-press ⇒ debounce.
- Voice capture yields empty transcript ⇒ no note.
- Note JSON unreadable ⇒ skip + log; never crash the list; Phase 6 quarantines.
- `index.sqlite` corrupt/missing ⇒ auto `rebuild_index()` from JSON.
- Model download interrupted ⇒ resumable/cancellable via hf-hub fork; toggle
  stays off until verified on disk.
- Semantic toggle OFF must short-circuit *before* any engine code path.
- Feature toggle OFF while overlay open ⇒ destroy window, drop engine.
- Reminder `fire_at` in the past on arm ⇒ fire immediately or mark missed.
- Date grouping uses LOCAL timezone; timestamps stored as epoch ms UTC.
- Notes > 512 tokens: truncated embedding is acceptable (title+tldr carry
  meaning); chunking is Phase 6.
- Two writers (quick-add during open overlay) ⇒ serialized by the Mutex; overlay
  refreshes via a `grain-space://notes-changed` Tauri event.
- Never block the async runtime with model load or file I/O batches (spawn
  blocking / dedicated thread — same rule as ASR model loads).

---

## 6. Cross-session continuity protocol

1. **On every stop:** update `docs/Grain space files/TRANSITION-LOG.md` —
   sections: `Status snapshot` (per-phase checklist), `What was done this
   session` (files touched + why), `Next concrete step` (exact file + function
   to continue in), `Gotchas discovered`.
2. **SQLite MCP log** (domain `space`, ≤3 sentences, ≤3 keywords, ≤5-word anchor)
   for architecture decisions, hard bugs, discoveries, and the handoff itself.
3. Commit + push at every completed task (clean human commit messages).
4. New sessions: read this file → TRANSITION-LOG.md → continue. Do not re-plan.
