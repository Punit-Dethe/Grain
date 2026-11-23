# Build Instructions

This guide covers how to set up the development environment and build Grain from source across different platforms.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- Python 3.11+ for the native dependency/upstream verification scripts (or use
  `uv run --python 3.12` to run them with a managed interpreter).
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-Specific Requirements

#### macOS

- Xcode Command Line Tools
- Install with: `xcode-select --install`

Both Intel and Apple Silicon builds use the pinned transcribe.cpp source.
The build script weak-links CoreAudio and supplies clang's availability runtime
for older macOS deployment targets. ONNX Runtime is no longer required.

#### Windows

- Microsoft C++ Build Tools: Visual Studio 2019/2022 with C++ development
  tools, or Visual Studio Build Tools 2019/2022
- [CMake](https://cmake.org/download/) (must be on `PATH`):

  ```powershell
  winget install Kitware.CMake
  ```

- [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) from LunarG — required to
  build the Vulkan GPU backend (`vulkan-shaders-gen` needs the SDK's headers
  and `glslc`):

  ```powershell
  winget install KhronosGroup.VulkanSDK
  ```

  Open a new terminal afterward so `VULKAN_SDK` is set.

> [!IMPORTANT]
> Windows' 260-character path limit **will** break the native Vulkan build in
> most checkouts (the shader generator nests very deep). Before your first
> build, do BOTH steps in
> [Windows build fails with path-limit errors](#windows-build-fails-with-path-limit-errors-msb3491--ftk1011--msb6003)
> in Troubleshooting — enable long paths **and** set a short
> `CARGO_TARGET_DIR`. Neither alone is sufficient.

#### Linux

Grain keeps the WebKitGTK compositor on shared memory for WebKitGTK 2.54 and
newer so transparent overlays render fully. Older versions retain the DMA-BUF
disable workaround. Explicit `WEBKIT_DISABLE_DMABUF_RENDERER` or
`WEBKIT_DMABUF_RENDERER_FORCE_SHM` settings override this policy.

- Build essentials
- ALSA development libraries
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential cmake pkg-config patchelf libasound2-dev \
    libssl-dev libvulkan-dev vulkan-tools glslc libopenblas-dev libgtk-3-dev \
    libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libgtk-layer-shell-dev \
    libx11-dev libxtst-dev libxrandr-dev xdg-utils

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel openblas-devel \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel alsa-lib pkgconf openssl vulkan-devel openblas \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake
  ```

## Setup Instructions

### Windows backend tests

Run the real Tauri lib tests through the maintained Cargo runner. It supplies
Common Controls v6 from an isolated executable copy and cleans that copy after
each invocation. Cargo's cached binary is unchanged. From the repository root:

```powershell
$grainRunnerPath = (Resolve-Path scripts/run-rust-test.ps1).Path
$grainRunnerArgs = ConvertTo-Json -Compress -InputObject @('powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $grainRunnerPath)
$grainRunnerConfig = "target.x86_64-pc-windows-msvc.runner=$grainRunnerArgs"
Push-Location src-tauri
try {
    cargo test --locked --lib --config $grainRunnerConfig
} finally {
    Pop-Location
}
```

The array configuration preserves paths containing spaces. Append a test filter
or `-- --test-threads=1` as needed. Runner regressions, including failure exit
codes and concurrent cleanup, run with:

```powershell
py -m unittest discover -s scripts -p test_run_rust_test.py -v
```

### 1. Clone the Repository

```bash
git clone git@github.com:Punit-Dethe/Grain.git
cd Grain
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Start Dev Server

```bash
bun tauri dev
```

On Linux, start and stop dictation with `Ctrl+Alt+Space`. For an existing
profile with overlays disabled, select **Live** in the application settings.
On a desktop that cannot register Grain's shortcut, bind the installed
`handy --toggle-transcription` command in the desktop's keyboard settings.
Launch Grain before using that command.

### 4. Build for Production

```bash
bun run tauri build
```

This compiles a release binary and generates platform-specific bundles (deb, rpm, AppImage on Linux; dmg on macOS; msi on Windows).

### Reopen onboarding during development

Quit any running Grain instance, then run from the repository root:

```powershell
bun run dev:onboarding
```

This opens the real Tauri application at the first onboarding step, even if
models are already installed or the app normally starts hidden. It applies only
to this debug launch; it does not reset settings or delete models. Choices you
make during setup use the normal model and shortcut settings. Installed models
can be reused without downloading them again. Launch normally with
`bun run dev:asr` afterward. Release builds ignore the onboarding override.

## Linux Install (from source)

On Ubuntu or Debian, build and install the actual package with its resources and
Recall MCP helper. Run these commands from the repository root after installing
the prerequisites above. This local build skips updater signing.

```bash
bun install --frozen-lockfile
cargo build --locked --release -p grain-recall-mcp --target-dir target
install -Dm755 target/release/grain-recall-mcp \
  "src-tauri/helpers/grain-recall-mcp-$(rustc --print host-tuple)"
CARGO_TARGET_DIR="$PWD/src-tauri/target" bun run tauri build --bundles deb \
  --config '{"bundle":{"createUpdaterArtifacts":false,"externalBin":["helpers/grain-recall-mcp"]}}'
version=$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
sudo apt install "./src-tauri/target/release/bundle/deb/Grain_${version}_$(dpkg --print-architecture).deb"
```

Launch **Grain** from the desktop menu. For a quicker visual pass, launch the
real development application from the repository root:

```bash
bun run dev:asr
```

## Linux desktop acceptance

Record the distribution, desktop, and `XDG_SESSION_TYPE` with any failure.
Run the checklist in your normal session first. If your login screen offers
both X11 and Wayland, repeat the shortcut, paste, overlay, and Agent checks in
both sessions.

| Check | What to try |
| --- | --- |
| Install and launch | Open Grain from the launcher, choose a microphone and model, and check that the app reopens from the launcher after closing its window. Try start-hidden if your desktop has a tray and if it lacks one. Run `handy --list-devices` on the installed build and confirm a CPU device is listed. |
| Dictation and quick stop | Use `Ctrl+Alt+Space` to start and stop. Speak a full sentence in Standard and again with a Flow-capable model. Try a brief tap, a held shortcut, and two recordings in a row. Check that the pill clears after each run, the text reaches the editor, and history has the transcript. If the shortcut does not register on Wayland, bind `handy --toggle-transcription` as a desktop shortcut and repeat with Grain running. |
| Recording overlay | Select **Live** in Application settings. Check recording, processing, and any live preview on a streaming model. Verify that typing focus stays in the editor. The pill can show the active app icon on X11 when the app provides one. Expect the status dot on Wayland. |
| Copy recovery | In Output settings, select **External script** and point it at an executable shell script containing `exit 1`. Dictate and stop. Check that **Copy to clipboard?** appears. Confirm that Copy puts the transcript on the clipboard, and that declining or letting the next offer expire preserves the prior clipboard. Restore your usual paste method afterward. |
| Dictionary | Enable **Learn from edits** in Tools. Dictate into an accessible text field, then replace one word and pause. Verify the confirmation appears and that saving adds the corrected word. Repeat with another word and decline. |
| Agent | Open Agent from AI settings and send a typed message. Test its shortcut, voice capture, expansion, dismissal, and reopening. Summon it with text selected in an accessible editor, then try from a terminal running `sleep 30` and verify the terminal command continues. |
| Recall and MCP | Record a short microphone-only meeting, stop, play it back, and reopen it. Turn on Recall access for an AI client using the installed package. Restart the client and test reading the meeting while Grain is closed, then turn access off and confirm the client cannot read it. For hosted MCP sign-in, test cancel, connection, and restart with the desktop credential store unlocked. |
| Other desktop paths | Import a short audio file, open the resulting history entry, and test export. Switch input or output devices, change display scaling if available, and try launching after login with autostart enabled. |

To create the executable for the copy recovery check, run this and paste the
printed path into the external script setting:

```bash
paste_failure=$(mktemp)
printf '#!/bin/sh\nexit 1\n' > "$paste_failure"
chmod +x "$paste_failure"
printf '%s\n' "$paste_failure"
```

For AppImage, also test the extracted-and-run path if you use it and confirm
Recall still works after the original Grain window closes. Debian, RPM, and Nix
installs get updates from their installation source. Test-build artifacts can be
generated with **Build Test** → **linux-only** once the branch is on GitHub.

## Troubleshooting

### macOS Accessibility remains enabled after a local rebuild

Local builds use ad-hoc signing. A rebuild can acquire a new code identity while
the old **System Settings > Privacy & Security > Accessibility** entry remains
visibly enabled, leaving Grain unable to receive global input.

After installing the final bundle at `/Applications/Grain.app`, quit Grain,
clear only its stale Accessibility record, and reopen it:

```bash
osascript -e 'tell application id "com.grain.app" to quit' || true
tccutil reset Accessibility com.grain.app
open /Applications/Grain.app
```

Grant Accessibility again when prompted. This does not reset Microphone or
other TCC services. For optional diagnosis, compare the designated requirement
of the previous and rebuilt bundles with `codesign -dr - <app> 2>&1`.

### AppImage build fails on Arch / rolling-release distros

`linuxdeploy` bundles its own `strip` binary which is too old to process system libraries built with newer toolchains on rolling-release distros (Arch, CachyOS, Manjaro, EndeavourOS).

The error from Tauri:

```
Bundling Grain_*_amd64.AppImage
failed to bundle project `failed to run linuxdeploy`
```

Tauri swallows the real linuxdeploy error. To see it, run linuxdeploy manually:

```bash
cd src-tauri/target/release/bundle/appimage
~/.cache/tauri/linuxdeploy-x86_64.AppImage --appimage-extract-and-run \
  --appdir Grain.AppDir --plugin gtk --output appimage
```

**Workaround:** The binary, deb, and rpm bundles all build fine — only the AppImage step fails. To skip it:

```bash
bun run tauri build -- --bundles deb
```

Then install using the deb extraction method above.

### Windows build fails with path-limit errors (`MSB3491` / `FTK1011` / `MSB6003`)

On Windows the native build can fail partway through `transcribe-cpp-sys` with
any of these (all the same root cause):

```
error MSB3491: Could not write lines to file "...VCTargetsPath.tlog\VCTargetsPath.lastbuildstate".
Path: ... exceeds the OS max path limit. The fully qualified file name must be less than 260 characters.
```

```
FileTracker : error FTK1011: could not create the new file tracking log file:
...\vulkan-shaders-gen-build\...\cmTC_xxxxx.tlog\link.write.1.tlog.
The system cannot find the path specified.
```

```
error MSB6003: The specified task executable "CL.exe" could not be run.
System.IO.DirectoryNotFoundException: Could not find a part of the path ...
```

This is **not** a code or toolchain problem — it's Windows' legacy 260-character
path limit (`MAX_PATH`). The Vulkan shader generator builds as a nested CMake
sub-project (`...\vulkan-shaders-gen-prefix\src\vulkan-shaders-gen-build\...`),
which alone adds ~140 characters on top of Cargo's already-deep
`target\release\build\<crate>-<hash>\out\build\...` directory. If your checkout
isn't very shallow, the build overflows the limit. (CI doesn't hit this because
it builds from a short root such as `D:\a\Handy`.)

**You need BOTH of the following fixes.** Different parts of the toolchain hit
the limit in different ways: MSBuild's native `FileTracker` (`tracker.exe`)
ignores the long-paths flag entirely, so a short target dir is required for it —
while other .NET-side path operations overflow even from a short root unless
long paths are enabled. Each fix alone still fails.

**1. Enable Windows long paths** (one-time, machine-wide; needs an
Administrator PowerShell):

```powershell
Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -Value 1 -Type DWord
git config --global core.longpaths true
```

**2. Build with a short target directory:**

```powershell
# Per-shell:
$env:CARGO_TARGET_DIR = "C:\h"

# Or persist it for all future terminals (note: redirects ALL your
# Rust projects' build output, not just Handy):
[Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', 'C:\h', 'User')
```

Artifacts then land in `C:\h\release\...` instead of the repo's
`src-tauri\target\`.

Keep machine-specific target paths in your local environment or ignored Cargo
configuration. Shared checkouts use Cargo's platform-native default; an absolute
Windows target path must not be committed into the shared configuration.

Open a **new terminal** afterward — both the registry flag and the persisted
environment variable are only picked up by freshly started processes. Then
`bun run tauri dev` and `bun run tauri build` work normally.

### Windows `tauri build` fails at bundling with `program not found`

If the build compiles all the way to `Built application at: ...\handy.exe` and
then fails with:

```
Signing C:\...\handy.exe with a custom signing command
failed to bundle project `program not found`
```

that's the code-signing step: `tauri.conf.json` configures a custom
`signCommand` (`trusted-signing-cli`, Azure Trusted Signing) that only exists
in the release CI environment. Local development doesn't need it:

```powershell
# Development (no bundling/signing at all):
bun run tauri dev

# Or compile a release binary without the installer/signing step:
bun run tauri build --no-bundle
```
