# Grain Space — Things To Work & What We Can Learn From Reor

> **Purpose:** one place for (1) work that still needs to happen in Grain Space,
> and (2) a clear read of the **Reor** reference repo
> (`C:\Projects\Grain\grain\Refrence\reor-main`) — what they do, what we share,
> what we must **not** copy, and what we can adapt under Grain’s constraints.
>
> **Read first:** `PRODUCT-VISION.md`, `RECALL-PLAN.md`, `SEARCH-OVERHAUL-PLAN.md`.
> **Shipped foundation:** flat JSON notes + FTS + optional BGE embeddings,
> Capture A/B/C, overlay browser, Grain Recall (hybrid top‑6, sources, ACTION
> writes). This file is the gap + adaptation map, not a second product vision.

---

## 1. Same problem space, different philosophy

| | **Reor** | **Grain Space** |
|---|---|---|
| **Product** | Local AI **note-taking / PKM** app | Local AI **memory companion** |
| **Primary UI** | Always-open vault: editor + sidebars + chat | Tray utility: destroy-on-close surfaces |
| **User goal** | Write notes; AI links & answers from the corpus | Speak/type a fragment → **answer first**; notes = evidence |
| **Storage** | Markdown files in a vault directory | Flat JSON notes under `grain_space/notes/` |
| **Index** | LanceDB (vectors in-table); chunk-level rows | SQLite FTS5 + optional `vec0`; **one row per note** |
| **Embeddings** | Transformers.js (ONNX), often resident with app | Candle BGE, **only while overlay or Recall agent is alive** |
| **RAG style** | Chat agents + tools; optional pre-fetch N chunks | One panel brain + trailing conventions (`SOURCES` / `ACTION` / planned `SEARCH:`) |
| **Memory policy** | App is the workspace; index/models stay for the session | **Destroy if not in use**; zero idle RAM for Space |

**Short version:** Reor is “a note app with semantic brain always nearby.”
Grain is “a memory you talk to, that only loads the brain while you’re talking
or browsing.” Both solve **remember + retrieve + edit with AI**. Grain refuses
the “always indexed, always warm” cost model.

### Hard Grain constraints (never trade these away)

1. **Embedding model is not a background service.** It may load only when:
- the Grain Space **overlay** is open (note browser), or
- the user is in an **active Agent session** that needs it (Recall turn /
semantic path — voice or text).
2. When those surfaces close → **engine drops**. Full stop. No idle timers
keeping weights warm “just in case.”
3. **No always-on notes window.** Create-on-summon, destroy-on-close.
4. **FTS must work without the embed model** (silent degrade).
5. **Correctness → RAM/CPU → maintainability.** Prefer zero-overhead patterns
over new engines.
6. Extra search/LLM work is allowed **only during an active turn** (see
`SEARCH-OVERHAUL-PLAN.md`).

---

## 2. What Reor actually does (reference map)

Path: `C:\Projects\Grain\grain\Refrence\reor-main`.

### 2.1 Indexing & storage

- **Vault = directory of markdown files** (Obsidian-like).
- On add/update: **chunk** content (markdown headings, then recursive char
split if big — `electron/main/common/chunking.ts`), strip markdown for
embed text, write **chunk rows** to LanceDB (`notepath`, `content`,
`subnoteindex`, timestamps, vector).
- Re-index a file = delete all chunks for that path, re-add (batched).
- Schema is **chunk-centric RAG**, not “one memory object.”

### 2.2 Embeddings

- `@xenova/transformers` feature-extraction pipelines.
- Default models include `Xenova/bge-small-en-v1.5` (same family as Grain’s
BGE-small), plus larger/multilingual options (`UAE-Large`, e5, jina-*).
- Mean pool + L2 normalize; HF cache under app userData.
- Embedding function is bound to the Lance table; search embeds the query
through the same path.
- **Philosophy:** model is part of the long-lived PKM app session — not
destroy-with-window.

### 2.3 Search

- **Vector (cosine)** via LanceDB `.search(query).limit(n)` with optional
**SQL prefilter** (especially **date range on `filemodified`**).
- **Hybrid** (`src/lib/db.ts`): vector results + keyword scoring on content
(regex term hits), then weighted combine (default ~70% vector / 30%
keyword). Toggle vector-only vs hybrid in the sidebar search UI.
- UI can fetch **large limits** (e.g. 50) for browsing similar notes.
- Chat can set **limit + minDate + maxDate** before the first message
(`DBSearchFilters`).

### 2.4 Chat / agent tools (this is the big product parallel)

Reor’s LLM is not stuck with one pre-fetched context bag. Agents get **tools**
(`src/lib/llm/tools/tool-definitions.ts`):

| Tool | Role |
|------|------|
| `search` | Query knowledge base; limit; **minDate / maxDate**; often **full note** into context |
| `createNote` / `createDirectory` | Create (confirm) |
| `readFile` | Read a path (confirm) |
| `editNote` / `appendToNote` | Mutate notes (confirm) |
| `deleteNote` | Delete (confirm) |
| `listFiles` | Enumerate vault |

`search` is **`autoExecute: true`** — the model can re-query mid-conversation
without a human clicking “search.” That is exactly the capability Grain needs
for “I’m talking about a project, then ask to change the wifi password” when
the initial top‑6 are wrong.

Chat also supports **initial RAG** (fetch N chunks into the first user turn)
and **source chips** that open the note in the editor.

### 2.5 “Related notes while writing”

While a note is open, a **Similar entries** sidebar embeds the current text
and shows vector neighbors. That is continuous corpus cross-linking — powerful
for PKM, expensive for Grain’s idle model.

### 2.6 UX packaging

- Full Electron multi-window vault manager.
- Rich TipTap editor, slash menu, backlinks, indexing progress UI.
- Ollama + remote LLMs; agent configs with templates and tool lists.
- Sources UI under answers (open file from chat).

---

## 3. What Grain already has (so we don’t re-invent blindly)

| Capability | Grain today |
|------------|-------------|
| Capture voice/text | Agent **Capture** mode (headless compose + save) |
| Capture selection | Quick-add raw note; Capture framing |
| List / edit / pin / todos / reminders | Overlay + settings |
| Keyword search | FTS5 (always) |
| Semantic search | BGE + vec0, **only with overlay or Recall path** |
| Hybrid retrieve | FTS ∪ semantic, **RRF**, top **6**, stable **M-ids** |
| Answer-first UI | Agent panel + evidence chips + NOT_FOUND |
| Conversational write | `ACTION: update/remember/complete/forget` |
| Continuous chat | Expanded panel; session M registry across follow-ups |
| Portability | Export JSON; rebuild index |

**The hole the user described is real:** mid-conversation topic switch (e.g.
project talk → “change the wifi password”) often **fails** because we only
hand the model ~6 notes from the **latest turn’s** retrieve, and the model
**cannot request another search**. Reor solves that with a **search tool**.
Grain’s planned equivalent is **`SEARCH:`** (see overhaul plan) — same idea,
convention-based, active-turn only, no always-on tool runtime.

---

## 4. What we can adapt from Reor (under our constraints)

### 4.1 High value — align with `SEARCH-OVERHAUL-PLAN.md`

| Reor idea | Grain adaptation | Constraint fit |
|-----------|------------------|----------------|
| **Agent can search again mid-turn** (`search` tool, auto-execute) | **`SEARCH: <query>`** convention + bounded hops (max 2) in `run_turn` | Active turn only; embed only if semantic on and surface alive |
| **Wider candidate pool then narrow** (limit 20+ then use top) | **S1: pool ~20 → heuristic rerank → 6** | No second model; pure CPU |
| **Date-scoped search** (`minDate`/`maxDate` on tools + filters) | Teach Recall prompt + optional FTS/meta filter for “last week / June” | FTS/meta works offline without embed; semantic still gated |
| **Write tools with confirm** (edit/append/delete) | Already have ACTION + forget confirm; extend with **SEARCH then ACTION** + **0/2+ match → clarify** | Matches overhaul S3 write guardrail |
| **Sources open the note** | Already: chips → `grain_space_open_window(id)` | Keep; polish titles/empty corpus |
| **Full note into context after hit** (not only chunk) | We already inject **whole note** (truncated body) into M-block | Prefer keep whole-note; chunking is optional later only if bodies explode |
| **Hybrid blend of semantic + lexical** | We use **RRF** (cleaner than Reor’s post-hoc keyword re-score on vector hits). Optional: add **title/tldr term boost** in S1 rerank (Reor-like keyword emphasis without their architecture) | Good |
| **Pass “full user query” to search** (tool description stresses this) | When model emits `SEARCH:`, use a **focused** query, but also consider **union of last user turn + SEARCH string** for retrieve | Active turn only |
| **Larger first-pass limit for chat** (configurable N) | Fixed 20 candidates then 6 in block; avoid Reor-style “50 chunks in prompt” | Token/RAM discipline |

### 4.2 Medium value — maybe later, carefully

| Reor idea | Notes for Grain |
|-----------|-----------------|
| **Multiple embed models** | We locked BGE-small for size; multi-model tables (Reor per-model Lance names) fight “one engine, drop when idle.” Skip unless product needs multilingual badly. |
| **Chunking long notes** | Personal “memories” are usually short. If bodies grow huge: chunk for **embedding only**, keep JSON as whole note (don’t become a markdown vault). |
| **Strip markdown before embed** | If we ever store markdown-ish bodies, strip like Reor; today plain text is fine. |
| **Keyword-only weight toggle** | Overlay already has exact vs semantic; could add “exact only” force in Recall when semantic fails (we already FTS-fallback). |
| **Indexing progress UI** | Only if batch re-embed of many stale notes is slow; keep progress events (we already have download progress). |
| **Similar-notes sidebar while editing** | **Conflicts with destroy-if-idle** if it forces always-warm embed. Only acceptable if overlay is already open and user opts into “related” (same engine already resident). Low priority vs Recall quality. |

### 4.3 Do **not** adapt (philosophy / RAM)

| Reor behavior | Why not in Grain |
|---------------|------------------|
| Embed model resident for the whole app session | Violates destroy-if-not-in-use |
| Background / continuous re-index on every keystroke | We mark `embed_stale` and re-embed on **next active semantic use** |
| LanceDB + Electron + Transformers.js stack | We already have SQLite + Candle; rewrite cost high, little win |
| Full PKM editor as primary product surface | Vision: answer first, browser secondary |
| Auto-link graph as core UX | Nice-to-have, not the memory-companion loop |
| Tool framework with many confirm dialogs for every read | Prefer trailing-line protocol + one forget confirm; keep latency low |
| Shipping large default embed models (UAE-Large, multilingual e5) | Against low-RAM default; stay opt-in small BGE |

---

## 5. Things that need to be worked (backlog)

Grouped by priority. Items marked **(plan)** are specified in
`SEARCH-OVERHAUL-PLAN.md`. Items marked **(Reor)** are inspired by the
reference. Items marked **(gap)** are product/engineering holes from current
Grain behavior.

### P0 — Retrieval that doesn’t get stuck on the wrong 6

1. **(plan) S1 Dual-stage retrieve**  
Fuse to ~20 candidates → heuristic rerank (RRF + title/tldr term overlap +
existing recency decay) → top 6 into the memories block.  
*Files: `recall.rs`. Tests for ordering.*

2. **(plan / Reor) S2 Agentic `SEARCH:` on reads**  
Model may reply with only `SEARCH: <query>`; `run_turn` retrieves, unions
into `RecallSession` (stable M-ids), rebuilds block, re-calls LLM.  
**Max 2 hops per turn.** After cap, force answer / clarify / NOT_FOUND.  
*This is Reor’s search tool, Grain-shaped.*

3. **(plan / Reor) S3 Agentic writes with disambiguation**  
“Change the wifi password” → model `SEARCH:`s first → then:  
- **1 match** → `ACTION: update Mn`  
- **0 or 2+** → **always clarify** (never silent wrong edit)  
- Prompt already specific → act without question  
Forget still in-panel confirm.

4. **(gap) Topic switch mid-conversation**  
Explicit test cases: multi-turn about Project A, then “update home wifi to
X.” Must `SEARCH:` (or equivalent), not invent from empty/wrong block.

### P1 — Quality of answers and writes

5. **(gap) Prompt / small-model adherence**  
SOURCES / NOT_FOUND / ACTION (and soon SEARCH) compliance is fragile on
small models. Iterate prompt; consider few-shot one-liners; never retry
loops that burn idle resources — only active-turn bounded retries if any.

6. **(gap) Reconcile reliability**  
`reconcile_note` LLM merge vs raw-append fallback. Wrong merges are worse
than append. Tune prompt; maybe prefer append when confidence low / no
structured output.

7. **(Reor) Temporal queries**  
“What did I save last week?” — Reor uses date filters on search. Add:
- prompt rules that expand relative dates using “now”, and/or  
- optional meta filter on `timestamp` in retrieve when query looks temporal  
(works with FTS-only; no embed required).

8. **(gap) Empty / thin corpus UX**  
Already special-case zero notes. Improve “one weak match” vs NOT_FOUND;
avoid fishing with endless clarifying questions (prompt rule 8 exists —
verify in real use).

9. **(gap) Source chip quality**  
Untitled raw notes → weak chips. Prefer first body line / tldr; match
Reor’s “open underlying note” polish.

### P2 — Capture & feedback

10. **(gap) Headless Capture confirmation**  
Capture saves with no panel. User needs a clear, low-RAM “saved” signal
(native toast / tray / short sound / pill flash — not a permanent webview).

11. **(gap) Capture metadata quality**  
Title/tldr/todos/reminders when post-process is off or Apple Intelligence
(no structured path) → raw notes only. Document; optionally light
heuristics without LLM for title = first 3 words.

12. **(gap) Selection + framing edge cases**  
Empty STT + empty selection; huge selection; framing that model wrongly
folds into body (prompt already forbids — verify).

### P3 — Overlay / browser (secondary surface)

13. **(gap) Overlay is search + manual edit only**  
No voice-append / ask-AI inside the browser by design. If we add “ask
about this note,” it should open **Recall** or a short agent turn, not a
second always-on chat engine in the overlay.

14. **(Reor, optional) Related notes while overlay is open**  
Only if embed engine is **already** resident; destroy with window. Do not
build continuous background similarity.

15. **(gap) Semantic UX**  
Consent download, uninstall, f16, model missing errors — tighten copy and
recovery paths.

16. **(gap) Large-body performance**  
List/scan all JSON; FTS rebuild. Fine at personal scale; watch if corpus
grows (pagination / incremental list later).

### P4 — Optional advanced retrieval (only if P0 proven insufficient)

17. **(plan) S4 Cross-encoder reranker**  
Opt-in second model; same lifecycle as embed (load only with surfaces).
Do **not** ship before S1 heuristic is proven wanting.

18. **(Reor) Multi-embedding / multilingual models**  
Only if English BGE is a real product failure. Prefer one small model.

19. **(Reor) Chunked embedding for huge notes**  
Embed chunks, retrieve chunk, load full note into M-block. Storage stays
one JSON note. Only if needed.

### P5 — Product polish / differentiation

20. **(gap) Visual “MEMORY” mode** on pill/panel (deferred in RECALL-PLAN).  
21. **(gap) Settings discoverability** of the four Space shortcuts.  
22. **(gap) Export / import round-trip** (export exists; import still thin).  
23. **(gap) Telemetry-free offline eval set** of recall queries for regression
(wifi password, app name fragment, date-relative, multi-todo complete).

---

## 6. Mapping: user pain → Reor → Grain plan

| User pain | Reor does | Grain will do |
|-----------|-----------|---------------|
| Initial 6 are wrong | Tools call `search` again with new query / filters | `SEARCH:` hops (S2), max 2 |
| “Change wifi password” while talking about something else | `search` → `editNote` / `appendToNote` (confirm) | `SEARCH:` → match count → `ACTION: update` or clarify (S3) |
| “What last week?” | `minDate`/`maxDate` on search | Temporal filters + prompt (P1) |
| Need better ranking | Hybrid weights + high limit | S1 20→6 rerank; RRF already; optional reranker S4 |
| Always-on RAM | Accepts long-lived Electron + models | **Refuse** — engine only with overlay / active Recall |
| PKM file tree | First-class | Not the product; overlay is secondary |

---

## 7. Suggested build order (execution)

```
1. S1 heuristic dual-stage (20 → 6)          ← SEARCH-OVERHAUL
2. S2 SEARCH: loop for reads                 ← Reor search tool, Grain protocol
3. S3 SEARCH: + write disambiguation         ← Reor edit tools, safer
4. Capture “saved” feedback                  ← product gap
5. Temporal filters / prompt polish          ← Reor date filters, light
6. Reconcile + source chip polish
7. Eval set + prompt iteration
8. S4 reranker only if still failing
```

**Invariants while building:** one embed engine; drop with surfaces; M-ids
stable within session; silent FTS fallback; no fabricate; writes never silent
when 0 or 2+ matches after search.

---

## 8. File / code touch map (when implementing)

| Area | Primary paths |
|------|----------------|
| Retrieve / SEARCH / prompt | `src-tauri/src/grain_space/recall.rs` |
| Compose / reconcile | `src-tauri/src/grain_space/capture.rs` |
| Store / FTS / vec | `src-tauri/src/grain_space/store.rs`, `embed.rs` |
| Agent modes / dispatch | `src-tauri/src/agent.rs` |
| Panel evidence / forget | `src/components/agent/AgentPanel.tsx` |
| Overlay search | `src/components/grain-space/GrainSpaceOverlay.tsx` |
| Plan docs | this file, `SEARCH-OVERHAUL-PLAN.md`, `TRANSITION-LOG.md` (update on stop) |

Reference only (do not vendor):  
`Refrence/reor-main/src/lib/db.ts`,  
`.../llm/tools/tool-definitions.ts`,  
`.../vector-database/*`,  
`.../common/chunking.ts`.

---

## 9. One-sentence summary

**Reor proves that agentic re-search + hybrid ranking + date filters +
tool-based writes make personal RAG usable; Grain already has the memory
surfaces and low-RAM skeleton — the work is to bring Reor’s *agentic search
and safer writes* into Recall via bounded `SEARCH:` / dual-stage ranking,
without ever becoming a always-on note app with a resident embedding model.**
