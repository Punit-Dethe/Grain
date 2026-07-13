# Auto-Categorization — Execution Plan

> Status: PROPOSED (2026-07-13). Un-defers the "AUTO-CATEGORIZATION deferred by
> user" note. Scope: the Obsidian/native Grain Space vault. Philosophy gate:
> **zero idle RAM** — no resident model, no background thread, no work while the
> window is hidden and no capture is in flight.

## 1. What the user asked for

"AI should create new categories (folders) once it sees repetition — 'this is
repeating, let me align this into one' — and then new notes get auto-arranged
into the right folder." Bleeding-edge quality, but never violating the low-RAM
philosophy: when nothing is being interacted with, **nothing runs**.

## 2. What the bleeding edge actually does (research)

- **TnT-LLM** (Microsoft, KDD 2024, arXiv:2403.12173) — the SOTA pattern.
  Splits the problem in two: (1) an EXPENSIVE, rare **taxonomy-generation** pass
  where an LLM iteratively proposes/refines a label set over a _sample_, and
  (2) a CHEAP, frequent **classification** pass that assigns each new item.
  Lesson we adopt: _discover categories rarely; route notes cheaply._
- **EvoTaxo** (arXiv:2603.19711, 2026) — INCREMENTAL taxonomy editing over a
  stream: each new item is a "draft edit" over the current taxonomy; structural
  evidence accumulates over windows; a **new branch is introduced only when an
  emerging concept becomes salient**; every node keeps a "concept memory bank"
  (a centroid) to hold its boundary. This is exactly "notice repetition → make a
  folder." We adopt the centroid-per-category + salience-threshold idea.
- **DP-means** (small-variance asymptotic of a Dirichlet-Process mixture) — the
  cheap mechanism for "spawn a category when a note fits nothing": assign a
  point to its nearest centroid; **if the distance to every centroid exceeds a
  single threshold λ, start a new cluster.** Online, O(#categories) per note,
  no fixed cluster count. This is our novelty detector.
- **BERTopic / DBSTREAM** — embed → (reduce) → cluster; true streaming variants
  exist (river `DBSTREAM`). HDBSCAN is batch-only and memory-heavy → rejected
  for the per-note path; fine for an occasional in-view re-cluster.
- **Mem / Reflect / Obsidian Smart Connections** — the product bar: "just
  write, the AI files it," all on-device embeddings, private. We already have
  the on-device embedding index (BGE-small, 384-d, chunked in `notes_vec`) — the
  substrate these apps build on.

## 3. Design: two tiers, both piggybacking existing work

The key to honoring zero-idle-RAM is that **we never add an engine** — we ride
two moments where compute is already spent:

### Tier A — per-note routing (at capture, LLM already running)

Capture already runs the structuring LLM (`extract_metadata` / `compose_note`).
We enrich that ONE existing call: pass it the list of current Grain subfolders
and ask it to return a `category` field — the best-fit existing folder, or a
short proposed new-folder name, or `null` (loose). Cost: a few tokens on a call
that already happens. No embedding needed at capture (headless capture never
loads the embed model). Result: a _suggested_ folder on the fresh note.

### Tier B — discovery + consolidation (only while the window is visible)

Embeddings and centroids only exist/refresh when the window is open (the embed
model is already gated to a visible overlay). So the "notice repetition" work
runs there, opportunistically, on data already in the index:

1. **Centroids** — one 384-d mean vector per Grain subfolder, persisted in the
   vault index (`folder_centroids` table). KB-scale. Recomputed lazily from
   `notes_vec` when notes change. This is EvoTaxo's per-node memory bank.
2. **Route unfiled notes** — for each loose note, cosine to every centroid.
   ≥ τ_assign → surface "File in #X?" (Tier A's guess is verified/overridden by
   geometry). This is TnT-LLM Phase-2 classification, done with dot products.
3. **Discover new categories** — over the loose/novel notes (those below λ from
   every centroid, DP-means-style), run a one-shot cheap agglomeration on their
   cached vectors. When a cluster reaches a salience bar (≥ N notes, tight
   spread), it's a _candidate category_. ONE LLM call (the structuring model
   that's already warm, or the agent when summoned) NAMES it. → "You've written
   5 notes about invoicing — create #Invoices?" This is TnT-LLM Phase-1, scoped
   to just the emergent cluster.

Nothing here runs hidden: no window visible ⇒ no centroids, no clustering, no
LLM. Centroids are inert bytes in sqlite when idle.

## 4. UX — human-in-the-loop, non-destructive

- New note → a subtle folder chip ("File in #Work?"). One-click accept; or
  silent auto-file ONLY above a high confidence τ_auto (configurable; default
  suggest-only). Never move a user's Obsidian file on a weak guess.
- New category → a gentle, dismissible prompt; accept creates the subfolder
  inside the Grain folder and moves the cluster's notes (all inside Grain →
  respects the read-only-outside-Grain rule; uses existing `save_note` renames).
- A settings toggle `grain_space_auto_categorize` (default off; feature gates to
  literally nothing when off). Reuses the existing "everything in Grain is
  editable/movable" work.

## 5. Phasing

- **P1 — routing scaffold (no model): ✅ SHIPPED (backend + settings toggle).**
  `category` rides the existing structuring call (`extract_metadata`/
  `compose_note`), validated back to an exact existing folder; when confident,
  capture auto-files via `vault::move_note_to_folder`. New primitives:
  `list_folders`, `move_note_to_folder` (+ `grain_space_list_folders` /
  `grain_space_move_note` commands). Gated by `grain_space_auto_categorize`
  (off by default). Zero new RAM — no model, no background work. REMAINING for
  P1.5: the in-editor suggestion chip for the low-confidence "suggest, one-click
  accept" case (backend `grain_space_move_note` is ready for it).
- **P2 — centroids + geometric routing:** `folder_centroids` table, lazy
  refresh from `notes_vec` on window-open reconcile; cosine routing to verify/
  improve P1 suggestions; τ thresholds.
- **P3 — discovery (DP-means + one-shot naming):** cluster novel loose notes in
  view; salience gate; single LLM naming call; "create folder?" prompt.
- **P4 — polish:** decay so stale folders don't dominate centroids; merge
  suggestion when two folders' centroids collapse ("align this into one");
  thresholds tuned on real usage; settings surface.

## 6. Hard invariants (do not break)

- Embed model loads only when semantic is on AND the overlay is visible.
- No background thread/watcher; discovery runs on the in-view reconcile only.
- Only files INSIDE the Grain folder are ever created/moved.
- Off-by-default; when off, no code path allocates.
