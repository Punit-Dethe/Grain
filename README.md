<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain logo" width="128" height="128" />
  <h1>Grain</h1>
  <p><strong>An extensible, local-first platform that turns speech into useful action.</strong></p>
  <p>
    <a href="https://github.com/Punit-Dethe/Grain/releases">Download</a> ·
    <a href="BUILD.md">Build from source</a> ·
    <a href="docs/grain-features.md">Feature guide</a> ·
    <a href="CONTRIBUTING.md">Contribute</a>
  </p>
</div>

---

Grain begins with a simple job: press a shortcut, speak, and put the result in the text field you are already using. But speech-to-text is only the beginning. Grain is built to host the personal workflows that come after the words: shaping them with AI, acting on a selection, remembering something for later, or building a workflow that only makes sense to you.

Its transcription foundation comes from [Handy](https://github.com/cjpais/handy). Grain builds on that foundation with a strict rule: **a capability should exist only while you are using it.**

## Features

Most dictation tools make you decide everything before you speak: real-time or batch, which model, which prompt, whether AI should touch the result. Grain is designed for the moment your mind changes.

### Three modes, always one shortcut away

Batch records first and transcribes after; Flow uses a rolling window for responsive long-form dictation; real-time ASR shows words as you speak. Each has its own configurable shortcut and can use local Whisper or Parakeet models, or an OpenAI-compatible speech-to-text provider.

*You are thinking through a long design note, so you use Flow. You need to watch every word while filling a form, so you use ASR. You are sending one careful message, so you choose Batch for its final punctuation—without opening Settings in between.*

### Switch the prompt while you are still talking

Change the active processing prompt mid-dictation with a shortcut.

*You start explaining a bug in your own words. Halfway through, you realise this should be a GitHub issue. Switch to your issue-template prompt and keep talking; there is no stop, settings change, or second recording.*

### Decide on AI at the end

Start in any dictation mode, then choose AI post-processing only when you finish.

*You dictate raw meeting notes. At the end, finish with the AI shortcut and turn them into a concise follow-up email with action items—without recording the same thought again.*

### Prompt Record

Speak a custom instruction during a session and Grain applies it to the text you have already dictated.

*After dictating an outline, you say: “Turn this into a confident three-paragraph proposal for the client.” There is no copying text into another AI chat just to give that instruction.*

### Agent and Quick Agent

Select text, give Grain an instruction by voice or typing, and use Quick Agent to replace it immediately. Expand into Agent for a proper back-and-forth conversation, or use Agent without a selection as a natural standalone chat.

*Highlight an awkward paragraph and say, “Make this clearer and less defensive.” Quick Agent places the revision where the original was. If that reveals a bigger question, expand it and continue the conversation with the same context.*

### Snippets, App Modes, and Voice Actions

Snippets expand repeated text. App Modes apply instructions only in the app or site where they matter. Voice Actions can open a user-approved application or safe web link from a phrase.

*Say “project dashboard” to open the dashboard, or dictate in your IDE and have only that app's mode format the result as code. In every other app, Grain leaves the transcript alone and makes no extra AI call.*

The Quick Panel keeps shortcuts, routes, prompts, providers, and history close when you need to adjust them.

## Why Grain is an extension platform

There are already hundreds of useful applications, and new niche workflows appear every day. Trying to win by permanently adding every feature to one speech-to-text app creates the opposite of a useful tool: more background services, more settings, more memory, and a product that is too heavy for the people who never wanted those features.

Grain keeps a small, capable core—audio, transcription, shortcuts, text insertion, permissions, and lifecycle—and lets workflows live as extensions. Install the features you use. Disable the ones you do not. Build the workflow no other app will prioritise because it is only useful to you.

That makes Grain more than speech-to-text: it is a place where speech, selected text, AI, local files, and focused UI surfaces can meet without turning every user’s install into a bundle of permanent background engines.

### What an extension can do

Extension authors work with clear, user-approved capabilities rather than an unrestricted API. In plain English, an extension can ask Grain to:

- transform a completed transcript or add an AI-processing step;
- react to text you explicitly selected, or to a shortcut you deliberately pressed;
- use the foreground app or website to choose the right behaviour;
- store its own settings and add a focused settings panel or workspace surface;
- open a safe web link, or an application you selected through Grain’s native picker;
- package prompts, snippets, layouts, or other data with no runtime at all.

Grain owns windows, permissions, and cleanup. Workers are started only for declared, granted activations and are reaped when idle; closing a surface destroys it instead of quietly leaving it around.

### Build something personal. Publish it responsibly.

Need a niche workflow for your own job, game, writing process, or research habit? Build it in Grain instead of waiting for a general-purpose app to guess what you need. Developer Mode loads an extension from a folder and reloads changes quickly while keeping the same Rust-enforced permission boundary as installed extensions.

The publishing model is deliberately strict: a submission points to a pinned source commit; Grain builds the package itself, so the bytes reviewed are the bytes published. A human reviews that exact source, and the app reads a signed catalogue rather than trusting a random repository or website. The public store is still being opened up; today, data packs and local scripted extensions are available through Developer Mode.

Read the [extension overview](docs/Extension%20Platform/README.md), follow the [authoring guide](docs/Extension%20Platform/AUTHORING.md), or inspect the [first-party registry](grain-extensions/README.md).

## An extension we think you will love: Grain Space

Grain Space is a built-in extension for the things you would otherwise forget, bookmark and never revisit, or message to yourself. It is not another notes app asking you to organise your life. It is a local memory companion: capture a thought in the moment, then ask for it naturally later.

**Capture without leaving your flow.** Highlight text anywhere and Quick Add saves it. Or summon Grain, speak or type a note, and let your configured AI provider add a title, summary, to-dos, or reminder. Without a provider, the original note is still saved exactly as you gave it.

**Ask instead of browsing.** Press Recall and say, “What did I decide about the buffer size?” Grain answers from your notes and shows the supporting sources underneath. You can open them, inspect the evidence, or update a note from the same conversation.

**Keep ownership of the memory.** Notes are plain Markdown in Grain’s local store or in a folder inside your Obsidian vault. Grain can search the rest of a vault without overwriting it. Uninstalling Space turns it off; it never deletes your notes.

Space 3.0 combines exact matching, optional on-device semantic search, recency, distilled searchable descriptions, and an entity graph to retrieve both literal facts and the half-remembered things people actually ask for. It has no watcher or background daemon: its index refreshes when you use it, and its optional embedding model is loaded on demand and dropped with the Space surface.

Read the [Grain Space product vision](docs/Grain%20Space%202.0/Grain%20space%20files/PRODUCT-VISION.md), the [Space extension guide](grain-extensions/core/grain.grain-space/README.md), or the [3.0 knowledge architecture](docs/Grain%20Space%202.0/KNOWLEDGE-ARCHITECTURE-PLAN.md).

## Local first, by default

- Local transcription and optional semantic retrieval run on your machine.
- Cloud speech-to-text and AI processing are opt-in and use only providers you configure.
- Grain Space sends note text to a provider only for the action you asked for, such as summarising a capture or answering a Recall question.
- Disabled extensions unregister their shortcuts, destroy their surfaces, and release their runtime resources.

## Quick start

1. Download the latest build from [Releases](https://github.com/Punit-Dethe/Grain/releases).
2. Grant microphone and accessibility/input permissions where your operating system requires them.
3. Choose a local model or an OpenAI-compatible speech-to-text provider.
4. Set a dictation shortcut and start speaking in any text field.
5. Enable only the extensions and workflows you want.

Grain targets Windows, macOS, and Linux. Linux text insertion may require `wtype` or `dotool` under Wayland.

## Build from source

Grain is a Tauri application: React and TypeScript provide on-demand surfaces; Rust handles system integration, audio, transcription, extensions, and native runtime work.

```bash
bun install
bun run tauri dev
```

For system prerequisites, platform-specific build notes, and release builds, see [BUILD.md](BUILD.md).

## Project guides

| Looking for | Start here |
| --- | --- |
| Full feature details | [Feature guide](docs/grain-features.md) |
| Building and packaging | [Build guide](BUILD.md) |
| Contributing to Grain | [Contributing guide](CONTRIBUTING.md) |
| Translations | [Translation guide](CONTRIBUTING_TRANSLATIONS.md) |
| Extension contract | [Extension specification](docs/Extension%20Platform/SPEC.md) |
| Handy compatibility | [Upstream tracking](Upstream/UPSTREAM.md) |

## License

Grain is released under the [MIT License](LICENSE).
