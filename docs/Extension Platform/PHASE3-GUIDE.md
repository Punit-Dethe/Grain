# Phase 3 Implementation Guide — surfaces, settings, and the Grain Space Test

A prescriptive, step-by-step guide for Phase 3 in THIS codebase, written in the
same spirit as [PHASE2-GUIDE.md](PHASE2-GUIDE.md) (which carried Phase 2 across
a context compaction intact). Follow the steps **in order** — each compiles and
tests on its own, and later steps assume earlier ones. Where a decision looks
open, it isn't: the resolution is here or in [SPEC.md](SPEC.md), which wins on
any conflict. Read [TRANSITION-LOG.md](TRANSITION-LOG.md) first.

**What Phase 3 delivers (SPEC §8 row 3):** schema settings render (levels 1–2)
incl. anchors + ordering; `workspace` extracted from Grain Space's `window.rs`
as a host-owned generic with Grain Space as first consumer; `overlay`; pill
slots; the store slide-over **shell**; and the **Grain Space Test** as the
acceptance bar.

**Why this phase is different from Phase 2.** Phase 2 built a runtime on empty
ground — nothing existed to break. Phase 3 **refactors shipped, user-visible
features** (Grain Space's window, the pill, the settings page). The governing
rule is therefore: *extract, then adopt, never rewrite.* Grain Space must behave
identically at every commit; if the user can tell you refactored it, you did it
wrong.

**Non-negotiables (do not "simplify" these away):**
- Extensions **never** create windows (SPEC §1.2). They declare a surface; the
  host builds, places, sleeps and destroys it. Any API that hands an extension a
  window handle is wrong.
- The security wall stays the Rust WS boundary. UI surfaces get their **own
  realm and their own token** (SPEC §7.1) — a `workspace`/`overlay`/
  `settings-panel` is not the supervisor and not the main window.
- No feature code inside `src-tauri/src/handy/` — hooks only, `[GRAIN]`-marked,
  budget-accepted individually.
- Additive contract changes only. `grain-sdk` grows; nothing is renamed.
- ⛔ **The store is a SHELL only** — see
  [GATE-DISTRIBUTION-AND-DEVMODE.md](GATE-DISTRIBUTION-AND-DEVMODE.md). Build
  the slide-over and its empty state. Do NOT build an index, submission, trust
  badges, or install-from-remote. That whole surface is gated.

---

## Step 0 — Re-review (SPEC §10.3)

Re-read SPEC §1.2 (surfaces), §4 (settings schema + anchors), §5.3 (store),
§7.1 (realms), and PLAN.md Part 5 (the Grain Space Test table). Record
"kept / changed / why" in your first commit message.

## Step 1 — Manifest growth: `contributes`, `surfaces`, `slots`

File: `crates/grain-sdk/src/manifest.rs`. Contract-first: every later step
consumes these, so they land first, purely, with tests.

Add to `ExtensionManifest` (all `#[serde(default)]`, so every existing pack
still parses):

```rust
pub surfaces: Surfaces,          // { workspace: Option<WorkspaceDecl>, overlay: Option<OverlayDecl> }
pub slots: Vec<String>,          // exclusive positions claimed (SPEC §3)
pub contributes: Contributes,    // { settings: Vec<SettingDecl>, shortcuts: Vec<ShortcutDecl>, pill: PillDecl }
```

- `WorkspaceDecl { title: String, min_size: Option<[u32; 2]> }`
- `SettingDecl` is the **level 1–2 schema**: `{ key, label, kind, default,
  description?, anchor?, order? }` where `kind` ∈ `bool | string | number |
  select{options} | shortcut`. Level 3 (custom iframe panel) is Phase 4 — do not
  design for it here beyond leaving the enum open.
- `ShortcutDecl { id, label, default_binding: Option<String> }`.

Extend `KNOWN_CAPABILITIES` with `surface:workspace`, `surface:overlay`,
`pill:slots`. Extend `validate()`: slots must be from the known slot list
(`overlay.recording`, `overlay.pointer`, `pill.theme`, `agent.reply-surface`,
`output.destination`, `overrides:<setting>`); a declared surface requires the
matching `surface:*` permission; setting keys must be unique.

**Pitfall — anchors are contract surface** (SPEC §4). Keep the anchor list
*few, semantic and versioned* — `snippets.after`, `context.after`,
`agent.after`, `space.after`. An anchor is a promise you cannot rename later.
Put the list in one `pub const ANCHORS: &[&str]` and validate against it.

## Step 2 — The slots registry

File: `crates/grain-core/src/extensions.rs` (the registry already lives here).

SPEC §3: **at most one enabled occupant per slot**; core defaults are
occupants; claiming an occupied slot raises an **explicit takeover prompt**,
never a silent steal; disable releases the slot.

Add `slot_occupant(slot) -> Option<String>` and make `set_enabled(id, true)`
fail with a structured `{"slotConflict": {slot, current_occupant}}` error when a
claimed slot is taken — the exact pattern the permission sheet already uses
(`needsPermissions`), so the frontend flow is familiar. `extension_take_slot(id,
slot)` disables the incumbent and enables the challenger in one step.

Unit-test: claim → conflict → takeover → disable releases. Pure, no Tauri.

**Pitfall:** the centre-layout variant already occupies `agent.reply-surface`
(SPEC §10.2). Backfill core defaults as occupants or the first real claim will
look free and silently displace a shipped feature.

## Step 3 — Schema settings render (levels 1–2)

The first user-visible win, and self-contained.

- Backend: `extension_settings_schema(id) -> Vec<SettingDecl>` reads the pack;
  values already round-trip through `host_api`'s `settings.get/set` (the
  extension's own `__settings` namespace — there is still **no** path to
  `AppSettings`). Add `extension_setting_get/set(id, key, value)` for the host
  UI to read/write the same namespace, **validated against the schema** (reject
  a value whose kind doesn't match; clamp/`default` on invalid, with a notice).
- Frontend: one renderer component mapping `kind` → control, driven entirely by
  the schema — no per-extension code. Sort by `order`, group by `anchor`.
  Anchored sections render inside the anchor's host section (SPEC §4: this is
  how an extension's settings appear *next to the feature they extend* rather
  than in a ghetto tab).

**Pitfall — validate host-side, not just in the form.** The same setter is
reachable from the extension itself via `settings.set`; a schema enforced only
in React is not enforced.

## Step 4 — `contributes.shortcuts` (+ chunk 2b: sessionMode)

Register an extension's declared shortcuts through the existing binding
registry, namespaced `ext:<id>:<shortcut-id>` (the `ext:` namespacing that
prompts already use — collisions unrepresentable). Register on enable,
unregister on disable, exactly like the Agent's `summon_agent` binding does.

Fold **chunk 2b** in here: `contributes.sessionMode` + a working
`session.start(mode)` (Phase 2 reserved the capability and returns "not
implemented yet"). The coordinator **serializes** — an extension session and
core dictation can never overlap; the loser is rejected exactly as two core
bindings are. On stop, deliver the transcript as
`HostCall{method:"sessionResult"}` with a **long** deadline (seconds — this is
the slow stage, not the 150 ms transform), then paste through the normal output
path.

**Pitfall — the shortcut-dispatch deadlock (learned the hard way).** NEVER
register or unregister a global shortcut synchronously inside a
`ShortcutAction`. Defer via `tauri::async_runtime::spawn` or every global
shortcut in the app hangs. This will bite you the moment an extension shortcut
enables another extension.

## Step 5 — `workspace` (the big one)

Extract the sleeping-window pattern out of
`src-tauri/src/grain_space/window.rs` (334 lines) into a host-owned generic —
`src-tauri/src/surfaces/workspace.rs` — and make Grain Space its **first
consumer**, unchanged in behavior.

What is being generalized (all of it already proven in `window.rs`):
`WINDOW_LABEL`, the `AWAKE` atomic, `SLEEP_EVENT` (`grain-space://sleep`), the
frontend ack (`grain_space_sleep_ready`) with `ACK_FALLBACK` so sleep can never
hang on a wedged webview, `toggle`/`open`/`close`/`finish_sleep`, the
`FOCUS_NOTE` stash, and true-destroy.

Generalize to `Workspace { id, label, url, title, min_size }` with:
`open(id, payload)`, `close(id)`, `toggle(id)`, `destroy(id)` — plus per-surface
`AWAKE` state and a **generic payload stash** replacing `FOCUS_NOTE` (Grain
Space passes its focus-note id through it; an extension passes whatever JSON it
likes).

Add the SPEC §1.2 requirement `window.rs` does not have: an **LRU cap on awake
windows** (start N=1 beyond Grain Space) — the cap sleeps the least-recently
used rather than refusing to open.

**Order of work, and it matters:**
1. Extract the generic with Grain Space's own constants passed in; Grain Space
   calls it. **Nothing about Grain Space changes.** Ship + verify this alone.
2. Only then add extension-facing `surface:workspace` (host call
   `workspace.open/close`), the extension's own realm + token, and the LRU cap.

**Pitfall — the sleep ack is load-bearing.** The unmount-then-hide handshake is
what makes the window cost ~nothing while asleep. A generic that hides without
waiting for the ack silently regresses Grain Space's RAM profile — the whole
reason the pattern exists. Keep the fallback timer.

**Pitfall — window creation off the main thread.** Build via
`app.run_on_main_thread` / `async_runtime::spawn`, never synchronously on a
shortcut or event thread (tauri#3990 freeze).

## Step 6 — `overlay`

Transient HUD: created per invocation, destroyed on dismiss, with a **size and
lifetime budget** (SPEC §1.2). Simpler than `workspace` — no sleeping, no LRU;
it is create-and-destroy. Host call `overlay.show(payload)` / auto-dismiss on
timeout or focus loss. Reuse the workspace realm/token machinery from Step 5.

Slot: `overlay.recording` is an occupied slot (core owns the pill's recording
HUD) — this is the first real exercise of Step 2's takeover prompt.

## Step 7 — pill slots + pill theme rendering

Folds in the **Phase 1 remainder** (pill-theme rendering, SPEC §9). Do named
patterns first (`breathe`/`sweep`/`static` + per-state backgrounds/dot colours);
the expression evaluator can wait.

- **No extension code ever runs in the pill process** (SPEC §1.2). Themes and
  chips are *declarative data* delivered to `grain-pill`. Decide the delivery
  route: an additive `DaemonEvent::PillTheme` emitted on connect + change is the
  straightforward one (the pill is already a WS client).
- Missing state → Grain's default **for that state only**; 3 strikes → default
  theme. **The pill must always render** — that is the hard rule.
- Action chips are capped, and the user may hide any chip.

## Step 8 — Close the Grain Space Test gaps

PLAN.md Part 5 attributes these to Phase 2, but Phase 2 shipped without them:

- **`embed()`** — currently returns "embed is not available in this version".
  Wire it to the Grain Space embedder (`grain_space::embed`). Grain Space's
  semantic recall is unbuildable without it.
- **`capture:selection`** — the selection quick-add path.
- **`storage` as a scoped dir + document store**, not only the KV file Phase 2
  shipped. Notes are documents; a 200 MB single JSON blob is the wrong shape.

## Step 9 — Store slide-over SHELL (gated)

A slide-in panel **inside the settings window** (SPEC §5.3, Zen-style) —
replacing the disabled "Browse extensions — coming soon" button that already
holds the layout. **Shell + empty state only.** No index, no fetch, no
submission, no trust badges: all gated behind
[GATE-DISTRIBUTION-AND-DEVMODE.md](GATE-DISTRIBUTION-AND-DEVMODE.md).

## Step 10 — Run the Grain Space Test

The acceptance bar. Write the manifest a third party *would* write to rebuild
Grain Space, then walk PLAN.md Part 5's table line by line and record, honestly,
which rows the platform actually satisfies.

Expected verdict per PLAN.md: ~90% reachable; the remaining ~10%
(agent-pill text-input integration, folder-watch reconcile) becomes recorded
contract work for Phase 4's re-platforming pass. **Record the gaps — do not
paper over them.** A gap found here is the phase working; a gap hidden here
surfaces as a third-party author's dead end.

---

**Definition of done for Phase 3 (SPEC §8 row 3):** settings schemas render at
levels 1–2 with anchors + ordering, validated host-side; slots enforce single
occupancy with an explicit takeover; `workspace` is a host-owned generic with
Grain Space consuming it **at unchanged behavior and unchanged RAM profile**,
plus an LRU cap; `overlay` ships; pill slots + theme rendering ship with the
always-renders guarantee; the store slide-over shell exists and is visibly
gated; the Grain Space Test is walked and its gaps recorded. `tsc` clean, all
Rust tests green, ratchet green.

## Appendix — file map for Phase 3

New: `src-tauri/src/surfaces/{mod,workspace,overlay}.rs`, a settings-schema
renderer component, a store slide-over component, `PHASE3-REVIEW.md` (the Step
10 record). Touched: `crates/grain-sdk/src/manifest.rs` (contributes/surfaces/
slots + anchors), `crates/grain-core/src/extensions.rs` (slot registry),
`src-tauri/src/grain_space/window.rs` (becomes a thin caller), `host_api.rs`
(workspace/overlay/embed methods), `grain_commands.rs` (schema + slot commands),
`lib.rs` (module decls + command registration — budgeted hooks), `grain-pill`
(theme rendering).
