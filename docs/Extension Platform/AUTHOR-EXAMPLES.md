# Worked Grain extension examples

These examples are deliberately small. Each demonstrates one platform shape
without adding an application framework. The checked-in files under
[`examples/`](examples/) are validated by the test suite.

## 1. Data pack: meeting prompts

A data pack contains no executable code and requests no permissions. Save
[`examples/prompt-pack.grainpack`](examples/prompt-pack.grainpack), then use the
import action in the **Extensions** header. The pack is installed disabled;
enable **Meeting Prompts** in **Extensions → Overview** to add its two prompts.

The important shape is:

```json
{
  "manifest": {
    "id": "com.example.meeting-prompts",
    "name": "Meeting Prompts",
    "version": "1.0.0",
    "grainApi": "^1.0",
    "tier": "pack"
  },
  "payloads": {
    "prompts": [
      {
        "id": "action-items",
        "name": "Extract action items",
        "prompt": "Extract action items. Name the owner and due date when present."
      }
    ]
  }
}
```

Prompt ids are namespaced by Grain at install time, so `action-items` cannot
collide with the same id from another pack. Re-importing an update preserves the
user's enabled state.

## 2. Scripted extension: persistent click counter

Create a normal scaffold, then copy the example manifest and source over it:

```powershell
grain-ext init "Click Counter" --id com.example.click-counter
cd click-counter
npm install
```

Use:

- [`examples/click-counter/manifest.json`](examples/click-counter/manifest.json)
- [`examples/click-counter/src/main.ts`](examples/click-counter/src/main.ts)

The manifest requests only `storage` and connects the `count` activation to a
contributed shortcut:

```json
"permissions": ["storage"],
"activation": ["onShortcut:count"],
"contributes": {
  "shortcuts": [
    {
      "id": "count",
      "label": "Count a press",
      "default_binding": "Ctrl+Alt+Shift+C"
    }
  ]
}
```

The handler persists its count and writes an authenticated developer log:

```ts
grain.onShortcut(async (id) => {
  if (id !== "count") return;
  const previous = (await grain.storage.get<number>("presses")) ?? 0;
  const presses = previous + 1;
  await grain.storage.set("presses", presses);
  await grain.log.info(`Shortcut pressed ${presses} time(s)`);
});
```

Build, check, load, and enable it using the [quickstart](AUTHORING.md):

```powershell
npm run build
grain-ext doctor
grain-ext dev
```

Press `Ctrl+Alt+Shift+C`. Each press increments the same value even after the
worker is reaped or Grain restarts.

## 3. Workspace surface

The workspace example has two isolated realms:

1. The worker receives a shortcut and asks Grain to open its declared surface.
2. Grain creates or wakes the host-owned window, mounts the sandboxed HTML, and
   delivers the opening payload.

Start from a scaffold, then use:

- [`examples/workspace-surface/manifest.json`](examples/workspace-surface/manifest.json)
- [`examples/workspace-surface/src/main.ts`](examples/workspace-surface/src/main.ts)

The declaration and permission must appear together:

```json
"permissions": ["surface:workspace"],
"activation": ["onShortcut:open"],
"surfaces": {
  "workspace": {
    "title": "Example Workspace",
    "min_size": [640, 420],
    "ui_source": "<!doctype html>..."
  }
}
```

After loading and enabling the checked example, press its suggested
`Ctrl+Alt+Shift+W` shortcut.

The worker opens only its own workspace; there is no extension id parameter to
spoof:

```ts
grain.onShortcut(async (id) => {
  if (id !== "open") return;
  await grain.workspace.open({ message: `Opened by ${grain.extId}` });
});
```

Inside `ui_source`, Grain injects the surface bridge before the author's HTML.
The page receives both the opening payload and later payloads through
`grain.onEvent`:

```html
<p id="message">Waiting for a payload...</p>
<button onclick="grain.workspace.close()">Close</button>
<script>
  grain.onEvent(function (payload) {
    if (payload && payload.message) {
      document.getElementById("message").textContent = payload.message;
    }
  });
</script>
```

Closing a workspace unmounts its iframe before the host hides/suspends the
window. Opening it again creates a fresh DOM and delivers a fresh payload, so
durable UI state belongs in `grain.storage` or `grain.doc`, not JavaScript
globals.

For an overlay, replace the declaration/permission with `surface:overlay` and
call `grain.overlay.show(payload)`. Overlays are created per invocation,
auto-dismiss, and cannot exceed the documented size/lifetime budgets.

## 4. Extension-owned voice note

The [voice-note example](examples/voice-note/) is the complete session-mode
acceptance extension. It declares one host-owned recording mode:

```json
"permissions": ["session:start", "llm", "storage"],
"contributes": {
  "sessionMode": {
    "id": "note",
    "label": "Dictate a voice note",
    "default_binding": "Ctrl+Shift+N"
  }
}
```

Its contributed shortcut starts and stops the serialized recorder. Grain shows
the extension name in the pill, wakes the worker for the bounded slow stage,
and supplies an `AbortSignal`. The handler structures and stores the note, then
suppresses the normal paste:

```ts
grain.onSessionStage(async (text, { signal }) => {
  const note = await grain.llm.complete(`Structure this note:\n\n${text}`);
  if (signal.aborted) return text;
  await grain.doc.put(`voice-note-${Date.now()}`, { original: text, note });
  return { handled: true };
});
```

The stage has a 30-second ceiling. A crash, timeout, disable, or hot reload
cancels the wait and restores the exact input text; owning a stage may change
the user's words but cannot lose them.

## 5. Developer-only native companion

The [native-companion example](examples/native-companion/) connects a separate
Rust process through the same authenticated WebSocket and capability filter as
a scripted worker. Native manifests use `tier: "native"` and a per-platform
`companion` map. Grain accepts them only from a human-selected unpacked folder
while Developer mode is enabled; native packages are not distributable before
Phase 5A trust rails.
