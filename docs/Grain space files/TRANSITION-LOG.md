# Grain Space — Transition Log

Newest entry first. Each entry assumes the reader has ZERO context: read
`FINAL-PLAN.md` in this folder first, then the top entry here, then continue.

---

## 2026-07-07 — Session 2 (part 8): Grain Recall R4 — export + model uninstall

Two more R4 items landed (both settings-tab, both fully compile/test-verified;
GUI look pending user QA).

1. **Export all notes** — `grain_space_export_notes` command serializes every
   note to a pretty JSON array (new pure `store::export_json`, tested) and writes
   it via the native save dialog (`tauri-plugin-dialog`, already a dep). Serializes
   BEFORE prompting (empty corpus / read failure never opens a dialog); returns
   the path or `None` on cancel. "Export all notes…" action in the Notes group.
2. **Uninstall embedding model** — `grain_space_uninstall_embed_model` drops the
   engine (mmap'd safetensors can't be deleted on Windows) then removes the whole
   `models--BAAI--bge-small-en-v1.5` HF-cache dir (`embed::uninstall_model`,
   derives the repo dir from a cached file `.ancestors().nth(3)`). Refuses
   mid-download. Quiet action under the semantic toggle, shown only when
   `semantic` is on (⇒ model present); on success the frontend flips
   `grain_space_semantic` off (visible confirmation). Commits `5673923`, `fa0298d`.

**R4 scoreboard:** DONE = corrupt-JSON quarantine, `retrieval_mode` removal,
export, model uninstall. REMAINING (unchanged reasons):
- INT8/quantized + long-note chunked embedding — need model artifacts + semantic
  KNN e2e; don't ship blind.
- Recall prompt v1 / reconcile-prompt iteration — need real usage transcripts.
- decay-λ half-life slider — small, but niche (semantic-only) and needs a new
  `change_grain_space_decay…` command; not yet built.
- overlay multi-monitor placement — needs visual verification in the running app.
- "append while another capture runs" — `STORE_LOCK` already serializes every op;
  believed satisfied, wants a targeted test to claim it.

---

## 2026-07-07 — Session 2 (part 7): Note capture moved onto the Agent pill (voice OR type + selection)

User-requested change (mid-R4), NOT part of RECALL-PLAN. Note CREATION used to be
an ordinary transcribe session on the MAIN recording pill (`grain_space_capture`
→ `is_transcribe_binding` → `TranscribeAction::stop` intake → `intake_transcript`).
Now it summons the **Agent pill** in a new **`AgentMode::Capture`**, exactly the
way Recall added a mode. This buys text input for free (type a note — good for
passwords/case-sensitive text) alongside voice, AND folds in the selection.

**Decisions (from the user, this session):**
- Selection = the note **body, verbatim** (never rewritten); the spoken/typed
  words **frame** it (title/summary only). No selection → the spoken/typed text
  IS the note. (Preserves the locked "body is verbatim" invariant.)
- **One-shot**: submit → structure → save → panel shows `Saved — "<title>"`.
  Nothing else changes functionally.
- Pill **selection chip** shows only when there IS a selection; empty = blank
  (was "No selection"). Applies to all modes (also de-noises Recall).

**What changed:**
1. `agent.rs`: `AgentMode::Capture` + `summon_capture` (= `summon_inner` with the
   mode). `summon_inner` gating refined — Assist: selection+field+paste-target;
   Capture: selection ONLY (never pastes); Recall: none. `agent_run` now
   `match`es the mode → `capture::run_capture_turn`.
2. `capture.rs`: new `run_capture_turn(app, messages, selection)` — picks body
   (selection else instruction) + framing, builds the note via the shared
   `compose_note` (renamed/extended with a `framing: Option<&str>` arg), saves,
   returns a one-shot `AgentReply::plain("Saved — …")`. `extract_metadata` took
   the same `framing` arg (a `framing_line` in the system prompt; body stays
   verbatim). Removed the now-dead `intake_transcript`.
3. `actions.rs`: new `GrainSpaceCaptureAction` (tap → `summon_capture`); ACTION_MAP
   entry swapped off `TranscribeAction`; removed the `grain_space_capture` branch
   in `TranscribeAction::stop`.
4. `transcription_coordinator.rs`: `grain_space_capture` removed from
   `is_transcribe_binding` (it no longer uses the batch record/transcribe path —
   the agent dictation lease handles STT).
5. `grain-pill/src/lib.rs`: selection chip rendered only when `selection_chars > 0`.
6. `grain-core/settings.rs`: binding display name `Dictate Note` → `Create Note`
   (+ description). `GrainSpaceSettings.tsx`: capture-group + empty-state copy.

**Verified:** `cargo build` clean (incl. grain-pill), `cargo test --lib` **172
passed**, `cargo fmt`, `tsc`, eslint clean. **Boot smoke test**: app starts, pill
supervisor + window come up, no panic. No new commands/types ⇒ `bindings.ts`
unchanged. Commit `ed59aed`.

### NEEDS USER TESTING (GUI, can't drive headlessly)
Press the Create Note shortcut (default ctrl+alt+n) with Grain Space enabled:
(a) speak → Enter → note saved; (b) type → Enter → note saved (no STT);
(c) select text elsewhere, summon, say "save this as X" → selection saved as body,
framed by the instruction; (d) pill shows the char chip only when text is
selected. Ctrl+Shift+C quick-add is unchanged.

### Gotchas
- Capture reuses the panel's paste-`Confirm` button (an Assist affordance) — it'd
  paste the "Saved" text if clicked. Left as-is (same as Recall); not wired for a
  per-mode hide. Revisit only if it confuses in testing.
- The pill placeholder still says "Ask anything…" for every mode (visual
  differentiation deferred, per the Recall precedent).

---

## 2026-07-07 — Session 2 (part 6): Grain Recall R4 STARTED (hardening — 2 items done)

Read `RECALL-PLAN.md` §8. R4 is a grab-bag of hardening/cleanup/polish. Two
fully-verifiable items landed this session; the rest are scoped below.

**Done:**
1. **Corrupt-JSON quarantine** (`store.rs`). The full-scan chokepoint
   `list_notes_unlocked` (backs listing + `rebuild_index`) now CLASSIFIES a bad
   file: a transient I/O error is skipped and retried; a genuine JSON PARSE
   failure moves the file to `notes/corrupt/` (new `quarantine_corrupt` +
   `corrupt_dir`, non-clobbering) so one bad file stops breaking every scan.
   `get_note`/`set_pinned` etc. still surface a read error for a single-note
   read — quarantine happens on the next scan. Test:
   `corrupt_note_is_quarantined_on_scan`.
2. **Removed dead `grain_space_retrieval_mode`** (the List/AiQa toggle that died
   with the recall pivot). Gone from grain-core `settings.rs` (enum + field +
   default-constructor line), `shortcut/mod.rs` (the change command),
   `lib.rs` (specta reg), `settingsStore.ts` (+ the `GrainSpaceRetrievalMode`
   import), and regenerated `bindings.ts`. Old configs load fine — AppSettings
   has no `deny_unknown_fields`, so the stale key is ignored.

**Verified:** `cargo test --lib` **172 passed**, `cargo fmt`, `tsc`, eslint clean;
`bindings.ts` regenerated. Two commits (`814cfdc`, `b32fb06`).

### Remaining R4 (next), split by why it's not done yet
- **Needs model artifacts / semantic e2e (defer to a session that can download +
  run the model):** INT8/quantized embedding model (also fixes the ~130 MB f32
  download); long-note chunked embedding. Don't ship these blind — they can
  regress the working semantic path and there's no way to verify KNN quality
  from here.
- **Needs real-usage data:** recall system-prompt v1 iteration (rules live in
  `recall.rs::system_prompt`) and reconcile-prompt tuning — wait for actual
  transcripts before touching wording.
- **Small polish, best done where the running-app UI can be seen (settings tab):**
  export-all-notes (command + button), embed-model uninstall button (delete from
  HF cache + status), decay-λ slider for the existing
  `grain_space_decay_half_life_days` (needs a `change_..._decay...` command —
  none exists yet), overlay multi-monitor placement.
- **Likely already satisfied:** "append while another capture is running" — every
  store op takes `STORE_LOCK`, so concurrent captures already serialize; confirm
  with a targeted test before claiming it.

---

## 2026-07-07 — Session 2 (part 5): Grain Recall R3 COMPLETE (conversational writing)

Read `RECALL-PLAN.md` §7 first. R3 = CRUD-by-conversation. Inside a recall chat
the user can say "add the parser refactor", "the first two are done", "actually
the password is X now", "remember that…", or "forget the Rust note" and Grain
folds it into the note the same structured way capture created it.

1. **`capture::reconcile_note(app, current, change, convo_context)` (new, §7.1).**
   The MERGE sibling of `extract_metadata`, reusing the SAME infra
   (`send_chat_completion_with_schema` + `strip_code_fences` + `record_usage` +
   `llm_usable` gate). Structured schema = capture's fields plus `body` and
   per-todo `done`. `MergedMeta::apply_to` is CONSERVATIVE: a blank field keeps
   the current value (a weak completion can never erase the note); todos are the
   model's full merged list, kept as-is only when it returns none; the reminder
   is touched only when the change mentions timing; id/timestamp/pin preserved.
   **Degrade-safe:** no usable provider or any LLM/parse failure →
   `raw_append` (change appended to body verbatim, rest untouched — never lose
   the user's words). Also added `compose_note` (verbatim body + one extraction)
   for the `remember` path, mirroring `intake_transcript` without saving.
2. **ACTION convention (§7.2).** New `RecallAction { Reconcile{m} | Remember |
   Complete{m,todos} | Forget{m} }`; `ParsedTail` gained `action`. `parse_tail`
   recognizes an `ACTION:` last line (mutually exclusive with SOURCES/NOT_FOUND);
   `parse_action` is tolerant of synonyms and reads todo indices ONLY from the
   substring after "todo" so an `Mn` number never leaks in. Prompt gained rules
   9–11 (action verbs + "confirm forget in words, don't delete" + "plain Q always
   uses SOURCES").
3. **Execution in `run_turn`.** After parse: `remember` → `compose_note` + save;
   `update/append` → `reconcile_note` + save; `complete Mn todos …` → direct
   todo-flip by index + save (no LLM); `forget` → NON-destructive: returns
   `confirm_delete = Some(AgentSource)` for in-panel confirmation. Writes go
   through new `persist()` (save off-runtime + `emit_notes_changed` +
   `reminders::sync`), so overlay + settings refresh live. `convo_context` = the
   previous Grain answer, so "the first two" resolves. Added `read_note` helper.
4. **`AgentReply.confirm_delete: Option<AgentSource>`** (specta; `None` for Assist
   and every non-forget turn).
5. **Panel forget-confirm (frontend).** `renderConfirmDelete(src)` shows
   "Delete "<title>"? This can't be undone." + **Delete** / **Keep it** buttons
   under the answer (compact + per-turn expanded). Delete → existing
   `grainSpaceDeleteNote(note_id)` then "Deleted "<title>."; Keep → dismiss.
   Resolution tracked per note-id in `deleteResolved` state. **Deviation from
   §7.2:** explicit buttons instead of hijacking global Enter/Esc (those already
   mean paste/close in the panel) — deletion still requires an explicit click,
   which is the safety property that matters. i18n: `agent.forget{Confirm,Delete,
   Cancel,Done}`. CSS: `.agc-confirm-delete/-q/-actions/.agc-forget-btn/
   .agc-cancel-btn/.agc-forget-done`.

**Verified:** `cargo test --lib` **171 passed** (5 new: raw_append, MergedMeta
conservative-on-blanks + todo-replace, parse_tail actions, parse_action todo/
synonym). `cargo fmt`, `tsc`, eslint(AgentPanel) clean. `bindings.ts` regenerated
(ran `handy.exe` ~10 s; `AgentReply` now carries `confirm_delete`).

### Next concrete step (R4 — hardening)
RECALL-PLAN §8 "adopted Phase-6 list" + §4.36 removal: corrupt-JSON quarantine
(`notes/corrupt/`), INT8/quantized embed model (also fixes the ~130 MB f32
download), long-note chunked embedding, export-all-notes, model-uninstall button,
decay-λ setting UI, overlay multi-monitor placement, append-while-capture; PLUS
remove the dormant `grain_space_retrieval_mode` setting field + command; PLUS
prompt iteration from real usage. Pick these off one at a time.

### Gotchas
- `reconcile_note` NEVER errors out — it always returns a `Note` (merged or
  raw-appended). Callers just save whatever it returns.
- An ACTION citing an unknown M-number is a silent no-op (model may still have
  said "Done"). Rare; acceptable. Don't turn it into a hard error.
- Forget is the ONLY deferred action; everything else executes before `run_turn`
  returns, so the spoken confirmation and the actual write land together.

---

## 2026-07-07 — Session 2 (part 4): Grain Recall R2 COMPLETE (evidence + escape hatch)

Read `RECALL-PLAN.md` §6 first. R2 = the panel now surfaces WHERE an answer came
from (source chips) and offers a one-click exit when Grain genuinely doesn't
remember (not-found button). Purely additive on top of R1; Assist is unchanged.

1. **Typed reply (backend).** `agent_run` now returns `AgentReply { text,
   sources: Vec<AgentSource>, not_found }` instead of a plain `String`. New
   specta types `AgentReply` + `AgentSource { note_id, title, saved_at }` in
   `agent.rs`. Assist wraps its string via `AgentReply::plain(text)` (empty
   sources / `not_found = false`), so the panel renders NO footer for Assist.
2. **`recall::run_turn`** returns `AgentReply`. It resolves the parsed
   `SOURCES: Mn` numbers to sources INLINE: the block-building `spawn_blocking`
   now also returns a `HashMap<usize, AgentSource>` (M-number → note title/date),
   so no second DB read after the LLM call. Unknown M-ids are dropped
   (RECALL-PLAN §10). Title falls back to the note's summary; a bare `NOT_FOUND`
   still carries `not_found = true` so the escape hatch shows. Removed the dead
   R1 `resolve_sources` helper; `RecallSession::note_id_of` is now `#[cfg(test)]`.
3. **Panel (`AgentPanel.tsx`).** `versions` is now `AgentReply[]` (each retry
   version carries its own evidence); assistant `ChatMessage`s gained optional
   `sources` / `notFound`. New `renderEvidence(sources, notFound)` draws a quiet
   strip under an answer — either source chips (title + relative age, click →
   `grainSpaceOpenWindow(note_id)` overlay focus) or the not-found button (click
   → `grainSpaceOpenWindow(null)`, opens the browser unfocused). Wired into BOTH
   the compact card body and per-assistant-turn in the expanded conversation;
   `expand()` carries evidence through the freeze. `relDate(ms)` for chip ages.
4. **i18n:** `agent.basedOn_one/_other`, `agent.untitledNote`,
   `agent.notFoundOpen`. **CSS:** `.agc-evidence/-label/-chips/-chip/-chip-title/
   -chip-date/.agc-notfound-btn` in `agent.css` (dark card palette, orange hover).

**Verified:** `bindings.ts` regenerated by running `C:\gt\debug\handy.exe` ~10 s
(agentRun now `Result<AgentReply,string>`; AgentReply/AgentSource exported).
`cargo test --lib` **166 passed**, `cargo fmt` clean, `tsc --noEmit` clean,
eslint clean on AgentPanel.tsx (css/json ignored by config, not errors).

### Next concrete step (R3 — conversational writing)
RECALL-PLAN §7. Add `grain_space::capture::reconcile_note(app, current, change,
convo_context)` — a structured MERGE pass mirroring `extract_metadata`, reusing
`send_chat_completion_with_schema`/`strip_code_fences`/`record_usage`/`llm_usable`.
Then an `ACTION:` trailing-line convention in the recall prompt (append / update /
remember / complete-todos / forget), parsed like SOURCES: non-destructive actions
execute immediately and confirm in words; `forget` confirms in-panel (Enter/Esc)
before `grain_space_delete_note`. Safe degrade = raw-append the change on any LLM
failure (never lose the user's words). `reconcile_note` writes via
`store::save_note` (re-indexes + marks embed stale) and emits
`grain-space://notes-changed` so overlay + settings refresh live.

### Gotchas
- Source resolution keys on the M-NUMBER (registry index+1), not note id, because
  `render_memory` numbers by registry position even when an unreadable note is
  skipped — keep that alignment if you touch block building.
- Assist MUST stay footer-free: it relies on `AgentReply::plain` giving empty
  sources + `not_found = false`. Don't let any Assist path set those.

---

## 2026-07-07 — Session 2 (part 3): Grain Recall R1 COMPLETE (conversational retrieval)

Read `RECALL-PLAN.md` + `PRODUCT-VISION.md` first. R1 = the memory conversation
works end to end, reusing the Agent ecosystem. What was built:

1. **Binding:** `grain_space_recall` (ctrl+shift+m / option+shift+m), seeded in
   grain-core defaults + migration, gated by the `grain_space_` prefix (registers
   only while enabled; toggle-off unregisters it). `GrainSpaceRecallAction` in
   `actions.rs` ACTION_MAP → `agent::summon_memory`. **Distinct from
   summon_agent — the mode is fixed by the key, the AI never routes.**
2. **agent.rs:** `AgentMode { Assist, Recall }` + `RecallSession` added to
   `AgentState` (mode set at summon, recall registry cleared each summon).
   `summon` refactored into `summon_inner(app, mode)`; `summon_memory` is the
   Recall wrapper that SKIPS selection/field/paste-target capture. Recall
   ignores quick-agent in `dispatch_instruction`. `agent_run` branches on mode
   → `recall::run_turn`. Extracted `pub(crate) run_messages(app, full)` from
   `run_conversation` so recall can supply its own system prompt + memories
   block through the SAME provider/rotation driver.
3. **`grain_space/recall.rs` (new):** hybrid retrieval — FTS (`search_notes`) ∪
   semantic (`semantic_search`, only if `grain_space_semantic` && model on disk,
   silent FTS-only degrade), fused with Reciprocal Rank Fusion (k=60). Re-embeds
   stale rows first. Memories block with stable M-ids unioned across turns
   (`RecallSession`, cap 10, never renumbered), human relative-age lines,
   head-biased body truncation, inline todo state. System prompt v1 (8 rules
   incl. NOT_FOUND terminal). `parse_tail` splits the trailing
   `SOURCES:`/`NOT_FOUND` line off the display text (tolerant — malformed =
   whole reply, never errors). Empty-corpus fast path. `resolve_sources` +
   `ParsedTail` ready for R2 (parsed now, surfaced later).
4. **Engine lifetime amended (RECALL-PLAN §3.4):** `embed::shutdown_engine()` →
   `shutdown_engine_if_idle(app)` on BOTH the overlay window AND the agent panel
   Destroyed hooks — engine survives while EITHER surface is open, drops when
   neither is. No-op for Assist sessions (never spawn it).
5. **Settings tab:** `grain_space_recall` ShortcutInput row added.

**Verified:** src-tauri `cargo test --lib` **166 passed** (10 new recall tests:
RRF fusion/dedup, SOURCES/NOT_FOUND/missing-line parsing, stable-M-id session +
cap, relative-age, truncation). grain-core 4. `cargo fmt`, `tsc` clean, eslint
clean. No new tauri commands ⇒ bindings.ts unchanged (recall reuses `agent_run`).

### Next concrete step (R2 — evidence + escape hatch, panel UI)
RECALL-PLAN §6. Change `agent_run`'s recall path to return
`{ text, sources: [{note_id,title,saved_at}], not_found }` (specta type; Assist
returns empty/false) — currently returns a plain String. Then AgentPanel.tsx
footer: source chips (click → `grain_space_open_window(note_id)`) and the
not-found button (click → `grain_space_open_window(null)`). Regenerate
bindings.ts. `recall::run_turn` already computes `ParsedTail` + `resolve_sources`
— wire them into a struct return.

### Gotchas
- `recall.rs` needs `use tauri::Manager` for `try_state` on `&AppHandle`.
- Recall must NOT capture selection (a synthetic Ctrl+C would be wrong AND slow);
  `summon_inner` gates all three captures on `mode == Recall`.

---

## 2026-07-07 — Session 2 (part 2): PIVOT — Phases 5/6 scrapped, Grain Recall planned

**FINAL-PLAN.md Phases 5 and 6 are DEAD as written.** The product vision was
rewritten (`docs/PRODUCT-VISION.md`): Grain Space is an AI memory companion —
answer-first conversational retrieval, notes are evidence, not the interface.
The replacement plan is **`RECALL-PLAN.md` in this folder** — read it (and
PRODUCT-VISION.md) before doing ANY retrieval work. Summary of the pivot:
- Retrieval reuses the ENTIRE Agent ecosystem (native pill input voice+type,
  bottom-right panel, expanded conversation, follow-up shortcut) via a new
  `AgentMode::Recall` — no new surfaces. Visual differentiation deferred.
- Pipeline: hybrid FTS+semantic retrieval with RRF fusion → memories block
  with stable M-ids → tight system prompt → answer + tolerant trailing
  `SOURCES:` line. Sources click-through to the overlay is phase R2.
- Old Phase 5's "results list mode" and `grain_space_retrieval_mode` are dead;
  old Phase 6's hardening list survives as phase R4.
- **Next concrete step: RECALL-PLAN.md phase R1** (binding + `summon_memory`
  via a `summon_inner(app, mode)` extraction in agent.rs + `recall.rs`).
No code was written for this pivot in this session — planning only.

---

## 2026-07-07 — Session 2 (part 1): Phases 3 + 4 COMPLETE (overlay window + semantic search)

### Status snapshot
- [x] FINAL-PLAN.md written (authoritative)
- [x] Phase 1 — Core storage & capture: DONE
- [x] Phase 2 — Settings UI + reminder scheduler: DONE
- [x] **Phase 3 — Overlay window (Raycast-style two-pane): DONE**
- [x] **Phase 4 — Semantic search (opt-in BGE-small via Candle): DONE**
- [ ] Phase 5 — Voice-first retrieval  ← **NEXT**
- [ ] Phase 6 — Hardening/polish

### Phase 3 — what was done
1. **Binding:** `grain_space_open` (ctrl+shift+g / option+shift+g) seeded in
   grain-core defaults + the missing-bindings migration; `GrainSpaceOpenAction`
   in `ACTION_MAP` (actions.rs) toggles the window. Registered only while the
   feature is on (existing `grain_space_` prefix gating covers it).
2. **Window (`src-tauri/src/grain_space/window.rs`):** label `grain-space`,
   840×560 frameless/transparent/centered, create-on-summon destroy-on-close,
   ALL ops on the async runtime (tauri#3990). Focus-note stash + take pattern
   (like AgentState) so the settings tab can open the overlay onto a note;
   `grain-space://focus-note` event handles the already-open case. The
   Destroyed hook drops the embed engine and clears the stash. New capability
   `src-tauri/capabilities/grain-space.json` (close/focus/drag only).
   `change_grain_space_enabled_setting(false)` closes the window + engine.
3. **Commands:** `grain_space_open_window(note_id?)`, `grain_space_close_window`
   (NOT gated — must close after disable), `grain_space_take_focus_note`.
4. **UI (`src/components/grain-space/GrainSpaceOverlay.tsx` + grain-space.css,
   branch on window label in main.tsx like agent-panel):** search top (clear
   button, Esc clears then closes), date-grouped list left (Pinned bucket
   first), full editor right (title + body textarea, tldr shown read-only,
   todo_tags as live checkboxes BELOW the body — the `<todo>` spans stay as
   raw text in the textarea; inline rendering deferred), bottom-right action
   row = reminder (pending ⇒ Arm button per auto-reminders directive; armed/
   fired ⇒ Dismiss) · pin · delete. Debounced 600 ms save with flush on
   blur/switch/close; draft notes persist on first content via
   `grain_space_create_note` (id-adoption guard prevents duplicate creation if
   keystrokes land mid-create). Blank-vs-list rule: 0 notes ⇒ straight into a
   blank draft; Ctrl+N + header button for new note. Refreshes on
   `grain-space://notes-changed`. Append-via-voice and Ask-AI deliberately
   ABSENT (scrap directive 12).

### Phase 4 — what was done
1. **Deps:** candle-core/nn/transformers 0.9, tokenizers 0.21 (onig only),
   zerocopy 0.8 (f32→blob for sqlite-vec).
2. **Engine (`grain_space/embed.rs`):** one OS thread owning
   tokenizer+BertModel (BGE-small-en-v1.5, f32, CPU, mmap'd safetensors) behind
   an mpsc channel; per-text encode → mean-pool → L2-normalize. Spawned lazily
   by the first semantic search (command refuses if the overlay window is not
   open), dropped by `shutdown_engine()` on window-destroy / feature-off /
   semantic-off. Load failure answers queued requests with the error.
3. **Download:** consent is FRONTEND-side; `grain_space_download_embed_model`
   pulls config/tokenizer/model.safetensors into the shared HF cache via the
   cancellable hf-hub fork with `grain-space://embed-model-{progress,complete,
   error}` events; `..._cancel_embed_model_download` + status command
   (`ready|downloading|absent`). NOTE: real download is ~130 MB (f32 export),
   not the ~34 MB in the old plan copy — UI copy says ~130 MB.
4. **Store:** `notes_vec` vec0 table created ONLY on first semantic use
   (`ensure_vec_table`); `stale_embed_texts` / `store_embeddings` /
   `semantic_search` (KNN LIMIT 24, cos = 1 − d²/2 on normalized vectors,
   decay `exp(-λΔt)` with λ = ln2/half-life, pinned ⇒ Δt=0). unindex/rebuild
   clean the vec table too (guarded by sqlite_master existence check so
   non-semantic users never create it).
5. **Command:** `grain_space_semantic_search` — gates: feature on → semantic
   setting on → overlay window open → model on disk (`model-not-downloaded`
   error string is the frontend's consent trigger) → re-embed stale batch →
   embed query → ranked KNN.
6. **Settings tab:** semantic toggle now runs consent → download (progress bar
   + cancel) → only flips the setting on the complete event (toggle stays off
   until verified on disk). `grain_space_open` ShortcutInput row added; note
   rows are clickable and open the overlay focused on that note.
7. **Overlay:** Exact/Semantic mode capsule (shown only when the setting is
   on), semantic falls back to FTS on any error, consent/progress/error banner
   for the model-missing recovery path.

### Verified
`cargo test --lib` (src-tauri): **156 passed** (2 new: vec roundtrip/ranking/
delete, embed-text composer). grain-core: 4 passed. `tsc --noEmit` clean.
eslint clean on all touched files (repo has PRE-EXISTING eslint errors in
ModuleC.tsx/ModelSelector.tsx — not from this work). `cargo fmt` run.
bindings.ts regenerated by running `C:\gt\debug\handy.exe` ~8 s.

### Next concrete step (Phase 5 — voice-first retrieval)
Read FINAL-PLAN.md §4 Phase 5. Register `grain_space_voice_search` binding
(defaults + migration + ACTION_MAP, gated like the others), record+transcribe
the query, then Mode A (RAG answer in a destroyable surface — reuse the agent
reply-panel pattern) vs Mode B (open overlay with results via
`window::open` + a results handoff) per `grain_space_retrieval_mode`.

### Gotchas discovered
- eslint here has no `react-hooks/exhaustive-deps` rule — referencing it in a
  disable comment is itself an error.
- `tsc` must be invoked as `./node_modules/.bin/tsc.exe` (bun install; naked
  `npx tsc` resolves to the wrong package).
- hf-hub `Progress` trait needs `init`/`update`/`finish` (async fns).
- vec0 has no UPSERT — delete+insert. KNN needs `ORDER BY distance LIMIT k`.
- BGE f32 `model.safetensors` is ~130 MB on disk (not 34 MB); RAM after mmap
  load is smaller but budget the download copy accordingly.

---

## 2026-07-06 — Session 1 (part 2): Phase 2 COMPLETE (settings tab + reminders)

### Status snapshot
- [x] FINAL-PLAN.md written (authoritative)
- [x] Phase 1 — Core storage & capture: DONE
- [x] **Phase 2 — Settings UI + reminder scheduler: DONE**
- [ ] Phase 3 — Overlay window (Raycast-style two-pane)  ← **NEXT**
- [ ] Phase 4 — Semantic search (opt-in BGE-small via Candle)
- [ ] Phase 5 — Voice-first retrieval
- [ ] Phase 6 — Hardening/polish

### Phase 2 — what was done
1. **Reminder scheduler (`src-tauri/src/grain_space/reminders.rs`):**
   generation-counter design — `sync(app)` bumps `GENERATION`, returns
   immediately if the feature is off, else scans notes once (blocking hop),
   marks + notifies anything due (OS notification via NEW dep
   `tauri-plugin-notification`, registered in `lib.rs`), and parks ONE tokio
   sleep until the earliest future `Armed` fire_at. A superseded timer wakes,
   sees a newer generation, and exits. No polling, nothing resident when idle.
   `sync` call sites: app start (`initialize_core_logic`), the master-toggle
   command, save/delete note commands, arm/dismiss commands, voice-capture
   intake. `store::set_reminder` added (like `set_pinned`: no embed-stale).
2. **New commands:** `grain_space_arm_reminder(id, fire_at)`,
   `grain_space_dismiss_reminder(id)`, plus per-setting commands
   `change_grain_space_semantic_setting`, `..._auto_reminders_setting`,
   `..._retrieval_mode_setting` (in `shortcut/mod.rs`); all specta-registered.
3. **Settings tab (`src/components/settings/grain-space/GrainSpaceSettings.tsx`):**
   registered in `Sidebar.tsx` SECTIONS_CONFIG as `grainSpace` (always visible,
   NotebookPen icon, i18n key `sidebar.grainSpace`). Contents: master toggle →
   (when on) Capture group (ShortcutInput rows for both bindings + auto-set
   reminders toggle), Search group (semantic toggle — setting only, download
   flow is Phase 4), Reminders group (pending/armed/fired rows with Arm/
   Dismiss), Notes group (pinned first, then Today/Yesterday/date buckets;
   pin + delete on hover). Refreshes on `grain-space://notes-changed`.
4. **Store updaters** for the four new settings in `src/stores/settingsStore.ts`.
5. **bindings.ts regenerated** by briefly running the debug exe
   (`C:\gt\debug\handy.exe`, killed after ~6 s — the specta export happens at
   startup before any window matters).
6. **Verified:** `tsc --noEmit` clean, eslint clean (three JSX literals moved
   to `settings.grainSpace.*` i18n keys), `cargo test --lib` 154 passed,
   `cargo fmt` run.

### Next concrete step (Phase 3 — overlay window)
Read FINAL-PLAN.md §4 Phase 3 first. Key decisions already made:
- New webview window (label e.g. `grain-space`), route `/grain-space` in the
  same React bundle, created by a `grain_space_open` tap binding (NOT yet
  seeded — add the binding to grain-core defaults + migration list + ACTION_MAP
  the way `grain_space_quick_add` was), destroyed on close/Esc. ALL window
  create/resize calls async (tauri#3990). Add a `grain-space` entry to
  `src-tauri/capabilities/` (copy agent.json shape).
- Two-pane Raycast-style UI (screenshot in chat, described in plan §1):
  search top, date-grouped list left, full note editor right, bottom-right
  action row. Reuse Agent Pill workflow components for any voice states
  (user directive 11).
- Blank-vs-list rule: 0 notes ⇒ straight into a new blank note.
- Search in overlay = `grain_space_search_notes` (FTS) until Phase 4.
- Feature toggled off while open ⇒ close the window (listen to settings or
  handle in the toggle command).

### Prior session summary (Phase 1, same day)

### Status snapshot
- [x] FINAL-PLAN.md written (authoritative; includes user's directive 11: reuse
      the Agent Pill workflow components for voice/typing UI states)
- [x] **Phase 1 — Core storage & capture: DONE (backend only, no UI yet)**

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
