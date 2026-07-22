# Phase 3 Review — The Grain Space Test, closed in Phase 4

The acceptance bar for the extension platform (PLAN.md Part 5): *if Grain Space
did not exist, could a third party build it on this platform?* This is the
honest walk after Phase 3 — the manifest such an author would write, and a
row-by-row verdict on what the platform actually satisfies. Phase 4 revisited
the two open rows with a checked-in voice-note extension rather than closing
them by assertion. **A gap found here is the phase working; a gap hidden here
is a third-party author's dead end.**

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
      "session:start",        // extension-owned voice capture
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
      "sessionMode": {
        "id": "note", "label": "Dictate a note",
        "default_binding": "Ctrl+Shift+N"
      },
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
| Declarative settings page | schema settings (levels 1–2) | ✅ **Satisfied** (Step 3). Anchored next to the feature; validated host-side. A custom settings iframe remains deferred because this manifest does not need one. |
| Recall answering in an overlay | `surface:overlay` | ✅ **Satisfied** (Step 6). Size- and lifetime-budgeted HUD, same realm as the workspace. |
| Capture-selection quick-add | `capture:selection` | ✅ **Satisfied** (Step 8). |
| **Voice capture mode** (dictate a note) | `session:start` + `contributes.sessionMode` | ✅ **Satisfied (Phase 4).** A declared shortcut or `grain.session.start()` enters the same serialized host-owned recording path. The owner receives a bounded, cancellable slow stage; failure and hot reload restore the exact transcript. The checked-in `examples/voice-note` extension structures with `llm`, stores with `storage`, and suppresses paste. |
| **Capture-mode pill indicator** | host-rendered session-owner label | ✅ **Satisfied (Phase 4).** The session-start event carries the approved extension display name and the pill renders it as data. This did not require the deferred extension-controlled `pill:slots` action-chip API. |

## Phase 4 closure evidence

The checked-in [`examples/voice-note`](examples/voice-note/) extension exercises
the whole open path: contributed shortcut → visible extension-owned recording →
slow-stage `llm` call → document storage → handled output. Host tests separately
cover busy-session rejection, cancellation, timeout, worker reload, and exact
transcript fallback.

`pill:slots` action chips and `surface:settings-panel` remain reserved additive
ideas. They are not Grain Space Test gaps: the required ownership indicator is
host-rendered, and the declarative settings schema covers this manifest.

## Verdict

**10 of 10 capability rows are now reachable.** The structural session gap is
implemented and demonstrated by a copyable third-party example. The ownership
indicator is implemented as a host statement of fact, without inventing an
extension-controlled pill layout API. Grain Space-specific polish may still be
useful, but no remaining item blocks a third party from building the product on
the public contract.

Phase 3's definition of done (SPEC §8 row 3) is met: schema settings render with
anchors and ordering, validated host-side; slots enforce single occupancy with
an explicit takeover; `workspace` is a host-owned generic with Grain Space
consuming it at unchanged behaviour and RAM, plus an LRU cap; `overlay` ships;
pill slots + theme rendering ship with the always-renders guarantee; the store
slide-over shell exists and is visibly gated; and the Grain Space Test is walked
with its gaps recorded rather than papered over.
