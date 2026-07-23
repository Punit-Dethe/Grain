# Grain extension capability reference

Capabilities are requested in `manifest.json` under `permissions`. Grain shows
them to the user at first enable, records only approved names, and checks the
connection-bound grant in Rust. A local extension receives no extra authority.

Request the smallest set that makes the extension work. Adding a capability to
an update requires a new approval; removing one immediately narrows the grant.

## Current capability names

| Capability | Enables | Important limits |
| --- | --- | --- |
| `events:sessions` | Non-transcript daemon events through `grain.onEvent`, including recording lifecycle and host-state signals | Declare the exact `onEvent:<Variant>` activation. High-frequency audio levels are not included. |
| `events:transcripts` | Transcript-bearing events such as `ChunkComplete`, `TranscriptionComplete`, `ProcessingComplete`, and `Asr*` text events | Transcript text is sensitive. The activation and live stream are both grant-gated. |
| `transform:transcript` | `grain.onTransform(handler)` in the paste pipeline | Worker warms at recording start. Hard 150 ms deadline; cold/error/timeout passes text through; three consecutive failures disable the extension. Empty output suppresses paste. |
| `session:start` | `grain.session.start({ mode })` and a declared `contributes.sessionMode` | Exactly one recording exists at a time. The owner gets a bounded 30-second slow stage through `grain.onSessionStage`; failure or reload returns the untouched transcript. |
| `net:<host>` | `grain.net.fetch(url, options)` to one exact approved host | No wildcard, scheme, path, or port in the grant. Every redirect is re-checked; requests time out after 15 seconds; request bodies are capped at 1 MiB and responses at 2 MiB. |
| `storage` | `grain.storage.*` and `grain.doc.*` | One namespace per extension. KV and documents share a 200 MB quota. Missing keys return `null`; corrupt/unreadable data throws a typed error. |
| `settings` | `grain.settings.get/set` for settings declared by this extension | A `secret` field is write-only: reads return `[REDACTED]`, and `net.fetch` can apply it to a named header without exposing the value. |
| `llm` | `grain.llm.complete(prompt)` | Uses Grain's configured service and may be unavailable or time out. Never place it in `onTransform`. |
| `embed` | `grain.embed(texts)` | On-device embeddings; at most 64 strings per call. Model initialization can be slow, so call from user/background work, not the transform hot path. |
| `surface:workspace` | Declare a workspace and call `grain.workspace.open/close` | Grain owns the window and sleeps/unmounts its iframe. The manifest must include non-empty `ui_source`. |
| `surface:overlay` | Declare an overlay and call `grain.overlay.show/dismiss` | Host-clamped HUD: at most 720×480 and 15 seconds. It also dismisses when focus is lost. |
| `pill:slots` | Reserved capability for pill chips/theme-token interactions | No public callable API yet. A tier-A pill theme uses the `pill.theme` slot contract instead. |
| `capture:selection` | `grain.captureSelection()` | Reads the user's current selection and returns `string | null`. Use from a user-initiated action such as a shortcut. |

`events:audio-levels` is an internal capability and is not requestable by third-
party manifests in API 1.0. Consequently, `onEvent:AudioLevel` does not pass
`grain-ext doctor`.

## Which activation needs which capability?

| Activation          | Required permission                                              | Rule                                                                                                                            |
| ------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `onShortcut:<id>`   | None                                                             | `<id>` must match an item in `contributes.shortcuts`. The press that wakes a cold worker is replayed to `grain.onShortcut`.     |
| `onEvent:<Variant>` | `events:transcripts` or `events:sessions` according to the event | Unknown event names and missing permissions fail `doctor`. The runtime also excludes ungranted activations from its wake index. |
| `onTransform`       | `transform:transcript`                                           | Registers the worker in the ordered transform pipeline and warms it on recording start.                                         |
| Session mode shortcut | `session:start` | Starts or stops the declared mode. Grain wakes the owner for its slow stage without requiring `onStartup`. |
| `onStartup`         | None in API 1.0                                                  | Keeps the worker resident. Use only when there is no event/shortcut alternative; it defeats idle reaping.                       |

Activation is not a substitute for permission. It says _when to wake_; the
grant says _what data or operation the worker may receive_.

## APIs that need no capability

`grain.log.info()` and `grain.log.warn()` are always available. Their messages
are prefixed with the authenticated extension id by Grain, so an extension
cannot forge another extension's log identity.

## Storage shapes

Use KV for small state that is naturally read and rewritten as a unit:

```ts
const count = (await grain.storage.get<number>("count")) ?? 0;
await grain.storage.set("count", count + 1);
```

Use documents for growing collections. Each key is one JSON file, so adding a
note does not rewrite an entire array:

```ts
await grain.doc.put("note-2026-07-23", { title: "Launch", body: "..." });
const keys = await grain.doc.list();
```

Document keys must be path-safe non-empty names. Both storage forms accept only
JSON-compatible values.

## Surface realm

Surface HTML runs in a sandboxed iframe with an opaque origin. It cannot use
Tauri IPC, inspect Grain's page, or create windows. Grain injects a smaller
`grain` bridge with logging, storage/doc, settings, LLM, embedding, `onEvent`,
and the close/dismiss method appropriate to the declared surface. Payloads passed
to `workspace.open(payload)` or `overlay.show(payload)` arrive through
`grain.onEvent` when the surface mounts and while it remains open.

See the [surface worked example](AUTHOR-EXAMPLES.md#3-workspace-surface) for the
complete handshake.
