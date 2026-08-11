<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain logo" width="128" height="128" />
  <h1>Grain</h1>
  <p><strong>Local, low-RAM speech-to-text with near-batch accuracy at sub-second delay.</strong></p>
  <p>
    <a href="https://github.com/Punit-Dethe/Grain/releases">Download</a> ·
    <a href="BUILD.md">Build from source</a> ·
    <a href="docs/grain-features.md">Full feature guide</a> ·
    <a href="CONTRIBUTING.md">Contribute</a>
  </p>
</div>

---

Grain is a desktop app that turns speech into text wherever your cursor is. Press a shortcut, talk, release it — your words land in the field you're already using. From there you can rewrite them, act on them, or save them for later, all without leaving the app you were in.

It's built on [Handy](https://github.com/cjpais/handy), the most battle-tested open-source STT engine available. Grain keeps that foundation and adds the modes, AI workflows, and extension platform described below — all opt-in, all off unless you turn them on.

## Upstream tracking

Track Handy upstream changes and Grain's integration status in the [live tracker](https://punit-dethe.github.io/Grain/).

## Flow: the headline feature

Every local dictation tool forces a trade-off: **batch** transcription (accurate, but you wait until you stop talking) or **live ASR** (instant, but noticeably less accurate). Grain adds a third option.

**Flow** transcribes in small overlapping chunks *while you're still speaking*, so a session finishes within about a second of you stopping — whether you spoke for 10 seconds or 10 minutes.

> You ramble through a 10-minute brain dump about a project instead of typing it out. With Batch you'd wait minutes for the transcript. With Flow, it's sitting in the text field about a second after you stop talking — the same wait as if you'd only spoken for 10 seconds.

| | Batch | Flow | ASR |
|---|---|---|---|
| **Accuracy** | Highest | High | Lower |
| **Delay after you stop talking** | Seconds to minutes, scales with length | Sub-second, regardless of length | None — instant |
| **Live preview while speaking** | No | Optional | Yes |
| **Best for** | A short message you want perfect on the first try, like a client email | Everyday dictation — journaling, notes, drafts. The recommended default | Forms or live captions, where you need to watch every word land as you speak |

All three modes get their own configurable shortcut, so switching is a keypress, not a trip to Settings. Pick your favorite and ignore the rest, or assign all three and switch by task. Grain transcribes with a local Whisper or Parakeet model, or with any OpenAI-compatible speech-to-text provider.

## Feature overview

- **Turn This Into… (Prompt Record).** Keeps what you say and what you want done with it as two separate spoken parts. Finish dictating your content as normal, then trigger Prompt Record and speak an instruction — Grain treats the first part as material and the second as the prompt, and runs the whole thing through AI automatically. *Say the facts of a bug out loud, then say "turn this into a GitHub issue" — Grain uses the first part as content and the second as the instruction.*
- **Decide on AI after you're done talking.** You don't have to decide up front whether a recording should go through AI. Start dictating with any shortcut, and only once you're finished, choose the AI shortcut to have Grain rewrite the transcript, or a normal stop to keep your raw words. *You ramble through messy meeting notes, then decide at the last second to finish with the AI shortcut and get a clean summary instead.*
- **Mid-speech prompt switching.** Press one shortcut and a small switcher opens on the pill. Use the arrow keys to step to the prompt you want, then stop — that's it, no menu to close, no need to restart the recording. *You're dictating a casual Slack reply and realize it should read like a support ticket — press the shortcut, arrow over to "support ticket," and keep talking.*
- **Agent.** Select some text (or none at all), speak or type an instruction, and trigger the shortcut. The result appears first in a small window in the corner of the screen, where you can accept it with Enter, dismiss it, retry it, or expand it into a full chat panel to keep the conversation going. With nothing selected, Agent works as a plain voice-friendly chatbox. *Ask "summarize this whole document" with nothing selected, check the summary in the corner window, then expand to chat and ask "now shorten it to two sentences."*
- **Quick Agent.** The instant version of Agent: select some text, speak or type an instruction, and press the shortcut. The result replaces your selection immediately — no popup window, no extra step to accept it. *Highlight a clunky paragraph, say "make this sound more confident," and the rewrite drops in right where the paragraph was.*
- **Snippets.** Assign a spoken keyword to a piece of text you use often — a URL, an address, a block of boilerplate. Say the keyword, and Grain pastes the saved text in its place. *Say "my address" and your street address appears wherever your cursor is.*
- **Voice Actions.** Assign a spoken keyword to an action instead of text — opening an app, a file, or a website you've approved. One keyword can trigger several actions at once, so a single phrase can kick off a whole routine. *Say "start my day" and it opens your email, calendar, and Slack together.*
- **Context awareness.** Grain notices which app or website you're dictating into and adjusts tone and formatting to match, without you picking a prompt yourself — and can read everything already in the text field so the AI understands what you've written so far. *Dictate in Gmail and it sounds like an email; dictate in your IDE and it sounds like a code comment — no manual switching.*
- **"Scrap That" voice cancel.** A spoken undo. Say "scrap that" at any point mid-dictation and everything you said before that moment — audio and transcribed text alike — is discarded, while the recording keeps running so you can pick the thought back up. *You trail off mid-sentence, say "scrap that," and keep going without touching a key.*
- **Full history.** Grain keeps a record of both what you actually said (the raw transcript) and what the AI turned it into (the processed result) for every session, plus a dictionary of words you've taught it to transcribe correctly from now on. *You paste the AI-cleaned version, then realize you need your exact original wording — it's still there.*
- **Quick Panel.** One window that gathers the settings you're likely to touch day-to-day — shortcuts, models, providers, prompts, and history — instead of hunting across separate tabs. *Need to swap models before a call? One window, not four settings tabs.*

*See [docs/grain-features.md](docs/grain-features.md) for the full breakdown, including model routing and smart key rotation for cloud providers.*

## The extension platform: go far beyond dictation

Dictation is the part of Grain everyone installs it for. Extensions are the part that decide how far it goes from there — and they're a first-class part of Grain, not an afterthought bolted on after the fact.

Most speech-to-text apps have two options as feature requests pile up: say no forever, or say yes and turn into a crowded settings page full of background services most people never asked for and never turn off. Grain instead stays a small, trusted core — microphone, shortcuts, windows, permissions, cleanup — and pushes every specialized workflow out into **extensions** that install, enable, disable, and uninstall independently of the speech engine and of each other.

Because an extension can work with a finished transcript, text you've explicitly selected, the app or site you're in, your configured AI provider, and Grain's own panels and windows, it isn't limited to tweaking dictation. It can replicate — and combine — what a dedicated notes app, a snippet manager, a clipboard tool, and a custom AI assistant each do on their own, as one voice-first workflow. Grain's own built-in features (Snippets, Context Awareness, Agent, Grain Space) are all built on this same contract, not a private shortcut only we get to use.

### Three ways to build, pick the smallest one that works

| Tier | What it is | Cost when idle | Good for |
|---|---|---|---|
| **Data pack** | Just files — prompts, snippets, a theme, a set of voice actions. No code at all | Zero — there's nothing to run | Sharing a prompt library, a snippet set, a pill theme |
| **Scripted** | JavaScript in its own sandboxed worker, started on activation | Zero — the worker is destroyed when idle, not paused in the background | Logic and UI: task trackers, custom panels, workflow automation |
| **Native companion** | Your own small program, started and stopped by Grain | Zero — not spawned means not running | Anything only a real OS program can do: screenshots, hardware, local integrations |

Extensions never create their own windows. They *declare* what they need — a settings section, an app-like window, an overlay — and Grain builds it, places it, puts it to sleep, and destroys it. The "destroy if not in use" rule the core app follows applies automatically to everyone else's code too, because it isn't optional.

### How we keep this safe

An extension platform is only as trustworthy as its security model, so trust is enforced in Rust, not in JavaScript:

- **Every extension runs in its own isolated realm**, with its own authenticated connection. Extensions can't see, patch, or impersonate one another — there's no shared JavaScript global to exploit. (This is the exact class of hole that once broke Figma's plugin sandbox; Grain's boundary doesn't depend on JS being well-behaved.)
- **Permissions are checked in Rust, on every message, at the connection itself.** An extension without a grant for something — your transcripts, say — doesn't receive a filtered or empty version of it. It never receives the message at all.
- **Settings are physically isolated per extension.** One extension can never write to Grain's own settings or another extension's. "An extension broke my settings" isn't a bug we have to prevent — it isn't possible.
- **Sensitive capabilities are named and approved individually** — reading the clipboard, taking a screenshot, opening a URL, sending data to a network host. Combining a sensitive read (like your transcripts) with network access is automatically flagged for closer human review before it can ever reach the store.

### Build it for yourself first

Developer Mode loads an extension straight from a folder, with reload times under a second, using the exact same permission checks an installed extension gets. What you test locally behaves exactly like what you'd ship — Developer Mode changes where the code comes from, never what it's allowed to do. A small CLI (`grain-ext`) scaffolds a project, runs it against a live Grain install, and checks it with the same rules the store's review uses.

You never have to publish anything to get value from this. If a workflow only matters to your job — turning a dictated support update into your team's exact format, say — build it, keep it on your machine, and never think about it again.

### Publish it for others, with trust that can't be faked

If you want to share an extension, publishing is deliberately different from a typical extension store:

1. You point at one exact commit in your own public source repository — never a built binary.
2. Grain's CI builds the package from that pinned commit and hashes it.
3. A human reads that exact source before it's ever listed — every extension, every update, no auto-publish, no exceptions.
4. The app installs the package from a signed catalogue that only Grain can produce, never from your repo or website directly — so nothing can change after it's been reviewed. An author who controls their own domain, repository, and build still cannot make their extension show up as trusted; that has to come from us reading it.
5. If something goes wrong later, a signed revocation list can disable an extension on every installed copy, with the reason shown to the user — a real kill switch, not just a quiet delisting.

The public store is still being opened up. Today, data packs and local scripted extensions are available through Developer Mode; publishing follows the process above once the store opens more broadly.

Read the [extension platform overview](docs/Extension%20Platform/README.md), the [authoring guide](docs/Extension%20Platform/AUTHORING.md), or the [full specification](docs/Extension%20Platform/SPEC.md).

## An extension you might love: Grain Space

Grain Space is a built-in **extension** — built on the exact same platform described above, with no private access Grain didn't also give a third-party author — for the things you'd otherwise forget, bookmark, or message to yourself. It isn't a notes app you manage. It's memory you talk to.

- **Capture in seconds.** Highlight text and hit Quick Add, or speak a note and let Grain structure it with a title, summary, and any reminders or to-dos it finds. *You read a good answer in a browser, highlight it, and save it before moving on — no notes app to open first.*
- **It runs on your Obsidian vault, as your own files — not inside a separate Grain ecosystem.** Point Space at an Obsidian vault and every note it captures is written straight into that vault as a plain `.md` file with YAML frontmatter — the same format Obsidian itself uses. There's no Grain account, no login, and no plugin to install on the Obsidian side; Grain reads and writes the files directly, and Obsidian doesn't even need to be open. Capturing and retrieving by voice — Quick Add, Recall, the overlay — work in full whether or not you ever open Obsidian; none of that depends on it.
- **Two ways to actually read and edit a note, your choice.** If you already use Obsidian, keep using it — that's where you'd naturally open and edit a captured note. If you don't want to install a separate app just to look at your notes, use the **Grain Note UI** instead: a lightweight built-in viewer and editor that's purely for browsing and editing notes as text. Either way you're opening the same file on disk, not two separate copies — picking one doesn't lock you out of the other.
- **Ask, don't search.** Recall answers in plain language first, then lists the notes it used underneath. *Ask "what was that app from Product Hunt?" and get "You're probably thinking of Superlist — you saved it after a launch about lightweight project management," with the source note one click away.*
- **A knowledge graph that doesn't need an extra AI model.** Retrieval borrows from two proven designs: the distil-then-embed and multi-signal ranking approach behind Cerebras's internal knowledge search, and [LightRAG](https://arxiv.org/abs/2410.05779)'s entity graph with dual-level (specific-entity and broad-theme) retrieval — combined on Grain's terms. The graph itself isn't a model: it's plain SQLite tables walked with ordinary SQL, so it costs zero idle RAM on its own. For the AI parts — extracting entities, answering Recall — Space simply reuses whichever model you've already picked for AI post-processing, edge/on-device or cloud; there's no second, dedicated model to pick, download, or pay for. New notes merge into the graph instead of triggering a full rebuild, which is also why Grain deliberately avoided the Microsoft GraphRAG-style approach, where a single query can cost hundreds of thousands of tokens and hundreds of API calls. With no post-processing model configured at all, Recall still works over lexical and vector search.
- **Nothing runs when you're not using it.** No file watcher, no background daemon — search and AI models load only for the moment you need them.

Because Space is just an extension, you can disable it entirely and it disappears — no residual process, no orphaned settings — or use it as a working example of what the platform can do before building your own.

Details: [product vision](docs/Grain%20Space%202.0/Grain%20space%20files/PRODUCT-VISION.md) · [knowledge architecture](docs/Grain%20Space%202.0/KNOWLEDGE-ARCHITECTURE-PLAN.md) · [Obsidian vault backend](docs/Grain%20Space%202.0/OBSIDIAN-PLAN.md)

## Local first, by default

- Local transcription and note search run entirely on your machine.
- Cloud speech-to-text and AI processing are opt-in and only ever use providers you configure.
- Grain Space sends note text to a provider only for the action you requested (e.g. summarizing a note, answering a Recall question).
- Disabled features and extensions unregister their shortcuts, close their windows, and release their memory — nothing idles in the background.

## Quick start

1. Download the latest build from [Releases](https://github.com/Punit-Dethe/Grain/releases).
2. Grant microphone and accessibility/input permissions where your OS requires them.
3. Pick a local model, or an OpenAI-compatible speech-to-text provider.
4. Set a dictation shortcut (Batch, Flow, or ASR) and start speaking in any text field.
5. Enable only the extensions and workflows you want — everything else stays off.

Grain targets Windows, macOS, and Linux. Linux text insertion may require `wtype` or `dotool` under Wayland.

## Build from source

Grain is a Tauri app: React/TypeScript provide on-demand surfaces; Rust handles audio, transcription, extensions, and system integration.

```bash
bun install
bun run tauri dev
```

See [BUILD.md](BUILD.md) for system prerequisites, platform-specific notes, and release builds.

## Project guides

| Looking for | Start here |
| --- | --- |
| Full feature details | [Feature guide](docs/grain-features.md) |
| Building and packaging | [Build guide](BUILD.md) |
| Contributing to Grain | [Contributing guide](CONTRIBUTING.md) |
| Translations | [Translation guide](CONTRIBUTING_TRANSLATIONS.md) |
| Extension contract | [Extension specification](docs/Extension%20Platform/SPEC.md) |
| Publishing an extension | [Distribution plan](docs/Extension%20Platform/DISTRIBUTION-PLAN.md) |
| Handy compatibility | [Upstream tracking](Upstream/UPSTREAM.md) |

## License

Grain is released under the [MIT License](LICENSE).
