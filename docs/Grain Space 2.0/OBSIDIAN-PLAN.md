# Grain Space × Obsidian — The Vault Backend (the plan)

> **This plan is the 2026-07-10 pivot.** `PRODUCT-VISION.md` still holds: Grain
> Space is an AI memory companion; the conversation is the product. What changes
> is WHERE the memories live. Grain is a **friction-reduction utility, not a
> note application** — so instead of asking users to keep "important" notes in
> Obsidian and "inflow" notes in Grain (a conflict we lose), the entire
> pipeline — capture, structuring, semantic+FTS retrieval, Recall Q&A,
> conversational writes — can now run **on top of an Obsidian vault**: plain
> `.md` files the user already owns, synced by whatever the user already uses.
> Obsidian handles storage maturity (devices, sync, editing, plugins); Grain
> stays the zero-friction capture + recall layer that works even when Obsidian
> is closed.
>
> Read `PRODUCT-VISION.md`, `RECALL-PLAN.md`, and the top `TRANSITION-LOG.md`
> entry first. Phases R1–R4 and the store/recall/capture code are the
> foundation; nothing in them is discarded.

---

## 1. Product shape

- **Two hard-switched backends, no sync between them (v1):**

  | Backend | Source of truth | Status |
  |---|---|---|
  | `grain` (default) | JSON files under `{app_data}/grain_space/notes/` | today's system, unchanged |
  | `obsidian` | `.md` files in a user-chosen vault | THIS PLAN |

  The switch is a setting. Flipping it swaps which corpus every surface
  (capture, recall, overlay, settings tab, reminders) sees. Nothing is
  migrated automatically in v1 (export/import is the manual bridge; a real
  migration assistant is future work).

- **Grain writes only inside its own folder** (default `Grain/`, configurable)
  in the vault. Captures land there as new `.md` files. The user's own notes
  anywhere else in the vault are **searchable and readable through Grain, but
  read-only in v1** — Grain never edits a file the user might have open in
  Obsidian with unsaved changes. "Promote" = the user moves a file out of
  `Grain/` in Obsidian; Grain keeps finding it (identity survives, §4).

- **The whole vault is retrievable.** Recall and the overlay search across
  every `.md` in the vault (hybrid FTS + semantic + RRF, the existing
  pipeline), not just Grain's folder. That is the pitch: ask Grain, get an
  answer sourced from your entire vault, without Obsidian running.

- **Obsidian never needs to be open, and no Obsidian plugin exists.** Grain is
  file-level only: it reads and writes Markdown on disk. A source chip /
  overlay preview shows the note natively; an "Open in Obsidian" action uses
  the `obsidian://open?vault=…&file=…` URI when the user wants the full app.

### Trust framing (for UI copy, keep it honest)
"Your notes live only in your vault, as plain files you already own. Grain
keeps a small local search index on your device so retrieval is instant —
nothing is uploaded except when a capture is processed by AI, same as either
backend." Do NOT claim "we store nothing" (the index is stored data) or
"device-agnostic for free" (sync is whatever the user's vault already uses —
iCloud/Syncthing/Obsidian Sync; Grain inherits it, doesn't provide it).

---

## 2. What the mature ecosystem taught us (research, 2026-07-10)

**Smart Connections** (the dominant semantic-search plugin): local embedding
model (quantized MiniLM ~25 MB via transformers.js/ONNX), embeddings stored
OUTSIDE the notes in its own folder, **block-level chunking** (headings/
paragraphs embedded separately, not whole files), an embedding queue, folder
exclusion settings. Lessons adopted: embeddings never in the notes (we already
do this); **chunked embedding matters for long vault notes** (our notes were
short; vault notes aren't — §7 V3); exclusion list for folders.

**Omnisearch** (the dominant FTS plugin): full in-memory MiniSearch/BM25
re-index on vault open — instant queries, zero persistence. Weights
**filename and headings above body**. Lessons adopted: title(=filename) must
outweigh body in ranking (FTS5 `bm25()` column weights); a full rescan of a
personal vault is cheap enough to do eagerly — which justifies our lazy
reconcile-scan approach (§5) instead of a resident watcher.

**Conflict handling (the stale-buffer problem):** Obsidian itself watches the
vault (chokidar) and, when a file changes on disk while the editor holds
unsaved changes, **auto-merges via diff-match-patch** (its sync product does
three-way merges for .md; "last-modified wins" only for binaries). So the
scary case — Grain writes a file Obsidian has open dirty — is largely handled
BY Obsidian for Markdown, except in environments where its watcher is
unreliable. Our defense in depth (§6): new-file-per-capture by default
(no shared file to conflict on), atomic tmp+rename writes, an mtime guard on
every read-modify-write, and no editing of non-Grain notes at all in v1.

**Auto-categorization** (Auto Classifier / Metadata Auto Classifier plugins):
one LLM call, the vault's EXISTING tag/folder taxonomy injected as the
reference list, output written to frontmatter. Nothing exotic — it maps
directly onto our existing `extract_metadata` structured call (§7 V4).

---

## 3. Note format on disk (vault backend)

Clean Markdown + YAML frontmatter — Obsidian-native, plugin-compatible,
human-editable. AI metadata lives ONLY in frontmatter; the body is never
polluted and stays the verbatim capture (locked invariant).

```markdown
---
grain_id: 8f2a1c9e-…            # identity, survives rename/move
tldr: Reduce pill RAM via WOFF2 font compression.
created: 2026-07-10T14:22       # local wall clock
pinned: false                   # omitted when false
todos:
  - "[ ] compress fonts"
  - "[x] measure baseline"
reminder: 2026-07-11T09:00      # omitted when none
reminder_status: armed          # pending|armed|fired|dismissed
source: grain                   # provenance marker
---
The wifi password for the home network is hunter2, router admin is …
```

- **Filename = title** (Obsidian convention). Sanitized (`\/:*?"<>|` stripped,
  length-capped), collision-suffixed (` 2`, ` 3`). A title edit through Grain
  renames the file; `grain_id` keeps identity stable.
- `Note` struct mapping: `title` ← filename stem, `body` ← content below
  frontmatter, `timestamp` ← `created` (fallback: file mtime), the rest ←
  frontmatter keys. The locked `Note` schema is the **wire type** everywhere
  above the store layer — frontend, recall, capture see zero difference.
- **Foreign notes** (no `grain_id`, anywhere in the vault): `title` ←
  filename, `body` ← content minus any frontmatter block, `timestamp` ← file
  mtime, everything else default. Read-only in v1.
- **YAML handling is minimal and ours**: we strip/emit a flat frontmatter
  block with a small hand-rolled codec (no serde_yaml — unmaintained — and no
  new heavy dep). Foreign frontmatter is treated as an opaque block: stripped
  for body/FTS purposes, preserved byte-for-byte if we ever rewrite a file we
  didn't create (we don't, in v1).

---

## 4. Identity (ids must survive rebuilds, renames, and the frontend round-trip)

- **Grain-owned notes:** `grain_id` frontmatter (uuid, minted at capture).
  Rename/move anywhere in the vault → same note.
- **Foreign notes:** deterministic id = hex hash (SHA-256, truncated) of the
  vault-relative path. Stable across index rebuilds with zero writes into the
  user's file; passes the existing `validate_id` charset. A rename changes the
  id — acceptable for read-only search results (the index reconcile treats it
  as remove+add).
- The index maps `id → relative path` (§5); `get_note(id)` resolves through it.

---

## 5. Indexing: lazy reconcile scan, not a resident watcher

**Decision (deviation from the advisory notes, which suggested a `notify`
file watcher):** no resident watcher in v1. Grain's core identity is zero idle
RAM / no unnecessary engines, and a watcher is a resident thread + OS handles
that must be supervised. Instead:

- `vault_index.sqlite` lives in `{app_data}/grain_space/` (NEVER inside the
  vault — we don't pollute it beyond our notes folder). Same schema family as
  today's index plus: `path TEXT`, `mtime INTEGER`, `size INTEGER`,
  `foreign INTEGER` on `notes_meta`.
- **`reconcile(vault)`** — a stat-walk of `*.md` under the vault (skipping
  `.obsidian/`, `.trash/`, `.git/`, configurable exclusions): compare
  `(mtime, size)` against the index; re-parse + re-FTS + mark `embed_stale=1`
  for changed files, insert new, drop vanished. A few thousand stats is
  milliseconds; Omnisearch full-re-indexes whole vaults in memory on every
  open and nobody notices.
- Reconcile runs at the **start of every retrieval entry point** (overlay
  search/list, recall turn) and after Grain's own writes. Between queries,
  literally nothing runs — the freshness contract is "correct at the moment
  you ask", which is the only moment that matters.
- Embeddings stay lazy exactly as today: reconcile only marks stale; the
  semantic leg re-embeds stale rows when the engine is next resident.
- **Escalation path:** if reconcile latency measurably hurts on huge vaults
  (>20k files), add a `notify-debouncer-full` watcher gated on
  (feature on ∧ vault backend ∧ a Grain surface open). Not before.

---

## 6. Write safety (the stale-buffer defense, layered)

1. **New file per capture** (the default write): `create_new` semantics via
   tmp + rename with collision suffix — never overwrites anything, no shared
   file to race on. This alone covers the overwhelmingly common path.
2. **Atomic writes always**: tmp file in the same directory, `fs::rename` over
   the target (same pattern as the JSON store).
3. **mtime guard on read-modify-write** (reconcile/pin/todo/reminder edits of
   Grain-owned notes): snapshot mtime at read; before the rename, stat again;
   if it moved, re-read and re-apply the change onto the fresh content (one
   retry, then last-write-wins with a warning log). Obsidian's own
   auto-merge covers the other direction (it merges our disk write into an
   open dirty buffer).
4. **Foreign notes are read-only in v1.** The only files Grain rewrites are
   ones it created, which the user is far less likely to have open dirty.
5. Never write anything under `.obsidian/`.

---

## 7. Phases

### V1 — Backend abstraction + vault store (foundation)
1. Settings (grain-core): `grain_space_backend` (`grain` | `obsidian`,
   default `grain`), `grain_space_vault_path` (string), `grain_space_vault_folder`
   (default `"Grain"`). Change-commands + specta registration.
2. New `grain_space/vault.rs`: frontmatter codec (parse/emit, tests),
   filename sanitize/collision, Note ↔ .md mapping, atomic + mtime-guarded
   writes, foreign-note read-only enforcement, `reconcile()` scan,
   `vault_index.sqlite` (FTS5 + vec + path/mtime columns), all store ops
   (`list/search[_ranged]/get/save/delete/set_pinned/set_reminder/rebuild/
   stale_embed_texts/store_embeddings/semantic_search[_ranged]/export`).
3. New `grain_space/backend.rs`: `Backend` enum resolved from settings; the
   dispatch surface every caller uses. `store.rs` becomes the grain-backend
   implementation, untouched.
4. Swap the ~28 call sites (commands.rs, capture.rs, recall.rs, reminders.rs)
   from `store::` + `base_dir` to the backend dispatch. Zero behavior change
   with backend = `grain`.
5. Tests: frontmatter round-trip (incl. quoting/multiline bodies/CRLF),
   foreign-note mapping, id hashing, collision suffixes, reconcile
   add/change/remove, mtime guard, read-only enforcement, bm25 title weighting.

### V2 — Full wiring + UI
1. Recall + overlay + settings tab run against the vault backend end to end
   (they already will via dispatch; verify + fix edge cases: empty-corpus
   copy, reminders sync over vault notes, delete semantics).
2. Settings UI: backend switch, native folder picker for the vault, Grain
   subfolder field, exclusion list. Overlay: read-only badge on foreign
   notes; "Open in Obsidian" action (`obsidian://` URI) on every note.
3. FTS ranking: bm25 weights title > tldr > body (both backends benefit).
4. bindings.ts regeneration; tsc/eslint clean.

### V3 — Retrieval hardening for real vaults
- **Chunked embedding for long notes** (heading/paragraph blocks, Smart
  Connections-style; the vec table gains a chunk dimension; KNN dedupes to
  note level). This was already in the R4 backlog; a vault makes it real.
- Rename detection in reconcile for foreign notes (content-hash match) so a
  rename doesn't lose its embedding.
- Windows path/long-vault stress, exclusion globs, `.md` in hidden dirs.

### V4 — Auto-categorization + promote flow (future, learned from Auto Classifier)
- At capture: one structured LLM call (the existing `extract_metadata`
  extended) receives the vault's existing folder list + tag taxonomy as
  reference and returns a destination subfolder and/or `tags:` frontmatter —
  the note lands pre-filed instead of in the inbox folder. Sparingly:
  suggestions must come FROM the user's own taxonomy, never invented.
- Promote affordance in the overlay (move out of `Grain/`, keep identity).

## 8. Non-goals (v1)
- No sync/migration between the two backends (export/import is the bridge).
- No Obsidian plugin, no reading `.obsidian/` config (vault = folder of .md).
- No editing foreign notes through Grain surfaces.
- No resident watcher, no background indexing daemon.
- No wikilink/backlink/graph features — Obsidian owns those.
