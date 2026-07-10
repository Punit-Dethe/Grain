# Grain Space — Transition Log

Newest entry first. Each entry assumes the reader has ZERO context: read
`FINAL-PLAN.md` in this folder first, then the top entry here, then continue.

---

## 2026-07-10 — PIVOT: Obsidian vault backend (OBSIDIAN-PLAN.md) — V1 COMPLETE

**Grain is a friction-reduction utility, not a note app.** New plan file
`OBSIDIAN-PLAN.md` (read it + `PRODUCT-VISION.md` first): the whole pipeline
(capture, structuring, hybrid FTS+semantic retrieval, Recall, conversational
writes) can now run on top of an **Obsidian vault** — plain `.md` files the
user owns, synced by whatever they already use. Two HARD-SWITCHED backends
(`grain` JSON store — default, unchanged — vs `obsidian` vault), no sync
between them in v1. Research digested in the plan §2 (Smart Connections =
block embeddings outside notes; Omnisearch = full rescan is cheap, weight
title>body; Obsidian auto-merges external disk writes via diff-match-patch).

**V1 shipped this session (all backend, zero frontend change):**
1. **Settings (grain-core):** `grain_space_backend` (`GrainSpaceBackend::
   {Grain, Obsidian}`, serde default Grain), `grain_space_vault_path`,
   `grain_space_vault_folder` (default "Grain"). Three change-commands in
   `shortcut/mod.rs` (backend switch closes overlay + drops engine + resyncs
   reminders; vault path validated `is_dir`; folder name rejects separators/
   dot-prefix), specta-registered. bindings.ts NOT yet regenerated (no
   frontend consumer yet — regen with the V2 settings UI).
2. **`grain_space/vault.rs` (new, ~700 lines + 13 tests):** frontmatter codec
   (flat YAML we emit/parse ourselves — NO serde_yaml dep; foreign frontmatter
   treated as an opaque block, stripped for body/FTS); filename = title
   (sanitized, collision-suffixed, rename-on-title-edit); identity =
   `grain_id` uuid frontmatter (grain notes, survives moves) / sha256(relpath)
   (foreign notes, zero writes into user files); `vault_index.sqlite` in
   app-data (path/mtime/size/foreign_note columns), NEVER in the vault;
   **lazy `reconcile()` stat-scan** at every retrieval entry point instead of
   a resident watcher (zero idle RAM — deviation from the advisory notes,
   justified in plan §5); atomic tmp+rename writes; **foreign notes are
   read-only** (save/delete/pin/reminder refuse) — the v1 stale-buffer
   defense; `list_notes`/empty-query browse = grain-owned only, real
   search/semantic = whole vault.
3. **`grain_space/backend.rs` (new):** `Backend::{Grain(PathBuf),
   Vault(Vault)}` resolved from settings (`backend::resolve(app)`), Clone,
   dispatching the full store surface. `store.rs` untouched as the grain
   implementation (only `ensure_vec_extension`/`validate_id` went pub(crate)).
4. **All ~28 call sites swapped** (commands.rs gate() now returns Backend;
   capture.rs quick_add + capture_and_save; recall.rs retrieve/run_turn/
   read_note/persist/run_tool_loop/execute_search_memory/build_block_and_meta
   — all `&Path base` params became `&Backend`; reminders.rs run_cycle).
   Behavior with backend = grain is byte-identical.

**Bugs the new tests caught (fixed):** (a) reconcile's vanished-sweep deleted
a renamed/moved grain note right after re-indexing it — now `indexed.retain`
drops the old-path entry by id after every upsert; (b) `created` frontmatter
was second-precision so timestamps drifted ±999 ms on re-parse — now emits
`%.3f` ms; (c) `get_note`/`mutate_grain_note` now reconcile-and-retry once on
a stale indexed path (user moved the file in Obsidian mid-session).

**Verified:** src-tauri `cargo test --lib` **199 passed** (13 new vault
tests: codec roundtrip, foreign mapping/read-only, sanitize/collision,
save-rename identity, external add/change/remove reconcile, promote-out-of-
folder keeps identity+writability, pin-not-stale, delete/rebuild, semantic
roundtrip+floor, browse-vs-search scoping). grain-core 4 + root workspace all
green. `cargo fmt` clean.

**V2 (settings UI) shipped same session:** "Storage" group in
`GrainSpaceSettings.tsx` — vault hard-switch toggle (turning it ON without a
vault opens the native picker first and only flips once a folder is chosen),
vault-path row + "Choose vault…", Grain-subfolder text field (commit on
blur/Enter through the validated command). New backend command
`grain_space_pick_vault` (backend-side `blocking_pick_folder`, same pattern as
export — no new webview capability). `settingsStore.ts` updaters for the three
settings. bindings.ts regenerated (exports `GrainSpaceBackend`,
`grainSpacePickVault`, the three change-commands). i18n keys
`settings.grainSpace.{vaultFolderLabel,vaultUnset,chooseVault,subfolderLabel,
subfolderHint}`. Verified: tsc clean, eslint clean, 199 tests pass.

**V3 shipped same session (two-way sync + wiring; NOT categorization, NOT the
AST editor — user deferred both):**

**A. Two-way-sync-safe write path (`vault.rs`) — the headline.** Grain never
clobbers a concurrent Obsidian edit. New `content` column on `notes_meta`
stores the last file text Grain synced = the 3-way-merge ancestor (migrated
via `ALTER TABLE … ADD COLUMN`, ignored-if-exists). New dep **`diffy = "0.4"`**
(line-based 3-way merge; diff-match-patch equivalent — user named `similar` as
an example, `diffy::merge` is the purpose-built tool). New `safe_write(abs,
base, ours)`: if disk still == base (or no base/file) → plain atomic write;
else `diffy::merge(base, ours, theirs)` — clean merge is written; a CONFLICT
writes Grain's version to the live file but preserves the external version in a
`<stem>.grain-conflict-<ts>.md` sidecar (never drops the user's words). Both
`save_note` and `mutate_grain_note` route through it. `mutate` keeps its
no-re-embed fast path when nothing merged (written == ours → light UPDATE,
embed_stale untouched); only a merge that folds external body edits triggers a
full re-index. Tests: `concurrent_edit_merges_cleanly_not_clobbers`,
`conflicting_edit_preserves_both_via_sidecar`.

**B. Open in Obsidian.** `grain_space_open_in_obsidian(id)` command — resolves
the note's abs path (`vault::note_abs_path`, vault only; `None` for grain
store), builds `obsidian://open?path=<percent-encoded>`, opens it BACKEND-side
via `app.opener().open_url` (no custom-scheme frontend capability needed).
`ExternalLink` icon button on note rows, shown only when backend = obsidian.

**C. Recall empty-corpus fix.** `run_turn`'s zero-notes fast path now calls
`backend::has_any_notes` (vault counts foreign notes too) instead of
`list_notes().is_empty()` — so Recall runs over a vault of purely the user's
own Obsidian notes.

**D. bm25 title weighting.** All four FTS `ORDER BY bm25(notes_fts)` sites
(both stores, ranged + plain) → `bm25(notes_fts, 1.0, 10.0, 5.0, 1.0)`
(id/title/tldr/body): title matches now outrank body (Omnisearch lesson).

Verified: 201 src-tauri tests pass (2 new merge tests), tsc + eslint clean,
bindings.ts regenerated (`grainSpaceOpenInObsidian`).

### IMPORTANT — foreign notes are still READ-ONLY, by design for now
The user wants two-way sync for NON-Grain files too (editable foreign notes),
but deferred it with the AST/Obsidian-format overlay editor ("don't do writing
now, just don't make conflicting decisions"). The merge machinery (`safe_write`)
is deliberately GENERIC — enabling foreign writes later is: (1) drop the
`foreign` guard in `save_note`/`mutate`, (2) preserve the foreign file's
original frontmatter block on write (currently we strip it for body/FTS — a
foreign write must round-trip it untouched; store the raw block or re-read +
splice). Do NOT inject `grain_id` into a foreign file. The overlay must also
stop calling the plain save for foreign notes until then.

### Next concrete step (V4+, all deferred by the user for now)
1. **NEEDS USER GUI TESTING** (can't drive headlessly): flip to a real vault,
   capture (voice/type/quick-add), search foreign notes, Recall over the vault,
   pin/delete/edit a grain note, promote a note out of Grain/ and re-edit,
   Open-in-Obsidian, and force a concurrent edit to see the merge/sidecar.
2. Foreign-note editing + the Obsidian-AST overlay editor (see the read-only
   note above) — the big two-way-sync + WYSIWYG piece.
3. Chunked embedding for long vault notes (plan §7 V3).
4. Auto-categorization (plan §7 V4) — explicitly NOT now.

### Gotchas
- The vault backend REFUSES to run when `grain_space_vault_path` is unset/
  missing (`backend::resolve` errors) — reminders::run_cycle silently returns
  in that case; every command surfaces the message.
- `search_notes_ranged("", Some(range))` semantics differ per backend by
  design: grain = all notes in window, vault = grain-owned in window (a
  foreign-notes browse would ship thousands of files to the frontend).
- Never quarantine/move vault files on parse errors — log and skip only
  (store.rs quarantine is for OUR JSON dir only).
- vault.rs `unique_path` excludes `current` so re-saving a note keeps its
  filename; pass `None` for new notes.

---

## 2026-07-09 — Note card two-field UI, dictation-into-panel, Quick-Agent Enter/Shift+Enter

Three user-reported bugs.

**1. Typed note card is now two-field (title + body), matching the prototype.**
`AgentInputUi` gained `title` + `focus_title` (body/query stays in `text`).
Capture expanded renders a top TITLE field + a wrapping, growing BODY (capped at
`CAP_MAX_BODY_LINES`, tail-scrolled, so it stays inside the fixed 170px canvas —
no window/RAM growth). New `wrap_plain` + `draw_caret` helpers. Focus starts on
the BODY; click either field to focus it (matches the prototype). Plain Enter =
newline (note formatting); **Shift/Ctrl+Enter = save** (the pill's focused
handler; the core's global-Enter fallback is suppressed for a capture newline so
it can't submit). Title typed → kept; empty → auto-generated (`fallback_title`/
LLM). `capture_and_save` gained `title_override`. The green-dots-fill "Saved"
confirmation now mirrors the prototype (wave fills green + "Saved", `phase`
reset on save) for the voice path too.

**2. Dictating into the Agent panel pasted the auto-copied AI reply.** The STT
paste chokepoint (`clipboard::paste`) now checks `agent::panel_dictation_target`
— when the panel is EXPANDED (owns a follow-up field, tracked via new
`AgentState.panel_expanded`) AND focused, it emits `agent-panel-dictation` to the
panel (AgentPanel appends it to the follow-up input) and SKIPS the OS paste. Any
other window → upstream Handy paste behavior untouched (per the fork constraint).

**3. Quick Agent had no paste target when nothing was selected.** Quick vs panel
is now a per-submit choice, not the `agent_quick_enabled` setting: plain **Enter
→ panel** (bottom-right), **Shift+Enter → Quick Agent** (paste in place). Threaded
via new `quick` field on `PillAction::AgentInputSubmit{Text,Voice}` →
`input_submit_text/voice` → `dispatch_instruction(quick)`. So "just ask a
question" (Enter) always shows the reply instead of silently copying to nothing.

Schema: `PillAction::AgentInputSubmitText { text, title, quick }`,
`AgentInputSubmitVoice { quick }` (serde-default → back-compat). Verified:
`cargo check --workspace` + `tsc` clean; 37 grain_space tests pass. Files:
`grain-core/event.rs`, `events_server.rs`, `agent.rs`, `grain_space/capture.rs`,
`clipboard.rs`, `grain-pill/lib.rs`, `AgentPanel.tsx`.

---

## 2026-07-09 — Recall precision + prompt efficiency (semantic floor, create-vs-edit, prompt slim)

Three user-reported edge cases:

**1. Unrelated notes leaked into the block (tiny corpus).** KNN always returns
the nearest notes even when nothing is related, so a 4-note corpus sent an
off-topic note just to fill the top-K. Fix: `store::semantic_search_ranged`
gained a `min_similarity` cosine floor (applied pre-decay). Recall passes
`SEMANTIC_MIN_SIMILARITY = 0.45` (BGE-small baseline is high — tuning knob,
documented); the overlay browser search still passes 0.0 (unchanged). FTS
(exact keyword) is unaffected, and the fuse/rerank never pads with non-matches —
so only FTS- or semantically-matched notes reach the model. Test added.

**2. "Create a new note" wrongly appended to a related memory.** The model
confused CREATE with EDIT: e.g. "save my new wifi password" folded into the old
wifi note instead of making a new one. Capability existed (`ACTION: remember` →
`compose_note` → new note); the prompt was the problem. Prompt now leads with an
explicit split: SAVE/CREATE/REMEMBER → `ACTION: remember` (ALWAYS a new memory,
even if a related one exists — never merge); CHANGE/ADD-TO existing → search then
`ACTION: update Mn` (1 match) or clarify (0/2+).

**3. Malformed trailing line ("clickable button" broke) + prompt bloat.** The
previous prompt had grown to 13 rules + an examples block that embedded LITERAL
`\n` ("…hunter2.\\nSOURCES: M2") — the small model learned to emit a literal
`\n` as text, so `parse_tail` couldn't find the trailing SOURCES/ACTION line and
the panel rendered no chip/action. Rewrote `system_prompt` tight (identity →
tool → answer contract → change contract), removed the literal-`\n` examples,
kept one inline done-confirmation phrasing. Shorter = less drift, correct line
format restored.

Verified: `cargo check --workspace` clean; 37 grain_space tests pass. Files:
`src-tauri/src/grain_space/{store,recall}.rs`.

---

## 2026-07-09 — Grain Space native card: separate memory surface + edge-case hardening

Two-part session. Part A hardened the Recall/capture backend; Part B gave Grain
Space its own visual variant of the ONE native pill card (no new window/webview,
zero RAM overhead — same winit + tiny-skia surface, just different strings/anchor).

**Part A — backend (capture.rs / recall.rs):**
- Reconcile confidence guard: a merge that drops >half a non-trivial body is
  distrusted → falls back to `raw_append` (`merge_lost_content`); merge prompt
  also biased toward append-when-unsure.
- `fallback_title` (first ~3 words, no LLM) for quick-add + no-LLM/blank-title
  capture; `display_title` (title→tldr→first-words) for polished Recall source
  chips.
- Huge selections: metadata LLM input capped (`sample_for_meta`, 4000 chars);
  full body still stored.
- Recall prompt v3: trailing-line examples, weak-match `NOT_FOUND` grace, write
  disambiguation reinforced.

**Part B — native Grain Space card (grain-core / agent.rs / grain-pill):**
- New `AgentInputKind { Assist, Capture, Recall }` on `DaemonEvent::AgentInput
  Show` (serde-default → back-compat). `agent.rs::input_kind` sets it at summon.
- The Grain Space kinds (Capture/Recall) render TOP-anchored (`agent_input_anchor`
  forces `OverlayPosition::Top`); Assist keeps the user's overlay anchor.
- Capture relabels the SAME card: "Noting…" / "Write down your thoughts…" /
  "Save Note"; Recall & Assist keep "Listening…" / "Ask anything…" / "Confirm".
- Headless capture confirmation IN the card (no new pill): `capture_and_save`
  now returns `Ok(bool)`; on a real save `capture_run` emits new
  `DaemonEvent::AgentInputSaved` → the card paints a green dot + "Saved"
  (`AgentInputUi.saved`), held `CAPTURE_SAVED_HOLD` (1.1s), then `AgentInputHide`.
  Capture submit no longer hides the card immediately (so it can confirm itself);
  no-speech / failure still hides silently.
- NO icons (strict zero-RAM stance — deferred to a later efficiency pass).

**KNOWN v1 SIMPLIFICATION (awaiting user visual verify):** Capture uses a SINGLE
editable field (title auto-generated by AI/`fallback_title`), not the prototype's
separate editable Title+Body two-field layout — that needs a two-buffer/focus
input model (a larger change). Flagged to the user to decide.

Verified: `cargo check --workspace` clean; 37 grain_space tests pass. Files:
`grain-core/src/{event,lib}.rs`, `src-tauri/src/agent.rs`,
`src-tauri/src/grain_space/capture.rs`, `crates/grain-pill/src/lib.rs`.

---

## 2026-07-09 — Recall LLM-interaction overhaul: native tool calling + dual-stage retrieval

Overhauls `recall.rs` retrieval + LLM interaction. Supersedes the planned text
`SEARCH:` convention (SEARCH-OVERHAUL S2) with **native tool calling**; also
lands S1 dual-stage, strict no-truncation, and context-in-user-message.

**1. Native tool calling (`search_memory`).** New transport in `llm_client.rs`:
`send_chat_with_tools` + `ChatEntry` (system/user/assistant/assistant-tool-calls/
tool-result), `ToolSpec`, `ToolCallOut`, `LlmChatResult`. Request gains optional
`tools`/`tool_choice`; response parses `tool_calls`. All new fields are `Option`
+ `skip_serializing_if none`, so `send_chat` / post-process wire bytes are
UNCHANGED. `agent.rs` gains `run_messages_with_tools` (single + rotation via the
existing health-ordered driver, structured reply captured out-of-band so
`CallOutcome` stays text-only for every other caller). `recall::run_tool_loop`
drives the bounded agentic loop (`MAX_TOOL_HOPS = 3`, then a no-tools forced
answer). One tool only: `search_memory(query, minDate?, maxDate?)`. Writes keep
the `ACTION:` trailing-line convention.

**2. Tool calling is an ENHANCEMENT, not a dependency (deviation from the naive
"model calls the tool for everything").** Grain runs arbitrary OpenAI-compatible
providers and the local Apple Intelligence path (NO tool support). So we STILL
pre-inject the dual-stage top-6 as a first pass; tool-incapable models answer
from it, tool-capable models refine via `search_memory`. Zero idle RAM — every
hop is active-turn only.

**3. Dual-stage 20→6 (S1).** `retrieve` now fuses to `CANDIDATE_POOL = 20`
(`fuse_scored` keeps RRF scores) then `rerank` = weighted (0.5 RRF-norm + 0.3
title/tldr term-overlap + 0.2 recency decay) → top 6. Deterministic, unit-tested.

**4. Strict no-truncation.** Removed `truncate_body` / `BODY_*` consts —
`render_memory` sends the FULL note body. `MAX_SESSION_MEMORIES` (now 12) is the
RAM/context safety bound.

**5. Context in the user message (lost-in-the-middle).** Memories are no longer a
system message; `build_entries` prepends the `[Mn]` block to the LATEST user turn.

**6. Date pre-filter (`store.rs`).** `search_notes_ranged` adds a SQL
`timestamp BETWEEN` join (works FTS-only, no embed model);
`semantic_search_ranged` filters the KNN pool by the same window. The tool's
minDate/maxDate map to local day bounds via `parse_date_ms`.

Verified: `cargo check` clean; 34 grain_space unit tests pass (added rerank,
term-overlap, date-bound, full-body-render tests). Files: `llm_client.rs`,
`agent.rs`, `grain_space/store.rs`, `grain_space/recall.rs`.

---

## 2026-07-07 — Session 2 (part 7): Capture-via-agent-pill + note-window fix + f16 option

Three user-requested changes (not from RECALL-PLAN — direct product feedback).

**1. Note CREATION now uses the Agent pill, not the main recording pill.** New
`AgentMode::Capture` (third mode alongside Assist/Recall). The `grain_space_capture`
binding no longer runs the transcribe pipeline (`intake_transcript` deleted, removed
from `is_transcribe_binding` + the `TranscribeAction::stop` interception); it now
`summon_capture` → summons the SAME agent pill. Gains **text input for free** (type
instead of speak) and shows the **selected-char chip** (the pill's chip now renders
ONLY when a selection exists — empty state shows nothing, cleaning up Recall too,
`grain-pill/src/lib.rs`). `summon_inner` gating refined: selection captured for
Assist+Capture, field-context/paste-target for Assist only.
  - **Selection = note body (verbatim), spoken/typed words = FRAMING** (shape
    title/summary only, never rewritten). `extract_metadata` + `compose_note` gained
    a `framing: Option<&str>` param.
  - **HEADLESS save, no panel** (user: confirmation handled elsewhere). `dispatch_
    instruction` routes Capture → new `agent::capture_run` (mirrors `quick_run`):
    reads the summon selection, calls `capture::capture_and_save` (was
    `run_capture_turn`, now returns unit), releases the input shortcuts. summon skips
    pre-creating the panel; `input_submit_voice` skips the loading reveal and, on a
    bad/empty transcript, cleans up silently instead of a panel error. The Capture
    branch is NOT in `agent_run` (capture never touches the panel).
  - `Ctrl+Shift+C` quick-add UNCHANGED. Settings label "Dictate Note" → "Create Note".

**2. Note overlay window is now reachable + frees memory.** It was `skip_taskbar` +
frameless + not-on-top, so losing focus dropped it behind everything with no way
back, and it stayed resident (holding the embed engine) because it was never
destroyed. Now `skip_taskbar(false)` (taskbar/Alt-Tab), and `window::toggle`
brings it forward when it's behind (only closes when already focused) — so it can
always be returned to and closed to free the engine. Backend-only (no capability
change).

**3. Optional f16 embedding model (side-by-side lighter option).** User chose f16
over INT8 (INT8 would need the heavy `ort`/ONNX dep + can't verify here; no fp16
safetensors exists upstream anyway). New `grain_space_embed_f16` setting →
`embed.rs` loads the SAME f32 file cast to F16 (≈half resident RAM, ~identical
results, download unchanged). `USE_F16` atomic (engine layer has no AppHandle),
seeded at startup + by `change_grain_space_embed_f16_setting` (which drops the
engine to force a precision re-load). Pooling/output cast to f32. Toggle under
Search (shown when semantic is on).

**Verified:** `cargo test --lib` **173 passed**, `cargo fmt`, `tsc`, eslint clean;
`bindings.ts` regenerated (new `changeGrainSpaceEmbedF16Setting`); boot smoke-tested.
Commits `d600235` (capture+window), `f2ceeaa` (f16).

### Gotchas
- Capture is headless — it must NEVER reveal the panel. If you touch
  `input_submit_voice`/`dispatch_instruction`, keep the `mode == Capture` guards.
- The pill selection chip renders only for `selection_chars > 0` now (all modes).
- f16 on CPU mainly saves RAM (CPUs lack native f16 compute; speed ≈ same). The
  setting command MUST `shutdown_engine()` or a live engine keeps the old precision.

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
