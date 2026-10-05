# Grain Linux release audit

Audited 2026-10-05. Baseline: GitHub `main`, commit `661f89ab83e806fe07126ffd3ab6bc661990cdaa` (v0.0.8). This is an audit and release plan; application code is unchanged.

## Verdict

**Main is not ready for a Linux release.** There is a reproduced build blocker in Grain's repository configuration, several deliberately empty Linux implementations in Grain-owned features, and no passing Linux application acceptance evidence.

The ASR algorithms, snippets and most settings do not need a general rewrite. The main work is platform integration, truthful feature availability, and the build/release gates. A limited Linux beta can ship earlier than Windows feature parity, provided unsupported features are explicitly unavailable and the supported desktop/package matrix is tested.

Extension-system implementation, OAuth/companion migration and extension-specific defects are excluded. No Handy sync or backend divergence is proposed.

## Evidence and limits

- Fetched `origin/main`; audited the exact commit above in an isolated checkout, rather than the open `extensions/tool-only-retirement` branch or either checkout's modified generated bindings.
- Used code-review-graph minimal context, semantic searches, architecture overview and impact radius first (five calls). Its Linux results and dependency edges were incomplete for this audit; direct source inspection established the actual compiled paths. Graph risk scores are not platform-readiness evidence.
- Read the ownership runbook, divergence policy and current overlay maintenance contract. The active overlay is `handy/overlay.rs`; `grain_overlay.rs` adapts presentation. The runbook's older “inert overlay” description is superseded by that compiled mapping and maintenance contract.
- Checked Grain's GitHub CI, with the repository explicitly scoped to `Punit-Dethe/Grain`. Current main's Linux jobs all fail **before application compilation completes**. Fixing the first error may reveal further build errors.
- Ran existing Windows-host tests: 217 grain-core unit tests, 4 core integration tests, 22 grain-tdt tests, 28 existing snippets/geometry source tests, and 10 native-fork script tests: **281 passed**. Source tests compiled the real snippets module against the real core Snippet type; no application or visual mock was introduced.
- A Linux desktop runtime was unavailable locally: WSL lists only Docker Desktop, and the Docker Linux engine is not running. No Linux launch, package installation, audio/input test or visual acceptance is claimed. A Windows pass does not validate Linux-specific defaults or APIs.

## Feature readiness

| Area                      | Linux behavior on audited main                                                             | Release action                                                   |
| ------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| Standard dictation        | Uses Handy capture/paste plus Grain routing/finalization                                   | Build and real-app testing; no identified Windows-only algorithm |
| Native ASR and Flow       | Owned scheduling/journal/TDT math is portable; native fork and loader need Linux execution | Verify exact fork ABI, CPU fallback and installed packages       |
| Snippets and “scrap that” | Pure Unicode/text processing                                                               | Retain; test final text delivery in every supported mode         |
| Dictionary                | Handy correction plus owned dictionary biasing                                             | Retain; test selected languages/models                           |
| Pill rendering/window     | Shared Handy WebView lifecycle already has Linux layer-shell/fallback                      | Test existing lifecycle; see defaults and desktop limitations    |
| Pill app/site identity    | No foreground context; app icon resolver returns None; watcher has no Linux event hooks    | Linux adapters, or explicit generic/no-icon behavior             |
| Context awareness         | Linux Stop snapshot has no surface or caret                                                | Implement supported context, or mark unavailable                 |
| Custom app profile picker | Always returns an empty application list                                                   | XDG desktop-entry catalogue, or disable application targets      |
| Agent replies/LLM         | Conversation/provider logic is portable                                                    | Keep; validate real input and result surfaces                    |
| Agent selected text       | Enigo copy path bypasses Handy's Linux typing-tool selection                               | Supported selection adapter or explicit selection input          |
| Agent paste destination   | Captured HWND and restoration are Windows-only                                             | Linux destination policy; manual-copy fallback where unavailable |
| Agent screen vision       | Both capture entry points return None                                                      | Implement consent-based capture or disable “See my screen”       |
| Paste Catch               | No Linux focus probe, so no automatic interception or confirmed-miss recovery              | Mark unavailable or add a bounded Linux probe                    |
| Packaging/updater         | Linux recipes exist; normal release/publish path assumes Windows                           | Add verified Linux assets/feed gates                             |
| Nix                       | Current dependency manifest check fails                                                    | Repair only if Nix is a supported release target                 |

## Prioritized Grain-owned findings

### 1. P0 — tracked Windows Cargo target directory breaks all Linux builds

[.cargo/config.toml:2](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.cargo/config.toml#L2) sets `target-dir = "C:\\t"` for every OS. Linux interprets this as a relative directory containing a colon. Cargo then cannot construct the dynamic library search path.

The current [main build run](https://github.com/Punit-Dethe/Grain/actions/runs/37193707105) fails on Ubuntu 22.04 x64, Ubuntu 24.04 x64 and Ubuntu 24.04 ARM64 with:

```text
failed to join paths from `$LD_LIBRARY_PATH` together
"/home/runner/work/Grain/Grain/C:\t/release/deps"
Caused by: path segment contains separator `:`
```

**Required:** remove the machine-specific value from shared configuration; keep Windows short paths in Windows-only environment setup. Use an ordinary target directory on Linux and re-run every Linux job. Align artifact lookup with the effective target directory. The Tauri launcher itself is correctly guarded by `process.platform === "win32"`; it is not the source of this error.

### 2. P1 — context awareness is absent, not merely unreliable

[context_detect.rs:1463](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/context_detect.rs#L1463) returns `None` outside Windows. [capture_stop_context:1476](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/context_detect.rs#L1476) produces an enabled but empty Stop snapshot on Linux. Therefore application/site profiles and nearby-caret insertion context cannot apply. Base-prompt post-processing still works.

The application picker in [app_catalog.rs:73](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/app_catalog.rs#L73) returns `Vec::new()` outside Windows. Comments describing XDG desktop entries are proposed implementations, not working code.

**Required for parity:** bounded Linux foreground/focus/accessible-text adapters; desktop-entry identity shared by detection, custom-profile matching and icons. X11 and native Wayland need distinct coverage. Investigate [AT-SPI application/text APIs](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/class.Accessible.html) as a candidate, then prove support in actual GTK, Qt, Electron and browser targets. Preserve password exclusions, read limits, confidence checks and once-at-Stop lifetime.

**Acceptable beta scope:** expose context as unavailable rather than offering a toggle and empty picker that imply it works. Static website favicon fetches are portable, but fetching a favicon does not provide active browser URL detection. No browser-extension migration dependency is assumed.

### 3. P2 — foreground icons require both detection and an icon resolver

[pill_icon.rs:199](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/pill_icon.rs#L199) depends on the missing foreground context; [resolve:225](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/pill_icon.rs#L225) also returns `None` outside Windows. [surface_watch.rs:80](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/surface_watch.rs#L80) installs change hooks only on Windows.

**Required for parity:** resolve desktop-file IDs and `Icon=` through the [Desktop Entry](https://specifications.freedesktop.org/desktop-entry/latest/) and [Icon Theme](https://specifications.freedesktop.org/icon-theme/latest/) specifications. Cover packaged apps, symbolic/SVG icons and executable-to-desktop-ID mismatches. Reuse the current bounded cache and event contract. Install any foreground listener only during an active session and remove it at session end.

**Beta alternative:** a generic pill without foreground identity. Missing icons alone need not block dictation, but implementing the icon resolver alone will not restore site/app icons.

### 4. P1 — Agent cannot reliably return output to its original Linux target

[agent.rs:540](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L540) captures an HWND only on Windows; [refocus_target:1864](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L1864) is a no-op elsewhere. Both [quick paste:1838](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L1838) and [confirmed paste:2143](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L2143) still invoke the shared paste pipeline after delays.

**Risk:** after opening the Agent panel or switching apps during generation, output goes to the currently focused field rather than a validated summon destination. Window managers restoring focus on close is not a portable guarantee. This is a missing Grain integration, not a request to alter Handy paste.

**Required:** define and validate a Linux destination where the desktop permits it. Otherwise keep output copyable and ask the user to select a destination before insertion; disable automatic Quick Agent insertion where that guarantee is absent. Test closed targets, app switches, focus refusal, overlapping windows and selected-text replacement.

### 5. P1 — Agent selected-text capture bypasses Handy's Linux helper strategy

[capture_selection_result:1004](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L1004) unconditionally uses Enigo, modifier release and simulated copy. It does not select `wtype`, `dotool`, `ydotool` or `xdotool` through the existing Linux dispatch used by [Handy clipboard:123](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/handy/clipboard.rs#L123).

**Risk:** ordinary dictation can use a supported Linux typing tool while “rewrite the selected text” still captures nothing. The public wrapper logs failures and converts them to no selection, so the LLM may receive only the instruction.

**Required:** a supported, bounded selected-text capture path (for example proven accessibility selection), or explicit user-provided selection. Distinguish “no selection” from “capture unavailable.” Test terminal copy shortcuts and identical-to-existing-clipboard selection as well as browser/editor selection. Do not fix this by introducing a new global input engine.

### 6. P1 — Agent panel placement cannot promise Windows behavior on native Wayland

[agent.rs:664](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L664) builds an ordinary transparent, undecorated, always-on-top window. [set_bounds:817](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L817) has a useful generic position/size fallback; the compact Windows clipping region is already guarded. Thus resizing is not wholly Windows-only.

However, [Tao documents absolute positioning and always-on-top as unsupported on Wayland](https://docs.rs/tao/latest/tao/window/struct.Window.html). Unlike Handy's pill, the Agent panel does not initialize layer shell. Side-card anchoring and center-top anchoring therefore cannot be assumed.

[monitor helpers:735](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/agent.rs#L735) try current/primary monitors only; the main settings window already has an available-monitor fallback. A hidden Agent window on a compositor without a primary monitor can miss placement altogether.

**Required:** choose a supported panel presentation per desktop: ordinary compositor-managed window where exact anchoring is unavailable, or a narrowly scoped Grain layer-shell adaptation on proven compositors. Correct missing-monitor handling. Record geometry/focus failures instead of ignoring them. Test compact/expanded/center modes, work areas, mixed scaling, keyboard focus, pointer hit regions and rapid close/reopen. Preserve existing observer/window/shortcut cleanup.

### 7. P1 — “See my screen” silently sends no image on Linux

Both [context_screen.rs:110](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/context_screen.rs#L110) capture functions return `None` outside Windows. The [Agent setting](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src/app/components/settings/experimentations/AgentSection.tsx#L97) remains available without a platform gate, while Agent quietly continues as text-only.

**Required:** disable/label the setting until implemented, or provide a Linux capture path that preserves the one-window contract. On Wayland, [ScreenCast portals](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html) provide selected-window streams through a user-mediated session; this needs product/consent handling, bounded frame processing, and prompt session teardown. Do not substitute whole-desktop capture or an indefinitely running screen service.

Automatic Agent field/window-text collection was retired in v0.0.8. Restoring those old features is not a Linux release requirement.

### 8. P2 — Paste Catch is exposed but cannot catch Linux misses

[read_focus_probe:737](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/context_detect.rs#L737) returns `None` outside Windows. [paste_catch.rs:340](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/paste_catch.rs#L340) treats missing evidence as unknown and permits normal paste; its postflight cannot confirm a miss either. [OutputPane](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src/app/settings/panes/OutputPane.tsx#L24) offers the enabled setting on Linux.

**Required:** truthful availability, or a Linux accessibility probe preserving the current conservative unknown policy. Do not change the shared paste behavior to work around an inherited platform limitation.

The manual held-text delivery chord also calls Enigo directly ([paste_catch.rs:694](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/paste_catch.rs#L694)). If Linux miss detection is added later, that delivery path needs Linux helper coverage too. Reliable Paste is correctly hidden on Linux today; its upstream platform limitation is not listed as a Grain fix.

### 9. P1 — normal release/publish validation is Windows-specific

[grain-release.yml](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.github/workflows/grain-release.yml#L1) runs on Windows and builds NSIS only. [publish-release.yml](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.github/workflows/publish-release.yml#L71) requires an `.exe` and `.exe.sig`, but does not require Linux packages or Linux updater entries. v0.0.8's actual assets are only the Windows installer, its signature and `latest.json`.

A separate [release.yml matrix](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.github/workflows/release.yml#L55) and reusable Linux build already exist. Grain needs to reconcile these paths; it does not need a new packaging system.

**Required:** decide architectures/formats, build the same audited commit, validate every intended package and updater platform, and preserve existing Windows feed entries. Validate exact payload/signature pairs and normal-user install/restart behavior. Nix already disables self-updating; preserve that.

The locked updater is **2.10.1**, which supports Debian, RPM and AppImage installers; [updater 2.10 release notes](https://tauri.app/release/updater/all-versions/) confirm that support. Do not assume the older “Linux updater is AppImage-only” restriction.

[build.yml:450](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.github/workflows/build.yml#L450) repacks AppImages after the Tauri action, which can already sign/upload release assets. It does not then re-sign/re-upload the rewritten file. **Before using this path for releases, finalize bytes before signing/uploading, or explicitly re-sign and replace all matching assets/feed metadata.** Otherwise local CI artifacts and release assets can differ.

### 10. P1 — Grain's LLM credentials lack Unix permission hardening

[context.rs:355](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/crates/grain-core/src/context.rs#L355) saves provider API keys in `grain.secrets.json`. [write_atomic:371](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/crates/grain-core/src/context.rs#L371) uses ordinary `fs::write` plus rename without private Unix modes.

**Conditional exposure:** under a usual 022 umask, a newly created file is 0644; another local account can read it if parent directories permit traversal. Actual installed-directory permissions were not measured. No credentials were read during this audit.

**Required:** create secret temporary/final files with 0600, harden existing files, and ensure appropriate private data-directory access. Preserve atomic replacement. Cover fresh/upgrade saves and verify with `stat` as a normal user. Grain's [private event-token writer](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/src/events_server.rs#L191) already shows a small Unix-gated pattern. This applies to core LLM credentials independently of excluded extension auth work.

### 11. P1 — Linux CI and acceptance gates cannot currently establish readiness

The [current Rust test run](https://github.com/Punit-Dethe/Grain/actions/runs/37193706981) fails on `cp: cannot stat 'src/managers/transcription_mock.rs'`. [test.yml:31](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/.github/workflows/test.yml#L31) still swaps files at pre-isolation paths and removes a retired dependency. Repair the test strategy around the compiled module layout rather than blindly swapping an obsolete mock.

The workflow path filter covers `src-tauri/**`, not the separately owned `crates/**`. Add meaningful headless core/TDT checks and backend Linux tests. Native source/feature checks already exist, but Linux builds lack an executed installed-runtime gate equivalent to Windows's `--runtime-dir` check.

On this main commit `tests/agent-harness/` is absent. The checked-in [Playwright suite](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/tests/app.spec.ts#L1) only checks a Vite page's HTTP/HTML response; it is not real-Tauri/Linux acceptance.

**Required:** bring the maintained real-app harness onto the release baseline through its normal branch integration, or record manual real-app evidence until that is available. Use isolated profiles, scoped test credentials and owned-process cleanup. Do not add browser-only Tauri replicas. Keep UI design work on `ui/grain-2.0` and obtain user visual approval for any resulting design changes.

### 12. P2 — Nix and source-install documentation need release alignment

The [current Nix check](https://github.com/Punit-Dethe/Grain/actions/runs/37193706963) fails because `.nix/bun.nix` is out of sync with `bun.lock`; flake build/installed-runtime success is therefore not established. Regenerate and verify the dependency manifest if Nix is included.

[Nix modules](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/nix/module.nix#L1) and flake metadata still describe Handy/upstream. Naming aliases can be retained deliberately; documentation should point users at Grain.

[BUILD.md:133](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/BUILD.md#L133) calls Debian-package extraction usable on any distro and refers to `src-tauri/target/release/Grain`. The crate's executable remains `handy`; a bundle/resource copy alone does not establish system-library compatibility. Document actual executable/resource locations, dependencies and tested distro baselines.

## Defaults and inherited Handy constraints

These are acceptance requirements or upstream limitations, not a proposal to patch Handy:

- Linux defaults are Tauri shortcuts, Direct insertion, and **no pill** ([core defaults](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/crates/grain-core/src/settings.rs#L237)). Grain copied the no-overlay policy, but Agent's voice input and follow-up presentation now depend on that pill. Explicitly test fresh-profile Agent operation with None; select a visible owned fallback or mark the feature unavailable when its controls cannot be shown. Changing Handy's default blindly is not the fix.
- The shared pill already initializes GTK layer shell and falls back when unsupported. [gtk-layer-shell supports KDE/wlroots, not GNOME Wayland or X11](https://github.com/wmww/gtk-layer-shell/blob/master/README.md). Treat inherited fallback placement limits as support constraints; do not fork the overlay lifecycle to manufacture parity.
- Ordinary Linux capture, device enumeration, paste-helper detection, global shortcuts, tray integration, GPU selection and allocator behavior largely live in Handy. Test them for release, and record inherited failures for upstream handling. Do not count them as Grain-owned defects without an owned regression.
- Dynamic cancel registration is deliberately disabled on Linux in Handy. Grain hides the ordinary cancel shortcut there, but Agent adds transient Enter/Escape/follow-up registrations. Exercise these explicitly and verify teardown; the upstream caution is evidence to test, not proof those Grain registrations fail.
- Browser accessibility and input helpers differ by compositor, toolkit and sandbox. XWayland success does not establish native Wayland support. Do not advertise universal Linux support from one successful desktop.
- No pending Handy commits were assessed or merged. Ancestry alignment alone does not establish runtime parity; any later sync follows the existing runbook/deferred-port policy.

## What can remain

- Shared overlay window creation/placement/hide machinery, with Grain presentation/events kept separate. The retired native pill sidecar is not part of current rendering; do not port it.
- Existing Standard/Native ASR/Flow routing, bounded PCM journal and TDT assembly. No OS-only logic was found in those owned algorithms. Verify the immutable native fork's Linux build, ABI, model behavior and cancellation; retain current backend features.
- Snippet matching/finalization and dictionary handling. The snippets module is physically in `handy/audio_toolkit/` but explicitly Grain-authored; it was inspected rather than excluded merely because of its directory. It needs integration tests, not Linux-specific matching code.
- Tauri commands/events, path APIs, settings serialization, provider fallback and Agent conversation logic. Windows API usage found in the audited owned areas is generally cfg-guarded; missing Linux implementations and integration guarantees are the larger issue.
- Existing Linux library staging/RPATH and package recipes ([build.rs:7](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/build.rs#L7), [tauri.conf.json](https://github.com/Punit-Dethe/Grain/blob/661f89ab83e806fe07126ffd3ab6bc661990cdaa/src-tauri/tauri.conf.json#L56)). Validate SONAMEs, backend modules and dependency resolution rather than replacing them.

## Release sequence

1. **Restore build evidence:** remove shared Windows target path, repair Rust test paths/strategy and core path filters, get all intended Linux builds green. Fix Nix only if included. Build x86_64 first; ARM64 is a separate acceptance gate.
2. **Choose an explicit feature contract:** either full supported-desktop context/icons/Agent parity, or an honestly scoped beta with unavailable context/vision/Paste Catch and copy-based Agent delivery. A beta must still have visible usable Agent controls if Agent is enabled.
3. **Implement owned adapters only:** share desktop identity across catalogue/context/icons; use bounded accessibility/capture calls, current caches and existing input infrastructure. No permanent polling service, new capture engine, or modifications to Handy to bypass desktop restrictions.
4. **Validate real Linux packages:** install onto clean systems without developer toolchains; verify the fork runtime contract, CPU fallback, resource lookup, Vulkan/CPU selection and all advertised features.
5. **Finish release/signing:** finalize package bytes, sign, validate format-specific feed entries, install/update/relaunch from a normal account, then publish only after acceptance.

A practical first candidate is **x86_64 with a tested X11 desktop**, plus explicit package/distro support. Add native Wayland desktops only after their acceptance rows pass. This is a recommendation, not evidence of a working X11 release today.

## Acceptance checklist

| Test group           | Evidence required                                                                                                                                                                            |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop matrix       | X11; KDE Wayland; GNOME Wayland; wlroots/Sway where advertised. Label native Wayland vs XWayland, desktop version, package and hardware                                                      |
| Install/runtime      | Clean .deb/.rpm/AppImage installs as applicable; resources/tray icons/audio feedback found; `ldd` resolves core libs; dlopen backend discovery and exact native ABI pass                     |
| Audio/capture        | Onboarding mic test; missing/disconnected mic; first sample; PTT/toggle; Standard, Native ASR and Flow; translation gate; cancel during startup and finalization                             |
| Delivery             | Browser, editor, terminal; Direct and supported paste chords; Unicode/multiline; helper missing; clipboard restore; optional auto-submit; history retains transcript on failure              |
| Snippets/dictionary  | Enabled/disabled; punctuation and longest trigger; Unicode; multiline expansions; Standard/Flow/Native ASR/Agent; repeated stop does not expand twice                                        |
| Context/icons        | Correct supported app/desktop identity and site; denied/missing accessibility; stale browser tabs; password exclusions; empty/unsupported targets; switch mid-session; generic icon fallback |
| Pill                 | None/Compact/Live; top/bottom; no focus stealing; real WebKitGTK layout/fonts/masks/scrolling; fullscreen; mixed DPI; clean session listeners/timers                                         |
| Agent                | Fresh Linux defaults; summon/selection; conversation/side/center; input focus; original target switch/closure; no accidental paste; supported screen capture consent/cancel                  |
| Lifecycle/RAM        | Repeated start/stop/cancel; close/reopen settings/Agent; Enter/Escape released; capture/session resources dropped; no sustained screen/input polling; measured idle/peak/recovered RAM       |
| Persistence/security | Restart/save/migration; settings survive; provider credentials 0600 including temporary files; supported data-directory access; update permissions/normal-user failure recovery              |
| Release/update       | Exact finalized assets verify against signatures; all supported formats/architectures in feed; Windows entries retained; Linux install/restart works; Nix self-update stays disabled         |

Use real Tauri builds, not Vite-only screenshots. Run these manual launch/build commands in a disposable Linux test account, after installing the documented system dependencies and choosing a normal Linux target directory:

```bash
export CARGO_TARGET_DIR="$PWD/src-tauri/target"
rtk bun install --frozen-lockfile --ignore-scripts
rtk cargo test --locked -p grain-core -p grain-tdt
rtk bun run dev:asr
rtk bun run dev:onboarding
rtk bun run build:asr --bundles deb
rtk bun run build:asr --bundles appimage,rpm
rtk python scripts/transcribe_fork.py --runtime-dir src-tauri/transcribe-libs
```

Run the two dev commands separately (close the first owned test process before opening the next). These commands are manual launches, not an isolated acceptance runner. Keep real-user profiles/credentials out of test runs; use the maintained isolated-profile runner once present. Use the underlying command if RTK is unavailable. `--ignore-scripts` here avoids regenerating tracked Nix files during a candidate build; handle Nix regeneration as an intentional packaging change. The target-directory override is a diagnostic workaround, not a substitute for fixing finding 1. Runtime-dir inspection is necessary but does not replace testing the installed binaries/packages.
