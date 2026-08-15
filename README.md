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

**Flow** transcribes in small overlapping chunks _while you're still speaking_, so a session finishes within about a second of you stopping — whether you spoke for 10 seconds or 10 minutes.

> You ramble through a 10-minute brain dump about a project instead of typing it out. With Batch you'd wait minutes for the transcript. With Flow, it's sitting in the text field about a second after you stop talking — the same wait as if you'd only spoken for 10 seconds.

|                                  | Batch                                                                  | Flow                                                                    | ASR                                                                          |
| -------------------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| **Accuracy**                     | Highest                                                                | High                                                                    | Lower                                                                        |
| **Delay after you stop talking** | Seconds to minutes, scales with length                                 | Sub-second, regardless of length                                        | None — instant                                                               |
| **Live preview while speaking**  | No                                                                     | Optional                                                                | Yes                                                                          |
| **Best for**                     | A short message you want perfect on the first try, like a client email | Everyday dictation — journaling, notes, drafts. The recommended default | Forms or live captions, where you need to watch every word land as you speak |

All three modes get their own configurable shortcut, so switching is a keypress, not a trip to Settings. Pick your favorite and ignore the rest, or assign all three and switch by task. Grain transcribes with a local Whisper or Parakeet model, or with any OpenAI-compatible speech-to-text provider.

## Feature overview

- **Turn This Into… (Prompt Record).** Keeps what you say and what you want done with it as two separate spoken parts. Finish dictating your content as normal, then trigger Prompt Record and speak an instruction — Grain treats the first part as material and the second as the prompt, and runs the whole thing through AI automatically. _Say the facts of a bug out loud, then say "turn this into a GitHub issue" — Grain uses the first part as content and the second as the instruction._
- **Decide on AI after you're done talking.** You don't have to decide up front whether a recording should go through AI. Start dictating with any shortcut, and only once you're finished, choose the AI shortcut to have Grain rewrite the transcript, or a normal stop to keep your raw words. _You ramble through messy meeting notes, then decide at the last second to finish with the AI shortcut and get a clean summary instead._
- **Mid-speech prompt switching.** Press one shortcut and a small switcher opens on the pill. Use the arrow keys to step to the prompt you want, then stop — that's it, no menu to close, no need to restart the recording. _You're dictating a casual Slack reply and realize it should read like a support ticket — press the shortcut, arrow over to "support ticket," and keep talking._
- **Agent.** Select some text (or none at all), speak or type an instruction, and trigger the shortcut. The result appears first in a small window in the corner of the screen, where you can accept it with Enter, dismiss it, retry it, or expand it into a full chat panel to keep the conversation going. With nothing selected, Agent works as a plain voice-friendly chatbox. _Ask "summarize this whole document" with nothing selected, check the summary in the corner window, then expand to chat and ask "now shorten it to two sentences."_
- **Quick Agent.** The instant version of Agent: select some text, speak or type an instruction, and press the shortcut. The result replaces your selection immediately — no popup window, no extra step to accept it. _Highlight a clunky paragraph, say "make this sound more confident," and the rewrite drops in right where the paragraph was._
- **Snippets.** Assign a spoken keyword to a piece of text you use often — a URL, an address, a block of boilerplate. Say the keyword, and Grain pastes the saved text in its place. _Say "my address" and your street address appears wherever your cursor is._
- **Voice Actions.** Assign a spoken keyword to an action instead of text — opening an app, a file, or a website you've approved. One keyword can trigger several actions at once, so a single phrase can kick off a whole routine. _Say "start my day" and it opens your email, calendar, and Slack together._
- **Context awareness.** Grain notices which app or supported website you're dictating into and adjusts tone and formatting to match, even if you switch destinations mid-dictation. It can also use the text already in the field to understand what you've written so far. The built-in Work, Email, Technical, Casual, and AI Chat profiles are editable, while custom profiles can target one or more apps or sites and override the defaults. _Dictate in Gmail and it sounds like an email; switch to your IDE and it follows you with the right profile — no manual prompt switching._
- **A pill that follows your context.** The default pill pairs a smooth, responsive waveform with the active app's icon or a supported site's favicon, and updates as your context changes. Prefer the previous pill? It remains available in Settings.
- **Lost Text recovery.** If a transcription finishes without a usable text field, Grain copies the result to your clipboard and notifies you, so you can paste it when you're ready.
- **"Scrap That" voice cancel.** A spoken undo. Say "scrap that" at any point mid-dictation and everything you said before that moment — audio and transcribed text alike — is discarded, while the recording keeps running so you can pick the thought back up. _You trail off mid-sentence, say "scrap that," and keep going without touching a key._
- **Full history.** Grain keeps a record of both what you actually said (the raw transcript) and what the AI turned it into (the processed result) for every session, plus a dictionary of words you've taught it to transcribe correctly from now on. _You paste the AI-cleaned version, then realize you need your exact original wording — it's still there._
- **Quick Panel and model status.** A searchable command palette gathers the settings you're likely to touch day-to-day — shortcuts, models, providers, prompts, and history — while the main sidebar reports the active model, its load state, and whether it runs locally or in the cloud. _Need to swap models before a call? Search once instead of hunting across settings tabs._

_See [docs/grain-features.md](docs/grain-features.md) for the full breakdown, including model routing and smart key rotation for cloud providers._

## The extension platform: build far beyond dictation

Grain is designed to stay small at its core while letting extensions build much larger workflows around speech, context, AI, memory, and external services.

Extensions can react to what you say, understand where you are working, inspect information you deliberately give them access to, use Grain's configured AI and local embedding systems, maintain their own persistent data, communicate with approved network services, and present their own Grain-managed interfaces.

Grain's own features such as Snippets, Context Awareness, Agent, and Grain Space are built on this extension contract rather than relying on a separate private plugin system.

### What extensions can access

Capabilities are granted individually. An extension only receives the parts of Grain that it explicitly requests and the user approves.

A scripted extension can currently request access to:

* **Speech and sessions** — listen for session/transcript events, transform completed transcripts, start Grain recording sessions, or contribute a custom recording mode.
* **Selected text** — read text the user has explicitly highlighted.
* **Application context** — identify the foreground application, executable, and browser host when available.
* **Visible screen text** — read text exposed by the active window's accessibility tree. Password fields are skipped.
* **Foreground-window images** — capture the active window as an image when the user explicitly grants screenshot access.
* **AI** — use the AI provider already configured in Grain, including vision when supported by that model.
* **Local embeddings** — use Grain's on-device embedding model for semantic matching, retrieval, classification, or an extension's own memory system.
* **Persistent storage** — private key/value storage and a document store for larger collections such as records, notes, histories, or indexes.
* **Settings** — expose configuration through Grain, including secret values.
* **Network services** — make host-proxied HTTP requests only to network hosts declared by the extension.
* **Grain UI** — open an extension workspace, show temporary overlays, contribute settings, and use supported Grain UI slots.
* **Launching** — open safe web links or applications the user has explicitly selected and approved.
* **Grain Space notes** — extensions that specifically request the high-privilege `notes` capability can read or modify Space notes.

This means an extension can be anything from a tiny voice utility to a persistent application with its own interface, semantic memory, AI processing, and external API integration.

### Three levels of extensions

Use the smallest tier that can do the job.

| Tier                   | What it does                                                | Best for                                               |
| ---------------------- | ----------------------------------------------------------- | ------------------------------------------------------ |
| **Data pack**          | Ships data such as prompts or themes. No executable code.   | Prompt packs, themes and simple customisation          |
| **Scripted extension** | Runs JavaScript inside its own isolated worker when needed. | Most integrations, tools, workflows and extension UIs  |
| **Native companion**   | Runs a separate native program controlled by Grain.         | Functionality that genuinely requires native OS access |

Scripted workers are created when required and destroyed when idle. Native companions are likewise started and stopped by Grain rather than becoming permanent background services.

### Security is enforced by Grain, not trusted to the extension

Giving extensions access to speech, screen content and external services makes the security boundary especially important.

Grain therefore treats permissions as an enforcement mechanism rather than a convention.

**Every scripted extension gets its own isolated worker and authenticated connection.** It cannot access Grain through normal Tauri IPC, share JavaScript globals with another extension, or impersonate another extension.

**Permissions are checked by the Rust host.** If an extension does not have `capture:selection`, `llm`, `embed`, `capture:screen-image`, or another capability, the host rejects the operation. The extension cannot bypass the check from JavaScript.

**Network access is host-proxied.** Extension workers do not receive unrestricted `fetch`. A manifest must request individual hosts such as `net:api.example.com`; wildcards, arbitrary URLs and undeclared hosts are rejected.

**Extension data is isolated.** Storage and settings belong to the extension that created them. An extension cannot simply modify Grain's settings or another extension's private storage.

**Sensitive capture is separated into individual permissions.** Reading selected text, detecting the active application, reading visible screen text, and taking a screenshot are different grants rather than one broad "screen access" permission.

**Launching is restricted.** URLs are limited to safe supported schemes, and applications can only be launched after the user selects them through Grain's own application picker. Extensions cannot provide arbitrary executable paths and ask Grain to run them.

**Powerful combinations are visible during review.** An extension requesting sensitive information together with network access can be treated differently from a simple local extension.

For published extensions, the distribution system adds another boundary: releases point to a specific source commit, Grain builds the package from that source, the exact code is reviewed before publication, installations come from Grain's signed catalogue, and extensions can be revoked if a serious problem is discovered later.

Developer Mode does not remove these runtime permission checks. Local development changes where the extension comes from, not what it is allowed to access.

### What could you build?

The platform is intentionally general. A few examples:

#### Spotify-style voice controller

A small scripted extension could expose a set of music actions such as:

> "Next song."
> "Play Daft Punk."
> "Play my Focus playlist."
> "Pause."

The extension could use Grain's local embeddings to semantically match speech against a small set of supported commands, call the permitted music-service API, remember user-defined aliases in private storage, and briefly show the result through an overlay.

For simple commands, no LLM needs to run at all.

#### GitHub workflow assistant

A more capable extension could combine several Grain primitives.

Select a stack trace or leave the error visible on screen and say:

> "Turn this into a GitHub issue."

With the appropriate permissions, the extension could read the selected or visible text, use the foreground application as additional context, ask the user's configured AI model to structure the information into a title and issue description, send it to the GitHub API, and show the created issue in a Grain overlay.

A workspace could then provide a richer interface for the extension's saved repositories, recent actions, or settings.

That entire workflow can live inside one scripted extension.

#### Screen-aware research collector

An extension could capture information while the user browses without becoming a general-purpose notes application.

Highlight a product, paper, quote, movie, library, or other item and trigger the extension. It could combine the selection, visible window text, or an explicitly permitted screenshot with AI to extract structured information.

The extension can store those records in its own document store and embed them locally.

Later:

> "What was that speech recognition paper I saved about rolling windows?"

The extension can semantically search its own collection using Grain's embedding system and display the result in an overlay or workspace.

No separate vector database, embedding service, AI daemon, or background process is required.

---

The goal of Grain extensions is not to provide a predefined collection of plugins.

It is to provide reusable primitives — **speech, context, AI, embeddings, storage, networking and UI** — while Grain handles the expensive platform concerns such as lifecycle, permissions, isolation and resource cleanup.

What those primitives become is up to extension authors.

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

- **Capture in seconds.** Highlight text and hit Quick Add, or speak a note and let Grain structure it with a title, summary, and any reminders or to-dos it finds. _You read a good answer in a browser, highlight it, and save it before moving on — no notes app to open first._
- **It runs on your Obsidian vault, as your own files — not inside a separate Grain ecosystem.** Point Space at an Obsidian vault and every note it captures is written straight into that vault as a plain `.md` file with YAML frontmatter — the same format Obsidian itself uses. There's no Grain account, no login, and no plugin to install on the Obsidian side; Grain reads and writes the files directly, and Obsidian doesn't even need to be open. Capturing and retrieving by voice — Quick Add, Recall, the overlay — work in full whether or not you ever open Obsidian; none of that depends on it.
- **Two ways to actually read and edit a note, your choice.** If you already use Obsidian, keep using it — that's where you'd naturally open and edit a captured note. If you don't want to install a separate app just to look at your notes, use the **Grain Note UI** instead: a lightweight built-in viewer and editor that's purely for browsing and editing notes as text. Either way you're opening the same file on disk, not two separate copies — picking one doesn't lock you out of the other.
- **Ask, don't search.** Recall answers in plain language first, then lists the notes it used underneath. _Ask "what was that app from Product Hunt?" and get "You're probably thinking of Superlist — you saved it after a launch about lightweight project management," with the source note one click away._
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

First-run onboarding introduces Batch, Flow, and ASR, then guides you through model setup, a real transcription test, and shortcut setup.

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

| Looking for             | Start here                                                          |
| ----------------------- | ------------------------------------------------------------------- |
| Full feature details    | [Feature guide](docs/grain-features.md)                             |
| Building and packaging  | [Build guide](BUILD.md)                                             |
| Contributing to Grain   | [Contributing guide](CONTRIBUTING.md)                               |
| Translations            | [Translation guide](CONTRIBUTING_TRANSLATIONS.md)                   |
| Extension contract      | [Extension specification](docs/Extension%20Platform/SPEC.md)        |
| Publishing an extension | [Distribution plan](docs/Extension%20Platform/DISTRIBUTION-PLAN.md) |
| Handy compatibility     | [Upstream tracking](Upstream/UPSTREAM.md)                           |

## License

Grain is released under the [MIT License](LICENSE).
