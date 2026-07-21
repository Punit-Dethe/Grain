# Final Review — flaws found before implementation

Last pass over the six design documents before Phase 0. Eight flaws, each
with a fix. Two of them contradict claims made earlier in the set; those are
marked **CORRECTION** because the earlier text was wrong, not merely
incomplete.

---

## FLAW 1 — Extension settings cannot live next to the feature they extend

**The problem.** [SETTINGS-AND-UI.md](SETTINGS-AND-UI.md) says every
extension's settings live in its card under the Extensions page. That is
right for self-contained extensions (a notes app) and **wrong** for
extensions that extend a core feature.

Concretely: snippet *actions* ("say a phrase → open an app or site") are
slated to become an extension while snippets stay core. Under the current
design their settings would move to a distant Extensions card, while plain
snippets stay in Dictation — splitting one mental concept across two places.
(Today they are even worse off: `SnippetsSection` and `ActionsSection` are
separate **tabs** in the Experimentations page.)

**Fix — anchored settings groups.**

Core settings pages expose a short list of **named anchors**. A manifest may
target one:

```jsonc
"settings": {
  "anchor": "snippets.after",
  "groups": [ { "id": "actions", "title": "Snippet actions", "items": [ … ] } ]
}
```

Rules that keep it clean:

- **Rendered there and *only* there.** One source of truth in the UI. The
  extension's card keeps Overview / Permissions / Data & Advanced and shows
  *"Settings appear in Dictation → Snippets"* with a jump link. Day-to-day
  tweaking happens next to the feature; lifecycle lives in the card.
- **Visually quiet**: a thin divider and one muted attribution line —
  *"Snippet actions · from the Snippet Actions extension ⚙"* — then the
  controls in Grain's own components (host-rendered, so they match by
  construction). No nested cards, no boxes inside boxes, no second tab bar.
- **Crowding rule**: with more than two extensions at one anchor, each
  collapses to an accordion row so the page can't grow unbounded.
- **Disabled → gone.** No ghost UI in core pages; re-enable from the card.
- **Anchors are contract surface** (R1): few, semantic, tied to *features*
  not layout — `snippets.after`, `dictation.pipeline.after`, `context.after`,
  `agent.after`, `models.after`. Adding one is a promise; removing one is a
  breaking change. If an anchor ever disappears, the group falls back to the
  extension's card — settings are never lost.

**Bonus, independent of extensions:** merge today's Snippets and Actions tabs
into one scrollable section now. They are one concept; the tab split is the
same mistake in miniature.

---

## FLAW 2 — **CORRECTION**: "tier-A packs are zero risk" is false

**The problem.** [PLAN.md](PLAN.md) repeatedly calls data packs "zero code,
zero risk," and Phase 1 is sold as *"a real ecosystem can start here with
zero attack surface."* That is wrong for one pack class: **provider packs**.

A declarative STT provider pack receives **your audio**. A post-process
provider pack receives **your transcript**. The pack contains no code — but
it names the URL those go to. Zero code is not zero risk when the data leaves
on a route the pack chose.

**Fix — split tier A in two:**

| Pack kind | Contents | Risk | Consent |
|---|---|---|---|
| **Inert packs** | prompts, snippets, voice-action sets, context modes, themes, surface variants | genuinely none — data consumed locally | install freely |
| **Egress packs** | STT / post-process / LLM providers | **your audio or transcript leaves the machine** | must declare the host; card and permission sheet state it in one plain sentence: *"sends your dictation audio to `api.example.com`"*; counts as `net:<host>` |

Phase 1 ships **inert packs only**; egress packs arrive with the consent
surface, not before.

---

## FLAW 3 — **CORRECTION**: an extension cannot wake inside the transform budget

**The problem.** The transform hook budgets ~150 ms
([SETTINGS-AND-UI.md](SETTINGS-AND-UI.md) #3), while a cold extension-host
wake is ~300 ms ([PLAN.md](PLAN.md) Part 6). An `onTransform`-activated
extension therefore times out on its own first call, every session — the two
numbers contradict each other.

**Fix.** `onTransform` activates at **session start**, not when the pipeline
reaches the step. Waking happens during recording (seconds of slack); by the
time a transcript exists the extension is warm and the 150 ms budget applies
to real work only. The idle reaper still collects it after the session.

---

## FLAW 4 — `requires.settings` and `overrides:` can fight

**The problem.** Extension A hard-requires `post_process_enabled = true`;
extension B holds `overrides:post_process_enabled` and sets it false.
[STRESS-TEST.md](STRESS-TEST.md) Part 1 defines each mechanism alone but not
their collision: today B would silently pause A.

**Fix.** The override slot's takeover prompt must state the collateral:
*"Focus Mode will turn Post-processing off. This pauses **Snippet Actions**,
which requires it. [Continue] [Cancel]"* — and A's card shows *"paused by
Focus Mode"* with a one-click resolution, rather than the generic "needs
Post-processing" that would misdirect the user to a setting another
extension controls.

---

## FLAW 5 — Uninstalling a provider silently breaks its consumers

**The problem.** [CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md) says a
missing capability "fails gracefully," but the uninstall flow never warns.
Remove the extension that provides `screen.capture@1` and two other
extensions quietly stop working.

**Fix.** Provider uninstall/disable enumerates dependents first: *"2 enabled
extensions use Screen Capture provided by this. They will stop working."*
Consumers then show *"needs Screen Capture — nothing on this system provides
it"* with a link to find a provider. Same visible-orphan discipline already
applied to leftover settings data.

---

## FLAW 6 — Session conflict has no defined user experience

**The problem.** "The coordinator serializes" says what the code does, not
what the user sees. If an extension owns a live session and the user presses
the core dictation key, the design is silent.

**Fix.** Second claimant is rejected, and the pill says which owner holds the
session: *"Clicky is listening"*. Core bindings do **not** preempt an
extension session (surprise cancellation loses speech); the user cancels
explicitly. `SessionCancelled` already exists as the escape hatch.

---

## FLAW 7 — Nothing can ever earn navigation presence

**The problem.** "The core sidebar never grows" correctly kills Obsidian's
per-plugin sprawl, but over-corrects: if Grain Space became an extension, a
major app-class surface would be reachable only through a row in a list.

**Fix — user-pinning.** Extensions can never *demand* navigation space;
the **user** may pin an extension's surface to the sidebar or tray from its
card. Extension-driven growth stays impossible; user-driven promotion becomes
possible. Unpinning is one click and never uninstalls.

---

## FLAW 8 — Pipeline order across the two hooks was never stated

**The problem.** With both a fast `transform` and a slow `sessionMode`, the
order was implied but never written — and getting it wrong lets an extension
silently undo the model's work.

**Fix — one documented order, matching Grain's existing pipeline:**

```
transcript → transforms (fast, ordered, user-visible) → slow stage (sessionMode
or core post-processing) → output slot → paste
```

Transforms run **before** the slow stage — exactly where `finalize_transcript`,
snippets, scrap-that and voice actions run today. Nothing runs after the slow
stage except the output slot. This ordering is contract, shown in the
pipeline UI.

---

## Amendments

25. Anchored settings groups (`settings.anchor` + a short, versioned anchor
    list); settings render at the anchor only, card carries a jump link.
26. Tier A splits into **inert** vs **egress** packs; Phase 1 ships inert
    only; egress packs declare their host and require consent.
27. `onTransform` activates at **session start**, resolving the wake-vs-budget
    contradiction.
28. Override takeover prompts must state collateral damage to other
    extensions' hard requirements.
29. Provider uninstall enumerates dependents; consumers show a "nothing
    provides this" state.
30. Session-conflict UX: reject + name the owner; core never preempts an
    extension session.
31. **User-pinning** of extension surfaces to sidebar/tray (user-driven only).
32. Canonical pipeline order documented as contract.

With these, the design set is internally consistent and Phase 0 can begin.
