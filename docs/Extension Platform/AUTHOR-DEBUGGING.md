# Debugging Grain extensions

Start with these three checks:

1. Run `grain-ext doctor` in the folder that contains `manifest.json`.
2. Run `npm run build` once and confirm the manifest's `entry` exists.
3. In Grain, open **Settings → Extensions → Developer**, select the extension,
   and use **Errors** or **Denials** before reading the full stream.

## Read a `doctor` finding

Findings are deterministic and begin with a relative path:

```text
src/main.ts:2:17 [E_UNICODE] forbidden invisible/bidirectional Unicode U+200B (ZERO WIDTH SPACE)
```

The two numbers are line and Unicode-scalar column. Remove the named codepoint;
do not silence the check. Other common codes:

| Code                  | Meaning                                                                                         |
| --------------------- | ----------------------------------------------------------------------------------------------- |
| `E_MANIFEST`          | JSON parsed, but the SDK contract rejected a field, capability, surface, slot, or contribution  |
| `E_ENTRY`             | `entry` is empty, absolute, escapes the project, is not a file, or resolves outside the project |
| `E_API_VERSION`       | `grainApi` is malformed or incompatible with the current SDK                                    |
| `E_ACTIVATION`        | Unknown/duplicate activation or a shortcut id that is not contributed                           |
| `E_CAPABILITY`        | An event/transform activation is missing its required permission                                |
| `E_BUDGET` / `E_SIZE` | A surface lifetime/size or source/entry file exceeds the host budget                            |
| `E_SYMLINK`           | Submitted source contains a symbolic link; source must be reviewable in place                   |

## Use the Developer log

Every line is tagged `[ext:<manifest-id>]` from the authenticated connection.
The filtered console provides:

- **Calls** — detailed host-call traffic for load-unpacked projects.
- **Denials** — missing capability plus the exact `manifest.json` permission to
  add, or an ungranted activation excluded from the wake index.
- **Errors** — worker failures and source-mapped authored stacks.
- **All** — lifecycle, reload, warnings, and the categories above.

Installed extensions do not emit detailed call traffic. That is a Developer-mode
diagnostic, not a permanent background cost.

## Handle typed host failures

Host calls reject with `GrainError`, not a successful `null` when Grain refused
the operation:

```ts
try {
  const selected = await grain.captureSelection();
  if (selected === null) await grain.log.info("Nothing is selected");
} catch (error) {
  const failure = error as GrainError;
  await grain.log.warn(`${failure.code}: ${failure.message}; ${failure.hint}`);
}
```

`null` remains a valid absence result—for example, a missing storage key or no
current selection. Permission denial, corrupt storage, timeout, quota, invalid
arguments, and unavailable services throw. See [ERRORS.md](ERRORS.md).

## Reload failures

| Symptom                                                | Check                                                                                                                                     |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `read developer token ... enable Developer mode first` | Turn on Developer mode in **Extensions → Overview**. The token exists only while the mode is enabled.                                     |
| `connect to Grain developer channel`                   | Grain must be running. Confirm another process is not occupying loopback port 7124.                                                       |
| `npm build failed`                                     | Run `npm run build` directly; fix the first esbuild error.                                                                                |
| Reload succeeds but behavior is unchanged              | Confirm `manifest.entry` names the generated file and the build writes that exact path.                                                   |
| Extension is loaded but never runs                     | Enable its card, approve permissions, and make sure an activation can actually occur. `doctor` catches unknown and ungranted activations. |
| Worker count grows on every reload                     | Stop and report it. A healthy project replaces one generation while worker/token counts remain constant.                                  |

## Source maps and worker crashes

Keep esbuild's `--sourcemap` option. For load-unpacked workers, Grain reads a map
only after failure, accepts inline maps or an external map inside the approved
project, maps authored frames, then drops the parsed map. If a custom build has
no valid map, the raw generated stack remains in the log.

When reporting a crash, include:

- the `[ext:<id>] error` line and mapped stack;
- the preceding lifecycle/reload lines;
- `grain-ext doctor` output;
- Grain version and OS;
- the minimal manifest and source change that reproduces it.

Never include the contents of `extension-dev-token.json`. It is a short-lived
credential, not useful diagnostic data.

## Surface problems

- A declared workspace/overlay needs its matching `surface:*` permission and a
  non-empty `ui_source`.
- Surface markup runs in a sandboxed opaque-origin iframe. Browser code that
  expects same-origin access, Tauri globals, Node APIs, or arbitrary network
  access will not work.
- Listen for the opening payload immediately with `grain.onEvent`. A workspace
  can sleep and remount, so initialize the page on every execution.
- Use `grain.workspace.close()` or `grain.overlay.dismiss()` from inside the
  surface. The extension cannot resize, move, or directly destroy host windows.

If the surface opens but remains blank, start with a static `<h1>` in
`ui_source`, then add the injected `grain` calls one at a time while watching the
Developer **Errors** filter.
