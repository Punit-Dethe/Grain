# Grain — Feature Documentation

## What is Grain?

Grain is an open-source, native speech-to-text application and a fork of [Handy](https://github.com/handy). By building on Handy as a foundation, Grain inherits a battle-tested transcription base while staying current with all upstream updates.

The frontend is built as a **native Rust UI**, a direct consequence of the decoupled architecture described below.

### Market Position

Handy is the most battle-tested, model-agnostic open-source STT application available — the top of the open-source STT world. Grain is a direct fork of it, inheriting that foundation entirely.

On top of that base, Grain is the only product that simultaneously offers:

- **The strongest open-source STT foundation** (via Handy)
- **The lowest RAM footprint** (~70 MB all-in vs ~210 MB for Handy, and far below commercial alternatives)
- **A deep, production-grade feature set** across transcription modes, AI post-processing, context awareness, and capture
- **Completely free and open-source**
- **Fully opt-in** — every feature can be disabled and consumes zero resources when off
- **Invisible by design** — sub-second response, native UI, no friction

No other open-source STT tool combines all of these at once.

---

## Upstream Tracking

Grain maintains a dedicated tracking document (MD file) inside the GitHub repository to monitor upstream changes from Handy. This document tracks:

1. **Implemented** — upstream changes we have already adopted
2. **Pending** — upstream changes still left to integrate
3. **Intentionally Ignored** — upstream changes we have deliberately skipped, along with the reasoning behind each decision

---

## Core Philosophy

Before development began, a foundational philosophy was established: **Grain is a utility tool, not a persistent application.** It does not need to be running at all times.

This philosophy drives four key principles:

1. **Low RAM Usage** — Keep the memory footprint as small as possible.
2. **Destroy if Not in Use** — Any component that is not currently active should be destroyed to free resources.
3. **Performance Balance** — Maintain harmony between speed and low RAM usage. Although there is typically a trade-off between loading times and memory efficiency, Grain uses specific optimization techniques and human-delay mitigation to remain fast despite its low overhead.

---

## Architecture

### Decoupled Frontend / Backend

The primary motivation for forking Handy was to implement a **decoupled architecture**: the frontend can be completely shut down while the backend remains active in the background.

- With all features enabled, the combined footprint (backend + native pill UI) hovers around **~70 MB RAM**. For context, Handy sits at ~210–220 MB combining its backend and WebView frontend. The native Rust UI costs marginally more than Handy's backend alone, but trades away 150 MB of always-on WebView overhead in exchange — a net saving of ~140–150 MB versus Handy.
- Every feature, when toggled off, consumes **zero RAM** — fully destroyed, nothing running in the background.
- Further reduction is possible, but Grain deliberately keeps Handy's core logic **isolated from its own code**. This isolation ensures smooth upstream merges and prevents conflicts with the battle-tested transcription base.

### Native Rust UI

Because the frontend and backend are decoupled, a native solution was required for the UI layer. Grain's frontend is built as a **native Rust UI tool**, keeping the entire stack in Rust and avoiding the overhead of a webview-based framework.

### WebView Pre-allocation Removed

Unlike Handy, Grain removes WebView memory pre-allocation entirely. This adds a small launch delay (up to ~1–2 seconds on very slow machines, sub-second on most) in exchange for a meaningfully lower idle memory footprint — a deliberate trade-off aligned with the utility-tool philosophy.

---

## Features

### Optional Features and Extensions

Grain is moving its optional workflow features onto a public extension
contract. **Snippets, Snippet Actions, Context Awareness, Agent, and Grain
Space** will all be extensions: Grain owns the secure lifecycle, permissions,
settings, shortcuts, and surfaces; a feature owns only its declared behavior.
This keeps specialised workflows out of the core runtime and makes room for
first- and third-party alternatives without forks.

The basic built-in **Snippets**, **Context Awareness**, and **Agent**
extensions will ship with Grain, but are **disabled by default on new
installs**. Disabled extensions register no listeners, windows, or background
runtime. Existing users retain their current enabled features during the
migration. Snippet Actions and Grain Space follow the same extension model as
their public contracts mature.

### 1. Rolling Window Transcription

Production-grade, model-agnostic rolling window architecture. A 10-minute dictation that takes 3–4 minutes in batch processes in **under a second**, with no loss in model accuracy. Includes **live transcription** — text appears on screen as you speak (3–5× compute cost, opt-in).

**Trade-off:** Occasional punctuation/capitalization errors are unavoidable. Users who cannot tolerate this should use batch mode.

**With AI post-processing:** Grain injects a tiny invisible prompt (~3–4 lines) into the LLM layer that corrects rolling window artifacts — delivering speed, accuracy, and clean punctuation simultaneously. Recommended default for most users.

**Batch mode** remains available for short recordings where perfect punctuation is required without post-processing.

**Primary use cases:** Daily speech-to-text, AI prompting, note-taking, any dictation workflow where speed matters. Essentially the recommended mode for all everyday usage.

---

### 2. OpenAI-Compatible Endpoints

Both the **STT endpoint** and the **LLM post-processing endpoint** are now fully OpenAI-compatible. Previously only the LLM endpoint was compatible; the STT endpoint now is too.

This means users can plug in any OpenAI-compatible speech-to-text source — cloud providers like Deepgram, other web-based STT models, or their own self-hosted endpoint — instead of running a local model. When using cloud transcription, system resource usage drops to **well under ~70 MB**, making the full Grain pipeline available at near-zero local overhead.

---

### 3. Smart Routing

Available on both the STT and LLM endpoints. Users can configure multiple API keys — from the same provider or different providers — and Grain routes between them intelligently based on:

- Per-key daily usage limits (manually configurable)
- Fallback logic when a key or provider is unavailable
- Other routing signals (extensible)

Multiple APIs can be active simultaneously or a single one can be used — fully up to the user.

---

### 4. Three Dictation Modes

Grain offers **three distinct dictation modes** versus Handy's two (batch and real-time ASR):

| Mode | Speed | Accuracy | Live Preview |
|------|-------|----------|--------------|
| **Batch** | Slow | Highest | No |
| **Flow** (Rolling Window) | Sub-second | High | Yes (opt-in) |
| **ASR** (Real-time Streaming) | Instant | Lower | Yes |

In Handy, switching between ASR and batch requires going into settings and changing the model. In Grain, each mode has its own **user-configurable shortcut** — switching is instant, no settings required.

**Model selection:** Users select two models simultaneously — one ASR model and one standard model (shared by batch and Flow). Only one is loaded in memory at a time; switching shortcuts swaps the loaded model immediately.

**Flexible for all users:**
- Power users assign all three shortcuts and switch freely by task
- Casual users pick one favorite mode, one shortcut, and use Grain exactly like Handy

---

## Smaller Enhancements

### Mid-Speech Prompt Switching

While speaking, you can switch the active prompt — from a general prompt to a coding, email, or any workflow-specific prompt — instantly via shortcut or pill interaction (pill interaction coming soon). No need to stop, change settings, and restart.

### Flexible AI Post-Processing Trigger

In Handy, sending output to AI requires committing upfront: start with the AI shortcut, end with the AI shortcut. In Grain, you can start dictating with any shortcut (Standard, Flow, or Live) and still choose to end with the AI shortcut — the transcript will be sent through AI post-processing regardless of how the session was started.

This means the decision to use AI post-processing no longer needs to be made before you start speaking. It can be made mid-speech or even at the very end.

> Note: Works in non-push-to-talk mode, which is the default in Grain.

### Prompt Record

Mid-recording, clicking the pill (or using a shortcut — additional interaction methods being evaluated with testers) switches into prompt recording mode. Instead of dictating content, you're now dictating an instruction — e.g. "convert this into an email." Stop with any key (AI shortcut or regular stop) and Grain forces the session through the LLM using your spoken instruction as the prompt. The transcript gets processed and pasted accordingly.

Removes the need to decide on instructions before speaking. You can speak freely, then record your intent at any point mid-session.

### "Scrap That" Voice Cancel

Say "scrap that" mid-dictation and everything before it — audio and text — is discarded. Starts fresh without needing to stop and restart the session.

### Full History (Transcription + Processed Text)

Grain records history for both raw transcription output and AI-processed output. Handy only records raw transcription history.

---

### 5. Snippets

Assign keywords to frequently used text. When the keyword is spoken, Grain instantly pastes the configured content — URLs, boilerplate text, addresses, or anything you repeat often. Works with links and plain text.

**Snippet Actions:** Keywords can also trigger actions — opening applications, files, or websites. Multiple actions can be tied to a single keyword, making it easy to set up a full workflow trigger (e.g. say "start workflow" → opens your apps, websites, and files all at once).


Snippets and Snippet Actions are being separated into extensions: the basic
text-expansion experience ships as the optional built-in Snippets extension,
while action-oriented workflows can evolve independently without adding cost
to ordinary dictation.

---

### 6. Context Aware

Uses the LLM post-processor to adapt transcription output based on what the user is currently doing. Multiple tiers:

**Basic — App & website-aware tone adjustment:** Detects the active application and, when in a browser, identifies the current website (browser-agnostic). Adjusts tone and formatting accordingly — e.g. professional in an email client or on an email website, code-friendly in an IDE. Also corrects likely transcription errors based on context — coding terms, library names, and language syntax get transcribed correctly when a relevant environment is detected.

**Unique Words:** Scans the current text field for unique words — names, company terms, custom function names, etc. — and sends them as a reference to the LLM, preventing those words from being mistranscribed.

**Full Context:** Sends the entire contents of the active text field to the LLM for maximum context. The AI has full awareness of what has already been written when processing new dictation.

**App/URL-specific instructions:** Custom instruction sets can be configured per application or per URL. For example, dictating on gmail.com can automatically trigger an "email format" instruction without any manual switching.

**Image-based context (planned):** Captures the screen and sends it to the LLM for even richer context. Still under consideration — privacy and user need will determine inclusion. Since Grain is open-source and fully user-controlled in LLM selection, the risk is low, but it has not been committed to yet.

Context Awareness is becoming an optional built-in extension. Its basic
app-aware behavior will ship with Grain but remain off by default on new
installs, so it never observes application context unless the user enables it.

---

### 7. Agent

The Agent is becoming an optional built-in extension. It ships with Grain but
is disabled by default on new installs; when disabled, its panel, shortcuts,
and supporting runtime do not stay resident.

Select any text, trigger the agent shortcut, and give an instruction — by voice or by typing. The agent processes the selection and returns a result in a compact window in the bottom-right corner.

**Interaction options in the compact window:**
- Press Enter to paste the result into the text field
- Escape to dismiss
- Retry to re-run with the LLM
- Expand via shortcut into a sidebar chat interface for back-and-forth conversation (appears as a side panel — does not take over the screen)

**Input:** Supports both voice instructions and typed input.

**UI Architecture:** The agent input (pill expansion and instruction field) is native Rust UI — instant, zero-delay. The moment the user begins interacting with the input, a Tauri WebView window is silently loaded in the background. On confirm, the result surfaces immediately in the WebView — the load time is hidden behind the natural human action of typing or recording the instruction. On escape or dismiss, the WebView is fully destroyed. This follows the same lifecycle as the rest of Grain: nothing persists beyond its moment of use.

**Auto-copy:** Configurable — auto-copy all AI replies, first reply only, or none.

**Context aware:** Agent supports both Unique Words and Full Context tiers. This makes it especially powerful mid-document — e.g. select nothing, press the shortcut, and say "summarize this essay" and the agent has full awareness of everything written in the text field. Or ask "what's a good ending based on the last paragraph?" and get a response that understands the full document.

**No text selected:** Agent works as a standalone chatboard — ask anything, no selection required.

---

### Quick Agent

A frictionless variant of the agent. The result is pasted directly at the cursor (replacing the selected text if any) — no window, no paste step.

A small pill appears at the bottom center showing the follow-up shortcut. Clicking it or pressing the shortcut expands to the full agent chat interface, so the full conversation can continue. Quick agent removes friction without removing features.

---

### 8. Auto-Dictionary *(Implemented — pending production decision)*

If a word is corrected by the user multiple times across multiple sessions, Grain detects the pattern and shows a small pop-up: "Press Enter to add to dictionary." Once added, that word will never be mistranscribed again.

**Status:** Fully implemented but not yet in production. Under debate with testers because it requires a short-lived listener process after pasting (to detect corrections) — a minor violation of the "destroy if not in use" philosophy. The tradeoff between usefulness and the process overhead is still being evaluated.

---

### 9. Quick Panel

A single unified window that surfaces ~90% of the settings a user might need day-to-day, eliminating the need to navigate across multiple tabs. Configurable as the default window on open.

The Quick Panel is organized into three modules visible simultaneously:

- **Module A — Configuration:** All three dictation mode shortcuts at a glance, microphone selection, audio settings (play sound, process audio), system behaviour (launch on boot, minimize to tray), and signal output
- **Module B — Transcription:** The Aura Core Monitor (dot matrix pill display), model route toggle (local/cloud), active model selection, model unload/idle timeout setting, full transcription history, and Smart Rotate configuration for the STT endpoint (available when cloud route is selected)
- **Module C — Processing:** Directive prompt selection, dictionary word management, processor LLM selection, Smart Rotate toggle, and processed text history

The panel covers switching between local and cloud models, toggling smart routing, managing the dictionary, selecting prompts, adjusting shortcuts, and most other configuration a user might reach for — all in one place.


---

### 10. Grain Space

Grain Space is Grain's personal memory layer for capturing, organizing, and retrieving information with minimal friction.

#### A Frictionless Second Brain

Grain Space is designed to feel like a natural extension of memory rather than another app to manage. It appears when needed, stays out of the way when not in use, and keeps capture and retrieval fast.

#### Capture Thoughts at the Speed of Life

Grain Space supports both text and voice input, and it can capture information in several ways:

- **The Quick Grab:** Highlight text anywhere, trigger a hotkey, and Grain saves it into Space in the background.
- **The Smart Voice Note:** Speak naturally and Grain can structure the note with a short title, a TLDR, and extracted reminders, timers, or checklist items.
- **The Magic Appender:** When a note is already open, use the microphone action or typed input to append new thoughts directly into that note.
- **Context-aware saving:** Select text on your screen and save it by voice or text, with context attached. For example, you can say "save this for later reference" or "save this for the essay I'll be writing later," and Grain stores both the selection and the reason it matters so retrieval is easier later.

Notes can be edited as Markdown, organized into folders, pinned, and given
reminders. The workspace includes an upcoming-reminders view and calendar;
optional AI-assisted categorization can suggest where a capture belongs and
help describe a folder's purpose.

#### Automated Personal Assistant

Grain Space can extract reminders and timers from captured notes. A spoken instruction like “Remind me to call David at 3 PM” can be turned into an actionable reminder inside the system.

#### Search That Understands Meaning

Grain Space offers fast exact search and optional semantic search, so users do
not need exact keyword recall.

- Search by meaning, not literal phrasing
- Recent notes are prioritized naturally
- Pinned notes can stay important over time

#### Talk to Your Notes

Voice-first retrieval allows users to ask questions like “What was the Wi-Fi password Lawrence gave me?” and get an answer surfaced from prior notes.

Recall also supports typed questions. Its conversation cites source notes,
opens them directly when needed, and can start fresh without carrying an old
thread forward.

#### Obsidian Vaults

Grain Space can use either its native local store or a user-selected Obsidian
vault. With the vault backend, captures are ordinary Markdown files with YAML
frontmatter inside a configurable `Grain/` folder. Grain-owned notes remain
editable in Grain; notes elsewhere in the vault are indexed for Recall and
opened read-only, with an option to open the source in Obsidian.

Grain does not run a separate sync service or require an Obsidian plugin. It
inherits whatever sync the vault already uses, such as Obsidian Sync, iCloud,
or Syncthing. It reconciles the vault only when a Grain Space surface needs
fresh data, preserves user frontmatter, and uses conflict-safe writes so it
does not keep a watcher or indexing daemon alive while idle.

#### A Stunning, Distraction-Free Design

The interface is designed around Grain's floating pill language.

- Sleek floating UI
- Minimal distraction
- Fast transitions
- Compact note viewing and retrieval experience

Grain Space itself is moving onto the extension contract. Its current
workspace, capture, storage, semantic retrieval, shortcuts, and settings are
the acceptance test for making the public platform capable enough for a
third-party memory extension.

#### In Short

Grain Space gives users a silent, instantly accessible memory bank that captures itself, organizes itself, and can answer questions from personal history.

***

## Inherited from Handy

Grain is a fork of Handy, so the following battle-tested features come standard:

- **Batch transcription** — standard record-then-transcribe mode
- **Real-time transcription via ASR models** — see live words as you speak
- **Dictionary** — manually add words that keep getting mistranscribed; they will always be transcribed correctly going forward
- **Instant model load/unload** — models are loaded only when in use and fully unloaded when not, directly enabling the "destroy if not in use" philosophy
- **GPU offloading** — hardware-accelerated transcription where available
- **Multi-language support** — all languages supported by Handy are supported in Grain

Grain's base is battle-tested and production-grade. Every Grain-specific feature listed above is built on top of this foundation — with a lower RAM footprint than Handy itself.
