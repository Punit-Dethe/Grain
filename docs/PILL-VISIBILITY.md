# Windows pill visibility investigation — 2026-09-15

## Findings

- The affected running instance was the installed
  `%LOCALAPPDATA%/Grain/handy.exe`, with one child of that executable running
  `--pill`. It was not a dev executable or an orphaned standalone pill.
- The September 15 application log records `RecordingStarted`, followed by
  `window: show (content primed)`, for the recent dictation sessions. The overlay
  anchor was `Bottom`. These entries confirm event delivery and execution of the
  reveal path, but do not establish successful composition or actual visibility.
- `crates/grain-pill/src/lib.rs` requested `AlwaysOnTop` at window creation and
  subsequently used only `ShowWindow(SW_SHOWNOACTIVATE)` to reveal it. There was
  no explicit native topmost reassertion on later reveals. This path already
  exists in v0.0.1; the inspected history does not identify v0.0.3 or v0.0.4 as
  introducing it. Competing topmost windows are a plausible explanation for
  occlusion, not a reproduced cause of every reported disappearance.
- The fade lifecycle also allowed an in-progress close to reach its hide
  threshold after visibility had been requested again. This predates v0.0.3.
  It can cause a transient hide/reopen; the recent plain dictation log does not
  establish it as the cause of this report.

## Changes

Each reveal now explicitly calls `SetWindowPos(HWND_TOPMOST)` with
`SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW`. It also does so when
a recording takes over an already-visible preview/offer. Failures are logged.
The existing frame loop reverses a closing fade when visibility is renewed.
There are no new timers, threads, listeners, or per-frame Windows calls.

Windows semantics: [ShowWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow)
and [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos).

## Verification

- `cargo test -p grain-pill --lib -- --skip png`: 47 passed. This includes
  interruption just before the hide threshold and successful closing after a
  renewed request ends. Seven image-export tests were excluded.
- `cargo fmt -p grain-pill -- --check`: passed.
- `bun run tauri build --debug --no-bundle`: passed, including TypeScript and
  the Vite production build. Built application: `C:\gt\debug\handy.exe`.
- Strict Clippy is blocked by existing lints in unchanged code (including
  excessive precision, too many arguments, and constant assertions).
- Real-app visual verification is pending. Test repeated recordings, switching
  applications between invocations, the affected game, and a quick restart
  during a Studio/follow-up fade. Confirm both visibility and that typing/paste
  retains focus in the target application.

With the installed app closed, run `& 'C:\gt\debug\handy.exe'` in PowerShell
to test the completed build. Alternatively, from `C:\Projects\Grain\grain`,
run `bun run dev:asr`. Restarting the installed app does not apply these changes.
This patch does not guarantee overlays over exclusive fullscreen content or
windows raised after the pill is revealed.
