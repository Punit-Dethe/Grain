# Capability Governance — how the platform grows

> **Rationale, not instructions.** The normative spec is [SPEC.md](SPEC.md) —
> build from that. This document records *why* the rules are what they are;
> where it differs in detail, SPEC.md wins.

Fifth document of the set ([PLAN.md](PLAN.md), [SPEC.md](SPEC.md),
[STRESS-TEST.md](STRESS-TEST.md), [CASE-HEYCLICKY.md](CASE-HEYCLICKY.md)).

HeyClicky was one test case and it exposed six gaps. There will be hundreds
of ideas, each wanting something we don't have. **The point is not to build
those six features** — it is to have a principled answer to *"the thing I
want to build needs something Grain can't do."* This document is that answer.

Three questions, answered in order:

1. Which missing capabilities must be built **early**, and which can wait?
2. May an extension **implement a missing capability itself**?
3. How does a request travel from *"I hit a wall"* to *"it's in the contract"*?

---

## Part 1 — Two kinds of missing capability

Not all gaps are equal, and conflating them is how platforms end up in a
forced migration.

### Structural capabilities — *change the shape of what an extension can be*

They define an extension's relationship to the app's core loop: who may own
a session, when extension code runs, what a "stage" is. **Every later
capability hangs off them.**

Grain's structural set is small, and CASE-HEYCLICKY found both members:

- **`session:start` + `contributes.sessionMode`** (R-1) — without it, an
  extension can only decorate *Grain's* dictation. Every voice-driven
  extension that isn't core dictation is impossible. That is an entire class.
- **The slow stage** (R-2) — the 150 ms `transform` hook cannot hold a
  multi-second model call. Without a sanctioned slow stage, every
  "think about this, then produce text" extension is impossible. Also a class.

**Test for structural:** *if we add this later, do extensions written before
it need rewriting, or does the contract's shape change?* Yes → structural.
Chrome's MV3 migration is what "yes, and we shipped it late" costs.

**Rule: structural capabilities land early or never.** These two belong in
Phase 2 with the scripted runtime, not deferred — not because HeyClicky
wants them, but because they determine whether the runtime has one shape or
two.

### Additive capabilities — *one more thing an extension may touch*

`screen:capture`, `audio:play`, `surface:pointer`, TTS, and almost everything
else. They are leaves: adding one later invalidates nothing, breaks nobody,
and costs exactly what it costs whenever it is built.

**Rule: additive capabilities are built on demand, never speculatively.**
Designing an interface for something no real extension is using produces a
bad interface — you cannot test a guess. Reserve the *name* so nobody squats
it; design the *shape* when the first real consumer exists.

Per your priorities, this classifies cleanly:

| Capability | Class | When |
|---|---|---|
| `session:start` + `sessionMode` | **structural** | Phase 2 — with the runtime |
| Image parts in `llm.complete()` | additive (small, high leverage) | soon — you called it required |
| `screen:capture` | additive | build it, but late — needs OS work per platform |
| `surface:pointer` | additive | on demand |
| `audio:play` / TTS | additive | not now |

### Phase 4 status (2026-07-23)

The structural contract is now live: `session:start`,
`contributes.sessionMode`, and the bounded/cancellable slow stage. Exact-host
`net:<host>` and write-only `secret` settings are also live because the
voice-note/network paths supplied concrete consumers and testable shapes.
Per-worker heap enforcement and the developer-only native `companion` tier are
runtime safeguards/shapes rather than requestable capability names.

The following names remain reserved and intentionally undesigned until a real
consumer exists: `provides:`/the provider broker,
`surface:settings-panel`, `screen:capture`, `surface:pointer`, `audio:play`,
`open:url`, `open:app`, `clipboard:read`, `clipboard:write`, pill action chips,
`contributes.promptLayer`, `overrides:*`, and `resident`. A native companion is
the current escape hatch for OS work; it does not create a new privileged Grain
API or confer capabilities on another extension.

---

## Part 2 — May an extension implement a missing capability itself?

The most important question here, and there are two well-documented
precedents pointing in opposite directions.

### Precedent A — Thunderbird Experiments: *yes, and here is the bill*

Thunderbird lets an add-on ship its **own privileged API implementation**
alongside the extension, giving access to internals that no WebExtension API
exposes. It is genuinely powerful — and the documented cost is exact:
Experiment APIs **bypass the WebExtension permission system entirely**, so
the per-permission prompt is replaced by a single *"have full, unrestricted
access to Thunderbird, and your computer."* Thunderbird also notes the
design burden — APIs must be "generic and distinct… designed with foresight
to avoid backward incompatible changes."

Translated to Grain: it deletes D3 (the Rust capability boundary), deletes
the per-capability permission sheet, and re-creates the XUL trap (R1) where
private privileged code couples to internals and freezes our ability to
refactor. **Verdict: no, not for community extensions.**

### Precedent B — xdg-desktop-portal: *the implementation is swappable, the interface is not*

Linux solved the same problem with a broker: sandboxed apps never touch host
resources directly; they call **host-defined portal interfaces**, and the
*implementation* behind each interface is pluggable (GNOME, KDE, wlroots…
all implement `org.freedesktop.impl.portal.*`). The app neither knows nor
cares who implements it; the boundary and the consent flow stay in the
broker.

That is the pattern Grain should adopt.

### The answer: capability providers

> An extension **may** implement a missing capability — as a **provider of a
> host-defined interface**, never as an API of its own invention.

```jsonc
// A tier-C native extension that implements screen capture before core does
"provides": ["screen.capture@1"]

// Any tier-B extension, unchanged before or after core implements it natively
"permissions": ["screen:capture"]
```

Mechanically:

- **Grain designs the interface** (`screen.capture@1`) — a schema, cheap to
  write, versioned. R1 holds: the interface exposes no internals.
- **The implementation may come from core or from a provider extension.**
  The consumer calls the same host API either way and never learns which.
- **The broker is still Rust.** The consumer still needs the `screen:capture`
  grant; capability filtering, rate limits, and the session-scoping rules are
  enforced by the host, not by the provider. A provider cannot hand out more
  than the interface allows.
- **Provenance is visible.** The permission sheet and the extension card say
  *"Screen capture — provided by **Clicky Companion** (community)"*, because
  the consumer's data flows through that provider. A provider is a trust
  dependency and must never be invisible.
- **Graceful absence.** No provider and no core implementation → the call
  fails with a typed "capability unavailable" the extension can handle, and
  the card shows *"needs Screen Capture, which nothing on this system
  provides"* with a link. Same UX as a `requires.settings` mismatch.
- **Core implementing it later is a non-event.** Same interface → consumers
  don't change a line; the provider extension simply becomes unnecessary and
  can be uninstalled.

This gives the ecosystem its escape hatch without giving away the boundary:
**you may build the missing thing, but you must build it to our shape.** That
single constraint is the entire difference between a healthy platform and the
XUL extinction event.

Two guard rails:

- Providers are **tier C only** (native, reviewed, lifecycle-audited) — the
  tier that already carries human review and kill-clean supervision.
- Providers are **not a dependency graph.** A consumer depends on an
  *interface*, never on a named extension (STRESS-TEST #13 still stands:
  extension→extension dependencies are rejected). If two providers offer the
  same interface, it is a **slot** — one enabled occupant, takeover prompt.

---

## Part 3 — The request pipeline

A public, boring, predictable process beats case-by-case judgment.

**Lane 0 — Workaround triage.** Can it be done with what exists? Most
requests end here, happily. (CASE-HEYCLICKY Part 3 is a worked example:
prefix routing, output suppression, typing, and memory all looked like gaps
and were not.)

**Lane 1 — Request.** A public issue with four fields: *what are you
building · what's blocked · the smallest interface that would unblock it ·
who else needs it.* The last field is the one that matters — it separates a
class from an app.

**Lane 2 — Interface design + experimental.** *We* design the interface;
it ships on the `experimental` channel (R3: dev-mode + manifest opt-in,
unstable, no marketplace listing). Implementation by core **or** by a
provider extension — whichever is faster. Nothing stabilizes here.

**Lane 3 — Stabilize.** After at least one real extension has used it in
anger, it enters the versioned contract under `grainApi` semver. Interfaces
that nobody used after a full release cycle are removed from experimental
rather than stabilized.

### Assessment criteria (so it isn't vibes)

1. **Class or app?** Does it unlock a category of extensions or exactly one?
2. **Structural or additive?** Structural gets priority and scrutiny;
   additive waits for a real consumer.
3. **Could it be declarative?** Data beats code (the Zen Mods lesson —
   themes, packs, and variants need no runtime).
4. **Does it expose internals?** If yes, redesign the interface until it
   doesn't (R1). This is usually possible: `surface:pointer` was "let me draw
   on a full-screen window" reshaped into "tell the host where to point."
5. **What is the worst-case abuse, and is there an honest consent surface
   for it?** If the sheet can't state the risk in one plain sentence, the
   interface is wrong.
6. **Does it survive destroy-if-not-in-use?** Anything requiring a permanent
   resident process needs an extraordinary reason.
7. **Who maintains it if the requester vanishes?** Core owns every stabilized
   interface forever. Budget for that before saying yes.

### Publish the anti-roadmap

A public list of capabilities **considered and declined, with reasons** —
alongside the accepted ones. It prevents re-litigation, sets author
expectations honestly, and makes "no" a documented position rather than
silence. Current entries: raw window handles, direct DOM/React access to
Grain's UI, arbitrary filesystem access outside the extension's scope,
extension→extension dependencies, always-on background processes without
`resident`.

---

## Part 4 — What this means for an extension author

The promise we can put in the docs, honestly:

- **Most ideas need nothing new.** Packs and the scripted tier cover a lot;
  check Lane 0 first.
- **If you hit a wall, there is a queue, not a void** — with published
  criteria and a visible decision.
- **If you can't wait, you may build the OS-facing part yourself** as a tier-C
  companion. It is an ordinary separate program at the OS boundary and still
  enters Grain through the same authenticated, capability-checked protocol.
  Companions are developer-only until Phase 5A adds distribution trust rails.
- **What you may never do** is invent your own privileged API surface. That
  road ends in an extinction event, and we have read the postmortems.

---

## Part 5 — Amendments

Appended to STRESS-TEST's ten and CASE-HEYCLICKY's six:

17. **Capability classification** — every proposed capability is labeled
    *structural* or *additive*; structural lands early or never, additive is
    built on demand with only its name reserved in advance.
18. **`provides:` in the manifest** — tier-C extensions may implement
    host-defined interfaces; consumers request the capability, the broker
    routes, provenance is shown, absence fails gracefully, and duplicate
    providers are a slot.
19. **The four-lane request pipeline** + published assessment criteria +
    a public **anti-roadmap** of declined capabilities.
20. **Structural capabilities promoted into Phase 2** (`session:start`,
    `contributes.sessionMode` with its slow stage) — they set the runtime's
    shape and cannot be retrofitted cheaply.
21. **Phase 4 correction.** The structural session contract, exact-host network
    proxy, secrets, worker memory ceiling, and developer-only companion are
    live. Amendment 18's provider broker remains a reserved proposal with zero
    consumers; no `provides:` manifest field or extension-to-extension routing
    was shipped.

---

*Sources: [Thunderbird — Introducing Experiments](https://developer.thunderbird.net/add-ons/mailextensions/experiments)
· [Thunderbird — Working with WebExtension Experiments](https://webextension-api.thunderbird.net/en/mv3/guides/experiments.html)
· [XDG Desktop Portal](https://flatpak.github.io/xdg-desktop-portal/)
· [VS Code — using proposed API](https://code.visualstudio.com/api/advanced-topics/using-proposed-api)*
