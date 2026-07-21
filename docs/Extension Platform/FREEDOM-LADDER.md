# The Freedom Ladder — building beyond the contract

> **Rationale, not instructions.** The normative spec is [SPEC.md](SPEC.md) —
> build from that. This document records *why* the rules are what they are;
> where it differs in detail, SPEC.md wins.

Sixth document ([PLAN.md](PLAN.md), [SPEC.md](SPEC.md),
[STRESS-TEST.md](STRESS-TEST.md), [CASE-HEYCLICKY.md](CASE-HEYCLICKY.md),
[CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md)).

The question this settles: **"Grain doesn't have screen capture / TTS / X.
Can I just build it myself?"** Answer: **yes — and here is exactly how,
today, without waiting for us.** But one popular idea for how doesn't work,
for reasons that have nothing to do with our rules.

---

## Part 0 — The honest physics (what no policy can grant)

> **A transparent window cannot see what is behind it.**

This is worth stating plainly because it is the most common plugin-author
misconception. A window — transparent, click-through, full-screen, any of it
— only ever contains **what you drew into it**. The compositor never hands a
window the pixels underneath; that isolation is the whole point of a window
system. So "open a transparent overlay and read the screen out of it" is not
blocked by Grain's policy — it is not a thing any OS permits.

Real screen capture always means calling an OS capture API:

| Platform | API | Permission |
|---|---|---|
| Windows | Desktop Duplication / `Windows.Graphics.Capture` / GDI | none historically — a native binary can just do it |
| macOS | ScreenCaptureKit | **TCC Screen Recording**, granted per signed binary |
| Linux/X11 | X11 | none |
| Linux/Wayland | `xdg-desktop-portal` ScreenCast | portal consent dialog |

What an overlay window *is* genuinely good for is the **UI half** — drawing a
selection rectangle, a highlight, a pointer. The **capture half** is always
an OS call. HeyClicky does exactly this split (NSPanel for the triangle,
ScreenCaptureKit for the pixels), and any Grain extension will too.

So the real question isn't "can I trick a window into capturing" but **"can
my extension call an OS capture API?"** — and that has a good answer, below.

---

## Part 1 — Your two examples, answered

### "Can I build screen capture myself?"

**Yes — from a companion process (rung 4), today.** Your extension ships a
small native binary. It calls the platform capture API directly, holds its
own OS permission (on macOS it prompts for Screen Recording against *your*
signed binary), draws its own selection overlay if it wants one, and sends
the image wherever you like — including into Grain's `llm.complete()` once
image parts land, or to your own endpoint via your own network stack.

Grain supplies what only Grain can: the shortcut, the session, the
transcript, the slow stage, and typing the answer back. Grain is **not** in
the capture path at all, so no `screen:capture` capability is required for
your own use of it. You need one only when you want *other* extensions to
use your capture ability — that's rung 3.

### "Can I add a speech-to-text provider?"

**Yes, and more cheaply than you expect — usually with no code at all.**

Grain already routes STT through `stt_router` with HTTP adapters
(`stt_client.rs`). A new cloud STT service is therefore a **declarative
provider pack** (tier A): describe the endpoint, auth header, request and
response shape. Zero code, zero runtime, zero risk — and you never touch raw
audio, because the host does the capture→provider plumbing for you.

If the service needs something a declarative template can't express (custom
websocket framing, a proprietary streaming handshake), it escalates to a
**tier-C provider** implementing `stt.provider@1` — a native binary that
receives audio frames from the host and returns transcripts. Same shape as
LSP: we define the protocol, you implement it in any language.

Note what did *not* happen: you never needed a "give me the raw microphone"
capability. **Most requests for a low-level primitive are really requests for
a slot in a pipeline the host already runs** — and the slot is safer, easier,
and survives our refactors.

---

## Part 2 — The freedom ladder

Four rungs. Power increases; convenience and reach decrease. Pick the lowest
rung that does the job — that is the whole guidance.

| Rung | What it is | Freedom | Cost |
|---|---|---|---|
| **1. Pack** | pure data (prompts, snippets, providers, themes, surface variants) | whatever the schema allows | none — no code, no review friction, no runtime |
| **2. Scripted** | JS in the shared extension host | everything the capability set allows | cannot exceed the contract, by construction |
| **3. Provider** | native binary implementing a **host-defined interface** (`provides: ["screen.capture@1"]`) | full native power *inside your process*, and you **extend what every other extension can do** | human review; you must fit our interface |
| **4. Companion** | native binary that is **just your program**, spawned and supervised by Grain | full native power inside your process — any OS API your binary is permitted to call | human review; the ability stays **private to your extension**; you carry your own OS permissions |

**Rung 4 is the answer to "can I just build it myself?"** — and it is the
same trust model every serious ecosystem lands on. Chrome and Firefox call it
[native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging):
the extension itself can do nothing native, but it may exchange messages with
a native host binary, pinned by `allowed_origins` to specific extension ids.
Mozilla states the trade honestly — the security model for those files "is
much more like that for native applications than that for extensions."

That sentence is the deal, and Grain takes it:

- **Inside your own process: the OS is your sandbox, not us.** We neither can
  nor should police what a native binary does with permissions its user
  granted it.
- **At the Grain boundary: capability-checked, always.** Your companion gets
  Grain's data (transcripts, sessions, context) only through grants the user
  approved, over the same authenticated channel everything else uses.
- **Supervised like the pill**: spawned on activation, health-checked, killed
  on disable, never orphaned. Destroy-if-not-in-use is enforced by the
  supervisor, and tier-C review *measures* idle RAM rather than trusting a
  promise.
- **Marked in the marketplace**: "includes a native component," with the
  permissions it requests. Users decide with that in front of them.

LSP is the proof this is not a compromise but an *upgrade*: VS Code chose
"separate process + protocol" over in-process language plugins, and the
result outgrew the editor entirely — one server, every IDE. The documented
costs are the honest ones — a separate process is slower, is another failure
point, and is more limited than an editor-specific plugin — and they were
worth paying.

---

## Part 3 — The line that never moves

Exactly one thing is forbidden at every rung, and it is the Thunderbird line
from [CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md):

> **You may not inject new APIs into Grain's own runtime.**

Write any native code you like in your own process. You may not hand Grain's
JS host a privileged object, patch our internals, or invent an API surface
that other extensions then depend on. That is what turned XUL into an
extinction event and what makes Thunderbird's Experiments collapse every
permission prompt into *"full, unrestricted access to your computer."*

The distinction is clean: **your process is yours; Grain's process is ours;
the boundary between them is a capability-checked protocol.**

---

## Part 4 — The graduation path (nothing is wasted)

```
rung 4 companion            rung 3 provider              core
"I built it for me"   →     "I implement                 "Grain ships it
                             screen.capture@1"      →      natively"
```

- Build it as a **companion** because you need it now.
- If others want it, we design the interface *with you* and your binary
  becomes a **provider** — now every extension can use your ability.
- If it becomes common, **core implements the same interface** natively.
  Consumers change nothing; your provider becomes redundant and can retire.

At every step the *interface* is the stable artifact and implementations are
swappable — the xdg-desktop-portal and LSP lesson. Work done at rung 4 is
never thrown away; it is the prototype that earns the interface.

---

## Part 5 — What this means concretely for a HeyClicky-class extension

With only the **two structural capabilities** from
[CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md) (`session:start` +
`sessionMode` with its slow stage) plus image parts in `llm.complete()`,
a third party can ship a full HeyClicky equivalent **as a rung-4 companion**:

| Piece | Who does it |
|---|---|
| Hotkey, session, STT, transcript | **Grain** (local, private, free) |
| Screenshot + selection overlay + pointer | **their companion** (own OS permission) |
| Vision LLM call | **Grain** `llm.complete()` with image parts — user's own key and quota |
| Typing the answer back | **Grain** output path (clipboard-restore etiquette) |
| Speaking the answer | *skipped* — or their companion plays it |

So `screen:capture`, `surface:pointer`, and `audio:play` are **not blockers
for the idea existing** — they are the *later* graduation of an ability
someone will have already proven at rung 4. That is precisely the order
CAPABILITY-GOVERNANCE prescribes: additive capabilities get built when a real
consumer exists, and now we know how that consumer survives in the meantime.

---

## Part 6 — Amendments

Appended to the previous twenty:

21. **The freedom ladder is the documented answer to "can I build it
    myself?"** — four rungs, guidance is *pick the lowest that works*.
22. **Rung 4 (companion) is formalized**: manifest-declared native binary,
    spawned/supervised/killed by the host, pinned to its owning extension id
    (Chrome's `allowed_origins` model), reaching Grain only through granted
    capabilities, marked "includes a native component" in the marketplace.
23. **"Most primitives are really pipeline slots"** becomes Lane-0 triage
    doctrine — ask *which existing pipeline wants a new participant?* before
    designing any new low-level capability. (Worked example: a new STT
    service is a declarative pack, not a microphone capability.)
24. **The physics note ships in the author docs** — a transparent window
    cannot capture the screen; overlays are the UI half, OS APIs are the
    capture half — so nobody spends a week discovering it.

---

*Sources: [Chrome — Native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)
· [MDN — Native messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging)
· [LSP — the idea behind the protocol](https://microsoft.github.io/language-server-protocol/overviews/lsp/overview/)
· [VS Code — language server extension guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
· [XDG Desktop Portal](https://flatpak.github.io/xdg-desktop-portal/)*
