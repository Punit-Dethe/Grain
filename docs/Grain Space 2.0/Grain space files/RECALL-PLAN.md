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

**NON-NEGOTIABLE: Recall has its OWN shortcut, distinct from the Agent's.**
The user differentiates the two features by which key they press — NOT by the
AI deciding at runtime whether a request is "assist" or "memory". The mode is
fixed at summon time by the binding that fired, stamped into `AgentState`, and
never re-derived. The Agent brain never routes. `summon_agent` → Assist;
`grain_space_recall` → Recall. Two doors, never one door with a bouncer.

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
8. If — after looking at every memory below — the thing the user is asking
   about is genuinely NOT among them (not merely thin, but absent), do not
   keep asking questions to fish for it. Give one honest sentence
   ("I don't have a memory about that") and make your LAST line exactly
   `NOT_FOUND` (in place of the SOURCES line). Use this ONLY when you are
   confident the memory does not exist — never as an escape from a hard
   question when a relevant memory IS present.
```

**Anti-loop / anti-hallucination framing.** There is no fixed cap on
conversational turns; the model self-terminates. Two failure modes to guard
against in the prompt wording: (a) inventing an answer when the memory is
absent — rule 2 forbids it; (b) endlessly asking clarifying questions instead
of admitting absence — rule 8 gives a clean terminal state (`NOT_FOUND`).
Rule 5 already caps clarifying questions at one, so the model cannot stall by
interrogating. `NOT_FOUND` is a distinct terminal token (not prose) precisely
so the frontend renders the escape hatch deterministically instead of
pattern-matching "I couldn't find" out of free text.

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

### 5.3 SOURCES / NOT_FOUND parsing (tolerant by design)

Case-insensitive scan of the LAST non-empty line:
- `not_found` (whole line) → set `not_found = true`, strip the line, resolve
  no sources. The panel renders the escape-hatch button (§6.1).
- `sources:` → extract `M\d+` tokens, strip the line, resolve to note ids.
- neither → show the answer as-is, no sources, no button; log it.

Anything malformed → show the answer as-is. Never retry, never error the turn
on a bad final line. These two trailing tokens are the ENTIRE machine-readable
surface — deliberately minimal so a small model rarely breaks the contract.

### 5.4 The confident "not found" escape hatch

When retrieval returns hits but none actually match (the prompt's rule 8
fires) the model answers `NOT_FOUND`. That is a signal, not a failure — the
user asked for something Grain doesn't remember, and the right move is to hand
them the manual browser rather than loop. `not_found = true` flows back to the
panel; §6.1 renders the button. This is distinct from a provider/transcript
error (which goes through `deliver_agent_error`).

---

## 6. Surfacing sources + not-found (panel UI — phase R2)

R1 keeps the panel frontend untouched (recall returns a plain string; the
not-found answer reads as ordinary text). R2 adds the panel footer for BOTH
provenance and the escape hatch — they are the same kind of change (a quiet
strip under each assistant message) so they ship together.

- `agent_run`'s recall path returns `{ text, sources: [{note_id, title,
  saved_at}], not_found }` (specta type change; Assist mode returns empty
  sources / `not_found = false`).

### 6.1 Not-found button
- When `not_found`, the panel renders a single quiet button under the answer:
  **"Couldn't find that memory — open your notes"**.
- Click → `grain_space_open_window(null)` (existing command): the manual
  memory browser opens with the newest note selected. `null` = no focus
  target, so the user immediately searches/browses themselves.
- This is the ONLY thing the user must do manually when Grain genuinely
  doesn't remember — one click from "I don't have that" to the full browser.

### 6.2 Source chips
- When `sources` is non-empty, a quiet footer: `Based on 2 memories` →
  expands to chips (title + relative date).
- Clicking a chip calls the EXISTING `grain_space_open_window(note_id)` — the
  overlay opens (or refocuses via `grain-space://focus-note`) on that note.
  The whole click-through already exists; this is UI-only.
- The vision's answer→evidence contract lands here. If R2 slips, R1 is still
  fully usable (sources/button may come later; frictionless flow first).

---

## 7. Conversational writing (CRUD becomes conversation — phase R3)

The vision's "Actually that's changed / add this / forget that / first two are
done". The user navigates to a memory by voice ("you remember the Rust tasks
note?" → Grain answers yes and shows it), then speaks a change ("the first two
are done, and add: refactor the parser"). Grain must fold that change into the
note **the same way capture created it** — not by dumb text concatenation.

### 7.1 The reconcile call (the key mechanism)

Editing a note conversationally re-runs a structured LLM pass that mirrors
`capture::extract_metadata`, but for MERGE instead of creation. New helper
`grain_space::capture::reconcile_note(app, current: &Note, change: &str,
convo_context)` → structured JSON, reusing the SAME infra
(`send_chat_completion_with_schema`, `strip_code_fences`, `record_usage`,
`llm_usable` gate). It is the capture pipeline's sibling, deliberately in the
same module.

**What it returns and how conservative it is** — the whole point of the user's
note "in a conservative manner":

| Field | Rule |
|---|---|
| `body` | Incorporate the new information. APPEND by default; only restructure/rewrite when the change genuinely supersedes existing wording (e.g. "the password changed to X" rewrites the password, keeps the rest). Never drop content the user didn't ask to remove. |
| `title` | Re-derive the 3-word title ONLY if the note's core subject shifted. A note that gains one todo keeps its title. Prompt: "keep the existing title unless the note is now about something different." |
| `tldr` | Update to reflect the merged content (one sentence, as in capture). |
| `todos` | Merge: add new items, mark named ones done, remove ones the user says to drop. Preserve existing `done` states and order where unchanged. |
| `reminder` | Re-derive per capture's rules only if the change mentions timing. |

The prompt is handed the current note as JSON + the spoken change + a little
conversation context (so "the first two" resolves against the todos it just
showed). Output schema = capture's ExtractedMeta plus `body`. **Degrade
safely** (capture's Input-A→B rule): on any LLM/parse failure, fall back to a
plain append of the raw `change` text to the body and leave title/tldr/todos
untouched — the user's words are never lost.

`reconcile_note` writes via `store::save_note` (which re-indexes + marks the
embedding stale, so the memory stays searchable with its new content) and
emits `grain-space://notes-changed`, so the overlay + settings tab refresh
automatically.

### 7.2 ACTION convention (routes to the right operation)

One more tolerant trailing line on the SAME single LLM turn, as easy as
SOURCES (they're mutually exclusive per turn — a turn either answers or acts):

- `ACTION: update M2` / `ACTION: append M2` / `ACTION: remember` → run
  `reconcile_note` on that memory (or create a fresh note for `remember`),
  using the user's turn text as the change. Non-destructive → execute
  immediately; the spoken answer confirms ("Done — marked the first two done
  and added the parser refactor.").
- `ACTION: forget M2` → **destructive: confirm in-panel first** (one Enter
  confirms, Esc declines) before `grain_space_delete_note`. Trust is the
  product; deletion by hallucinated intent would kill it.
- `ACTION: complete M2 todos 1,2` → a pure todo-state flip needs no LLM merge;
  apply directly via `store::save_note` (cheaper than a reconcile round-trip).

The reconcile call is a SECOND LLM hop only for `update/append/remember`
turns; plain Q&A and todo-completion stay single-hop. That is an acceptable
cost — editing is rarer than asking, and correctness > latency for writes.

The panel already re-renders on `grain-space://notes-changed`, so edits made
by conversation appear in the overlay and settings tab live.

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
   turns, system prompt v1 (incl. rule 8 / `NOT_FOUND`), SOURCES+NOT_FOUND
   parse-and-strip, session registry in `AgentState`.
4. Engine lifetime amendment (`shutdown_engine_if_idle` on both Destroyed
   paths).
5. Empty-corpus fast path; all error surfaces through `deliver_agent_error`.
6. Tests: RRF fusion ordering, block formatting (age strings, truncation),
   final-line parser (sources good/missing/garbled, not_found), union/eviction
   across turns. Acceptance: speak a fragment → correct answer + follow-up
   works with the model absent (FTS-only) AND present; asking for something
   absent yields an honest not-found (no invention, no endless questions).

### R2 — Evidence + escape hatch (panel UI)
Typed return `{text, sources, not_found}`; panel footer: source chips (click →
overlay focus) and the not-found button (click → `grain_space_open_window(null)`).
Regenerate bindings.ts. R1's not-found answer already reads fine as plain
text; R2 makes the escape hatch one click.

### R3 — Conversational writing
`reconcile_note` structured merge (§7.1) reusing the capture infra; ACTION
conventions (append / update / remember / complete-todos / forget), immediate
non-destructive execution, in-panel confirm for `forget`, answer-confirms-in-
words. Safe degrade to raw-append on LLM failure. Prompt additions ~8 lines.

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
  memory of that". Never fabricate. Genuinely absent → `NOT_FOUND` terminal
  state + escape-hatch button (§6.1); the model must not loop on questions.
- Conversational edit whose reconcile LLM call fails/malforms → raw-append the
  change text to the body, keep the rest (never lose the user's words).
- `forget` action → in-panel confirm before delete; Esc cancels.
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
