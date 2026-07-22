# Phase 3 Review — The Grain Space Test, walked for real

The acceptance bar for the extension platform (PLAN.md Part 5): *if Grain Space
did not exist, could a third party build it on this platform?* This is the
honest walk after Phase 3 — the manifest such an author would write, and a
row-by-row verdict on what the platform actually satisfies versus what is still
a recorded gap. **A gap found here is the phase working; a gap hidden here is a
third-party author's dead end.**

## The manifest a third party would write

This is a real `spaces.grainpack.json` skeleton — every capability, surface,
shortcut and setting below is one the platform accepts today (or is explicitly
flagged as a gap in the next section). It would not fully run end to end yet,
for exactly the reasons the gap ledger records.

```jsonc
{
  "manifest": {
    "id": "com.example.spaces",
    "name": "Spaces",
    "version": "1.0.0",
    "grain_api": "1.0",
    "tier": "scripted",
    "description": "Local notes with semantic recall.",
    "permissions": [
      "storage",              // notes on disk (KV + the document store)
      "embed",                // on-device BGE vectors for recall
      "llm",                  // structure a note / answer a recall question
      "capture:selection",    // quick-add from the current selection
      "events:sessions",      // know when a recording starts/stops
      "events:transcripts",   // read the dictated note text
      "session:start",        // ⚠ GAP — voice capture (see ledger)
      "surface:workspace",    // the three-pane notes window
      "surface:overlay"       // the recall answer HUD
    ],
    "activation": ["onShortcut:open", "onShortcut:quick-add", "onShortcut:recall"],
    "entry_source": "/* worker.js — capture, recall, structuring */",
    "surfaces": {
      "workspace": { "title": "Spaces", "min_size": [900, 600],
                     "ui_source": "<!-- the notes UI -->" },
      "overlay":   { "size": [420, 160], "timeout_ms": 8000,
                     "ui_source": "<!-- the recall answer -->" }
    },
    "contributes": {
      "shortcuts": [
        { "id": "open",      "label": "Open Spaces",       "default_binding": "Ctrl+Shift+G" },
        { "id": "quick-add", "label": "Quick-add a note",  "default_binding": "Ctrl+Shift+C" },
        { "id": "recall",    "label": "Ask your notes",    "default_binding": "Ctrl+Shift+M" }
      ],
      "settings": [
        { "key": "recall_floor", "label": "Recall match threshold", "kind": "slider",
          "min": 0.0, "max": 1.0, "step": 0.05, "default": 0.5,
          "anchor": "models.after" }
      ]
    }
  },
  "payloads": {}
}
```

## The walk — PLAN.md Part 5, row by row

| Grain Space needs | Platform provides | Verdict after Phase 3 |
|---|---|---|
| React notes UI in its own window, sleep-on-close | `surface:workspace` | ✅ **Satisfied.** The workspace generic was extracted from Grain Space's own window and verified RAM-neutral (sleep returns to baseline, ack handshake intact); the extension-facing half builds it in a sandboxed realm with its own token. |
| Store notes on disk | `storage` — scoped dir + document store | ✅ **Satisfied.** KV (`storage.*`) plus the per-file document store (`doc.*`) added in Step 8, with path-safe keys and a shared quota. |
| Embeddings + semantic recall | `embed()` host call | ✅ **Satisfied.** Wired in Step 8 to the same on-device BGE model Grain Space uses — local, free, private. |
| AI structuring / recall answers | `llm.complete()` via the router | ✅ **Satisfied** (Phase 2). Keys stay host-side; the extension never sees them. |
| Global shortcuts (open / quick-add / recall) | `contributes.shortcuts` | ✅ **Satisfied** (Step 4a). Namespaced, toggle-order arbitration, rebindable. |
| Declarative settings page | schema settings (levels 1–2) | ✅ **Satisfied** (Step 3). Anchored next to the feature; validated host-side. A **custom** settings iframe (level 3) is Phase 4 — not needed for this manifest. |
| Recall answering in an overlay | `surface:overlay` | ✅ **Satisfied** (Step 6). Size- and lifetime-budgeted HUD, same realm as the workspace. |
| Capture-selection quick-add | `capture:selection` | ✅ **Satisfied** (Step 8). |
| **Voice capture mode** (dictate a note) | `session:start` + `contributes.sessionMode` | ⛔ **GAP.** Reserved and plumbed but returns "not implemented yet" — an extension cannot start its own recording session. This is chunk 2b, deferred from Step 4. |
| **Capture-mode pill indicator** | pill action chip (`pill:slots`) | ⛔ **GAP (deferred, name reserved).** The theme half of pill slots shipped in Step 7; contributed action chips have no consumer yet, so the interface is reserved, not built (capability-governance doctrine). |

## Recorded gaps → Phase 4 contract work

Honest ledger. None of these is a surprise; each is a deliberate deferral, and
this is where they become tracked items rather than an author's dead end.

1. **`session:start` + `contributes.sessionMode`** — the one *structural* gap.
   Grain Space's voice-note capture starts a recording; an extension can't do
   that yet. The capability name, the host-API method, and the router entry all
   exist (they return a clean "not implemented"), so an author discovers a wall
   with a door in it, not a missing wall. **Highest-priority Phase 4 item** —
   structural capabilities land early or never (CAPABILITY-GOVERNANCE.md).
2. **`pill:slots` action chips** — additive. Reserve the name (done), design the
   shape when a real chip consumer exists. A capture-mode indicator is a nice-to-
   have, not a blocker: the workspace/overlay already give the extension a place
   to show state.
3. **Level-3 custom settings iframe** (`surface:settings-panel`) — additive,
   Phase 4. The declarative schema covers this manifest; only an extension whose
   settings need bespoke UI reaches for it.
4. **Agent-pill text-input integration** and **folder-watch reconcile** — the
   ~10% PLAN.md always expected to fall out of dogfooding. Neither is a platform
   primitive; both are Grain-Space-specific polish that a re-platforming pass
   (Phase 4) would fold in.

## Verdict

**~90% reachable, exactly as PLAN.md predicted.** Eight of ten capability rows
are satisfied and were each verified as they shipped; the two gaps are one
structural item already named and plumbed (`session:start`) and one additive
item deliberately left for its first real consumer (`pill:slots` chips). A
determined third party could stand up the notes UI, on-disk documents, semantic
recall, AI structuring, global shortcuts, settings, and the recall overlay
today — and would hit precisely one door that says "not implemented yet" rather
than an unexplained absence.

Phase 3's definition of done (SPEC §8 row 3) is met: schema settings render with
anchors and ordering, validated host-side; slots enforce single occupancy with
an explicit takeover; `workspace` is a host-owned generic with Grain Space
consuming it at unchanged behaviour and RAM, plus an LRU cap; `overlay` ships;
pill slots + theme rendering ship with the always-renders guarantee; the store
slide-over shell exists and is visibly gated; and the Grain Space Test is walked
with its gaps recorded rather than papered over.
