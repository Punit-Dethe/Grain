# Grain Space — Transition Log

Newest entry first. Each entry assumes the reader has ZERO context: read
`FINAL-PLAN.md` in this folder first, then the top entry here, then continue.

---

## 2026-07-06 — Session 1: plan finalized, Phase 1 backend COMPLETE

### Status snapshot
- [x] FINAL-PLAN.md written (authoritative; includes user's directive 11: reuse
      the Agent Pill workflow components for voice/typing UI states)
- [x] **Phase 1 — Core storage & capture: DONE (backend only, no UI yet)**
- [ ] Phase 2 — Settings UI ("Grain Space" tab)  ← **NEXT**
- [ ] Phase 3 — Overlay window (Raycast-style two-pane)
- [ ] Phase 4 — Semantic search (opt-in BGE-small via Candle)
- [ ] Phase 5 — Voice-first retrieval
- [ ] Phase 6 — Hardening/polish

### What was done this session
1. **Plan:** `docs/Grain space files/FINAL-PLAN.md` — read it in full before
   touching code. The Raycast screenshot is the overlay UI reference; the old
   3D-carousel idea and `temp/essential_space_prototype.html` are BOTH dead.
2. **Settings (`crates/grain-core/src/settings.rs`):** added
   `grain_space_enabled` (master gate, default OFF), `grain_space_semantic`,
   `grain_space_auto_reminders`, `grain_space_retrieval_mode`
   (`GrainSpaceRetrievalMode::List|AiQa`), `grain_space_decay_half_life_days`
   (30). Two new bindings seeded via the existing missing-bindings migration:
   `grain_space_quick_add` (ctrl+shift+c / cmd+shift+c) and
   `grain_space_capture` (ctrl+alt+n / option+shift+n).
3. **Gating:** both shortcut backends (`shortcut/tauri_impl.rs`,
   `shortcut/handy_keys.rs`) skip `grain_space_*` ids at init when the feature
   is off. `shortcut::change_grain_space_enabled_setting` (in
   `shortcut/mod.rs`, specta-registered in `lib.rs`) flips the setting AND
   registers/unregisters the shortcuts live.
4. **Store (`src-tauri/src/grain_space/store.rs`):** locked-schema `Note` JSON
   files under `{app_data_dir}/grain_space/notes/{uuid}.json` (atomic
   tmp+rename writes); derived `index.sqlite` (journal_mode=TRUNCATE, FTS5
   `notes_fts` + `notes_meta` with `embed_stale`); one app-wide `STORE_LOCK`
   Mutex; per-op connections (zero idle RAM); `rebuild_index()` recovery;
   FTS5 prefix search with quoted-term escaping + substring fallback.
   sqlite-vec registered as auto-extension (vec0 table itself is Phase 4).
   **FTS5 confirmed working in rusqlite `bundled`** (tests prove it).
5. **Commands (`grain_space/commands.rs`,** registered in `lib.rs`**):**
   `grain_space_list_notes / search_notes / get_note / save_note / create_note
   / delete_note / set_pinned / rebuild_index`. All gate on the master toggle
   and run store work via `spawn_blocking`. Mutations emit
   `grain-space://notes-changed` (const in `grain_space/mod.rs`).
6. **Input C (`grain_space/capture.rs::quick_add`):** tap action
   (`GrainSpaceQuickAddAction` in `actions.rs`) → reuses
   `agent::capture_selection` (made `pub(crate)`) on a spawned thread →
   raw note, silent no-op on empty selection, 500 ms debounce.
7. **Inputs A/B:** `grain_space_capture` added to `is_transcribe_binding`
   (`transcription_coordinator.rs`) so it shares the serialized record
   lifecycle, and registered in `ACTION_MAP` as a `TranscribeAction`
   (post_process=false → the user's rewriting prompt is never applied to
   notes). Interception point: in `TranscribeAction::stop`'s success branch in
   `actions.rs`, `binding_id == "grain_space_capture"` routes
   `processed.final_text` to `capture::intake_transcript` instead of paste.
   Intake: if a usable HTTP post-process provider exists (Input A) → ONE
   structured call (`llm_client::send_chat_completion_with_schema`, strict
   JSON: title/tldr/todos/reminder_at with local-now injected in the system
   prompt); reminder auto-armed or parked per `grain_space_auto_reminders`;
   ANY failure degrades to raw save (Input B). Body is ALWAYS the verbatim
   transcript. Apple Intelligence provider currently degrades to raw
   (deliberate Phase-1 simplification, revisit in Phase 6); smart-rotation
   also not used for extraction (single active provider only).
8. **Deps added (`src-tauri/Cargo.toml`):** `sqlite-vec = "0.1"`,
   `uuid = { v1, ["v4"] }`.
9. **Tests:** 10 grain_space unit tests (store roundtrip, locked-schema guard,
   FTS search + odd queries, delete, rebuild recovery, id validation, pin
   without embed-stale, metadata apply, bad reminder string, fence stripping).
   Full `cargo test --lib` in src-tauri: **154 passed**. grain-core: 4 passed.

### Next concrete step (Phase 2)
Build the "Grain Space" settings tab:
1. Add a section to `SECTIONS_CONFIG` in `src/components/Sidebar.tsx` (always
   visible, like `experimentations`), new component folder
   `src/components/settings/grain-space/`.
2. Top of tab: master toggle → call `commands.changeGrainSpaceEnabledSetting`
   (regenerate `src/bindings.ts` by running the dev app once — the new
   commands aren't in the committed bindings.ts yet).
3. Reminders section (notes with `reminder_state.status ∈ {pending, armed}`)
   + notes list grouped by local date; listen for `grain-space://notes-changed`.
4. Semantic toggle UI (setting only — download flow is Phase 4) + reminder
   scheduler (`grain_space/reminders.rs`, single timer armed to earliest
   `fire_at`, notification on fire, re-arm on mutation; NOTHING resident when
   feature off or no armed reminders).
5. i18n: English keys only; other locales fall back.

### Gotchas discovered
- Repo has TWO cargo workspaces: repo root (crates/*, target-dir `C:\t`) and
  `src-tauri` (package name **`handy`**, target-dir `C:\gt`). Run
  `cargo check --lib` / `cargo test --lib` from INSIDE `src-tauri`.
- PowerShell here doesn't support `&&`; native stderr shows as fake
  NativeCommandError noise — check the actual cargo tail output.
- `sqlite3_auto_extension` needs a plain `std::mem::transmute(...)` cast of
  `sqlite_vec::sqlite3_vec_init as *const ()` (typed transmute annotations
  fail the expected fn-pointer arity).
- FTS5 MATCH: quote every term (`"term"*`), drop punctuation-only terms, or
  user input like `(` errors the query (fallback path covers the rest).
- The user's working tree had unrelated deletions (old `temp/` prototype,
  old `docs/TRANSITION-LOG.md`) and a `src/bindings.ts` edit NOT from this
  work — left unstaged on purpose.
- `temp/essential_space_prototype.html` (now deleted) must NOT be used as a UI
  reference — user explicitly rejected it mid-session.
