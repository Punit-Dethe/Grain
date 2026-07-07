# Grain Recall — Conversational Memory Retrieval (the plan)

> **This file SUPERSEDES Phases 5 and 6 of `FINAL-PLAN.md`.** Those phases were
> written when Grain Space was still imagined as a note app. `PRODUCT-VISION.md`
> (repo `docs/`) redefined the product: Grain Space is an **AI memory
> companion** — the user asks, Grain answers; notes are evidence, not the
> interface. Read `PRODUCT-VISION.md` first, then this file. Phases 1–4 of
> FINAL-PLAN.md (storage, capture, settings tab, overlay browser, semantic
> index) remain valid and are the foundation this builds on.

---

## 1. What Grain Recall is

The user presses one shortcut anywhere, speaks (or types) a half-remembered
fragment — *"what was that Mac app from Product Hunt?"* — and Grain answers in
a small bottom-right card: *"Superlist — you saved it after a Product Hunt
launch about lightweight project management."* Follow-ups are conversational
("what was the pricing?", "actually that's changed, it's $8 now"). The
supporting notes stay one click away but never in the way.

**Answer first. Notes are evidence. The conversation is the product.**

The two retrieval modes from the vision:

| Mode | Surface | Status |
|---|---|---|
| 1 — Conversational retrieval (DEFAULT) | the Agent pill + panel, in a new **memory mode** | THIS PLAN |
| 2 — Manual memory browser | the existing `grain-space` overlay (`grain_space_open`) | already built (Phase 3) — unchanged |

---

## 2. The core decision: reuse the Agent ecosystem wholesale

We do NOT build a new input surface, window, LLM client, or conversation UI.
The Agent workflow already implements exactly the interaction the vision
describes, end to end:

| Need | Existing Agent piece (verified in `src-tauri/src/agent.rs`) |
|---|---|
| Instant voice input, expands to typing on first keystroke | NATIVE pill summon card — `summon()` → `DaemonEvent::AgentInputShow`; back-channel `input_submit_text/voice`, `input_typing`, `input_cancel` |
| "It's listening" with zero window cost | dictation starts at summon (`start_dictation`, `AGENT_BINDING` lease) |
| Bottom-right reply card, revealed in loading state at submit | pre-warmed hidden `agent-panel` webview; `reveal_panel_loading()` |
| Expanded conversation with typed follow-ups | panel EXPANDED mode (`agent_set_panel_mode`), in-window input |
| "Press a shortcut to speak/ask further" | transient `agent_followup` binding (+ pill offer) → `open_followup()` |
| LLM call with provider selection / smart rotation / failover | `run_conversation()` → `run_agent_once` / `agent_run_rotated` |
| Esc/Enter global routing, focus stealing, destroy-on-close | transient shortcut machinery + `build_window` Destroyed hooks |

**What changes is only the brain behind the panel**: in memory mode the
submitted query goes through retrieval → synthesis instead of the generic
assistant prompt. Visual differentiation of the pill/panel (colors, "MEMORY"
chip) is explicitly deferred — for now the surfaces look identical, and that
is fine for testing (user's call, 2026-07-07).

---

## 3. Architecture

### 3.1 Entry point and mode

- New binding **`grain_space_recall`** (suggest `ctrl+shift+m` / macOS
  `option+shift+m`, rebindable), seeded via the usual missing-bindings
  migration, registered ONLY while `grain_space_enabled` (same `grain_space_`
  prefix gating as the others).
- `ACTION_MAP` entry → `agent::summon_memory(app)`.
- `AgentState` gains a mode: `pub mode: Mutex<AgentMode>` where
  `enum AgentMode { Assist, Recall }`, set at each summon, read wherever the
  instruction is dispatched/run. Everything else in `AgentState` is reused
  as-is (conversation retention, input_active, transients).

`summon_memory` = `summon` minus the parts that make no sense for memory:
- **NO selection capture** (no synthetic Ctrl+C — nothing to operate on, and
  the clipboard dance costs ~300 ms of summon latency).
- **NO field-context capture** (UIA read of the focused field is assist-only).
- **NO paste-target HWND snapshot** (recall never pastes).
- Everything else identical: clear stale offer, mark input_active, start
  dictation, `AgentInputShow`, transient Enter/Esc, pre-warm the hidden panel.

Refactor shape: extract the shared body into `fn summon_inner(app, mode)`;
`summon` and `summon_memory` are thin wrappers. Prefer extending over
modifying (upstream-compat boundary).

### 3.2 Dispatch in memory mode

`dispatch_instruction` checks the mode:
- **Recall ignores `agent_quick_enabled`** — Quick Agent pastes at the cursor,
  which is meaningless for a memory answer. Recall ALWAYS opens the panel.
- The pending-instruction queue + `agent-instruction` event + panel mount
  consumption stay exactly as they are.

`agent_run` (the command the panel calls per turn) checks the mode:
- `Assist` → today's `run_conversation` (unchanged).
- `Recall` → `grain_space::recall::run_turn(app, messages)` (new module).

The panel frontend needs **zero changes in R1** — same command, same string
reply (the SOURCES line is stripped backend-side until R2 renders it).

### 3.3 New module: `src-tauri/src/grain_space/recall.rs`

Owns the retrieval pipeline + prompt building, then delegates the actual LLM
call to the agent's existing driver (`run_agent_once` / rotation — expose the
needed pieces as `pub(crate)` rather than duplicating them).

Per user turn:

```
user turn text
   ↓
retrieve(app, turn_text, session)      ← hybrid search, §4
   ↓
memories block (stable M-ids for the whole conversation, §5.2)
   ↓
system prompt + memories + conversation turns → LLM (existing rotation infra)
   ↓
parse trailing "SOURCES:" line → strip from display, resolve M-ids → note ids
   ↓
answer text back to the panel (R1); answer + source refs (R2)
```

Session state (`RecallSession`, stored in `AgentState`, cleared on each
summon): the M-id → note-id registry, so source numbering is stable and
additive across follow-up turns.

### 3.4 Embedding engine lifetime (amended rule)

FINAL-PLAN directive 7 bound the engine to the overlay window. Recall needs it
too. **New rule: the engine may be resident while the overlay window OR the
agent panel in Recall mode is alive; it is dropped when the last of those
goes away.** Implementation: replace the unconditional `shutdown_engine()` in
the overlay's Destroyed hook with `embed::shutdown_engine_if_idle(app)` which
checks both surfaces; call the same from the agent panel's Destroyed path
(cheap no-op when the engine isn't resident — Assist sessions never spawn
it). Feature-off and semantic-off still shut it down unconditionally. Still
zero idle RAM: every surface involved is destroy-on-close.

The existing overlay-gated `grain_space_semantic_search` command is untouched;
recall.rs calls `store::`/`embed::` directly with its own gate (recall
session live).

---

## 4. Retrieval design (what we actually search)

Notes are short (dictated thoughts, clipped selections), the corpus is
personal-scale (hundreds, not millions). Design for **recall over precision**
— the LLM does the final filtering; missing the right note is fatal, an extra
note is a few hundred tokens.

1. **Hybrid, always.** Run BOTH:
   - FTS5 prefix search (existing `store::search_notes` ranking, top 12) —
     wins on exact fragments: names, "hunter2", "Superlist".
   - Semantic KNN (existing `store::semantic_search`, top 12) — wins on
     meaning: "that polished Mac app". Only when `grain_space_semantic` is on
     AND the model is on disk; **degrade to FTS-only silently otherwise** —
     recall must work for users who never opted into the model.
   - Before the semantic leg: re-embed stale rows (existing
     `stale_embed_texts`/`store_embeddings` batch — same as the overlay path).
2. **Merge with Reciprocal Rank Fusion**: `score(note) = Σ 1/(60 + rank_i)`
   across the two lists. No score normalization headaches, standard constant
   k=60. The semantic side already carries recency decay + pin exemption;
   don't re-apply decay after fusion.
3. **Take top K = 6** memories per turn. With per-note caps (below) this
   bounds the block at roughly 2–3k tokens — safe for small models.
4. **Truncation:** per-memory body cap ~1,500 chars (head-biased: keep the
   first 1,200 + last 300 with an `[…]` marker — dictated notes put the
   point up front). Title/tldr always included whole.
5. **Follow-up turns:** retrieve with the RAW new turn text (no query
   rewriting — an extra LLM hop costs latency and the anaphora problem is
   handled differently): the memories block for turn N is the UNION of the
   session's already-shown memories and the new turn's top hits (still capped;
   evict oldest-unused beyond ~10). So "what about the second one?" works
   because the referenced memory is still in context, with the same M-id.
6. **Empty corpus / no hits:** skip the LLM only when the store has zero
   notes ("You haven't saved any memories yet — press <capture shortcut> and
   speak."). With hits-but-weak matches, still ask the LLM — rule 2 of the
   prompt makes it decline honestly.

---

## 5. Synthesis (the prompt — the heart of the feature)

Constraints: the model is whatever the user configured for post-processing —
often small/fast. So: short imperative rules, one simple output convention, no
JSON output (a malformed-JSON retry loop would wreck latency), structured
input blocks.

### 5.1 System prompt (v1 — iterate from here)

```
You are Grain, the user's personal memory. You answer their questions using
ONLY their saved memories, listed below. Current date/time: {local_now}
({weekday}).

Rules:
1. Answer directly in the first sentence — short, natural, conversational.
   The user is mid-flow; no preamble, no headers, no markdown lists unless
   they ask for structure.
2. Use only the memories below. If they don't contain the answer, say so
   plainly ("I don't have a memory of that") — NEVER guess or invent details.
3. When memories conflict, trust the most recent one; mention the older value
   only if the difference matters ("It's 8842 now — you updated it last week;
   before that it was 7731.").
4. If more than one memory could be the answer, lead with the most likely and
   offer the runner-up in one clause.
5. Ask at most ONE short clarifying question, and only when you genuinely
   cannot choose between interpretations.
6. Each memory shows when it was saved. Use that to resolve time references
   ("yesterday", "back in June", "recently").
7. End with exactly one line: `SOURCES: M2, M4` naming only the memories your
   answer actually used. If you used none, write `SOURCES: none`.
```

### 5.2 Memories block (a system message, re-sent fresh each turn)

```
MEMORIES:
[M1] saved 2026-07-06 14:32 (yesterday) · pinned
Title: Wifi Password | Summary: Home network credentials.
The wifi password for the home network is hunter2, router admin is …

[M2] saved 2026-05-12 09:10 (8 weeks ago)
(untitled raw capture)
Superlist — lightweight project management, saw it on Product Hunt …
```

- One block per turn, replacing the previous (the conversation turns
  themselves stay verbatim, so the model keeps its own prior answers).
- Human-readable relative age in parentheses — small models use it far more
  reliably than raw timestamps alone.
- Include todo state inline when present (`Todos: [x] first task, [ ] second`)
  and reminder state when armed — the vision's "state, not documents".

### 5.3 SOURCES parsing (tolerant by design)

Case-insensitive scan of the LAST non-empty line for `sources:`; extract
`M\d+` tokens; strip the line from the displayed answer. Anything malformed →
show the answer as-is, no sources, log it. Never retry, never error the turn
on a bad SOURCES line. This one convention is the entire machine-readable
surface — deliberately minimal.

---

## 6. Surfacing sources (transparency — phase R2)

- `agent_run`'s recall path returns `{ text, sources: [{note_id, title,
  saved_at}] }` (specta type change; Assist mode returns empty sources).
- Panel renders a quiet footer under each assistant message: `Based on 2
  memories` → expands to chips (title + relative date).
- Clicking a chip calls the EXISTING `grain_space_open_window(note_id)` —
  the overlay opens (or refocuses via the `grain-space://focus-note` event)
  on that note. The whole click-through already exists; R2 is UI-only.
- The vision's answer→evidence contract lands here. If R2 slips, R1 is still
  fully usable (user's explicit note: sources may come later; core
  frictionless flow first).

---

## 7. Conversational state (CRUD becomes conversation — phase R3)

The vision's "Actually that's changed / add this / forget that / first two
are done". Design now, build after R1+R2 are solid:

- Same single LLM call, one more output convention (kept as easy as SOURCES):
  an optional final line `ACTION: update M2` / `ACTION: append M2` /
  `ACTION: complete M2 todos 1,2` / `ACTION: forget M2` / `ACTION: remember`,
  followed by a payload block when content is needed.
- Non-destructive actions (append, todo-complete, remember-new, reminder-arm)
  execute immediately through the existing store commands + notes-changed
  event; the answer confirms in words ("Done — marked the first two Rust
  tasks complete.").
- **Destructive actions (forget/overwrite) always confirm in-panel first** —
  one Enter to confirm, Esc declines. Trust is the product; deletion by
  hallucinated intent would kill it.
- The panel already re-renders on `grain-space://notes-changed` surfaces
  (settings tab, overlay), so edits made by conversation appear everywhere.

---

## 8. What survives from the old Phases 5–6 (adopted), and what is dead

**Adopted from old Phase 5:**
- A dedicated voice-retrieval binding (now `grain_space_recall` through the
  agent input, not a bare recording).
- Top-K retrieval feeding a RAG call with recency preference (now §4–5, with
  hybrid fusion instead of semantic-or-FTS).
- The lightweight destroyable answer surface "reusing the agent reply-panel
  pattern" (now literally the agent panel itself).

**Adopted from old Phase 6 (unchanged, final hardening phase R4):**
- Corrupted-JSON quarantine (`notes/corrupt/`).
- INT8/quantized embedding model (also fixes the ~130 MB f32 download).
- Long-note chunked embedding.
- Export all notes; model uninstall button; decay-λ setting UI; overlay
  multi-monitor placement; append while another capture is running.

**Dead:**
- "Mode B: open the overlay with results in a list" as a retrieval outcome —
  the overlay is the manual browser (Mode 2), not an answer surface.
- `grain_space_retrieval_mode` setting (`AiQa | List`): obsolete. Leave the
  setting field dormant (settings-schema compat) but build no UI for it;
  remove field + command in R4 cleanup.
- Any new "carousel"/results UI. The FINAL-PLAN §4 Phase 5 section should be
  read as historical only.

---

## 9. Phases

### R1 — The conversation works (core, everything else depends on it)
1. `grain_space_recall` binding + migration + `ACTION_MAP` → `summon_memory`
   (extract `summon_inner(app, mode)`; add `AgentMode` to `AgentState`).
2. Recall dispatch: ignore quick-agent, route `agent_run` by mode.
3. `recall.rs`: hybrid retrieval (FTS ∪ semantic + RRF, stale re-embed,
   silent FTS-only degrade), memories block with stable M-ids + union across
   turns, system prompt v1, SOURCES parse-and-strip, session registry in
   `AgentState`.
4. Engine lifetime amendment (`shutdown_engine_if_idle` on both Destroyed
   paths).
5. Empty-corpus fast path; all error surfaces through `deliver_agent_error`.
6. Tests: RRF fusion ordering, block formatting (age strings, truncation),
   SOURCES parser (good/missing/garbled), union/eviction across turns.
   Acceptance: speak a fragment → correct answer + follow-up works with the
   model download absent (FTS-only) AND present.

### R2 — Evidence (sources UI)
Typed return `{text, sources}`, panel footer chips, click → overlay focus.
Regenerate bindings.ts.

### R3 — Conversational state
ACTION conventions (append / update / complete todos / remember / forget),
immediate non-destructive execution, in-panel confirm for destructive,
answer-confirms-in-words. Prompt additions kept to ~6 lines.

### R4 — Hardening (adopted Phase-6 list above + retrieval_mode removal +
prompt iteration from real usage).

Protocol unchanged: one phase at a time, commit+push per task, update
`TRANSITION-LOG.md` on every stop, SQLite MCP log (domain `space`).

---

## 10. Edge cases (check every phase against these)

- No LLM provider configured → `deliver_agent_error` with the existing "choose
  one in Post-Processing settings" message; retrieval never runs.
- Semantic on but model deleted from HF cache → silent FTS-only (log once);
  never block an answer on a download.
- Empty/failed transcript → existing "Nothing was heard" panel error.
- Zero notes → canned answer, no LLM call.
- Weak/no hits → LLM still called; prompt rule 2 produces an honest "no
  memory of that". Never fabricate.
- Recall summon while assist input/panel is live (or vice versa) → fresh
  summon supersedes, exactly like today's re-summon path; mode is overwritten.
- Very long conversations → send only the last 8 turns + the memories block
  (the block is authoritative, old turns are style context).
- SOURCES referencing an unknown M-id → drop that ref, keep the answer.
- Capture (quick-add/dictation) landing mid-conversation → store mutex
  serializes; next turn's retrieval sees the new note.
- Panel destroyed mid-LLM-call → reply is discarded (existing behavior);
  engine drop via `shutdown_engine_if_idle`.
- `grain_space_enabled` off → binding never registered; if toggled off while
  a recall panel is open, next `agent_run` in Recall mode errors gracefully
  ("Grain Space is disabled") — and the master-toggle already closes the
  overlay + engine.

## 11. Explicit non-goals (do not build)

- A separate recall window, pill, or input surface — the agent surfaces ARE
  the product surface.
- A query-rewriting / multi-call retrieval chain — one LLM call per turn.
- JSON model output — two tolerant trailing-line conventions max.
- Embeddings in note JSON, WAL, resident services — all prior directives
  stand.
- Competing with note apps: no folders, no tags, no backlinks. The overlay
  stays a plain browser/editor.
