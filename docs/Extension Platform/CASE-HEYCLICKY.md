# Case Study — Could HeyClicky be built on Grain's extension platform?

Fourth document of the set ([PLAN.md](PLAN.md) → architecture,
[SETTINGS-AND-UI.md](SETTINGS-AND-UI.md) → settings/UI,
[STRESS-TEST.md](STRESS-TEST.md) → arbitration). The three carve-outs in
STRESS-TEST were *our* features decomposed. This is the harder test: a
**real third-party app nobody designed our contract around**.

[HeyClicky](https://www.heyclicky.com/) is a Mac ambient agent: hold a
hotkey → stream speech to STT → screenshot every monitor → send both to a
vision LLM → speak the reply and/or type text and/or fly a fake cursor
triangle to point at an element.

**Premise (user's instruction):** ignore the "our LLM path has no image
input" roadblock — multimodal input with capability-detection and graceful
fallback is planned. What *else* blocks it?

**Verdict up front:** buildable as a **tier-B scripted extension** after
**6 additions**, 2 of which are contract-only (cheap) and 4 of which are
host features core must build once (an extension author cannot add them
themselves at tier B). No addition breaks an architectural invariant — and
one of them ends up *better* than HeyClicky's own design.

---

## Part 1 — What already maps cleanly (more than expected)

HeyClicky's spine is *"push-to-talk → transcript → LLM with context →
typed output"*. That is **exactly Grain's dictation pipeline**, which is
why so much lands for free:

| HeyClicky piece | Grain equivalent | Notes |
|---|---|---|
| Push-to-talk hotkey | binding registry + `contributes.shortcuts` | hold semantics already exist (`push_to_talk`) |
| Streaming STT (AssemblyAI/Mistral, cloud, per-word cost) | **core, local, free, private** | Grain is strictly better here — no vendor, no egress |
| "Clean up my rambling into a real prompt" | post-process stage + `contributes.promptLayer` | this is literally Grain's existing feature |
| Typing the result | `utils::paste` (clipboard restore etiquette, paste-method settings) | more careful than a raw synthetic-type |
| Conversation memory ("last 10 exchanges") | `storage` | scoped, quota'd, wiped on uninstall |
| App/URL context hints | `context:app` (`exe`, title, `url_host`) | cheaper than OCR for the "which app am I in" question |
| Settings (provider, hotkey, voice) | declarative schema, incl. `secret` + `keybind` | zero settings-UI code |
| Its own LLM backend | `llm.complete()` (user's keys, quota-attributed) **or** `net:<host>` | via `llm` the user's own Claude budget is used — better than a vendor Worker |
| Cloudflare Worker holding keys | unnecessary | Grain already brokers keys; extensions never see them |

That last column matters: several of HeyClicky's architectural choices exist
to work around *not being inside a dictation app*. On Grain they dissolve.

---

## Part 2 — The roadblocks

Ranked by how much they block, and split by who must build them.

### R-1. Extensions cannot start a capture session *(contract — blocking)*

STRESS-TEST chokepoint #1 says the recording session is a hard singleton and
"extensions **observe** sessions; they never start/own recordings."
HeyClicky's entire interaction is *its own* push-to-talk mode. Dead stop.

**Fix — `session:start` capability + `contributes.sessionMode`.** An
extension declares a named mode with its own binding; `capture.startSession(mode)`
asks the coordinator, which still serializes (an extension session and a
core dictation can never overlap; the loser is rejected exactly as two core
bindings are today). The singleton invariant survives — it just gains a
sanctioned way to be *requested*.

### R-2. No slow stage for an extension to own *(contract — blocking)*

`transform:transcript` carries a hard ~150 ms timeout (pass-through on
breach). A vision-LLM round trip is 2–5 **seconds**. The fast transform hook
is the wrong instrument, and raising its budget would wreck the paste path
for everyone.

**Fix — the session mode from R-1 owns a *slow* stage**, the same stage core
post-processing already occupies: pill shows "processing", cancel works,
result flows to the normal output path. Two clearly separated hooks:
`transform` (fast, synchronous-ish, every utterance) vs `sessionMode`
(slow, async, only for sessions that mode owns).

### R-3. No screen capture *(host feature — blocking, and the risky one)*

Nothing in Grain captures pixels. `context_detect` returns app *identity*
(exe/title/url_host); `monitor_logical` is window placement. There is no
capability, no host API, no OS permission plumbing (macOS Screen Recording,
Windows equivalent), no per-monitor geometry.

**Fix — `screen:capture` capability + `screen.capture({monitors, maxDim, quality})`**
returning images plus the geometry HeyClicky hand-rolls (monitor index,
dimensions, which one holds the cursor). Non-negotiable constraints:

- **only while a session that extension owns is active** (or on an explicit
  user action) — never continuous, never background, never on a timer;
- a **visible indicator** while capturing (pill chip) — the user always knows;
- egress named in plain words on the permission sheet: *"takes pictures of
  your screen and sends them to `api.anthropic.com`"*;
- OS permission requested lazily, with a clear pre-prompt.

This is the most dangerous capability on the platform and deserves the
loudest consent. It is also the one that most needs core to own it: an
extension author cannot write ScreenCaptureKit/DXGI code at tier B.

### R-4. The pointing overlay *(host feature — blocking as designed)*

`surface:overlay` is specified as a transient HUD near the pill/cursor with
size and lifetime budgets. HeyClicky needs a **full-screen, click-through,
multi-monitor** window that draws an arbitrary marker and animates it along
a Bézier arc.

**Fix — `surface:pointer`, host-rendered from declarative commands.** The
extension sends `pointer.point({x, y, screen, label})`; **Grain** owns the
window, the coordinate transforms (screenshot-space → display-space, the
y-flip, the multi-monitor offset), the animation, and the teardown. The
extension never touches a window handle.

This is strictly **better than HeyClicky's own architecture**: it keeps R1
(never expose internals), keeps lifecycle host-owned (destroy-if-not-in-use
enforced by construction), fixes the coordinate math once instead of per
extension, and means a pointing extension is ~20 lines instead of an
NSPanel/overlay subsystem. It also generalizes — "point at this" is useful
to any tutorial/assistant/accessibility extension.

Note the marker itself is a *theme* concern → `surface-variant` pack
(STRESS-TEST GAP-5), so a third party can restyle the triangle without code.

### R-5. No TTS / audio output for extensions *(host feature — degrades, not blocks)*

Grain has **no TTS at all**. It does have an audio-output primitive
(`play_feedback_sound`). HeyClicky speaks its answers.

**Fix — `audio:play(bytes)`**, not a TTS engine: the extension fetches audio
from whatever provider it likes via `net:<host>` and hands bytes to the host,
which owns the device, ducks/respects mute-while-recording, and stops on
cancel. Cheapest possible unblock, no vendor baked into core. (A core `tts`
capability can come later if several extensions want it.)

Without this the extension still works — it just types instead of speaking.

### R-6. Image parts in `llm.complete()` *(contract — the premise)*

Excluded by the user's premise, but named for completeness because the
*contract* shape is still work: `llm.complete()` must accept image parts,
advertise per-provider capability, and fail gracefully to text-only with a
surfaced reason. That plumbing is required even once a vision model is
wired up.

---

## Part 3 — Things that turned out *not* to be roadblocks

- **"Hey clicky" prefix routing** — the session mode owns its transcript;
  prefix matching is ordinary extension logic (and Grain's snippet/voice-action
  matchers already prove the pattern).
- **Suppressing output** — a transform returning empty text already
  suppresses the paste, so "consume this utterance" needs nothing new.
- **Typing text** — the session's own output path does it, through Grain's
  existing paste machinery. No `input:type` capability needed for this flow.
  (Only a *fire-and-forget, outside-a-session* typer would need one; deferred
  until something actually demands it — R1 says grant narrowly.)
- **Multi-monitor geometry** — folded into the `screen.capture` result rather
  than exposed as a standing API (less ambient fingerprinting surface).
- **Memory / history** — plain `storage`.
- **Agent mode (background tasks)** — an `onShortcut`-activated extension
  doing async work with `llm` + `storage`; nothing new.

---

## Part 4 — Honest caveats

1. **Four of six fixes are host features.** A JS extension author cannot add
   screen capture, a pointer overlay, audio output, or session ownership
   themselves — core must ship them once. So the truthful answer to *"can
   anyone who wants to build this, build it?"* is: **yes, once Grain ships
   these four primitives** — after which the HeyClicky-equivalent really is a
   few hundred lines of JS in a manifest. Before that, only a tier-C native
   extension could do it, which is "a native app with a Grain-managed
   lifecycle" rather than a platform citizen.
2. **Cross-platform.** HeyClicky is macOS-only. `screen:capture` needs a
   per-OS implementation, and manifests should carry `"platforms": [...]`
   so cards can say "macOS only" instead of failing mysteriously.
3. **Philosophical load.** Grain is local-first; an extension that ships
   screenshots to a cloud model is the opposite. The platform should *allow*
   it (the user's governance stance: allow, review, mark) with the loudest
   consent surface we have — and the marketplace review checklist should
   treat `screen:capture` + `net:` together as an automatic human-review
   trigger.
4. **Scope discipline.** None of these six should be built speculatively.
   They belong to Phase 3/4 (surfaces, native tier) and should land when a
   real extension — possibly this one — is being written against them.

---

## Part 5 — Amendments this adds to the contract

Appended to STRESS-TEST's ten:

11. **`session:start` + `contributes.sessionMode`** — sanctioned, serialized
    session ownership with a *slow* stage (R-1, R-2).
12. **`screen:capture`** — session-scoped, indicator-backed, egress-named,
    geometry included (R-3).
13. **`surface:pointer`** — host-rendered pointing from declarative
    commands; marker styling via surface-variant packs (R-4).
14. **`audio:play`** — host-owned playback for extension-fetched audio (R-5).
15. **Image parts in `llm.complete()`** with capability detection + graceful
    text-only fallback (R-6).
16. **`platforms` in the manifest** — per-OS availability, shown on the card.

**Bottom line:** the contract survived a hostile test case. Every gap it
exposed is additive, none required loosening an invariant, and the platform's
answer to the hardest piece (the pointer overlay) is architecturally
*superior* to the app being copied.
