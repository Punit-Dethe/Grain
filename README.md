<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain logo" width="128" height="128" />
  <h1>Grain</h1>
  <p><strong>Voice operating layer</strong></p>
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

Most local dictation tools force a trade-off: **Batch** transcription gives you the best final result, but only after you stop talking; **live ASR** is instant, but less accurate. Grain adds a third option.

**Flow** transcribes while you're still speaking so the amount of work left when you release the shortcut stays bounded. A long recording should therefore finish with roughly the same short finalization delay as a much shorter one.

Today, Flow supports all configured speech models through Grain's generic rolling backend. Supported **Parakeet TDT v2/v3** models already use a dedicated stateful Flow path that preserves their native punctuation and capitalization across chunks.

> You ramble through a 10-minute brain dump about a project instead of typing it out. With Batch you'd wait for the entire recording to be processed after you finish. With Flow, most of that work has already happened while you were speaking, so the transcript can land shortly after you release the shortcut.

|                                  | Batch                                                                  | Flow                                                                    | ASR                                                                          |
| -------------------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| **Accuracy**                     | Highest                                                                | Near-batch with optimized models                                        | Lower                                                                        |
| **Delay after you stop talking** | Seconds to minutes, scales with length                                 | Short and largely independent of total recording length                 | None — instant                                                               |
| **Punctuation & casing**         | Native                                                                 | Native with supported Parakeet TDT v2/v3 models                         | Model-dependent                                                              |
| **Live preview while speaking**  | No                                                                     | No                                                                      | Yes                                                                          |
| **Best for**                     | A short message you want perfect on the first try, like a client email | Everyday dictation — journaling, notes, drafts. The recommended default | Forms or live captions, where you need to watch every word land as you speak |

All three modes get their own configurable shortcut, so switching is a keypress, not a trip to Settings. Pick your favorite and ignore the rest, or assign all three and switch by task.

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

## Extensions: give the Agent tools

Grain extensions are deliberately narrow.

**An extension gives the Grain Agent additional external tools.** Grain remains responsible for understanding the user's request, gathering local context, choosing the right tools, asking for approval when required, and deciding what to do with the result. An extension does not become a second Agent and does not receive broad access to Grain.

That means an extension can expose useful capabilities such as:

- searching repositories, issues, pull requests, or commits on GitHub;
- reading or updating work in Linear, Notion, or another connected service;
- checking calendars and creating events;
- controlling a supported music service;
- storing or retrieving information from an external memory service;
- exposing any other well-scoped function that the Agent can call with explicit arguments.

The same extension can expose several related tools, and the Agent can combine tools from multiple extensions in one task.

### One tool contract, two adapter types

Grain can reach tools through two implementation paths:

| Adapter | What it is | Intended use |
| --- | --- | --- |
| **MCP** | A remote HTTPS Model Context Protocol server discovered and called through the official MCP client stack. | External services and integrations that already expose MCP. |
| **Native/direct** | A reviewed Grain-maintained provider implemented locally against the service or API. | First-party integrations where a direct implementation is simpler or gives a better desktop experience. |

Both adapters normalize into the same host-owned contract:

```text
Extension
├── identity and metadata
├── authentication / necessary configuration
├── tool catalogue
└── execute tool → result
```

"Native" does **not** mean more privileged. It only describes how the provider is implemented.

The initial reduced platform does not treat arbitrary third-party executables, unrestricted scripts, or local stdio processes as trusted extensions. Wider local-process support would require a separate containment and installation design.

### The Agent owns context

Extensions do not independently inspect the user's computer.

Grain's Agent can understand the task using first-party context such as the selected text, textbox contents, foreground application, supported website context, OCR, or an image when that feature is enabled. The Agent then chooses what minimum information a tool actually needs and passes that information as ordinary tool arguments.

```text
Speech / typed instruction
          │
          ▼
      Grain Agent
   ┌──────┼────────┐
   │      │        │
selection OCR   app/site context
   └──────┼────────┘
          │
     search / load
     relevant tools
          │
          ▼
   policy + approval
          │
          ▼
      Extension
          │
          ▼
        result
          │
          └────────────→ Agent continues
```

An extension cannot query Grain for extra context behind the Agent's back, and extensions do not call one another directly. Cross-service workflows are orchestrated by the Agent.

### What extensions can do

A tool-only extension can:

- expose named tools with typed input schemas and descriptions;
- receive the explicit arguments selected by the Agent;
- authenticate to its external service through Grain's host-owned connection flow;
- use narrowly scoped networking or private adapter storage when its implementation requires it;
- return bounded text or structured JSON results;
- be enabled, disabled, reconnected, cancelled, and reloaded without becoming a permanent background service.

Credentials remain host-owned. OAuth tokens are kept out of model context, tool arguments, ordinary results, and logs.

### What extensions cannot do

The reduced extension contract intentionally does **not** expose Grain's internal feature surface. Extensions cannot independently:

- read the screen, OCR, selected text, textbox contents, foreground application, transcript history, or Context Awareness stream;
- start or control dictation/recording sessions;
- add Context Awareness prompts, prompt layers, prompt packs, transcript transforms, or replacement rules;
- register Grain shortcuts or recording modes;
- open arbitrary overlays, workspaces, UI slots, or contributed settings pages;
- call Grain's configured LLM or embedding model as a privileged host service;
- access another extension's state, Grain's private application state, or unrestricted internal APIs;
- launch arbitrary executables or gain unrestricted OS access.

Features such as **Snippets, Context Awareness, Agent, dictation, overlays, and transcription history are Grain features**, not extension capabilities.

This separation is intentional: **Grain owns understanding and orchestration; extensions execute tools.**

### Execution and safety

Discovering a tool does not authorize it to run.

Before dispatch, Grain binds the call to the exact extension instance, tool definition, account/configuration state, schema, and arguments that were selected. Stale approvals are refused if that identity changes.

The host also owns lifecycle and outcome semantics:

- disabling, disconnecting, replacing, or reloading an extension invalidates stale work;
- authentication state and credentials stay outside the model;
- potentially completed writes are not automatically replayed when their outcome is uncertain;
- tool descriptions and results are treated as untrusted data, not instructions that can override Grain's policy;
- MCP connections and native workers are created when needed and released according to Grain's lifecycle rules.

During the current hardening phase, extension calls are handled conservatively. Less intrusive automatic-read behavior can only be added through a host-reviewed policy; an extension cannot declare itself safe and bypass approval.

### Tool discovery instead of loading everything

Grain does not need to place every schema from every connected service into the model's context.

The Agent starts from a bounded directory of enabled extensions, searches for relevant capabilities, and loads only the tool definitions needed for the current task. A GitHub request should not force unrelated Calendar, music, or other schemas into the prompt.

This keeps model context smaller and lets Grain support larger tool catalogues without turning every connected service into permanent prompt overhead.

### Examples

**GitHub**

> "Find the open issue about the extension lifecycle and tell me whether the latest related commit addresses it."

The Agent can search/load the GitHub tools it needs, inspect the issue and commit, and continue reasoning over the returned results. If the user asks to comment on or close the issue, the write goes through Grain's execution policy before dispatch.

**Calendar**

> "Do I have anything after 4 PM tomorrow? If not, create a 30-minute review block."

The Agent can first use a calendar read tool, reason over the result, and then prepare the appropriate write tool only if the requested condition is satisfied.

**External memory**

> "Save this decision to my connected memory service."

A memory provider can expose ordinary store/search/retrieve tools. Grain does not need a built-in second-brain or Grain Space subsystem for the Agent to work with an external memory service.

### Current development status

The old broad extension platform is being retired in favor of this tool-only contract. Legacy declarations and privileged host APIs are being blocked, tested, and physically removed in stages while the new native/direct and MCP adapters are hardened against real lifecycle, authentication, permission, cancellation, and Agent-continuation cases.

The current implementation plan and progress live in:

- [Tool-only native and MCP extension plan](docs/Extensions%202.0/MCP-EXTENSION-PLAN.md)
- [Extension progress and evidence](docs/Extensions%202.0/MCP-EXTENSION-PROGRESS.md)

The reduced public authoring contract is still being finalized. Older Extension Platform documentation describes the previous architecture and should not be treated as the current capability model.

## Local first, by default

- Local transcription runs entirely on your machine.
- Cloud speech-to-text and AI processing are opt-in and only ever use providers you configure.
- Disabled features and extensions unregister their shortcuts, close their windows, and release their memory — nothing idles in the background.

## Quick start

First-run onboarding introduces Batch, Flow, and ASR, then guides you through model setup, a real transcription test, and shortcut setup.

1. Download the latest build from [Releases](https://github.com/Punit-Dethe/Grain/releases).
2. Grant microphone and accessibility/input permissions where your OS requires them.
3. Pick a local model, or an OpenAI-compatible speech-to-text provider.
4. Set a dictation shortcut (Batch, Flow, or ASR) and start speaking in any text field.
5. Enable only the extensions and workflows you want — everything else stays off.

Grain targets Windows, macOS, and Linux. Linux text insertion may require `wtype` or `dotool` under Wayland.
On Debian or Ubuntu, install the downloaded `.deb` with `sudo apt install ./Grain_*.deb` so APT resolves its dependencies.

### Known input and audio behavior

- On macOS, recording through a Bluetooth headset microphone can temporarily reduce playback quality or volume because Bluetooth switches into bidirectional audio mode. Keep the headset as the output and select the Mac's built-in microphone or an external microphone in Grain to avoid it.
- On macOS, shortcuts containing the `fn`/Globe key work only on Apple keyboards. Third-party keyboards usually handle `Fn` entirely in firmware and send no key event to macOS; use `control`, `option`, `shift`, `command`, or a regular key for a shortcut that must work across keyboards.

### Earlier clipboard text is pasted instead of your transcript

If History shows the correct transcript but another app receives text you copied earlier, that app may be reading the clipboard after Grain restores its previous contents. Open Grain's main window and press `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux) to show Debug settings.

- On macOS and Windows, try **Reliable Paste** with a clipboard paste method. It waits for the receiving app to read the text before restoring your clipboard.
- Otherwise, increase **Paste Delay (After)** and retry in the affected app. **Paste Delay (Before)** waits before sending the paste shortcut and addresses a different problem.

If it persists, report your OS, receiving app, paste method, Reliable Paste setting, and both delay values. Keep private dictated text out of logs you share.

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
| Extension architecture  | [Tool-only extension plan](docs/Extensions%202.0/MCP-EXTENSION-PLAN.md) |
| Extension progress       | [Progress and evidence](docs/Extensions%202.0/MCP-EXTENSION-PROGRESS.md) |
| Handy compatibility     | [Upstream tracking](Upstream/UPSTREAM.md)                           |

## License

Grain is released under the [MIT License](LICENSE).
