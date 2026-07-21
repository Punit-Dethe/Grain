# Grain Extensions — start here

Plain-language guide to the design — no jargon, no prior context assumed.
Read this page, then [SPEC.md](SPEC.md) if you are implementing it.

**The goal in one sentence:** if Grain Space didn't exist, someone outside the
team should be able to *build* it — without forking Grain.

---

## The five ideas that hold everything up

**1. Three ways to build, pick the smallest one that works.**
Some extensions are just *files* — a set of prompts, snippets, or a colour
theme. No code, nothing to run, nothing to go wrong. Some need real logic, so
they're JavaScript running in a hidden window Grain creates when needed and
throws away when idle. A few need to do things only a real program can do
(take a screenshot, talk to hardware) — those ship a small native program that
Grain starts and stops. Most extensions will be the first kind.

**2. Extensions never own windows.**
An extension doesn't get to create windows and leave them lying around. It
*declares* what it needs — "I have a settings section," "I need an app-like
window," "I want to point at something on screen" — and **Grain** builds it,
places it, sleeps it, and destroys it. That's how "destroy what isn't in use"
stays true no matter who writes the extension: it isn't a rule people have to
remember, it's the only thing possible.

**3. The security wall is in Rust, not in JavaScript.**
Every extension talks to Grain over a local connection. That connection checks
permissions on every single message. So an extension without permission to see
transcripts doesn't get filtered results — it never receives them at all.
Clever JavaScript can't get around it, because the wall isn't made of
JavaScript. (Figma learned this the hard way: their clever JS sandbox had a
security hole and they moved the wall lower.)

**4. Your settings can't collide with anyone's.**
Every extension's settings live in their own labelled drawer
(`ext.<its-id>.…`), physically separate from Grain's own settings, and **no
extension can write to a Grain setting at all**. So "an extension broke my
settings" isn't a bug we have to prevent — it's not expressible.

**5. Extensions describe their settings; Grain draws them.**
An extension lists what it needs — a toggle here, a folder picker there, a
password field — and Grain renders it with Grain's own controls. Everything
looks native, everything is searchable, and the settings page works even when
the extension is asleep. (Obsidian let plugins draw their own settings; the
result is unsearchable, and the community had to write a plugin just to tidy
the sidebar.)

---

## "But what if Grain can't do the thing I need?"

This is the question that decides whether a platform lives, so it gets three
answers depending on what you need.

**Most of the time, you don't need a new power — you need a slot.**
Example: adding a new speech-to-text service. That *sounds* like it needs deep
microphone access. It doesn't — Grain already handles microphones and already
knows how to call transcription services. You just describe the service (its
address, its request format) in a small file. No code, no risky permissions.

**If you need something genuinely new, there's a queue, not a void.**
You file a request; we design the interface; it ships as "experimental" while
someone actually uses it; then it becomes permanent. We also publish the list
of things we've said **no** to, and why — so "no" is a documented position
rather than silence.

**If you can't wait, build it yourself.**
Your extension can ship its own small native program. Inside that program you
can do anything your operating system lets you do — take screenshots, whatever
— because that program is *yours*, and the OS is what polices it, not us. What
Grain controls is the door between your program and Grain: your program only
gets the transcripts, sessions, and other data the user explicitly approved.

The one thing you may never do is bolt new abilities onto *Grain's* insides.
That's the road Firefox went down: old add-ons could reach into anything, which
meant Firefox couldn't improve itself without breaking them, and escaping cost
an extinction event that destroyed thousands of add-ons. We'd rather say no
once than do that.

And nothing is wasted: the thing you build privately can graduate into a shared
capability others use, and eventually into something Grain does itself — with
the *interface* staying the same, so nobody's work breaks.

**One physical fact worth knowing up front:** a transparent window can't see
what's behind it. That isn't our rule — no operating system allows it. Windows
only contain what you drew in them. Screenshots always need a real screen-capture
API. So an overlay is great for *drawing* a selection box or a pointer, but the
actual picture-taking is a separate thing.

---

## The documents

**If you are building this, read [SPEC.md](SPEC.md) and build from it.** It is
the single normative document: manifest schema, every capability, the settings
system, the UI, lifecycle, security, and a per-phase "done means" checklist.
Everything decided in earlier passes — including all corrections — is folded
into it, so you never have to reconcile two files.

The rest are **rationale**. Read them to understand *why* a rule exists;
never to decide what to build.

| Document | Role |
|---|---|
| **[SPEC.md](SPEC.md)** | **Normative.** What to build. |
| [PLAN.md](PLAN.md) | Why the architecture is shaped this way (the five decisions) + phases |
| [STRESS-TEST.md](STRESS-TEST.md) | Contract tested against three of our own features; how clashes are arbitrated |
| [CASE-HEYCLICKY.md](CASE-HEYCLICKY.md) | Contract tested against a real outside app |
| [CAPABILITY-GOVERNANCE.md](CAPABILITY-GOVERNANCE.md) | How the platform grows: which gaps get filled early, and how requests are decided |
| [FREEDOM-LADDER.md](FREEDOM-LADDER.md) | The four levels of power, and how to build past the contract |

---

## Build order

| Phase | What ships |
|---|---|
| **0** | Lock the door: the local connection gets authentication and permission checks. *(This is a security fix Grain needs anyway — right now any program on your computer can read your transcripts.)* |
| **1** | Extensions become visible: an installed list, permission sheets, and shareable data packs. |
| **2** | Real extensions run: the JavaScript runtime, plus the two abilities that decide the *shape* of everything later (an extension owning a voice session, and a stage slow enough for an AI call). |
| **3** | Extensions get faces: settings sections, app-like windows, pointer overlays. **This is where the Grain Space test passes.** |
| **4** | Native programs, custom pill looks, and moving some built-in features onto the public contract. |
| **5** | The marketplace. |
