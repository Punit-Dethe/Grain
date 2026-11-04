# Linux port readiness

Audit date: 2026-11-05 (Asia/Calcutta). Source: `main` / `origin/main` at
`b40330eef86a996dc2c8bdc59ce9adf9eb5f63d8`. Planning branch:
`linux/port-readiness`.

## Conclusion and scope

Grain already has Linux build targets and packaging. The remaining work is
primarily desktop integration and proving Grain's own behavior on real Linux
desktops. A successful package build does not establish feature parity.

This audit covers Grain-owned code, its native fork integration, frontend,
extensions/MCP, packaging and acceptance tooling. Handy bugs and upstream-sync
backlog are excluded. Handy files are referenced only where Grain depends on an
existing platform boundary; this plan does not propose repairs inside that tree.
No product code was changed or Linux application session exercised for this audit.

Evidence labels:

- **Confirmed gap:** the audited code explicitly lacks the Linux implementation.
- **Integration risk:** a portable path exists, but Linux behavior needs proof.
- **Release gap:** build, test or distribution wiring does not cover the product.

## Findings

| Priority | Area                              | Evidence                         | Consequence                                                                                             |
| -------- | --------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------- |
| P0       | Safe selection capture            | Confirmed gap                    | Agent's terminal guard always returns false off Windows before synthetic copy.                          |
| P0       | Agent focus and paste destination | Confirmed gap + integration risk | The original target is not retained/restored on Linux; wrong-target delivery needs prevention.          |
| P0       | Real Linux acceptance             | Confirmed gap                    | The maintained acceptance runner explicitly rejects non-Windows hosts.                                  |
| P1       | Expanding chat window             | Integration risk                 | Linux has a resize fallback; anchored placement and focus are compositor dependent.                     |
| P1       | Extension/MCP sign-in             | Integration risk + release gap   | Secret Service needs a working session; bundled GitHub registration is absent from normal Linux builds. |
| P1       | Extension runtime                 | Integration risk                 | Hidden supervisor and workers must be verified under WebKitGTK.                                         |
| P1       | Active-app/site context           | Confirmed gap                    | App/site profiles, caret context and Paste Catch have no equivalent native evidence.                    |
| P1       | Installed apps and pill icons     | Confirmed gap                    | App enumeration is empty; native icon resolution returns none.                                          |
| P1       | Packaging and updates             | Release gap + integration risk   | Packages exist, but production release/auth/update coverage needs completion.                           |
| P2       | Agent screen vision               | Confirmed gap                    | Capture returns none on Linux; requests proceed without the optional image.                             |
| P2       | Recall system audio               | Confirmed gap                    | Only the microphone source is currently supported on Linux.                                             |
| P2       | Onboarding and author tooling     | Integration risk                 | Linux capability setup and Unix child-process cleanup need targeted verification.                       |

P0 means resolve or explicitly constrain before inviting users to rely on the
Linux Agent. P2 features can be deferred for a limited first release if the UI
accurately describes availability. These are proposed priorities, not a decision
to drop features.

### 1. Selection capture can send an interrupt to a terminal

[agent.rs](../../src-tauri/src/agent.rs), `foreground_is_terminal` (line 1262),
returns `false` on all non-Windows targets. `capture_selection_result` (line 1286)
checks that guard, releases modifiers, then calls `send_copy_ctrl_c`. The existing
[input helper](../../src-tauri/src/handy/input.rs), line 291, implements Linux copy
as Control+C.

If input injection succeeds while a Linux terminal has focus, that shortcut can
interrupt its foreground job instead of copying text. This is Grain's selection
capture policy, even though the low-level input helper comes from Handy. It is
not evidence of an ASR or Handy shortcut bug.

Work: use positive evidence that the target supports safe copy, or decline
automatic selection capture when that evidence is unavailable. Do not treat an
unknown target as permission to send Control+C. Investigate bounded accessibility
selection reads; do not add a permanent selection poller. Merely replacing the
shortcut with Control+Shift+C globally would break ordinary applications.

Acceptance: summon Agent from a browser text field, editor and terminal running a
disposable job. The job survives, the selection is correct where supported, and
the user's clipboard is preserved. Repeat for X11, native Wayland and XWayland.

### 2. Chat expansion exists; desktop placement is the hurdle

The chat surface is Grain's core Agent panel, shared by extension/MCP tool
conversations. It is not a separate native chat window implemented by each
extension. See [AgentPanel.tsx](../../src/app/components/agent/AgentPanel.tsx),
`expand`, and [agent.rs](../../src-tauri/src/agent.rs):

- `build_window` (line 853) creates a transparent, frameless, always-on-top window.
- `place_panel` (line 1120) uses a Windows fixed viewport and native input region.
- The non-Windows path (line 1161) resizes to the visible card's footprint.
- `set_bounds` (line 1004) uses separate position and size calls off Windows and
  discards their results. Center mode also depends on desktop coordinates.
- `focus_now` (line 786) has no Linux-specific activation handling.

Therefore, expansion is not intrinsically Windows-only. Linux needs validation
and possibly a different placement policy, rather than an automatic rewrite of
the conversation UI. Native Wayland does not provide the same positioning
assumptions as X11; even GTK's ordinary move/keep-above calls are requests the
window manager may ignore. See [GTK move](https://docs.gtk.org/gtk3/method.Window.move.html)
and [keep-above](https://docs.gtk.org/gtk3/method.Window.set_keep_above.html).

The existing recording pill has a layer-shell path in
[Handy's overlay boundary](../../src-tauri/src/handy/overlay.rs), lines 83–142.
The Agent panel does not use it. Copying that path is not a universal solution:
[gtk-layer-shell's supported desktops](https://github.com/wmww/gtk-layer-shell#supported-desktops)
include KDE/wlroots but exclude GNOME Wayland. A typing chat also needs keyboard
interaction, unlike the recording pill's `KeyboardMode::None`.

Work: choose a documented policy for X11, layer-shell-capable Wayland, and
Wayland without layer shell. Prototype through the real Agent window. A normal
compositor-managed chat window is a candidate where exact anchoring is
unavailable; changing the visual behavior requires user approval. Report useful
placement/activation failures rather than silently claiming success.

Acceptance: compact/expanded/center modes, first keystroke, Enter/Escape, follow-up
reopen, window close, transparent-area clicks, long replies, mixed DPI, negative
monitor origins and panels/docks. Neither the card nor composer may be clipped or
block unrelated desktop input.

### 3. Paste-back needs its own Linux contract

[agent.rs](../../src-tauri/src/agent.rs), `foreground_hwnd` (line 801), returns none
off Windows; `refocus_target` (line 2186) does nothing there. Grain may call the
existing paste pipeline after the Agent has taken focus, without an explicit
Linux target restoration. Whether the desktop restores the previous application
on hide/close must be tested, not assumed.

Work: retain a platform-appropriate target identity only where usable. Confirm
the destination before automatic delivery; provide an explicit copy/manual-paste
outcome when restoration cannot be established. Basic dictation may use the
existing Handy Linux paste path; port Grain's Agent behavior around it without
rewriting Handy. Include transient Agent Enter/Escape/follow-up bindings and
cleanup in this work: they add requirements beyond ordinary dictation hotkeys.

Acceptance: paste from compact chat, expanded chat and Quick Agent into native
Wayland, XWayland and X11 targets; user focus changes during a response must not
send text to an unrelated app. Closing Agent releases all transient bindings.
The [GlobalShortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
is an option to evaluate where needed, not a drop-in replacement for arbitrary
temporary bare-key grabs: binding typically involves user consent.

### 4. Authentication is portable, with two specific gaps to close

[grain_auth.rs](../../src-tauri/src/grain_auth.rs), `loopback_redirect` (line 37)
and `connect_attempt`'s callback setup around line 1229, use the system browser,
loopback HTTP, state and PKCE. [grain_mcp.rs](../../src-tauri/src/grain_mcp.rs)
similarly uses the opener and native callback listeners; the fixed callback is
`127.0.0.1:31938` where the registration requires it, and dynamic registration can
use an ephemeral port. These mechanisms do not depend on Windows browser APIs or
a Windows custom URI handler.

**Vault:** both authentication paths use `keyring::Entry` and propagate vault
errors. The locked keyring 4.1.6 v1 backend uses
[Secret Service on Unix](https://docs.rs/keyring/4.1.6/keyring/v1/index.html).
Linux needs a reachable session bus and an available/unlockable provider. KDE
must be tested with its actual Secret Service configuration; the desktop name
alone proves nothing. This is a deployment and recovery requirement, not a
missing Linux keyring backend. Preserve vault-only token storage.

**Bundled GitHub registration:** `builtin_client` in `grain_mcp.rs`, line 996,
reads `GRAIN_GITHUB_OAUTH_CLIENT_ID` and `GRAIN_GITHUB_OAUTH_CLIENT_SECRET` at
compile time. [build.yml](../../.github/workflows/build.yml), lines 376–377,
provides them only for `github-extension-alpha`; that option's validation allows
Windows only (line 93). Normal Linux jobs therefore build without that bundled
registration. [grain-release.yml](../../.github/workflows/grain-release.yml)
supplies it for the main Windows release. This is a confirmed gap for the
supported GitHub store connection, not a claim that all Linux MCP OAuth fails.

Work: supply and validate the same approved public/extractable desktop
registration through the normal Linux product build. Retire obsolete alpha-only
wiring as appropriate; do not create a parallel legacy Linux auth mode. Keep
account tokens and application private keys out of build inputs.

Acceptance: browser launch, valid callback, occupied fixed port, denial, cancel,
timeout, restart/persistence, refresh, disconnect and extension removal. Exercise
locked/missing vaults and a changed session; verify actionable errors without
plaintext persistence. Use controlled fixtures first and an explicitly opted-in
real account only when required.

### 5. Active context, app discovery and icons form one dependency chain

[context_detect.rs](../../src-tauri/src/context_detect.rs), `read_focus_probe`
(line 646), `detect_active_context` (line 1285) and `capture_stop_context` (line
1298), return no native facts or an empty snapshot off Windows. Consequently:

- [grain_post_process.rs](../../src-tauri/src/grain_post_process.rs) cannot apply
  native app/site/caret context as it does on Windows.
- [Paste Catch](../../src-tauri/src/paste_catch.rs) lacks the positive editable
  target evidence it uses to catch missed dictation delivery.
- [surface_watch.rs](../../src-tauri/src/surface_watch.rs), lines 79–112, installs
  no Linux foreground-change watcher.
- [app_catalog.rs](../../src-tauri/src/app_catalog.rs), line 73, returns an empty
  installed-app list. Its picker can fall back to typed names, but that does not
  make native matching work.
- [pill_icon.rs](../../src-tauri/src/pill_icon.rs), `foreground` (line 199), needs
  detected identity; `resolve` (line 225) additionally returns none off Windows.

Linux icons are obtainable. Desktop entries provide `Icon=`, which can name an
absolute file or a themed icon; GTK/freedesktop lookup can resolve it to pixels.
See the [Desktop Entry](https://specifications.freedesktop.org/desktop-entry/latest-single/)
and [Icon Theme](https://specifications.freedesktop.org/icon-theme/latest/)
specifications. Getting the active application's reliable identity is a separate,
harder requirement on Wayland. Reading installed desktop files alone does not
identify the focused application or current browser tab.

Work: implement Linux identity/discovery at the existing Rust seams, then icon
resolution. Evaluate X11 window metadata and bounded AT-SPI reads; evaluate
Wayland capabilities per compositor instead of assuming a universal foreign-app
inspection API. Match desktop-file IDs, executable identities and window hints
carefully, including Flatpak/exported launchers. Reuse GTK's theme lookup and
rasterization for SVG/XPM/theme inheritance instead of adding another image
engine. Keep the existing small RGBA output and bounded cache. Watch only while
the feature is active and disconnect listeners when it ends.

Distinguish icon types: Grain's own bundled PNG/SVG UI assets and extension PNG
masters in [extension_icons.rs](../../src-tauri/src/extension_icons.rs) do not use
the Windows shell. Website favicon fetching is also portable, but automatic
selection of the current site's favicon still depends on browser identity.
Missing native pill identity already degrades to the plain-dot presentation;
this is feature loss, not evidence of an icon-induced launch crash.

Acceptance: native packages, Flatpak apps, renamed/missing desktop entries,
theme changes, app/tab switching, browser accessibility disabled, password and
unreadable fields. Unknown must remain unknown; do not guess profile or target
identity from a window title.

### 6. Screen vision and Recall audio are separate parity work

[context_screen.rs](../../src-tauri/src/context_screen.rs), lines 104–135, has
only a Windows capture implementation. Agent's `capture_screen_image` in
[agent.rs](../../src-tauri/src/agent.rs), line 1370, therefore produces no image
on Linux, even if the option is enabled. It proceeds as text-only.

Work: use a capability-detected, consent-aware Linux capture route or mark the
feature unavailable. The [Screenshot portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html)
has versioned target capabilities; query the installed interface/backend instead
of assuming current documentation matches every user's desktop. Preserve the
one-window capture scope, bounded frame size and prompt release/zeroization. Do
not substitute silent full-desktop capture. Current extension host APIs already
reject retired `capture.*` methods: see [host_api.rs](../../src-tauri/src/host_api.rs),
lines 752–785. Do not resurrect them for the port.

[grain_meet/sources.rs](../../src-tauri/src/grain_meet/sources.rs), line 75,
supports system audio only on Windows. Linux defaults to microphone-only;
attempting system capture fails explicitly (line 368). Full Recall parity needs
a Linux output-monitor capture implementation, evaluated against PipeWire and
PulseAudio-compatible setups, plus device changes and independent source timing.
This is Grain-owned work, not a Handy microphone bug. Capture streams must close
on stop, cancellation and failure.

### 7. WebKitGTK acceptance is missing

Extensions run in Web Workers constructed by
[extension-host.ts](../../src/app/extension-host.ts), with transport in
[extension-runtime.ts](../../src/app/extension-runtime.ts), inside a hidden
supervisor created by [extension_host.rs](../../src-tauri/src/extension_host.rs),
line 1101. This avoids native extension binaries but makes WebKitGTK's real
worker/blob URL/local WebSocket behavior part of the product contract. No source
evidence here proves that Linux workers are broken.

The existing [acceptance runner](../../tests/agent-harness/run.mjs), line 1763,
explicitly blocks non-Windows systems; it uses WebView2 CDP, PowerShell native
input and Windows process cleanup. Its current success cannot certify Linux.
[Tauri's WebDriver route](https://v2.tauri.app/develop/tests/webdriver/) supports
Linux through WebKitWebDriver and is a candidate for a real-app adapter.

Work: extend the maintained `tests/agent-harness/` runner with a Linux adapter,
retaining isolated profiles, scoped test vault accounts, owned process trees,
timeouts and recorded evidence. Verify multi-window/hidden-supervisor access
before choosing the driver. Use native desktop sessions for placement, focus,
input and compositor acceptance; headless X11 alone cannot certify Wayland.
Never create a browser-only replica or mock Tauri visual harness.

Acceptance: install/run/disable/remove, worker crash and reload, IPC, local
transport authorization, MCP cancel/refresh, hidden-window execution, zero live
workers after idle expiry, supervisor teardown, and bounded idle RAM. Include
real WebKit rendering of chat expansion, clipboard, fonts and dialogs. User
visual approval remains required for any UI design changes.

### 8. Packaging exists; production release behavior needs proof

Existing scaffolding:

- [Cargo.toml](../../src-tauri/Cargo.toml) has Linux GTK/layer-shell dependencies
  and the transcribe-cpp Vulkan/dynamic-backend posture.
- [build.rs](../../src-tauri/build.rs) stages shared runtime libraries and adds
  relative Linux rpaths. [tauri.conf.json](../../src-tauri/tauri.conf.json) maps
  them into `/usr/lib/Grain` for deb/rpm and `usr/lib` for AppImage.
- [main-build.yml](../../.github/workflows/main-build.yml) covers x86_64 deb,
  AppImage/rpm and aarch64 Linux. [flake.nix](../../flake.nix) covers both Linux
  architectures. These are useful starting points, not a port from zero.
- The current [native fork contract](../../native/transcribe-fork.json) is
  Grain-specific; package tests must exercise its actual CPU and Vulkan backends.

Work: validate staged libraries in installed artifacts, not just a build tree:
`libtranscribe`, core ggml and dynamically loaded backend modules, CPU-only
operation, missing Vulkan driver, model load/transcription and resource release.
Check distro runtime dependencies, WebKitGTK, OpenSSL/TLS roots, layer shell,
OpenBLAS, launcher/tray assets, autostart and user data permissions. Test AppImage
on clean systems and include its extraction/relocation path. Nix packages need
the same first-party extension registration and runtime checks as other builds.

The dedicated [Grain release](../../.github/workflows/grain-release.yml) is
Windows-only. The reusable cross-platform release path exists, but
[publish-release.yml](../../.github/workflows/publish-release.yml) validates NSIS
assets and accepts a feed with merely one platform. Define one release contract
that requires the intended Linux assets and updater entries, without losing
other platforms when publishing `latest.json`.
[grain_update.rs](../../src-tauri/src/grain_update.rs) already supports
`GRAIN_DISABLE_UPDATER` for package-managed installs. Prove update discovery,
install and restart for the chosen AppImage distribution; explicitly define
deb/rpm/Nix update ownership instead of promising identical self-update behavior.

### 9. Smaller integration checks

[grain_onboarding.rs](../../src-tauri/src/grain_onboarding.rs), line 158, reports
unknown permission status on Linux. The
[onboarding UI](../../src/app/components/onboarding/AccessibilityOnboarding.tsx),
line 86, treats its `other` platforms as granted and refreshes devices. Add
accurate capability/setup guidance where Grain needs it: vault, screen capture
and Agent shortcut delivery. Unknown must not be presented as verified access.
Retain inherited audio/shortcut behavior; Handy defects are outside this audit.

The [extension CLI](../../crates/grain-ext-cli/src/lib.rs), line 638, already uses
XDG paths and `npm` on Unix. Its `BuildWatcher::drop` (line 683) kills only the
direct child on Unix, unlike Windows tree termination. Verify that stopping a
Linux development watch leaves no npm/shell/bundler descendants; fix ownership
and process-group cleanup if it does. This is a developer workflow risk, not a
blocker for installing an ordinary tool-only extension.

## Execution plan

1. **Establish the supported matrix and safety gate.** Proposed first target:
   x86_64 Linux, using deb/AppImage already built by CI. Establish one X11
   baseline and native GNOME/KDE Wayland trials before claiming general Linux
   support. ARM64, Nix and wlroots remain explicit additional targets. The final
   support promise and deferrals are product decisions, not approved by this doc.
   Constrain unsafe copy/paste behavior first; keep unknown targets safe.
2. **Make acceptance possible.** Add the real Linux harness adapter and record
   baseline package startup, extension/MCP execution and cleanup. Resolve CI
   infrastructure failures separately from product defects. No new runtime
   service or alternate UI render path is needed for this step.
3. **Complete Agent interaction.** Implement the minimal Rust placement,
   activation, selection and delivery seams for the chosen matrix. Preserve
   existing conversation rendering. Test transient shortcuts and teardown. Any
   changed UI design stays on `ui/grain-2.0` and needs user visual approval.
4. **Complete auth and runtime packaging.** Wire normal Linux GitHub registration,
   verify Secret Service recovery, OAuth callbacks and WebKitGTK worker behavior.
   Clean obsolete alpha-only/compatibility-only paths rather than extending them.
5. **Restore contextual parity.** Implement reliable context/app identity first,
   then app enumeration, pill icons and session-scoped switch notifications.
   Treat unavailable site/caret/Paste Catch evidence explicitly.
6. **Resolve optional parity scope.** Implement screen vision and Recall system
   audio or document/represent their unavailability for the first Linux release.
   Do not add legacy host capture APIs or migration compatibility.
7. **Qualify distributables and publish only after acceptance.** Verify CPU/Vulkan,
   installed runtime layout, updates, autostart/tray, both supported architectures
   and clean-machine behavior as applicable. Require actual Linux visual approval
   and recorded evidence before a release claim. This task does not publish one.

For each implementation phase: run targeted unit tests plus frontend
lint/typecheck/build and relevant Rust checks. Both Cargo roots matter: the
workspace excludes `src-tauri`. Follow with real-app acceptance for the touched
flows. Keep platform work in Grain-owned Rust modules/crates; frontend calls use
Tauri commands and backend notifications use events. Avoid polling engines,
resident capture services, broad icon caches and compatibility-only adapters.

## Verification record

Read-only CI inspection of Grain's repository, not Handy's:

- On the audited SHA, [Main Branch Build](https://github.com/Punit-Dethe/Grain/actions/runs/41897522113)
  completed successfully for Ubuntu 22.04 x86_64 deb, Ubuntu 24.04 x86_64
  AppImage/rpm, and Ubuntu 24.04 ARM64 deb/AppImage/rpm during this audit. This
  establishes build/package progress, not installed runtime or desktop acceptance.
- The [test run](https://github.com/Punit-Dethe/Grain/actions/runs/41897521987)
  passed workspace checks/tests. Its native Linux Rust job failed restoring a
  cache with `No space left on device`, before tests; frontend lint failed on
  untranslated hero text in `GrainApp.tsx`. Neither establishes a Linux product
  defect, and neither should be reported as a native test pass.
- Graph-first exploration was attempted. The available index covered three files
  and returned no relevant nodes/impact for these modules. Direct source reads
  were required; the empty graph result is not evidence of a small blast radius.
- Local verification for this change is documentation formatting, source-link
  existence and diff checks. No Linux compile, graphical acceptance, account
  sign-in, upstream sync or product fixes were performed in this planning task.

Suggested real Linux development entry point after dependencies are installed:

```sh
bun install --frozen-lockfile
bun run dev:asr
```

Implementation phases should supply exact updated build and maintained harness
commands once the Linux adapter exists; there is no working Linux acceptance
command to claim today.
