# Grain Extension Platform — Specification

**This is the normative document.** If you are implementing the platform,
build from this file. Everything here is decided and current; all corrections
from earlier design passes are folded in.

The other documents are **rationale, not instructions** — read them to
understand *why* a rule exists, never to decide *what* to build:
[PLAN.md](PLAN.md) (architecture decisions + phases),
[CASE-HEYCLICKY.md](CASE-HEYCLICKY.md) / [STRESS-TEST.md](STRESS-TEST.md)
(the contract tested against real features),
[CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md) /
[FREEDOM-LADDER.md](FREEDOM-LADDER.md) (how the platform grows).
Start at [README.md](README.md) if you have no context at all.

---

## 1. Model

### 1.1 Tiers

| Tier | Runtime | Idle cost | May hold |
|---|---|---|---|
| **A-inert** | none (data) | zero | prompts, snippets, voice-action sets, context modes, themes, surface variants |
| **A-egress** | none (data) | zero | provider definitions (STT / post-process / LLM). **Data leaves the machine** → must declare `net:<host>`; consent required |
| **B scripted** | JS in **its own Worker** under a shared hidden supervisor webview — one isolated realm and one authenticated connection per extension (§7.1) | zero (created on activation, destroyed by the reaper) | logic + UI extensions |
| **C native** | own process, host-supervised | zero (not spawned = not running) | `companion` (private ability) or `provider` (implements a host interface for all) |

Tier A-egress and tier C require human review before marketplace listing.

### 1.2 Surfaces

Extensions **never** create windows. They declare surfaces; the host builds,
places, sleeps and destroys them.

| Surface | Host behavior |
|---|---|
| `settings-panel` | sandboxed iframe inside the extension's own settings section; created on scroll-into-view, destroyed on navigate-away |
| `workspace` | app-class window; generalization of Grain Space's sleeping-window pattern (built hidden once, shown on summon, UI unmounted + hidden on close, destroyed after idle). LRU cap on awake windows |
| `overlay` | transient HUD; created per invocation, destroyed on dismiss; size + lifetime budget |
| `pointer` | full-screen click-through marker layer. Host owns the window, coordinate transforms, animation, teardown; extension only sends `pointer.point({x, y, screen, label})` |
| `pill` slots | declarative contributions to the native pill: action chips, and **theme packs** that restyle its background and dot-grid animation (§9). No extension code ever runs in the pill process |
| `agent.reply-surface` variants | declarative layout packs for the Agent's reply surface (§10.2) |

### 1.3 Capabilities

Enforced in **Rust**, at the WebSocket boundary, per connection token. An
extension without a grant does not receive filtered data — it never receives
the message at all.

| Capability | Grants | Notes |
|---|---|---|
| `events:sessions` | session lifecycle events | |
| `events:transcripts` | live + final transcripts | |
| `events:audio-levels` | level events | never delivered to a sleeping extension (no wake-on-level) |
| `events:context` + `context:app` | foreground app identity (exe, title, url_host) | privacy-marked, separate consent line |
| `transform:transcript` | a step in the fast transform pipeline | hard timeout, 3-strike disable |
| `session:start` + `contributes.sessionMode` | own a serialized capture session incl. its slow stage | **structural** |
| `capture:selection` | read the current selection | host-serialized, queued |
| `clipboard:read` / `clipboard:write` | clipboard access | host-mediated, rate-limited, restore etiquette |
| `open:url` / `open:app` | launch a URL / application | danger-marked; **code tiers only** |
| `screen:capture` | screenshots + display geometry | session-scoped only; visible indicator; egress named. Highest-risk capability |
| `audio:play` | play audio bytes through the host | host owns the device, ducks, stops on cancel |
| `storage` | scoped KV + document store + directory | quota'd, wiped on purge |
| `llm` | `llm.complete()` via Grain's router | keys never exposed; quota-attributed; supports image parts with capability detection + text-only fallback |
| `embed` | `embed()` via Grain's model | |
| `net:<host>` | proxied fetch, per host | host shown in consent |
| `shortcuts` | register bindings | via the binding registry (conflict UI inherited) |
| `surface:*` | the surfaces above | |
| `pill:slots` | pill chips / theme tokens | capped; user may hide any chip |
| `settings` | read/write own namespace | schema-validated, rate-limited |
| `overrides:<core-setting>` | manage a core setting | attributed chip, restore on disable, slot-exclusive |
| `resident` | exempt from the idle reaper | must be justified; shown in plain words |

Reserved (named, not yet designed — do not implement speculatively):
`tts`.

---

## 2. Manifest

```jsonc
{
  "id": "com.example.spaces",         // reverse-dns, unique in the index
  "name": "Spaces",
  "version": "0.3.1",
  "grainApi": "^1.0",                 // contract semver; mismatch = refuse to load
  "tier": "scripted",                 // pack | scripted | native
  "platforms": ["windows", "macos"],  // omit = all; shown on the card
  "description": "Voice-first notes.",// one line; shown on hover in Overview
  "repository": "https://github.com/…", // optional; linked from Overview
  "entry": "main.js",                 // tier B bundle | tier C per-OS binary map

  "permissions": ["events:transcripts", "storage", "llm", "surface:workspace"],
  "activation": ["onShortcut:spaces.open", "onEvent:TranscriptionComplete"],

  "surfaces":  { "workspace": { "title": "Spaces", "minSize": [900, 600] } },
  "slots":     ["agent.reply-surface"],       // exclusive positions claimed
  "provides":  ["screen.capture@1"],          // tier C only: host interfaces implemented

  "requires": {
    "settings": [
      { "key": "post_process_enabled", "equals": true, "level": "hard" }
    ]
  },

  "contributes": {
    "shortcuts":  [{ "id": "spaces.open", "default": "Ctrl+Shift+S" }],
    "pill":       { "actions": [{ "id": "spaces.capture", "icon": "note", "when": "recording" }] },
    "promptLayer":{ "id": "spaces.ctx", "budgetTokens": 200 }
  },

  "settings": {
    "anchor": "snippets.after",       // optional; see §4.3
    "groups": [ /* §4.1 */ ]
  },

  "packs": { "prompts": "prompts.json" }
}
```

**Activation events** (tier B; the reaper is their inverse):
`onEvent:<Name>`, `onShortcut:<id>`, `onSurfaceOpen:<id>`, `onPillAction:<id>`,
`onTransform`, `onStartup` (requires `resident`).

> `onTransform` activates at **session start**, never at pipeline-reach time —
> a cold wake (~300 ms) cannot fit inside the transform budget (~150 ms).

---

## 3. Pipeline and arbitration

### 3.1 Canonical order (contract)

```
transcript → transforms (fast, ordered, user-visible)
           → slow stage (sessionMode OR core post-processing)
           → output slot
           → paste
```

Transforms run **before** the slow stage — where snippets, scrap-that and
voice actions run today. Nothing runs after the slow stage except the output
slot.

### 3.2 Slots (exclusive positions)

At most one **enabled** occupant per slot; core defaults are occupants.
Claiming an occupied slot raises an explicit takeover prompt — never silent,
never load-order dependent.

Slots: `overlay.recording`, `overlay.pointer`, `pill.theme`,
`agent.reply-surface`, `output.destination`, `overrides:<setting>`,
one per `provides:` interface.

Hard singletons, never extensible: the recording session itself (extensions
request one via `session:start`; the coordinator serializes), the model engine
slot, the auto-dictionary OS watcher.

### 3.3 Conflict UX (required behaviors)

| Situation | Behavior |
|---|---|
| Two extensions want one shortcut | binding registry warns on both rows; later registrant inactive until rebound; core bindings always win |
| Extension owns a session, user presses a core key | reject the second claimant; pill names the owner ("Clicky is listening"); core **never** preempts an extension session |
| Override conflicts with another extension's hard `requires` | takeover prompt states collateral: *"…this pauses **Snippet Actions**"*; the paused extension's card names the cause with a one-click fix |
| Provider uninstalled while consumers exist | uninstall dialog enumerates dependents; consumers then show "nothing provides this" with a find-a-provider link |
| Transform misbehaves | timeout → pass-through; 3 strikes → auto-disable + pill notice; state visible in Data & Advanced |

### 3.4 Fairness

Call attribution per token. Two priority classes: `interactive` (user
waiting) preempts `background` (extension default). Per-extension caps on
llm/embed calls, settings writes, clipboard writes, storage quota — all
tunable and visible on the card ("used 41 AI calls today").

---

## 4. Settings

### 4.1 Schema settings (default; renders without waking the extension)

```jsonc
{ "id": "capture", "title": "Capture", "items": [
  { "key": "autoFile", "type": "boolean", "default": true,
    "title": "Auto-file new notes", "description": "Let AI pick the folder." },
  { "key": "rules", "type": "rows",
    "columns": [ { "key": "trigger", "type": "string" },
                 { "key": "target",  "type": "string" } ] }
] }
```

Types: `boolean`, `string`, `number`, `enum`, `multi-enum`, `keybind`
(binding registry), `directory`, `file`, `secret` (secrets file, masked,
excluded from bulk reads and from export unless explicitly included),
`color`, `slider`, `rows` (typed column list — add/edit/delete/reorder;
covers snippets, voice actions, app modes, custom words).

Level 2 adds groups + `"when": "capture.autoFile == true"` visibility.
Level 3 is a custom `settings-panel` iframe, rendered *below* any schema
groups, requiring `searchTerms`.

### 4.2 Storage model

- Namespace `ext.<id>.<group>.<key>`, in a store **physically separate** from
  `AppSettings`. Secrets in the existing secrets file under the same
  namespacing. Portable mode: under the same portable root.
- **No write path to core settings exists.** Reads of core settings are
  limited to a published allowlist (app language, theme, overlay position,
  and the keys usable in `requires.settings`).
- Defaults are not materialized at install; a value is written only when the
  user or extension first sets it.

### 4.3 Anchors — settings that live next to the feature they extend

An extension that extends a core feature renders its settings **at the
feature**, not in a distant list.

- Manifest: `"settings": { "anchor": "snippets.after", … }`.
- Rendered **there and only there**. The extension's own section keeps
  Overview / Permissions / Data & Advanced plus a jump link ("Settings appear
  in Dictation → Snippets").
- **Anchor list is contract surface** — few, semantic, versioned. v1:
  `snippets.after`, `dictation.pipeline.after`, `context.after`,
  `agent.after`, `models.after`. Adding one is a promise; removing one is a
  breaking change. Unknown anchor → group falls back to the extension's own
  section (settings are never lost).
- Visual rules: one thin divider, one muted attribution line
  (*"Snippet actions · from the Snippet Actions extension ⚙"*), then controls
  in Grain's own components. No nested cards, no second tab bar.
- **Disabled → the section disappears** (no ghost UI).

### 4.4 Ordering when several extensions share an anchor

1. **Default order is toggle order** — the order the user *enabled* them in.
   First enabled sits top, next below it, and so on. This is what a user
   expects and can predict; install order is not (you may install five things
   and enable one). Disabling and re-enabling moves an extension to the end,
   which is the natural reading of "I just turned this on". Never load order,
   never author-declared priority integers (priority wars are why the
   transform pipeline is user-ordered too).
2. **The user can reorder**, by drag, at the anchor itself — that is where
   they are seen together. Order persists in the registry, per anchor.
3. **Crowding**: up to 2 groups render expanded; from the 3rd onward each
   collapses to an accordion row (still in the user's order) so a core page
   cannot grow unbounded.
4. Same rules govern multiple transform steps and multiple prompt layers —
   one ordering mechanism everywhere.

---

## 5. Extensions UI

Lives in the Extensions area of settings (today's Experimentations page). The
**tab bar never grows with extension count**.

### 5.1 Tab 1 — Overview (new, and the default tab)

The master list. Every installed extension, **enabled and disabled alike**,
visually distinguished, one row each:

| Element | Behavior |
|---|---|
| Name + icon | click → jumps to that extension's settings (its anchored section, or its own section) |
| Enable toggle | inline; first enable opens the permission sheet |
| Description | one line from the manifest; full text on hover |
| Repository link | shown when the manifest supplies `repository` |
| Status chips | trust badge, "paused — needs X", "2 shortcuts", "uses AI", "native component" |
| Sort/filter | enabled first by default; filter by enabled/disabled/needs-attention |
| **Browse extensions** | opens the store (§5.3) |

### 5.2 Remaining tabs

One per **core-adjacent feature group** only (today: Snippets, Context,
Agent — with Actions merged under Snippets, §5.4). Third-party extensions
never add tabs; their settings render at an anchor or in a detail view opened
from an Overview row.

### 5.3 Store

- **Entry point:** "Browse extensions" in Overview.
- **Presentation:** a slide-in overlay panel **inside the settings window** —
  no new window, therefore no window lifecycle to manage; dismiss with Esc or
  backdrop click. Zen Mods' store is the visual reference: a card grid
  (icon, name, author, one-line description, install button), search, and
  category/sort filters.
- **Content:** entries from the index repo (§7). Install verifies the manifest
  hash; trust badge shown before install; permissions shown before the first
  enable, not at install.
- *Open question (deliberately deferred):* whether the store also gets a home
  in the post-Quick-Panel shell. The slide-over is shell-independent, so this
  can change later without a manifest change.

### 5.4 Migration note (do this regardless of extensions)

Today `SnippetsSection` and `ActionsSection` are separate tabs. They are one
concept: merge Actions into a section **below** Snippets in one scrollable
view. When Actions later becomes an extension, it re-appears in exactly that
position via `anchor: "snippets.after"` — so the UI does not move twice.

---

## 6. Lifecycle

| Transition | Store | UI |
|---|---|---|
| Install | manifest cached; no values written | row appears, toggle off |
| First enable | grants recorded | permission sheet → shortcuts register, surfaces/pill slots available, settings section appears |
| Disable | values + grants retained | shortcuts unregister, surfaces close, anchored sections disappear; card still browsable |
| Update (same perms) | schema diff: new keys default; removed keys quarantined one version then pruned; `renames` map applied; invalid values → default + notice | changelog badge |
| Update (new perms) | installs but **held disabled** until the permission *diff* is approved | "needs review — new permissions" |
| Uninstall | dialog, default **keep data**; explicit purge checkbox | row goes; kept data listed under "Orphaned extension data (N)" with per-item purge |
| Broken manifest | untouched | error row with reason; nothing else degrades |

**Runtime lifecycle:** the supervisor webview is created when the first
tier-B extension activates; each extension's **own Worker** (§7.1) is created
on its activation and destroyed by the idle reaper (no active subscriptions,
no open surface); the supervisor itself is destroyed when its last worker
dies. `resident` exempts and must be justified on the sheet. Tier-C
processes are spawned on activation, health-checked, killed on disable, never
orphaned. Uninstall is one transaction: storage wiped (unless kept), token
revoked, surfaces destroyed, shortcuts unregistered, slots released.

---

## 7. Security & distribution

### 7.1 Identity and isolation (one realm, one connection, one extension)

**A shared JavaScript global would break the entire security model.** If
several extensions ran in one page with one connection, they would share a
global object (extension A could patch the bridge B calls), and Rust would see
a single caller — so identity would have to be *asserted in the message*,
which any extension could forge. Capability enforcement would become
fiction. Figma's history is the warning: JS-side cleverness is not a boundary.

Therefore:

- **One isolated realm per extension.** Each tier-B extension runs in its own
  **Web Worker** — its own global scope, no DOM, message-passing only, and no
  reference to any other extension's worker. Workers cannot see or patch each
  other. (Workers, not iframes, for headless logic: no DOM is needed and they
  are far cheaper.)
- **The shared webview is only a supervisor.** It is Grain's own code whose
  sole job is to spawn, terminate and relay for workers. **No extension code
  ever executes in the supervisor's global.**
- **One WebSocket connection per extension, each with its own token.**
  Identity is therefore bound to the *channel*, not claimed in the payload —
  Rust always knows exactly which extension is calling, and forging another's
  identity is not expressible. This is the same reason Chrome pins native
  messaging hosts to specific extension ids rather than trusting a self-
  reported name.
- **Tokens** are high-entropy, minted per app run, bound server-side to
  `(extension id → granted capability set)`, and revoked on disable,
  uninstall, or permission change. A worker receives only its own token, at
  creation, inside its own isolated global. The token is presented **in the
  first WebSocket frame after connect** — never in the URL (query strings
  leak into logs); the server drops any connection that hasn't authenticated
  within a short deadline, and the listener binds to `127.0.0.1` only.
- **UI surfaces get their own realms too** — a `settings-panel` iframe,
  `workspace`, or `overlay` runs at its own opaque origin with its own token,
  scoped to the same extension.
- **RAM is unaffected:** one supervisor webview plus N workers costs far less
  than N webviews, and workers exist only while their extension is activated
  (the idle reaper is unchanged).

### 7.2 Boundary

- **Capability filtering happens in Rust**, per connection, on every message
  and every command. The JS sandbox is not a security assumption — it is
  defence in depth on top of the Rust check.
- **Providers** (`provides:`): the host defines the interface; the
  implementation may be core or a tier-C extension; the broker still enforces
  the consumer's grants. Provenance is shown to the consumer's user. Absence
  fails with a typed "capability unavailable".
- **Extensions may never inject APIs into Grain's runtime.** Native code runs
  in the extension's own process only.
### 7.3 Crate layout — `grain-sdk` is the dependency leaf

The contract must not depend on Grain's internals, or it cannot be versioned
independently and every internal change ripples into the public API.

```
grain-sdk   ← wire types + manifest schema + capability names + error codes
   ▲   ▲        (depends only on serde/specta — nothing Grain-specific)
   │   └── grain-pill
   └────── grain-core ── tauri shell
```

- **`grain-sdk` owns** `DaemonEvent`, `PillAction`, the manifest schema, the
  capability vocabulary, typed errors, and the `grainApi` version handshake.
- **`grain-core` depends on `grain-sdk`**, never the reverse. Phase 0 work
  item: move `crates/grain-core/src/event.rs` into `grain-sdk` and invert the
  dependency — cheap now, painful once anything third-party consumes it.
- A third party pulling `grain-sdk` therefore gets the contract and nothing
  else: no settings substrate, no Grain internals.

### 7.4 Distribution

- **Distribution:** a GitHub index repo, one JSON entry per extension (id,
  repo, version, manifest hash, tier, trust). Trust levels: `builtin` /
  `verified` / `community` / `dev` (local folder, badged). Tier A-inert lints
  automatically; A-egress, B and C get human review. `screen:capture` + `net:`
  together always triggers human review.

---

## 8. Phase conformance

Each phase is done when its checks pass.

| Phase | Done means |
|---|---|
| **0** | `grain-sdk` extracted as the dependency **leaf** (event.rs moved out of grain-core; dependency inverted, §7.3); WS rejects tokenless clients; **per-connection identity** proven by test (a client with extension A's token cannot act as B); pill works through the token path; `grainApi` handshake; capability filter unit-tested |
| **1** | Registry persists installed/grants/enabled **+ toggle order**; Overview tab renders from manifests; Snippets / Context Awareness / Agent ship as toggleable built-ins with the upgrade rule honoured (§10.1); **A-inert** packs import/export incl. **pill themes** (§9) and the Agent centre-layout variant (§10.2); no code executes |
| **2** | Supervisor webview + **one Worker and one authenticated connection per extension** (§7.1), created on activation and reaped when idle (verified by RAM measurement); capability-checked host API (events/storage/llm/embed/capture/clipboard/shortcuts); transform hook with timeout + strikes; **`session:start` + `sessionMode` slow stage**; auto-categorization ported as the first scripted built-in |
| **3** | Schema settings render (levels 1–2) incl. anchors + ordering; `workspace` extracted from Grain Space's window.rs as a host-owned generic with Grain Space as first consumer; `overlay`; pill slots; store slide-over; **Grain Space Test passes** |
| **4** | Tier-C supervisor (companion + provider roles); `settings-panel` iframes; `screen:capture` / `pointer` / `audio:play` as demand appears; 1–2 built-ins re-platformed |
| **5** | Index repo live; browse/install/update/remove; hash verification; trust badges; review checklist incl. lifecycle measurement |

> ⛔ **GATE before Phase 4/5 — see
> [GATE-DISTRIBUTION-AND-DEVMODE.md](GATE-DISTRIBUTION-AND-DEVMODE.md).**
> Rows 4 and 5 assume two things this project has never designed: a **hosting +
> submission + review + trust-progression platform** (with the guarantee that an
> author cannot forge "verified"), and a **developer mode** giving extension
> authors a real build/run/debug loop. Phase 3 is unaffected and proceeds. Row 5
> and the trust-dependent parts of row 4 do not start until that gate produces a
> design and a guide.

---

## 9. Pill theme packs

The main pill is the most visible surface in Grain and the cheapest to make
personal, because it is fundamentally a **25 × 8 grid of dots** (`COLS=25`,
`ROWS=8`, `DOT_D=3.0` in `crates/grain-pill`) plus a background. Restyling it
needs no code execution, no webview, and no process — so it is a **tier-A
pack** occupying the `pill.theme` slot.

### 9.1 What a theme may and may not change

| Themeable | Fixed by the host (v1) |
|---|---|
| Background per state (colour / gradient / alpha) | Pill **size** and grid geometry (25×8, dot diameter) |
| Dot colours (base + emphasis) | Position, anchoring, show/hide lifecycle |
| The animation for each of the four states | **Interactivity** — the pill is not clickable or hoverable |
| Whether the animation reacts to the microphone at all | The set of states themselves |

Size and interactivity stay locked deliberately, and this is recorded as a
restriction to revisit — not a principle. A future pass may open them once
the interaction model is settled.

### 9.2 All four states, or a per-state fallback

Grain's pill has exactly four states: **`idle`, `recording`, `processing`,
`fallback`**. A theme should define all four.

- A **missing state falls back to Grain's built-in animation for that state
  only** — never the whole theme, so a partial theme is still usable. The card
  states it plainly: *"Defines 3 of 4 states; Grain's default is used for
  Processing."*
- **Any failure falls back the same way**: an expression that fails to compile
  is rejected at install with a line-referenced error; one that misbehaves at
  runtime (NaN, non-finite, budget overrun) drops that state to the built-in
  animation, shows a one-time notice, and counts a strike. Three strikes
  disable the theme and restore the default. **The pill must always render.**

### 9.3 Reactivity is optional

`"reactive": false` means the animation ignores the microphone entirely — a
purely aesthetic design. This is explicitly allowed: a user who prefers a
beautiful non-reactive pill understands they are trading away the visual
"is my mic hearing me" cue, and the tray icon plus the recording sound still
signal state. The card labels it: *"Does not react to your voice."*

### 9.4 Format

```jsonc
"pillTheme": {
  "reactive": true,
  "background": { "idle": "#0b0a09e6", "recording": "#141312f2",
                  "processing": "#141312f2", "fallback": "#0b0a09cc" },
  "dot":        { "base": "#6c665a", "accent": "#d6a44c" },
  "states": {
    "idle":       { "pattern": "breathe", "params": { "period": 4.0 } },
    "recording":  { "expr": "clamp(level * (0.5 + 0.5*sin(t*3 + x*6)), 0, 1)" },
    "processing": { "pattern": "sweep",   "params": { "speed": 1.5 } },
    "fallback":   { "pattern": "static",  "params": { "brightness": 0.25 } }
  }
}
```

Each state is either a **named built-in pattern** with parameters (the easy
path — no expression knowledge needed) or an **expression** evaluated per dot
per frame.

**Expression environment** — a pure function, nothing else:

| Variable | Meaning |
|---|---|
| `x`, `y` | normalised position across the grid (0..1) |
| `col`, `row` | integer grid coordinates |
| `t` | seconds since entering this state |
| `level` | mic energy 0..1 (`self.energy`); **always 0 when `reactive:false`** |
| `elapsed` | seconds since recording began |

Whitelisted operators and functions only (`+ - * / %`, comparisons,
`sin cos abs clamp min max floor fract sqrt pow smoothstep noise`). No
variables outside this table, no state between frames, no I/O, no allocation.
Returns dot brightness 0..1 (and optionally a colour mix factor).

**Implementation note:** compile to a small bytecode at install; evaluate in
the pill process with a per-frame time budget. 25×8 = 200 evaluations per
frame is trivial, and the evaluator is a few hundred lines — this is a
calculator, deliberately **not** a scripting engine (Grain's "no unnecessary
engines" rule). No JS, no webview, no extra process: a theme's idle cost is
its JSON.

### 9.5 Scope: the main pill only

Themeable now: the main capsule (idle / recording / processing / fallback).

**Not themeable yet** — the Studio streaming window, the Agent input card, the
Grain Space surfaces. Those are still being designed in-house; freezing a
theming contract over a moving target would either block our own iteration or
break every theme when we change something. They open up once their designs
settle, and the `pill.theme` mechanism generalises to them unchanged.

---

## 10. Built-in extensions and shipped defaults

### 10.1 What ships pre-installed

Three of today's features become **built-in extensions**: pre-installed,
listed in Overview like any other, individually toggleable.

| Extension | Ships | Default (new installs) | Notes |
|---|---|---|---|
| **Snippets** | pre-installed | **off** | text snippets only |
| **Context Awareness** | pre-installed | **off** | generic app-category context |
| **Agent** | pre-installed | **off** | needs a real on/off switch — one does not exist today and must be added |
| **Snippet Actions** | *not* installed | — | recommended in the store; the "say a phrase → open an app/site" half, anchored at `snippets.after` |
| **Agent — Center layout** | *not* installed | — | see §10.2 |

> **Upgrade rule (required).** Defaulting to *off* applies to **new installs
> only**. An existing user who already has snippets configured, context
> awareness enabled, or the agent in use keeps them **on** through the
> migration — their features must not silently vanish. The one-time importer
> that moves each feature's settings from `AppSettings` into `ext.grain.*`
> also decides its initial enabled state from whether the user was using it.

### 10.2 Agent reply-surface variants

The Agent has two shipped looks today (`agent_panel_position`): the sidebar
and the centre panel. They become the first test of surface-variant packs:

- **Sidebar ships as the built-in default**, occupying slot
  `agent.reply-surface`.
- **The centre layout ships as an installable pack.** Installing it adds its
  name to the existing agent-position dropdown; selecting it takes the slot.
  Uninstalling returns the dropdown to the built-in options.

This is deliberately chosen as a **dogfood**: it answers "can the Agent's look
really be varied by an extension?" using a variant we have already built and
can compare against, before any third party depends on the mechanism. A
variant pack declares layout parameters only (anchor, max height, grow
behaviour); the async window-resize rule stays host-side where it already
lives.

### 10.3 Standing rule — re-review each phase before building it

Before starting any phase, re-check that its plan is still the best available
approach given what the previous phase taught. Phases are a route, not a
contract; a phase that no longer makes sense should be re-planned rather than
executed faithfully. Record the outcome of that review (kept / changed / why)
in the phase's commit, so the reasoning survives.
