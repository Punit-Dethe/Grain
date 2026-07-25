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

## Dictation without the setup ritual

Most dictation tools make you decide everything before you speak: real-time or batch, which model, which prompt, whether AI should touch the result. Changing your mind means opening Settings, stopping the recording, reconfiguring, then starting again.

Grain is designed for the moment your mind changes.

### Three modes, always one shortcut away

**What it is:** Batch, Flow, and real-time ASR each have their own configurable shortcut. Use local Whisper or Parakeet models, or an OpenAI-compatible speech-to-text provider.

**Why it exists:** “Fast” and “perfectly polished” are different needs. You should not have to choose one workflow forever because switching means a settings detour.

**In real life:** Use Flow for a long stream of thoughts, ASR when you need immediate words on screen, and Batch for a short message where final punctuation matters—all without interrupting the work in front of you.

### Switch the prompt while you are still talking

**What it is:** Change the active processing prompt mid-dictation with a shortcut.

**Why it exists:** A thought can start as a rough note and become an email, a bug report, or code documentation halfway through. Stopping just to change a prompt breaks the thought you were trying to capture.

**In real life:** Start explaining a bug in your own words. When you realise this needs to be a GitHub issue, switch to your issue-template prompt and keep speaking.

### Decide on AI at the end

**What it is:** Start with any dictation mode, then choose AI post-processing only when you finish.

**Why it exists:** You do not always know whether a transcript needs rewriting until you see what you said. Grain removes the up-front commitment.

**In real life:** Dictate a meeting note normally. At the end, finish with the AI shortcut and turn it into “a concise follow-up email with action items”—without recording it again.

### Say the instruction when the instruction occurs to you

**What it is:** Prompt Record lets you speak a custom instruction during a session. Grain uses it to process the text you have already dictated.

**Why it exists:** Sometimes the right instruction is not a saved prompt; it arrives after you have worked through the thought. Copying text into another AI chat just to give that instruction is needless ceremony.

**In real life:** After dictating an outline, say: “turn this into a confident three-paragraph proposal for the client.” Grain processes the existing dictation with that instruction.

### An agent where the work already is

**What it is:** Select text, give Grain an instruction by voice or typing, and use Quick Agent to replace it immediately. Expand into Agent when you need a real back-and-forth conversation; use it without a selection for a natural, standalone chat.

**Why it exists:** Small text changes should not require opening a separate chatbot, pasting context, copying the reply back, and finding your place again.

**In real life:** Highlight an awkward paragraph and say “make this clearer and less defensive.” Quick Agent puts the revision in place. If the first revision raises a bigger question, expand it and keep the conversation going.

### Small workflows that stay small

Snippets save repeated text; Context-Aware Modes apply instructions only in the app or site where they matter; Voice Actions can open a user-approved app or safe web link from a phrase. A non-matching app mode makes no AI call. The Quick Panel keeps shortcuts, routes, prompts, providers, and history close when you actually need to adjust them.

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
