# Grain Extension Platform — Implementation Plan

> **Rationale, not instructions.** The normative spec is [SPEC.md](SPEC.md) —
> build from that. This document records *why* the rules are what they are;
> where it differs in detail, SPEC.md wins.

The product bar is set by [WHAT-GRAIN-NEEDS.md](../WHAT-GRAIN-NEEDS.md); this
document decides the technical shape and the build order. Its acceptance test
is one sentence:

> **The Grain Space Test.** If Grain Space did not exist, a third-party
> developer must be able to build it — capture hooks, its own storage,
> semantic recall, an app-like window, shortcuts, a settings page — as an
> extension, without forking Grain.

Every design decision below is scored against that test plus Grain's standing
rules: destroy-if-not-in-use, low RAM, no unnecessary engines, and the
Handy/Grain isolation boundary (extensions are Grain surface — nothing here
touches `src/handy/`).

---

## Part 1 — The five architecture decisions

### D1. One contract, three runtimes

There is exactly **one extension contract** (manifest + capabilities + typed
events + host API), but three ways to run against it, ordered by power and
risk:

| Tier | Runtime | For | Cost when idle |
|---|---|---|---|
| **A. Packs** | none — pure data | prompt packs, snippet sets, voice-action sets, context modes/categories, pill themes (tokens), provider definitions (declarative HTTP) | zero |
| **B. Scripted** | JS in ONE shared, hidden, on-demand **extension-host webview** | logic + UI: capture flows, transforms, panels, workspace apps (Grain Space class) | zero (webview exists only while an enabled extension needs it; destroyed on idle) |
| **C. Native** | separate process, supervised by the host (the pill model, generalized). Two roles: a **companion** (private to its extension) or a **provider** implementing a host-defined interface for everyone — see [FREEDOM-LADDER.md](FREEDOM-LADDER.md) | screen capture, custom pill renderers, alternative overlays, hardware/OS integrations | zero (not spawned = not running) |

Why JS and not WASM for tier B: Grain's extension ideas are UI-heavy (pill
looks, notes apps, agent panels) and its authors are app developers — the
ecosystems that won this game (Obsidian, Raycast, VS Code) are all JS. Grain
is Tauri, so a webview host is native to the stack, and WebView2/WKWebView
RAM is paid **only while the host is alive**, which the lifecycle rules bound
tightly. WASM (Extism-style) stays on the roadmap as a *fourth* tier for
hot-path text transforms if the JS timeout budget ever proves limiting — the
contract is runtime-agnostic, so adding it later breaks nothing.

Why three tiers and not one: most real submissions will be tier A (zero code,
zero risk, reviewable by diff), and forcing them through a code runtime would
be engine-building for nothing. Tier C exists so ambition has a sanctioned
path — better a marked, permission-gated native tier than forks.

### D2. Extensions never own windows. They declare surfaces; the host owns lifecycle.

This is the answer to the Tauri-window question. An extension manifest
*declares* surfaces; Grain creates, positions, sleeps, and destroys them.
Lifecycle discipline is enforced **by construction**, not by review:

| Surface | What it is | Lifecycle owner behavior |
|---|---|---|
| `settings-panel` | a custom section inside the extension's own detail view in Settings (sandboxed iframe — see [SPEC.md](SPEC.md)) | exists only while that detail view is open; destroyed on navigate-away |
| `workspace` | an app-class window (the Grain Space class) | generalization of Grain Space's proven **sleeping-window** pattern (`grain_space/window.rs`): built hidden once, shown on summon, React unmounted + hidden on close, destroyed after an idle timeout. Cap: at most N workspace windows awake (LRU sleeps the rest). |
| `overlay` | transient HUD near the pill / cursor (agent-panel class) | host-created per invocation, destroyed on dismiss; hard budget on size and lifetime; separate permission |
| `pill` slots | declarative contributions to the native pill: extra action chips, state→animation mapping, theme tokens | no code runs in the pill process — the pill renders data. Full pill *replacement* is tier C. |

So: a third party with "a different idea for an agent overlay" doesn't get a
raw `WebviewWindow` — they get `surface: overlay` and the host instantiates
it under the same rules the built-in agent panel obeys (async creation per
tauri#3990, positioning, destroy-on-dismiss). If someone genuinely needs a
window shape the surface catalog can't express, that's a tier-C native
extension and is *marked* as such in the marketplace — the "allow, review,
ask to resubmit with better lifecycle handling" policy the marketplace
section encodes.

**No tab system, no persistent bottom dock.** Tabs and docks are
always-visible chrome for sometimes-used features. Settings-class UI renders
in the Extensions detail view; app-class UI is a `workspace`; transient UI is
an `overlay` — and the launcher (Part 4) reuses surfaces that already exist.
(The Quick Panel is being retired and appears nowhere in this design; if its
successor shell wants an extension rail later, surfaces are declared data, so
it can add one without any manifest change.)

### D3. Capabilities are enforced in Rust at the process boundary, not by trust in the runtime

Everything — the pill today, tier-B host, tier-C processes — talks to the
core over the local WS (`events_server.rs`). That server currently has **no
authentication**: any local process can connect and read every transcript.
Fixing that is Phase 0 and becomes the enforcement point for the whole
platform:

- On connect, a client presents a **per-client token** (pill: generated at
  spawn; extension host: injected at webview creation; tier C: passed in env
  at spawn). No token → no events.
- Each token maps to a **capability set** derived from the extension's
  manifest *as granted by the user*. The Rust session layer filters events
  (an extension without `events:transcripts` never receives
  `TranscriptionComplete`) and rejects commands (no `llm` capability → the
  `llm.complete` host call errors). JS-sandbox quality is therefore not a
  security assumption — the boundary is in Rust.
- Capability names are the manifest's `permissions:` and drive the
  marketplace's "what access it requires" display 1:1.

Initial capability vocabulary (extend as needed):
`events:sessions`, `events:transcripts`, `events:audio-levels`,
`events:context` + `context:app` (foreground app info — privacy-marked),
`transform:transcript` (fast hook), `session:start` (own a serialized
capture session with a *slow* stage — see CASE-HEYCLICKY.md), `capture:selection`,
`clipboard:read`, `clipboard:write`, `open:url`, `open:app` (danger-marked,
code tiers only), `screen:capture` (session-scoped, indicator-backed,
egress-named — the platform's most dangerous capability), `audio:play`,
`storage`, `llm`, `embed`, `net:<host>` (proxied fetch, per-host),
`shortcuts`, `surface:settings-panel`, `surface:workspace`,
`surface:overlay`, `surface:pointer` (host-rendered pointing from
declarative commands), `pill:slots`, `settings`,
`overrides:<core-setting>` (tracked, attributed, reversible — see
STRESS-TEST.md Part 1), `resident`.

Two contract concepts refined by [STRESS-TEST.md](STRESS-TEST.md):
**slots** (exclusive positions — recording overlay, agent reply surface,
pill theme, output destination — registry-enforced single occupancy with
explicit takeover prompts; core defaults are occupants too) and
**activation events** (`onEvent:` / `onShortcut:` / `onSurfaceOpen:` /
`onPillAction:` / `onTransform` / `onStartup`-requires-`resident` — the
manifest says when a tier-B extension wakes; the reaper is the inverse).
New host APIs ship on an **experimental channel** (dev-mode + manifest
opt-in) and stabilize only after a built-in dogfoods them.

### D4. Built-in features become extensions by *manifest first*, by *runtime later*

Converting, say, voice actions from a zero-overhead inline Rust interceptor
into out-of-proc JS would add latency to the paste path — a real trade-off,
and the user's instruction is explicit: least effort must not trade anything
away. So conversion happens in two steps, and the second is optional per
feature:

1. **Manifest-ize (cheap, uniform):** every optional feature gets a manifest
   + card in the Extensions settings page (which already exists as the
   ex-Experimentations page) with the same enable/disable, permission
   display, and "what does this cost when on" fields a third-party extension
   shows. Implementation stays exactly where it is (in-proc Rust, `builtin`
   tier). The user sees ONE mental model; the code changes are a registry and
   UI, not a rewrite.
2. **Re-platform (selective):** move a feature's implementation onto the
   public contract only where the contract is genuinely sufficient — which is
   itself the best possible test of the contract ("dogfooding the SDK").
   Hot-path text interceptors (snippets, scrap-that) likely stay in-proc
   forever; that is fine and costs nothing.

First manifest-ize wave (power-user features, per the user's call): voice
actions, snippets, "scrap that", context awareness (+ its modes), auto-
dictionary, Prompt Record, prompt switcher, auto-categorization, Grain Space
itself, agent panel variants.

### D4b. Growth is governed, and extensions may implement missing capabilities — to *our* interface

The long tail is the real design problem: every ambitious extension will want
something that doesn't exist yet. [CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md)
is the standing answer — capabilities are classified *structural* (land early
or never) vs *additive* (built on demand); requests travel a four-lane
pipeline (workaround triage → request → experimental interface → stabilize)
against published criteria and a public anti-roadmap of declined capabilities.

An extension that cannot wait **may implement a missing capability itself** —
but only as a tier-C **provider of a host-defined interface** (`provides:`),
never as a privileged API of its own invention. Consumers request the
capability; the Rust broker routes, enforces, and shows provenance; when core
implements it natively, nothing downstream changes. This is
xdg-desktop-portal's swappable-backend model, and it is deliberately *not*
Thunderbird's Experiments model — those bypass the permission system entirely
and collapse every prompt into "full, unrestricted access to your computer."

### D5. The marketplace is an index and a policy, not an infrastructure

Obsidian-style: a public `grain-extensions` GitHub repo holding one JSON
entry per extension (id, repo, version, manifest hash, tier, trust level).
The app fetches the index, installs from GitHub releases, verifies the
manifest hash. Trust levels: `builtin` / `verified` / `community` /
`dev` (local folder, loaded unsigned with a visible badge). Submission is a
PR to the index repo — and the review checklist encodes the philosophy
mechanically:

- tier A/B: automatic — manifest lints, no undeclared capabilities, surface
  declarations only (no window APIs exist to misuse).
- tier C: human review against the lifecycle rules (idle RAM measured,
  windows created only through the surface API it ships, kill-clean
  verified). Failing submissions get "resubmit with better lifecycle
  handling" — the exact governance the user described: possibility preserved,
  philosophy defended.

---

## Part 2 — The contract (grain-sdk)

A new small crate `crates/grain-sdk` (Tauri-free, depends only on serde +
grain-core types) is the versioned, published contract:

### Manifest (`grain-extension.json`)

```jsonc
{
  "id": "com.example.spaces",          // reverse-dns, unique in the index
  "name": "Spaces",
  "version": "0.3.1",
  "grainApi": "^1.0",                  // contract semver — host refuses mismatches
  "tier": "scripted",                  // pack | scripted | native
  "entry": "main.js",                  // tier B: bundle; tier C: per-OS binary map
  "permissions": ["events:transcripts", "storage", "llm", "embed",
                   "surface:workspace", "shortcuts", "capture:selection"],
  "surfaces": {
    "workspace": { "title": "Spaces", "minSize": [900, 600] },
    "panel":     { "title": "Spaces settings", "kind": "settings" }
  },
  "contributes": {
    "shortcuts": [{ "id": "spaces.open", "default": "Ctrl+Shift+S" }],
    "pill": { "actions": [{ "id": "spaces.capture", "icon": "note", "when": "recording" }] }
  },
  "packs": { "prompts": "prompts.json" } // tier A payloads ride along in any tier
}
```

### Host API (the JS/native bridge — every call capability-checked in Rust)

- `events.on(name, cb)` — filtered DaemonEvent stream (the 39-variant bus that
  already drives the pill; **this is the API being promoted, not invented**).
- `commands.*` — session queries, `capture.selection()`, `clipboard.*`.
- `storage.*` — per-extension scoped KV + document store + a directory under
  app-data (quota'd; wiped on uninstall).
- `llm.complete(prompt, opts)` — routed through Grain's post-process
  router/rotation. Extensions never see API keys.
- `embed(texts)` — Grain's embedding model (the vault already runs one);
  Grain Space-class semantic features without shipping a model.
- `transform.transcript(cb)` — opt-in interceptor with a **hard timeout**
  (e.g. 150 ms, configurable down): on timeout the transcript passes through
  unmodified and the pill shows a one-time warning chip. The paste path can
  never hang on an extension.
- `surface.open/close/update(...)`, `settings.get/set` (own namespace),
  `shortcuts` registration (through the existing binding registry).

### Events versioning

`DaemonEvent` gains `#[serde(other)] Unknown` on the consumer side and a
`schema` handshake on connect. Additive changes are free; breaking changes
bump `grainApi` major. The pill becomes the first client of the versioned
contract (it already speaks it unversioned).

---

## Part 3 — Extension points mapped to today's code

| Extension point | Where it lives today | Contract shape |
|---|---|---|
| Session/pill lifecycle events | `grain_actions.rs` helpers → `bridge::emit` | `events:sessions` (exists) |
| Live/final transcripts | `AsrStreamText`, `TranscriptionComplete`, … | `events:transcripts` (exists) |
| Transcript transforms | `voice_actions::intercept`, `apply_snippets`, `strip_scrapped`, `finalize_*` | `transform:transcript` hook; built-ins stay in-proc |
| Prompt composition layers | `context_detect::compose_prompt` (BASE/CONTEXT/MODE + spoken) | `contributes.promptLayer`: fixed insertion point (between CONTEXT and MODE), per-layer token budget, user-visible order — STRESS-TEST 4b |
| Post-process providers | `post_process_router` + provider list | declarative provider packs (HTTP template) + `llm` host call |
| STT providers | `stt_router` / `stt_client` | declarative provider packs |
| Prompt packs / snippets / voice actions | settings vectors | tier-A packs (pure data, ships first) |
| Pill look & feel | grain-pill (native, winit) | `pill:slots` tokens/params (data); full replacement = tier C |
| Agent tools & context | `agent.rs` (`run_turn`, summon modes, panel) | `agent:tool` contributions (command + schema), context providers; reply-surface *look* via surface-variant packs on slot `agent.reply-surface` (STRESS-TEST 4c); behaviorally new surfaces via `surface:overlay` |
| Core-feature carve-outs (voice actions, app-specific context modes, agent center panel) | `voice_actions.rs`, `AppMode`/`compose_prompt`, `agent_panel_position` | validated decompositions in STRESS-TEST Part 4 — each ships as a built-in extension when converted, with settings migrated `AppSettings` → `ext.grain.*` |
| Notes/knowledge apps | `grain_space/**` (vault, capture, recall, window) | the Grain Space Test — see Part 5 |
| Shortcuts | binding registry + `ACTION_MAP` | `contributes.shortcuts` → registry (dispatch machinery untouched) |
| Settings UI | Extensions settings page (ex-Experimentations) | declarative schema (default) or `surface:settings-panel` iframe — [SPEC.md](SPEC.md) |

---

## Part 4 — Discovery & launch (the "how do I open it" question)

No new persistent surface. Extensions are reachable through what exists:

1. **Settings → Extensions master list**: each installed extension's row
   carries an **Open** affordance when it declares a workspace/overlay
   surface. This is the "squarish launchers" instinct, placed inside a
   window that already exists instead of a new dock (see
   [SPEC.md](SPEC.md) for the master–detail layout).
2. **Global shortcuts**: `contributes.shortcuts` — the Grain-native way in a
   keyboard-first app.
3. **Tray submenu**: "Extensions ▸" listing open-able surfaces.
4. **Pill action chips** (`pill:slots`): in-session touchpoints, like Prompt
   Record's chip today.

A persistent dock is rejected: always-visible UI for sometimes-used features
inverts the product's minimalism, and the pill must stay sacred.

---

## Part 5 — The Grain Space Test, walked

Could a third party build Grain Space on this platform? Capability by
capability:

| Grain Space needs | Platform provides | Phase |
|---|---|---|
| React notes UI in its own window, sleep-on-close | `surface:workspace` (the generalized sleeping window — extracted from the very code Grain Space proved) | 3 |
| Capture selection quick-add + capture mode | `capture:selection`, `events:sessions`, pill action chip | 2–3 |
| Store notes on disk | `storage` (scoped dir + document store) | 2 |
| Embeddings + semantic recall | `embed()` host call | 2 |
| AI structuring / recall answers | `llm.complete()` via the router (no keys) | 2 |
| Global shortcuts (open / quick-add / recall) | `contributes.shortcuts` | 2 |
| Settings page | declarative schema settings; custom `settings-panel` iframe if needed ([SPEC.md](SPEC.md)) | 3–4 |
| Recall answering in an overlay | `surface:overlay` | 3 |

Verdict: after Phase 3, yes — a determined author rebuilds ~90% of Grain
Space; the remaining 10% (agent-pill text-input integration, folder-watch
reconcile) becomes contract work items discovered by dogfooding, which is
exactly what Phase 4's re-platforming pass is for.

---

## Part 6 — Lifecycle & resource enforcement (philosophy → mechanism)

- **Idle reaper**: the extension-host webview is created on first need and
  destroyed after a configurable idle window (no subscriptions firing, no
  surface open). Event-driven extensions cold-start on demand (~300 ms wake —
  acceptable for capture/summon flows; never in the paste path).
- **`resident` permission**: an extension that genuinely needs to stay warm
  must declare it; the card shows it ("keeps a background runtime alive"),
  and the reaper exempts it. Default is non-resident.
- **Transform budget**: hard per-call timeout + a strike counter; three
  timeouts disable the transform with a visible notice.
- **Workspace cap**: max N awake workspace windows (start N=1 beyond Grain
  Space), LRU sleeps the excess.
- **Kill-safety**: tier C processes get the pill supervisor's treatment —
  spawn, health, kill on disable, never orphaned.
- **Uninstall = gone**: storage wiped, token revoked, surfaces destroyed,
  shortcuts unregistered — in one transaction.

---

## Part 7 — Build order

Phases are sequential but each ships user-visible value on its own.

**Phase 0 — Secure, versioned transport** *(small; prerequisite for
everything, and a security fix Grain needs regardless)*
Token auth on the events WS; per-connection session layer with capability
filtering (pill = first client, full-trust token); `grain-sdk` crate holding
the manifest schema + event/command types + `grainApi` version handshake.

**Phase 1 — Registry, manifests, packs** *(the platform becomes visible)*
Extension registry (installed set, grants, enable state) in grain-core
settings; Extensions page renders cards from manifests; built-ins
manifest-ized (D4 step 1); tier-A pack import/export (prompt packs, snippet
sets, voice-action sets, context modes, declarative providers, pill theme
tokens). Zero code execution — a real ecosystem can start here (shareable
`.grainpack` files) with zero attack surface.

**Phase 2 — Scripted runtime, headless half** *(the SDK becomes real)*
Hidden extension-host webview + loader (dev-mode from a local folder);
bridge with capability-checked host API: events, storage, llm, embed,
capture, clipboard, shortcuts; transform hook with the timeout budget; idle
reaper. Dogfood: port **auto-categorization** (piggybacks an LLM call — pure
logic, no UI) as the first scripted built-in.

Also in Phase 2 — the two **structural** capabilities
([CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md) Part 1), because they
set the runtime's *shape* and cannot be retrofitted cheaply (the Chrome MV3
lesson): **`session:start` + `contributes.sessionMode`** (an extension may
own a serialized capture session) and its **slow stage** (the sanctioned
place for multi-second model calls, distinct from the fast `transform`
hook). Everything else CASE-HEYCLICKY found is *additive* and is built on
demand, never speculatively — names reserved, shapes designed when a real
consumer exists.

**Phase 3 — Surfaces** *(the UI half)*
The Extensions master–detail UI with Level 1/2 schema-rendered settings
(per [SPEC.md](SPEC.md)); `surface:workspace` extracted
from Grain Space's window.rs into a host-owned generic (Grain Space becomes
its first consumer — refactor, not rewrite); `surface:overlay` generalized
from the agent panel; pill slots (action chips + theme tokens); launcher
affordances (master-list Open buttons, tray, shortcuts). **The Grain Space
Test passes here.** (Custom `settings-panel` iframes land in Phase 4 with the
asset protocol + bridge hardening.)

**Phase 4 — Native tier + dogfood re-platforming**
Tier-C supervisor (generalized pill supervisor: manifest-declared binary,
token in env, health, kill-clean); pill replacement as the flagship tier-C
example. Re-platform 1–2 built-ins per D4 step 2 where the contract
suffices; every gap found becomes an SDK issue.

**Phase 5 — Marketplace**
Index repo + in-app browse/install/update/remove; manifest-hash
verification; trust badges; the tier-C review checklist (lifecycle
measurement, resubmission flow). Docs site page for authors ("build your
first extension in 10 minutes" — a tier-A pack).

Deliberate deferrals: WASM tier (until a hot-path need is proven), extension
auto-updates beyond index-version prompts, paid extensions, non-GitHub
distribution, sandboxed `net` beyond per-host proxying.

---

## Part 8 — Risks & honest unknowns

- **WebView RAM while the host lives** (~40–80 MB): bounded by the reaper;
  worst case is one webview for *all* extensions, not one each. Acceptable;
  measured in Phase 2 exit criteria.
- **JS is not a security sandbox**: true — the Rust boundary is the sandbox
  (D3). The webview additionally runs with no filesystem/shell access and a
  CSP that only allows the bridge.
- **Contract churn**: mitigated by dogfooding built-ins before opening the
  index (Phases 2–4 all eat the SDK before strangers do).
- **Two UI stacks temptation**: extensions must not import Grain's internal
  React components initially; a tiny published UI-kit (tokens + primitives)
  comes with Phase 3 to keep look-and-feel coherent without freezing
  internals.
- **`#[serde(other)]`/schema drift between pill and sdk**: single source —
  the pill migrates onto grain-sdk types in Phase 0, so there is exactly one
  event enum forever.
