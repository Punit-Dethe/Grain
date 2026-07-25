<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain logo" width="128" height="128" />
  <h1>Grain</h1>
  <p><strong>Speak to write. Add only the workflows you need.</strong></p>
  <p>
    <a href="https://github.com/Punit-Dethe/Grain/releases">Download</a> ·
    <a href="BUILD.md">Build from source</a> ·
    <a href="docs/grain-features.md">Feature guide</a> ·
    <a href="CONTRIBUTING.md">Contribute</a>
  </p>
</div>

---

Grain is a desktop app that lets you speak into the text field you are already using. Press a shortcut, talk, release it, and Grain puts the words at your cursor. From there, you can ask it to rewrite those words, act on them, or remember them for later.

The speech-to-text foundation comes from [Handy](https://github.com/cjpais/handy). Grain builds on it as an extension platform: the app handles recording, transcription, shortcuts, permissions, and cleanup; you choose the extra workflows that belong in your day.

## Features

### Three ways to dictate

**Batch** waits until you finish speaking, then transcribes the whole recording in one pass. It is for a short message when you want the most polished result. **Flow** starts turning your speech into text while you are still talking, working through small, overlapping pieces of audio so a long thought does not leave you waiting at the end. **ASR** is the live mode: it shows words on screen almost as soon as you say them.

*You are thinking through a long design note, so you use Flow and keep your momentum. You are filling out a form and need to see every word immediately, so you use ASR. You are sending one careful message, so you choose Batch. You switch with a shortcut instead of opening Settings and choosing a different model each time.*

Every mode has its own configurable shortcut. Grain can transcribe with a local Whisper or Parakeet model, or with an OpenAI-compatible speech-to-text provider.

### Change the kind of writing while you speak

A prompt is simply an instruction you save for your AI, such as “write this as a professional email” or “turn this into a GitHub issue.” Grain lets you switch that instruction while you are dictating, without ending the recording.

*You begin describing a bug in your own words. Halfway through, you realise it belongs in an issue tracker. Press your issue-prompt shortcut, keep explaining the bug, and get an issue-shaped result instead of stopping, changing a setting, and starting over.*

### Choose AI after you know you need it

You can dictate normally and decide only at the end whether Grain should paste the raw words or send them through your AI provider. A normal finish gives you the transcript; the AI finish gives you the rewritten version.

*You dictate rough meeting notes. Once you hear the whole thought, you decide it should be a follow-up email. Finish with the AI shortcut and Grain turns the same recording into a concise email with action items.*

### Say the instruction when it occurs to you

Prompt Record lets you separate the words you were dictating from an instruction about what to do with them. First speak the content; then enter Prompt Record and say what you want Grain to make from it.

*You dictate the facts for a proposal, then say, “Turn this into a confident three-paragraph proposal for the client.” Grain uses the first part as the material and the second part as the instruction—there is no copying text into a separate AI chat.*

### Change text where it already lives

Quick Agent takes selected text, an instruction you speak or type, and puts one answer back in the same place. Agent keeps the answer in a small panel when you need to ask follow-up questions. With no text selected, Agent is simply a voice-friendly chat.

*Highlight an awkward paragraph and say, “Make this clearer and less defensive.” Quick Agent replaces the paragraph. If the rewrite exposes a larger problem, expand Agent and continue the conversation with the same context instead of opening another chatbot and pasting the paragraph again.*

### Teach Grain the little things you repeat

Snippets turn a phrase you define into text you use often. App Modes tell Grain how to format dictation in one specific app or website. Voice Actions turn a phrase into an action, such as opening a link or an application you have approved.

*Say “project dashboard” and Grain opens the dashboard. Dictate in your IDE and Grain can format the result as code; dictate in your email client and it can use your email style. Outside those places, nothing changes and no unnecessary AI request is made.*

The Quick Panel puts the settings you adjust most—shortcuts, models, providers, prompts, and history—in one place. Grain also keeps raw and processed transcription history, plus dictionary tools for words you want recognised reliably.

## Why Grain is an extension platform

There are hundreds of useful applications and new workflows appear constantly. A speech-to-text app that tries to ship every one of those features becomes a crowded settings page and a collection of background services that many people never asked for.

An extension is a feature that you can install, enable, disable, or remove separately from the speech-to-text core. Grain stays responsible for the sensitive and difficult parts—microphone access, global shortcuts, windows, permissions, and stopping work when it is no longer needed. Extensions add the behaviour.

*If you have a workflow that only matters to your job—perhaps turning a dictated support update into the exact format your team uses—you can build it for yourself instead of waiting for a general-purpose app to add it. If other people need it, it can be prepared for the Grain catalogue rather than becoming a permanent feature everyone has to carry.*

### What you can build

An extension can work with a completed transcript, selected text you explicitly chose, the foreground app or website, your configured AI provider, its own settings, and Grain-provided panels or workspaces. It can open safe web links and applications that you selected through Grain’s picker. It can also be just a data pack—prompts, snippets, or a layout—with no code running at all.

Grain checks every request against the permissions you approved. An extension cannot create hidden windows, read every setting, or launch arbitrary programs. Its worker starts only for a declared action and is removed when idle; closing one of its surfaces destroys it rather than leaving it in the background.

### Build for yourself. Publish with trust.

Developer Mode lets you load an extension from a folder and see changes quickly while keeping the same permission checks used by installed extensions.

When an extension is submitted for publishing, the author points to one exact source commit. Grain builds the package from that commit, a reviewer reads that same source, and the app receives the published package from a signed catalogue—not from whatever a repository or website serves later. The public store is still being opened up; today, data packs and local scripted extensions are available through Developer Mode.

Read the [extension overview](docs/Extension%20Platform/README.md), follow the [authoring guide](docs/Extension%20Platform/AUTHORING.md), or inspect the [first-party registry](grain-extensions/README.md).

## An extension we think you will love: Grain Space

Grain Space is a built-in extension for the things you would otherwise forget, bookmark and never revisit, or send to yourself in a chat. It lets you capture a thought quickly, then ask for it later in the words you remember—not in the exact words you originally saved.

**Save something without opening a notes app.** Highlight text anywhere and use Quick Add, or summon Grain and speak or type a note. If you have an AI provider configured, Space can also give the note a title, a short summary, to-dos, or a reminder. Without one, it saves your original words exactly as they are.

*You read a useful answer in a browser, highlight it, and save it before moving on. A week later, you ask, “What was that thing I saved about the buffer size?” rather than hunting through bookmarks or trying to remember the page title.*

**Ask for the answer, then inspect the notes.** Recall searches your saved notes and replies with an answer first. It lists the notes it used underneath, so you can open them, check the evidence, or update one in the same conversation.

*You ask, “What did I decide about the buffer size?” Grain tells you the decision and shows the note where you made it. You do not have to search folders just to reconstruct your own memory.*

**Keep the files you own.** Space stores notes as ordinary Markdown files in Grain’s local folder or in a folder inside an Obsidian vault. It can search other notes in that vault without rewriting them. Turning Space off or uninstalling it never deletes your notes.

To find a note, Space looks for exact words, similar wording, recent notes, and related named things mentioned across notes. This helps it find both “buffer size” and a half-remembered question such as “that setting we changed after the timeout problem.” It does this only while you are using Space: there is no file watcher or memory daemon, and the optional local search model is loaded only when needed.

Read the [Grain Space product vision](docs/Grain%20Space%202.0/Grain%20space%20files/PRODUCT-VISION.md), the [Space extension guide](grain-extensions/core/grain.grain-space/README.md), or the [3.0 knowledge architecture](docs/Grain%20Space%202.0/KNOWLEDGE-ARCHITECTURE-PLAN.md).

## Local first, by default

- Local transcription and optional note search run on your machine.
- Cloud speech-to-text and AI processing are opt-in and use only providers you configure.
- Grain Space sends note text to a provider only for the action you requested, such as summarising a note or answering a Recall question.
- Disabled extensions unregister their shortcuts, close their surfaces, and release their runtime resources.

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
