# Extension Platform — Stress Test & Arbitration Design

Third document of the set ([PLAN.md](PLAN.md) → architecture,
[SETTINGS-AND-UI.md](SETTINGS-AND-UI.md) → settings/UI). This one pressure-
tests the contract *before* implementation: core-setting dependencies,
exclusive-resource collisions, shared-resource fairness, and three real
decompositions of shipped Grain features. Amendments it forces are listed at
the end and applied to PLAN.md.

Motivation is the Handy-isolation lesson: retrofitting an architecture costs
10× designing it. Everything here is cheap now and brutal later.

---

## Part 0 — What other ecosystems paid to learn

| Ecosystem | What happened | Binding rule for Grain |
|---|---|---|
| **Firefox XUL → WebExtensions** | Legacy add-ons were "as powerful as the browser itself"; every internal refactor broke them, which "progressively killed efforts to make Firefox secure and fast" — escape required the Firefox 57 extinction event that destroyed thousands of add-ons | **R1. Never expose internals.** Extensions see the contract, never Grain's structs, React components, window handles, or event internals. Power granted can never be taken back without an extinction event — so grant narrowly and widen later. |
| **VS Code** | Deliberately never exposed the DOM ("the structure can change and extensions tightly coupled to the UI would break"); extensions run in a separate host so the user is "always in control"; **lazy activation events**; **proposed APIs** gated to Insiders until stabilized | **R2. Activation events, not resident processes** (formalized below). **R3. An `experimental` API channel**: new host APIs ship gated behind a dev-mode flag + manifest opt-in, stabilize only after dogfooding — so v1 mistakes are correctable without breaking anyone. |
| **Figma plugins** | Whitelist sandbox, small auditable surface; logic sandbox split from UI iframe; their clever Realms shim still produced a security vulnerability → moved to a QuickJS VM. Lesson: JS-side cleverness is not a security boundary | **R4. The boundary is Rust** (already D3 in PLAN.md — this is the independent confirmation). Whitelist calls; keep the bridge surface small enough to audit in one sitting. |
| **Chrome MV2 → MV3** | Permission/lifecycle model bolted on later ⇒ ecosystem-wide forced migration, years of hostility | **R5. Permissions, slots, and lifecycle semantics must be right in v1.** This document exists because of R5. |
| **Obsidian** (from SETTINGS-AND-UI research) | Code-rendered settings + per-plugin sidebar = unsearchable, unscalable | Already applied in SETTINGS-AND-UI.md. |

**R2 concretely — activation events.** The manifest declares *when* a tier-B
extension wakes: `onEvent:<name>` (a subscribed DaemonEvent fires),
`onShortcut:<id>`, `onSurfaceOpen:<id>`, `onPillAction:<id>`,
`onTransform` (its pipeline step is reached), `onStartup` (requires
`resident`). The reaper is the inverse of activation. This replaces the
vaguer "woken by its subscriptions" in PLAN.md — same idea, now a contract
term the card can display ("wakes when: a recording finishes").

---

## Part 1 — Extensions that need core settings changed

The invariant stands: **no write path to core settings, ever** (a Grain
Space-class store of one's own is extension state; `post_process_enabled` is
the user's). Three mechanisms cover every legitimate need without breaking
it:

### 1a. Declarative requirements (`requires.settings`)

```jsonc
"requires": {
  "settings": [
    { "key": "post_process_enabled", "equals": true, "level": "hard" },
    { "key": "rolling_live_preview", "equals": true, "level": "soft",
      "why": "Live preview is needed for inline suggestions; without it only end-of-dictation suggestions work." }
  ]
}
```

- **hard**: the extension cannot run without it. At enable time the
  permission sheet gains a *requirements* block: "Needs **Post-processing**
  ON — currently OFF. [Turn it on and enable] [Cancel]". One click, explicit
  consent, no settings safari. If the user later flips the core setting off,
  the extension drops to a "paused — needs Post-processing" state on its
  card (never a silent failure), with the same one-click fix.
- **soft**: enable proceeds; the dependent features show the "why" text
  grayed in the extension's settings.
- The `key` universe is a **published subset** of core settings (an
  allowlist in grain-sdk — R1: not every internal knob becomes contract
  surface). Start with the obviously feature-relevant ones:
  `post_process_enabled`, `context_awareness_enabled`, `rolling_live_preview`,
  `push_to_talk`, `overlay_position != none`, `always_on_microphone`,
  `audio_conditioning`, agent enabled/panel settings, selected-model
  *category* (streaming vs standard — not the concrete model id).

### 1b. Consented, tracked, reversible overrides (`overrides:<setting>`)

For extensions whose *point* is to change core behavior (a "Focus Mode"
extension that moves the overlay, an alternative recording overlay that
needs the built-in pill hidden). Chrome solved this years ago for search
engine/homepage takeovers; we take the proven shape:

- `overrides:<setting>` is a **capability**, listed on the permission sheet.
- While the extension is enabled, the core setting shows a chip in Settings:
  *"Controlled by Focus Pill"* — with the previous value remembered.
- Disable/uninstall → prompt to restore the remembered value (default yes).
- Overrides are **slots** (below): two enabled extensions cannot override
  the same setting; the second prompts a takeover switch.

### 1c. Deep links

Any extension UI may link `grain://settings/<section>#<key>` for the "I'd
rather look at it myself" path. Never the primary mechanism.

**Decision rule:** *is the setting the user's preference the extension must
respect (→ `requires`, ask), or the very thing the extension exists to
manage (→ `overrides`, attributed chip)?* An extension gets one or the
other per setting, never silent writes.

---

## Part 2 — Exclusive resources: the chokepoint registry

"Only one allowed, but two extensions may try" — enumerated now, each with
its arbitration. The mechanism is uniform: **slots**. A slot is an exclusive
position declared in the manifest (`"slots": ["overlay.recording"]`);
the registry enforces **at most one enabled occupant per slot**; enabling a
second triggers an explicit takeover prompt ("Native Pill currently provides
the recording overlay. Switch to GlassPill? [Switch] [Keep current]") —
never silent, never load-order-dependent. Core defaults occupy slots too, so
"switch back to built-in" is the same UI.

| # | Chokepoint (today's code) | Kind | Arbitration |
|---|---|---|---|
| 1 | Recording session (`TranscriptionCoordinator` serializes capture) | hard singleton | Not extensible in v1. Extensions *observe* sessions; they never start/own recordings. Revisit only with real demand. |
| 2 | Recording overlay surface (grain-pill) | slot `overlay.recording` | tier-C replacement occupies it; built-in pill is the default occupant. |
| 3 | Pill theme (tokens) | slot `pill.theme` | one theme pack active; switching is instant (data only). |
| 4 | Agent reply surface layout (`agent_panel_position` side/center) | slot `agent.reply-surface` | surface-variant packs (Part 4c). |
| 5 | Active post-process prompt (`post_process_selected_prompt_id`) | user choice, not a slot | prompt packs only *add* prompts to the switcher; nothing selects itself. An extension wanting auto-selection needs `overrides:post_process_selected_prompt_id`. |
| 6 | STT route (local model / cloud rotation) | core policy | provider packs add providers to the *pool*; route/rotation policy stays core. No slot exposed in v1 (R1: widen later if demanded). |
| 7 | Model engine slot (one resident model) | hard singleton | never extension-visible; `llm`/`embed`/transcription requests queue behind core use. |
| 8 | Master chords (Alt+1/Alt+2) & core shortcuts | reserved | binding registry rejects extension claims on core-reserved combos; everything else = existing conflict UI (SETTINGS-AND-UI #1). |
| 9 | Final output action (paste via `utils::paste`) | slot `output.destination` | default occupant = paste. An "output to X instead" extension takes the slot with takeover prompt. *Additional* destinations ("also send to…") are pipeline taps, non-exclusive, post-paste. |
| 10 | Transcript transform chain | ordered pipeline (not a slot) | user-visible reorderable pipeline (SETTINGS-AND-UI #2), per-step timing + timeout strikes. |
| 11 | Prompt composition (BASE→CONTEXT→MODE→spoken) | budgeted layer list | Part 4b: extension layers get a fixed insertion point, per-layer token budget, user-visible order. |
| 12 | Auto-dictionary UIA watcher | hard singleton (OS hook) | stays core; extensions get its *events*, never their own watchers. |
| 13 | Selection capture (clipboard round-trip in `agent`/`capture`) | serialized primitive | host serializes `capture.selection()` calls (one at a time, short queue, timeout) — two extensions calling simultaneously is safe, just sequenced. |
| 14 | Clipboard write | shared, restorable | host-mediated write with Grain's existing restore etiquette; rate-limited per extension. |
| 15 | Provider identity (`provider.id` in packs) | namespace | pack-declared providers are auto-namespaced `ext:<extid>:<providerid>` — two "openai" packs can coexist; the *keys* are per-provider-entry, so no key-route collision is representable. |
| 16 | Global event bus itself | shared | read-only fan-out; extension `PillAction`-style reverse calls are namespaced per extension token. No extension can emit core events (no spoofing a `TranscriptionComplete`). |

Rows 15–16 are the direct answer to "two extensions taking the same API key
route": keys live per provider entry, provider ids are namespaced per
extension, extensions never read keys, and calls are attributed — the
collision is structurally unrepresentable, and the *contention* that remains
(quota/rate) is Part 3.

---

## Part 3 — Shared-resource fairness (the contention that remains)

- **LLM & embed calls**: every host call is attributed to its extension
  token. Two priority classes: `interactive` (dictation post-processing,
  agent turns — the user is waiting) always preempts `background` (extension
  calls default here; an extension serving a user-invoked surface may
  request interactive per-call, subject to rate cap). Per-extension defaults:
  N llm-calls/min, M embed-batches/min, strike-based like transforms;
  numbers tunable per extension in Data & Advanced. Router quotas
  (`quota_used_today`) gain per-extension attribution so the card can show
  "used 41 AI calls today" — the user sees who spends their budget.
- **CPU/latency on the paste path**: already bounded (transform timeout).
  The pipeline UI showing per-step milliseconds makes slow extensions
  socially visible, which Obsidian/VS Code both learned is half the battle.
- **Storage**: per-extension quota (default e.g. 200 MB, raisable by the
  user in Data & Advanced), usage shown on the card.
- **Event flood**: `AudioLevel`-class high-frequency events require their own
  capability (`events:audio-levels`) and are never delivered to sleeping
  extensions (no wake-on-level) — waking 3 extensions 30×/second is the kind
  of failure R2 exists to prevent.

---

## Part 4 — The three decompositions, run as tests

The user's chosen core/extension split: **core keeps basic snippets, basic
context awareness, one agent profile (sidebar)**. Each carve-out below is
walked against the contract; every gap found is an amendment (Part 5), which
is precisely the value of the exercise.

### 4a. Voice actions as an extension (core keeps text snippets)

Spoken trigger → strip from transcript → open apps/sites
(`voice_actions.rs` today).

| Needs | Contract has? |
|---|---|
| Match & strip trigger in final transcript | ✅ `transform:transcript` (pipeline step) |
| Launch an app / URL | ❌ **GAP-1**: no launch capability. Add `open:url` and `open:app` — separate, dangerous-marked permissions; the sheet lists them prominently; tier-A packs can never hold them (code tiers only). |
| Settings: trigger→targets table | ❌ **GAP-2**: Level-1 schema lacks structured lists. Add setting type `rows` (a typed column list — trigger/text/target). Snippets, voice actions, app modes, custom words are *all* rows-shaped; this one type unlocks the whole family and keeps them out of Level-3 iframes. |
| Capture an app path from the running system ("add current app") | covered by `context.current()` (4b) — nice-to-have, not blocking |

**Verdict: buildable** once GAP-1/GAP-2 land. Latency note: as an extension
this rides the transform pipeline (JS, budgeted) instead of inline Rust —
for a power-user opt-in that trade is acceptable, and it validates D4's
"runtime-later is selective" honestly.

### 4b. App-specific context modes as an extension (core keeps generic context awareness)

Per-app hard prompt layers (`AppMode` + `compose_prompt`'s MODE stage
today).

| Needs | Contract has? |
|---|---|
| Know the foreground app at dictation time | ❌ **GAP-3**: formalize `context.current()` + `events:context` behind a `context:app` capability (privacy-sensitive: exe/title/url_host — its own permission line, never bundled). Detection itself stays core (one OS call, one answer, shared). |
| Inject a prompt layer for matching apps | ❌ **GAP-4**: `contributes.promptLayer` — a declared insertion point in the composition (`base < context < extension-layers < mode < spoken`), **per-layer token budget** (default ~200), user-visible order alongside the transform pipeline. Two context extensions = two budgeted layers in a visible list, not a fight. |
| App-matcher settings table | GAP-2's `rows` type |

**Verdict: buildable** with GAP-3/GAP-4. Note the layering answer: core's
generic context awareness occupies the CONTEXT stage; extension layers slot
*between* CONTEXT and MODE; the spoken Prompt Record instruction stays
above everything — hierarchy preserved, no collisions possible.

### 4c. Agent center panel as an aesthetic extension (core keeps sidebar profile)

`agent_panel_position = center` + its layout/auto-grow behavior today.

| Needs | Contract has? |
|---|---|
| Re-layout/re-skin a **core-owned** surface | ❌ **GAP-5**: nothing in the plan restyles core surfaces — pill themes were the only variant mechanism. Add **surface-variant packs**: tier-A declarative layout+token bundles for *designated* core surfaces (`agent.reply-surface`, `pill.theme` retrofits into this), occupying the matching slot. Zen Mods proves declarative restyling carries a real ecosystem. |
| Position/size/grow parameters | part of the variant schema (from the two shipped profiles: anchor, max-height, grow mode — the window-resize-async rule stays host-side where it already lives) |
| Exclusivity vs the built-in profile | slot `agent.reply-surface` (Part 2 #4); built-in sidebar profile is the default occupant |

**Verdict: buildable as a tier-A pack** once GAP-5 lands — zero code, which
is the right cost for an aesthetic. A *behaviorally* different reply surface
(new interactions) escalates to `surface:overlay` (tier B) or tier C; the
three tiers hold.

**Migration corollary:** when these three ship as built-in extensions, their
existing user settings migrate from core `AppSettings` into `ext.grain.*`
namespaces via a one-time importer, and the vacated core fields are pruned —
that importer is part of each conversion, not an afterthought (Chrome MV3
rule R5).

---

## Part 5 — Amendments applied to the plan

1. **GAP-1** `open:url` / `open:app` capabilities (code tiers only,
   danger-marked).
2. **GAP-2** `rows` setting type in the Level-1 vocabulary.
3. **GAP-3** `context:app` capability: `context.current()` + `events:context`.
4. **GAP-4** `contributes.promptLayer` with fixed insertion point, token
   budget, visible ordering.
5. **GAP-5** surface-variant packs + the **slots** mechanism generally
   (manifest `slots`, registry-enforced single occupancy, takeover prompts,
   core defaults as occupants).
6. **Activation events** replace "woken by subscriptions" as contract terms
   (R2).
7. **`experimental` API channel** (R3): dev-mode + manifest opt-in for
   unstable host APIs; nothing stabilizes before a built-in has dogfooded it.
8. **Core-settings mechanism** (Part 1): `requires.settings`
   (hard/soft + one-click consented fix + paused-state), `overrides:<key>`
   capability with attribution chips and restore-on-disable, published
   settings allowlist in grain-sdk.
9. **Fairness** (Part 3): call attribution + priority classes + per-extension
   quotas visible on cards; no wake-on-high-frequency-events.
10. **Namespaced provider ids** (`ext:<extid>:<id>`) killing the API-key-route
    collision class.

## Readiness verdict

With these ten amendments, all three carve-outs pass, the collision classes
identified are either structurally unrepresentable (settings, keys,
provider ids, event spoofing) or arbitrated by one uniform mechanism
(slots + takeover prompts + attribution), and the ecosystem-lesson rules
R1–R5 are encoded in the contract rather than in good intentions.
**Phase 0 can start.**

---

*Sources: [Figma — How to build a plugin system and sleep well at night](https://www.figma.com/blog/how-we-built-the-figma-plugin-system/)
· [Figma — plugin security update](https://www.figma.com/blog/an-update-on-plugin-security/)
· [Why did Mozilla remove XUL add-ons](https://yoric.github.io/post/why-did-mozilla-remove-xul-addons/)
· [Deprecating XUL for WebExtensions (LWN)](https://lwn.net/Articles/668956/)
· [VS Code — activation events](https://code.visualstudio.com/api/references/activation-events)
· [VS Code — extensibility patterns & principles](https://vscode-docs.readthedocs.io/en/stable/extensions/patterns-and-principles/)
· [VS Code — using proposed API](https://code.visualstudio.com/api/advanced-topics/using-proposed-api)*
