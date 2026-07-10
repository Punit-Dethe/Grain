# Grain Space 2.0 — Execution Plan (roadmap phases 2–4)

> Read `OBSIDIAN-PLAN.md` (the shipped V1–V3 foundation), the top
> `TRANSITION-LOG.md` entry, and `OBSIDIAN_ROADMAP.md` (the master intent
> list) first. This plan turns the roadmap's INTENT into concrete,
> implementation-owned steps. Where the roadmap pitched a solution, it was
> treated as a suggestion only — deviations are called out and justified
> inline.

## Status legend
- `[x]` shipped and test-verified
- `[~]` in progress
- `[ ]` not started

---

## P0 — Leftovers from OBSIDIAN-PLAN.md (do first)

Audit result (2026-07-11, via TRANSITION-LOG): V1 (vault store), V2
(settings UI + wiring), V3-A/B/C/D (safe merge writes, Open-in-Obsidian,
empty-corpus fix, bm25 title weighting) are DONE — 201 tests green. What
remains from the plan:

1. `[x]` **Chunked embedding for long notes** (plan §7 V3). Long vault
   notes embedded whole dilute retrieval; Smart Connections chunks at
   block level. **Sequencing note:** deliberately executed AFTER the
   format unification below — unification deletes the JSON store, so the
   chunking lands ONCE in `vault.rs` instead of twice.
2. `[x]` **Foreign-note rename detection in reconcile** (plan §7 V3): a
   foreign id is `sha256(relpath)`, so a rename is remove+add and loses
   the note's embedding. The index already stores each row's last-synced
   `content` — match vanished rows against newly-added ones by content
   and re-key the vec rows instead of dropping them. Executed after
   chunking so the re-key is chunk-aware.
3. `[ ]` **User GUI testing** (cannot be driven headlessly — carry
   forward): flip to a real vault, capture, search foreign notes, Recall
   over the vault, promote a note out of `Grain/`, Open-in-Obsidian,
   force a concurrent edit to see the merge/sidecar.

Explicitly NOT leftovers (user deferred them; they ARE roadmap Phase 3/4
items and are scheduled there): foreign-note editing + AST overlay
editor, auto-categorization.

---

## P1 — Roadmap Phase 2: Format Unification

**Intent:** one note format everywhere. Sunset the JSON store; both
backends are folders of `.md` + YAML frontmatter.

**Implementation decisions (mine, not the roadmap's):**

- The native backend becomes a **Grain-managed vault**: `vault.rs` runs
  against `{app_data}/grain_space/notes/` exactly the way it runs against
  an Obsidian vault. No second markdown store implementation — `Backend`
  keeps its two variants (native still answers `note_abs_path = None` so
  no "Open in Obsidian" affordance leaks into the native backend), but
  both hold a `Vault` and every operation dispatches to the same code.
- Per-backend index files in app data: native `native_index.sqlite`,
  obsidian `vault_index.sqlite` (unchanged). The legacy `index.sqlite`
  is derived data — deleted after a successful migration.
- `store.rs` shrinks to the shared note model (`Note`, `TodoTag`,
  `ReminderState`, `validate_id`, `ensure_vec_extension`, `export_json`)
  and is renamed `note.rs`. All JSON *store* logic (scan, quarantine,
  per-id files, its FTS index) is deleted.
- **One-time migration, idempotent and non-destructive:** on the first
  native-backend resolve of a run (AtomicBool guard → zero steady-state
  overhead), any `{app_data}/grain_space/notes/*.json` is parsed and
  saved through `vault::save_note` (same `grain_id`, same timestamp, so
  identity and ordering survive; filename from the title, falling back to
  the first words of the body). Each converted file moves to
  `notes-json-backup/`; unparseable files move there too (logged, never
  lost). Re-running converges: `save_note` keyed on the preserved id
  cannot duplicate. Embeddings are NOT copied — notes re-embed lazily
  (`embed_stale = 1`), which is exactly the existing recovery path.
- Frontend: no changes needed — the `Note` wire type over Tauri commands
  is unchanged (the roadmap's "remove frontend JSON parsing" was already
  true: the frontend never parsed note files). JSON export stays as the
  portability bridge.

Steps:
1. `[x]` `store.rs` → `note.rs` (model + shared helpers only); fix
   imports; delete the JSON store + its tests, keep/move the
   schema-lock, export, and id-validation tests.
2. `[x]` `Vault` gains an index-file-name knob; `backend::resolve` builds
   the native vault (creates the folder on first resolve).
3. `[x]` `vault::migrate_legacy_json` + wiring into the native resolve
   path; migration tests (roundtrip, idempotence, unparseable file,
   blank-title filename fallback, backup contents).
4. `[x]` Full test pass (199 green, 2026-07-11); docs + TRANSITION-LOG
   entry.

## P2 — Chunked embedding (P0 item 1, landed here post-unification)

- Chunk long bodies on markdown structure (heading/blank-line blocks),
  greedily packed to a target size; short notes stay one chunk (today's
  behavior, zero regression).
- Vec rows keyed `"{note_id}#{n}"` — `#` is outside the id charset so
  chunk keys can never collide with real ids, and every existing caller
  of `stale_embed_texts`/`store_embeddings` already treats ids as opaque
  (they zip texts to vectors), so **no caller changes**.
- KNN dedupes chunk hits to note level (best chunk wins) before the
  decay/floor scoring; delete/rebuild/re-embed clean up chunk rows.

## P3 — Foreign-note rename detection (P0 item 2)

- In `reconcile_locked`'s vanished sweep: before dropping a foreign row,
  content-match it against files added in the same pass; on a match,
  re-key `notes_vec` rows old-id→new-id (chunk-aware) and carry
  `embed_stale` over instead of re-embedding.

## P4 — Roadmap Phase 3: The Native UI (Floem editor process)

**Intent:** an Obsidian-style live-preview markdown editor as a separate
process; the pill stays tiny; the legacy Tauri note overlay gets sunset.

Milestones (per `floem-getting-started.md` §11, build-verify early):
1. `[x]` Scaffold `crates/grain-editor` (root workspace): pinned `floem`
   (0.2.0), blank window + `text_editor` buffer. Proven building AND
   running on Windows (2026-07-11) — the riskiest dependency in the
   roadmap is de-risked.
2. `[ ]` `pulldown-cmark` parse-on-change producing span lists; log-only.
3. `[ ]` Decoration layer: spans → text-layout attributes (bold/heading/
   dim-marker styling).
4. `[ ]` Cursor-gating (raw markup on the active line only).
5. `[ ]` Targeted `notify` watcher for THE ACTIVE NOTE only + reuse the
   existing `safe_write`/`diffy` merge (deviation: the roadmap suggested
   `similar`; `diffy::merge` already shipped in V3 and is the
   purpose-built 3-way tool — no second diff dep).
6. `[ ]` IPC bridge pill-daemon ↔ editor process; pin/dock affordance.

Milestone 1 is scheduled THIS session if the P1–P3 verification is green;
2–6 need visual iteration and are a natural user-evaluation checkpoint.

## P5 — Roadmap Phase 4: Advanced editing & categorization

1. `[ ]` AI prompt upgrade: prompts understand the frontmatter schema and
   emit Obsidian-native notes.
2. `[ ]` Write access to foreign notes: drop the read-only guard, round-
   trip foreign frontmatter byte-for-byte (stored raw block or re-read +
   splice), never inject `grain_id` into a foreign file (constraints
   documented in TRANSITION-LOG "IMPORTANT — foreign notes").
3. `[ ]` Auto-categorization: vault taxonomy injected into
   `extract_metadata`; destination subfolder + `tags:` from the USER'S
   OWN taxonomy only.
4. `[ ]` AI formatting tiers (minimal vs full Obsidian power).
