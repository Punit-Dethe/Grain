<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain logo" width="128" height="128" />
  <h1>Grain</h1>
  <p><strong>Speak it. Shape it. Remember it.</strong></p>
  <p>A local-first, open-source voice utility for dictation, AI workflows, and personal memory.</p>
  <p>
    <a href="https://github.com/Punit-Dethe/Grain/releases">Download</a> ·
    <a href="BUILD.md">Build from source</a> ·
    <a href="docs/grain-features.md">Feature guide</a> ·
    <a href="CONTRIBUTING.md">Contribute</a>
  </p>
</div>

---

Grain gets a thought out of your head without making you change context. Hold a shortcut, speak, and receive text in the app you are already using. When you need more than dictation, Grain can shape the result with your AI provider, act on selected text, or keep a local memory you can ask about later.

It is built on the battle-tested [Handy](https://github.com/cjpais/handy) transcription foundation, with Grain-specific workflows designed around one rule: **nothing should stay alive when you are not using it.**

## The friction Grain removes

| Instead of… | With Grain | A small example |
| --- | --- | --- |
| Opening an app just to dictate | Use a global shortcut in any text field | Hold your shortcut, say “Send the revised estimate by Friday,” release. |
| Choosing an AI workflow before you start talking | Decide at the end whether to process the transcript | Dictate freely, then finish with the AI shortcut for “make this a concise email.” |
| Repeating boilerplate or navigating to a link | Say a phrase that expands text or triggers an action | Say “project dashboard” to open the dashboard without pasting its URL. |
| Rewriting the same thought for every app | Apply instructions only where they belong | In your IDE, format a spoken note as code; in email, make it professional. |
| Searching folders for a half-remembered fact | Ask your personal memory naturally | “What did I decide about the buffer size?” |

## Dictation that fits the moment

Grain gives each dictation style its own configurable shortcut, so changing pace does not mean opening Settings.

| Mode | Best for | What it feels like |
| --- | --- | --- |
| **Batch** | Short recordings and maximum punctuation accuracy | Speak, stop, receive the finished transcription. |
| **Flow** | Everyday dictation | A rolling window keeps long dictation responsive; live preview is optional. |
| **ASR** | Immediate feedback | Words appear as you speak through real-time streaming transcription. |

Use local Whisper or Parakeet models, or connect an OpenAI-compatible speech-to-text endpoint. Local models are loaded for work and released afterwards; supported hardware acceleration is used when available.

## Workflows, without the ceremony

- **AI post-processing** — Run dictated text through the LLM provider you choose, with prompts you can switch while speaking.
- **Prompt Record** — Turn a spoken instruction into the processing prompt mid-session: say the content first, then say how you want it changed.
- **Quick Agent** — Select text, give an instruction by voice or typing, and paste the answer back without opening a full chat window. Expand only when a conversation is useful.
- **Context-aware modes** — Apply an instruction only for a chosen app or website. A non-match means no extra AI call.
- **Snippets and voice actions** — Expand frequently used text, open user-approved apps, or launch safe web links from phrases you define.
- **History and dictionary tools** — Keep both raw and processed output available; maintain words that need special handling.
- **Quick Panel** — Reach the settings you use most—shortcuts, models, providers, prompts, and history—without digging through tabs.

## Grain Space: a memory companion, not another notes app

Grain Space is for the things you would otherwise send to yourself, bookmark forever, or hope to remember. Capture a selection, speak a note, or type one; later, ask for the answer rather than trying to recall the exact note title.

> **You:** “What was that Product Hunt app I saved?”
>
> **Grain:** An answer first, with the supporting memories underneath so you can inspect the source.

### What makes Space 3.0 different

- **Answer-first recall with provenance** — Grain answers conversationally, then shows the notes behind the answer. The original Markdown remains the source of truth.
- **Local Markdown, by default** — Use Grain’s own folder or an Obsidian vault. Your files remain ordinary Markdown, readable and editable outside Grain.
- **Hybrid retrieval** — Exact terms, optional on-device semantic search, recency, distilled searchable descriptions, and an entity graph are fused to find both literal and half-remembered ideas.
- **No memory daemon** — Space refreshes its view when you use it. Its optional embedding model loads on demand and is dropped when the surface closes.
- **Safe vault coexistence** — Grain-owned notes can live in a selected vault while other vault notes remain searchable without being overwritten.
- **Recall is not a black box** — Sources can be opened from an answer, so “I think it was…” never becomes unexplained AI fiction.

Space is a built-in extension: it follows the same install, enable, disable, and uninstall lifecycle as other extensions. Disabling or uninstalling it never deletes your notes.

Read the [Grain Space product vision](docs/Grain%20Space%202.0/Grain%20space%20files/PRODUCT-VISION.md), the [current Space extension guide](grain-extensions/core/grain.grain-space/README.md), or the [3.0 knowledge architecture](docs/Grain%20Space%202.0/KNOWLEDGE-ARCHITECTURE-PLAN.md).

## Private by design

- Local transcription and semantic retrieval run on your machine.
- Cloud speech-to-text and AI processing are opt-in; Grain uses only the providers you configure.
- Space sends text to an AI provider only for actions that need it, such as creating a summary or answering a Recall question. With no provider, capture and search still work.
- Extensions declare the capabilities they need. Grain checks access at the host boundary rather than trusting extension code.
- Disabled features unregister their shortcuts, destroy their surfaces, and stop consuming runtime resources.

## An extension platform that stays out of your way

Grain’s optional workflows are becoming extensions so they can evolve without turning the core app into a permanent background engine. Extensions can be simple data packs, short-lived scripted workflows, or narrowly scoped native companions.

The host owns windows, permissions, settings, and lifecycle. An extension asks for a capability; it does not get unrestricted access to Grain or your system. First-party examples include:

- **Grain Space** — local capture and answer-first memory retrieval.
- **App Modes** — app- and website-specific transcript formatting.
- **Voice Actions** — phrase-to-link and phrase-to-user-approved-app actions.
- **Agent Centre layout** — a zero-runtime layout option for Agent replies.

Browse the [extension registry](grain-extensions/README.md), learn the [authoring model](docs/Extension%20Platform/README.md), or see the [App Modes](grain-extensions/core/grain.app-modes/README.md) and [Voice Actions](grain-extensions/core/grain.voice-actions/README.md) examples.

## Quick start

1. Download the latest build from [Releases](https://github.com/Punit-Dethe/Grain/releases).
2. Grant the operating-system permissions Grain asks for, including microphone access and accessibility/input permissions where required.
3. Choose a transcription route: a local model, or an OpenAI-compatible provider.
4. Set a dictation shortcut and use it in any text field.
5. Enable only the optional workflows you want.

Grain targets Windows, macOS, and Linux. Linux text insertion may require `wtype` or `dotool` under Wayland.

## Build from source

The project is a Tauri application: React and TypeScript provide the on-demand application surfaces; Rust handles system integration, audio, transcription, and the native runtime.

```bash
bun install
bun run tauri dev
```

For system prerequisites, platform-specific build notes, and release builds, see [BUILD.md](BUILD.md).

## Project guides

| Looking for | Start here |
| --- | --- |
| Product capabilities and workflow details | [Feature guide](docs/grain-features.md) |
| Building and packaging | [Build guide](BUILD.md) |
| Contributing to Grain | [Contributing guide](CONTRIBUTING.md) |
| Translations | [Translation guide](CONTRIBUTING_TRANSLATIONS.md) |
| Building an extension | [Extension authoring guide](docs/Extension%20Platform/AUTHORING.md) |
| Extension contract | [Extension specification](docs/Extension%20Platform/SPEC.md) |
| Tracking compatibility with Handy | [Upstream tracking](Upstream/UPSTREAM.md) |

## License

Grain is released under the [MIT License](LICENSE). It is a fork of [Handy](https://github.com/cjpais/handy); Grain keeps the Handy-derived code isolated so upstream improvements can continue to flow in.
