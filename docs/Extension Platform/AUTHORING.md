# Build a Grain extension

This is the author entry point for Grain API 1.0. It gets a new scripted
extension from an empty folder to a live, hot-reloading worker. For the complete
permission vocabulary, examples, and failure guide, keep these pages nearby:

- [Capability reference](AUTHOR-CAPABILITIES.md)
- [Worked examples](AUTHOR-EXAMPLES.md)
- [Debugging guide](AUTHOR-DEBUGGING.md)
- [Typed error reference](ERRORS.md)

The public store is not live yet. Data packs can be imported from a file, and
scripted extensions run locally through Developer mode.

## Prerequisites

- A current Grain build.
- Node.js and npm. Scripted extensions use esbuild, which the scaffold installs
  as a development dependency.
- `grain-ext`. While release packages are not published, install it from a Grain
  checkout:

```powershell
cargo install --path crates/grain-ext-cli
```

Confirm the command is available:

```powershell
grain-ext --version
```

## 1. Create a project

Choose a permanent reverse-DNS id. It identifies storage, settings, grants, and
updates, so changing it later creates a different extension.

```powershell
grain-ext init "Focus Notes" --id com.yourname.focus-notes
cd focus-notes
npm install
```

The generated project contains:

| Path            | Purpose                                                        |
| --------------- | -------------------------------------------------------------- |
| `manifest.json` | Identity, permissions, activation, surfaces, and contributions |
| `src/main.ts`   | Worker code                                                    |
| `grain.d.ts`    | Generated Grain API and event types; do not edit               |
| `package.json`  | Reproducible esbuild command                                   |
| `dist/main.js`  | Generated entry loaded by Grain; do not hand-edit              |

Keep `grain.d.ts` committed. It makes an API mismatch visible in the editor and
records the contract version used by the project.

## 2. Build and check

```powershell
npm run build
grain-ext doctor
```

The expected result is:

```text
doctor: 0 findings (... files checked)
```

`doctor` runs the same shared checks intended for registry CI: manifest and
capability validation, activation checks, size/lifetime budgets, and rejection
of invisible or bidirectional Unicode. Fix every finding before loading the
folder. Generated `dist`, dependencies, `.git`, and Rust `target` trees are not
treated as submitted source.

## 3. Load it in Grain

1. Open **Settings → Extensions → Overview**.
2. Turn on **Developer mode**.
3. Under **Load unpacked**, choose **Choose folder…** and select the project
   root—the folder containing `manifest.json`, not `src` or `dist`.
4. Find the new card in **Overview** and enable it.
5. If it requests permissions, read the sheet and approve only what the
   extension needs.

Load-unpacked code has the same Rust-enforced permission checks as an installed
extension. Developer mode changes where bytes come from, not what they may do.

## 4. Start the development loop

Leave Grain running, then run from the project root:

```powershell
grain-ext dev
```

The command builds once, keeps esbuild in watch mode, and asks the running Grain
process to reload when `dist/main.js` or `manifest.json` changes. A successful
reload prints its latency plus worker/token counts. Stop it with `Ctrl+C`; the
watcher and child build process are cleaned up.

Open **Settings → Extensions → Developer** to see logs for the selected local
extension. The **Calls**, **Denials**, and **Errors** chips narrow the existing
log stream without creating a second logging service.

## 5. Make the first change

The generated shortcut logs a message. Change that message in `src/main.ts`,
save, and watch `grain-ext dev` report a reload. Trigger the extension's shortcut
from its card or assign a binding in Grain.

Host calls are promises. Treat failures as typed `GrainError` values:

```ts
try {
  await grain.storage.set("lastRun", Date.now());
} catch (error) {
  const failure = error as GrainError;
  await grain.log.warn(`${failure.code}: ${failure.message}`);
}
```

Use [the capability reference](AUTHOR-CAPABILITIES.md) before adding a host call,
and [the debugging guide](AUTHOR-DEBUGGING.md) when a reload or call fails.

## Lifecycle rules

- A worker starts only on a declared, granted activation.
- Non-resident workers are reaped after they are idle; do not rely on globals as
  durable storage.
- `onTransform` has a 150 ms deadline and three consecutive failures disable the
  extension. Persist or precompute outside that hot path.
- Workspaces are unmounted when sleeping. Rebuild UI state from storage and the
  opening payload on every mount.
- Turning off Developer mode unloads local projects, destroys their workers and
  surfaces, revokes the developer token, and restores any installed version that
  the local project overrode.
