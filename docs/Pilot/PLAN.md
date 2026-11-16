# Grain Pilot: plan

Status: **plan only, nothing built.** Written 2026-11-16. Name confirmed by the
user 2026-11-16: **Grain Pilot** ("Pilot" for short in this document).

Pilot lets the Agent act in the user's own apps and browser: open a page in the
browser they already use (signed in as them), read it, click, type, and hand
the window to the user at sign-in or payment. It is the second and third rung
of the action ladder:

1. Connectors (MCP extensions): already shipped, always preferred.
2. The user's browser: Pilot.
3. Any other app window: Pilot, same tools.

Grain does not build the engine. Pilot drives
[Cua Driver](https://github.com/trycua/cua) (MIT, Rust), the engine HeyClicky
and OpenClicky use. Grain owns a thin, fixed mapping from Agent tools to driver
calls, plus the part that keeps page reads small. No overlays, no agent cursor.

## What was measured (2026-11-15, Windows, driver 0.52.0)

- Drove the user's real Zen in a new background window: page read ~0.1 s,
  click ~0.1 s, no focus taken, BookMyShow showtimes → seat page worked.
  (Phase 0 later found Zen syncs that window's tabs into the user's main
  window and can take focus; see Phase 0 results.)
- A raw page read is large (showtimes page ~16k tokens). The same data filtered
  to controls only is ~3.7k (Agent Browser: ~4.0k); seat page 0.26k; Zomato
  booking page 1.0k. A `find` read is ~0.5k.
- The driver's own tool list is ~45k tokens. Pilot never sends it; the model
  sees only Pilot's short list.
- A click returns before the page changes, so an immediate re-read says
  "no change". Pilot must wait for the page to settle (see Adapter).
- Not measured: sign-in, OTP, payment, Chromium browsers, driver RAM.

## Decisions

- **Name / id (confirmed):** display name "Grain Pilot", extension id
  `grain.pilot`, Extensions page id `pilot`, setting `pilot_enabled` (default
  off).
- **Agent Browser:** left as it is, off by default, until Pilot passes Phase 3.
  Then removed in its own commit. Reach stays removed; Pilot replaces it.
- **Branch:** `feat/pilot`, cut from `feat/agent-browser` because it needs the
  compiled-provider registry (`grain_compiled_tools.rs`) and `links.rs`.
- **Sidecar binary, not linked in.** The driver crate is not on crates.io, so
  Pilot runs the released `cua-driver` executable. In-process embedding (the
  driver ships a C ABI) is a later option, not this round.
- **Grain-managed install, one-click Connect.** Connect downloads the pinned
  release asset for this OS (~30 MB), checks it against a SHA-256 compiled
  into Grain, and unpacks it into `<app data>/pilot/driver/<version>/`. The
  driver version changes only with a Grain release; its self-update is never
  called. Disconnect deletes the folder.
- **Windows first.** macOS and Linux report "not available on this OS yet"
  (macOS needs Accessibility and Screen Recording grants and a different
  daemon form).
- **Text only this round.** Tool results in Grain carry text, so Pilot never
  asks the driver for screenshots. A drawn surface (a seat map) is a hand-off:
  `show` the window and let the user do that step.
- **No per-call confirmations**, matching the current Agent policy. Read-only
  tools are `Safe`, the rest `Confirm`, so a later policy has the data.

## Layout

| Piece | Where |
| --- | --- |
| Ids, pinned driver version and per-OS SHA-256, limits | `crates/grain-core/src/pilot/mod.rs` |
| Tool schemas and argument parsing into a `Call` | `crates/grain-core/src/pilot/tools.rs` |
| `Call` → driver tool name and JSON arguments | `crates/grain-core/src/pilot/request.rs` |
| Page-read compaction (the token work) | `crates/grain-core/src/pilot/compact.rs` |
| Driver reply → model text; refusals → plain messages | `crates/grain-core/src/pilot/render.rs` |
| Safety rules (links, keys, text limits, refused targets) | `crates/grain-core/src/pilot/policy.rs` |
| Capability manifest text (the driver-side tool ceiling) | `crates/grain-core/src/pilot/manifest.rs` |
| Download, verify, unpack, remove | `src-tauri/src/grain_pilot_install.rs` |
| Daemon lifetime, one call, settle-read, window handles | `src-tauri/src/grain_pilot.rs` |
| Registry arms (enabled, directory, catalogue, digest, call) | `src-tauri/src/grain_compiled_tools.rs` |
| Extensions page row | `grain_mcp.rs`, `McpPresets.tsx`, `icons/pilot.svg` |
| Setting + updater + command | `grain-core settings.rs`, `settingsStore.ts`, `lib.rs` |

Everything in `grain-core/src/pilot/` is pure and unit-tested; no process or
network code lives there. `grain_mcp.rs` currently special-cases
`AGENT_BROWSER_ID` in six places; before adding Pilot, those become one small
table of no-account built-ins (id, install check, on-disconnect, setting), so a
second built-in adds a row, not six more branches.

## Tools (model-facing)

Eleven tools with short Grain-written descriptions. Budget for the whole list:
1.5k tokens.

| Tool | Does | Returns |
| --- | --- | --- |
| `open` | Open an `http(s)` link in a new background window of the default browser (or a vetted app link through the OS) | window handle + compact read |
| `open_app` | Launch or find an installed app, in the background | window handle |
| `windows` | List open windows | handle, app, title per row |
| `read` | Read a window. `find` narrows to matching rows; `detail: "page"` adds text | compact read |
| `click` | Click a numbered control | what changed |
| `type` | Put text in a numbered field (`replace` or append) | what changed |
| `press` | Press one allow-listed key in a window | what changed |
| `scroll` | Scroll a window | what changed |
| `choose` | Pick an option in a numbered drop-down by its text (added after Phase 0) | what changed |
| `steps` | Several `click` / `type` / `press` in one call, stopping at the first failure | one "what changed" |
| `show` | Bring the window to the front for the user (`sign_in`, `user_action`, `view`) | instruction to stop and wait |
| `close` | Close a window Pilot opened | confirmation |

The model never sees driver internals. Windows are `w1`, `w2`…; controls are
the bracketed numbers from the latest read. Grain keeps the mapping to the
driver's pid, window id and snapshot id, so the model cannot invent a handle.
A number from an older read is retried once by role and name, otherwise the
reply is a fresh read.

Not exposed, and denied in the manifest: screenshots, clipboard, scripts
(`run_script`), killing apps, downloads, file inputs, recording, cursor tools,
and the driver's browser-attach tools (they flip the browser's remote-debugging
switch).

## Keeping reads small

| Rule | This round |
| --- | --- |
| 1. Short fixed tool list instead of the driver's 62 | In |
| 2. No screenshots | In (always off) |
| 3. Compaction of every read | In |
| 4. Read only what is needed: `find`, and "what changed" after an action | In |
| 5a. Fewer rounds: `steps`, and `open`/`click` return their own read | In |
| 5b. Skipping a model round when the page already proves the step | Out (not shipped upstream either) |
| 6. Remembered recipes | Out |
| 7. Shrinking older reads already in the conversation | Stretch (Phase 4), only if the Agent loop has a clean seam |

Compaction (`compact.rs`), applied in order:

1. For a browser window, drop everything before the page's document node (the
   browser's own toolbars and menus: ~1.8k tokens per read, and it can name
   the user's tabs and bookmarks).
2. Strip link addresses, action lists and element ids from each row.
3. Drop container rows that only repeat their children's text.
4. Merge siblings that share one name into the single actionable one.
5. Default view keeps controls only (buttons, links, fields, options);
   `detail: "page"` keeps text rows too.
6. Flatten indentation to a small fixed depth.
7. Cap the result (4k tokens) and end with how many rows were left out and a
   hint to use `find`.

Targets, checked by golden tests on three saved reads (BookMyShow showtimes,
BookMyShow seat page, Zomato booking), with the browser's own rows removed from
the fixtures before they are committed:

| Read | Raw | Target |
| --- | --- | --- |
| Showtimes, controls | 16k | ≤ 4k |
| Seat page, controls | 2.6k | ≤ 0.4k |
| Zomato, controls | 8.3k | ≤ 1.2k |
| `find` on any of them | n/a | ≤ 0.6k |

Each test also asserts the control the booking needs survives
("Book 11:20 AM", "Select Seats").

## Adapter (`grain_pilot.rs`)

- **Daemon.** Started on the first call of an Agent run:
  `cua-driver serve --socket <private pipe> --permission-mode bounded
  --capability-manifest <app data>/pilot/manifest.yaml
  --approve-capability-manifest --no-overlay`. The pipe name is Grain's own, so
  it never collides with a driver the user installed themselves.
- **Calls.** One short `cua-driver call <tool> --socket <pipe>` process per
  call, JSON on stdin, stdout to a per-call temp file, bounded by a timeout
  (the pattern Agent Browser already uses). Every call carries
  `session: "grain"` so read tokens survive between processes, and
  `include_screenshot: false`.
- **Environment on every spawn:** `CUA_DRIVER_RS_TELEMETRY_ENABLED=false`,
  `DO_NOT_TRACK=1`, `CUA_DRIVER_DISABLE_RUN_SCRIPT=1`,
  `CUA_DRIVER_RS_UPDATE_CHECK=false`, `CREATE_NO_WINDOW`.
- **Settle-read.** After an action Pilot re-reads with the driver's diff
  (`since`) at short intervals until two reads agree or 3 s pass, then returns
  that one diff. Bounded, no background thread.
- **Lifetime.** Grain keeps no thread or socket between calls. The daemon is
  stopped when the Agent run that started it ends, on switch-off, on
  disconnect and at exit, and is placed in a Windows job object so it dies
  with Grain even after a crash. The window-handle map is cleared with it.
- **One call at a time**, behind a gate, as in Agent Browser.

## Safety rules (enforced in `policy.rs` and the adapter)

- `open` takes links that pass `links::validate_link`; nothing else reaches the
  browser or the OS opener.
- No typing or key presses into password fields, terminals or Grain's own
  windows (the rules Reach had). `press` uses a closed key list.
- `close` works only on windows Pilot opened.
- Background by default. Pilot never brings a window forward on its own; if the
  driver says background input is unavailable, the reply tells the Agent to
  `show` the window first.
- The provider description tells the Agent to stop before paying, sending,
  submitting or deleting, `show` the window, and wait for the user.
- Text read from a window is clipped and marked as untrusted content.
- Lengths are bounded and control characters refused in every argument.

## Phases

**Phase 0: verify before building** (cheap to run; each item changes the
design if it fails)

1. Capability manifest: exact file format, and that bounded mode allows the
   eleven mapped driver tools and refuses the rest.
2. Private pipe + `call` with a named session works as measured on the default
   pipe.
3. Password fields are identifiable from a read (else reuse Grain's existing
   focus probe, as Reach did).
4. Edge and Brave: does a background click on page content work, or return
   `background_unavailable`?
5. Daemon RAM at rest and after a large read; start-up time.
6. The config key that turns the update check off.
7. A real booking in Zen, signed in, up to the payment page.

### Phase 0 results (2026-11-16, driver 0.52.0, bounded mode, private pipe)

| # | Check | Result |
| --- | --- | --- |
| 1 | Manifest | Works. Format below. |
| 2 | Private pipe + session | Works. Ready for the first call ~0.6 s after start. |
| 3 | Password fields | Driver gives no flag. Windows UIA `IsPassword` does (Zen and Edge, 18–77 ms). |
| 4 | Edge / Brave | Background **click** works on on-screen controls. Background **typing does not**. |
| 5 | RAM / speed | 53–65 MB working set, 17–27 MB private; +2 MB after a 19k-character read. Read 0.45–0.6 s, click 0.4–0.6 s, `launch_app` ~1.8 s. |
| 6 | Update check | `CUA_DRIVER_RS_UPDATE_CHECK=false` (else it calls GitHub on every daemon start). |
| 7 | Zen booking | **Reached the last gate before payment** (details below), entirely in a parked unsynced window, the user's window in front throughout except ~1–3 s per drop-down. Stopped at BookMyShow's Terms & Conditions box (accepting terms needs the user's yes). |

**Manifest format** (YAML, version 3; a byte-order mark fails the parse;
unknown keys are rejected):

```yaml
version: 3
expires_after: 2h        # bounded mode requires both; max 24h
idle_timeout: 15m
resources:
  apps:                  # exact absolute paths; packaged apps use bundle_id (AUMID)
    - executable: 'C:\Program Files\Zen Browser\zen.exe'
      launch: true
      windows: all
      terminate: deny
allow:
  tools: [launch_app, list_windows, get_window_state, click, set_value,
          type_text, press_key, hotkey, scroll, run_steps, bring_to_front,
          start_session, end_session]
deny:
  tools: [run_script, kill_app, clipboard_read, clipboard_write,
          get_desktop_state, zoom, start_recording, browser_prepare,
          browser_download, browser_set_input_files, set_config,
          install_extension, move_cursor]
```

Bounded mode scopes **apps**, not just tools: reading, input and `launch_app`
work only on processes whose executable is listed. `list_windows` /
`list_apps` without a `pid` count as watching the whole screen and are refused
(that would need `desktop.display: true`, which also unlocks screen-wide input,
so Pilot never sets it). `list_windows` with a listed app's `pid` works.

**What this changes in the design**

- **Manifest is written per daemon start** from the default browser's
  executable (registry `UserChoice` → path) plus the apps Pilot may open.
  `open_app` for an app not in the manifest restarts the daemon with it added
  (~0.6 s).
- **Finding Pilot's window.** `launch_app` returns the launcher's pid, which
  hands off to the running browser and exits, and its `windows` list comes back
  empty. Grain finds the browser's main pid itself, then compares
  `list_windows(pid)` before and after the launch to find the new window.
- **`windows` tool** lists only windows of manifest apps (Grain-side, through
  `list_windows(pid)`), never the whole desktop.
- **Numbers.** Every read replaces the window's previous snapshot; a token from
  any older read is refused (`stale_element_token`), even if nothing changed.
  Row numbers stay put while the page is unchanged and shift after it
  changes. A `find` read still lets Pilot click a row it did not show (the
  token uses the latest snapshot id). So Grain keeps the latest snapshot id
  per window plus the role and name behind each number the model saw, and
  resolves a number against the latest snapshot by role and name.
- **Privacy:** the browser's own rows list **every open tab title** (here,
  one included the user's email address). Compaction step 1 is a privacy rule,
  not only a token saving, and gets its own test.
- **Password check is Grain's own.** Before `type` into a control, Grain asks
  Windows UIA for `IsPassword` descendants of that window and refuses if one
  has the control's bounds (read with `tree_format: "elements"`, whose
  `frame` matched UIA's rectangle exactly). The site's label or `id` is never
  trusted.
- **Chromium (Edge, Brave):**
  - The first read can come back nearly empty (reading is what switches on
    Chromium's page accessibility). Re-read once when the document has almost
    no rows.
  - Clicks are sent as synthetic events at the control's on-screen point. A
    control below the fold is "sent" and silently missed. Scrolling a given
    control into view is a no-op on Windows, so Pilot scrolls the window
    toward an off-screen target, re-reads, and only then clicks (bounded
    tries).
  - Background `type_text` and `press_key` return `background_unavailable`.
    `set_value` on a web field reports success but leaves it empty (checked
    through UIA). **Typing into Chromium pages needs the window in front.**
    Foreground typing could not be checked this round (the window was
    minimised mid-test while the user was at the desk).
- **Firefox family (Zen):** clicks go through UIA Invoke and work off-screen.
  `set_value` + Enter on the address bar works in the background. Typing into
  a web field in the background has not been checked yet.
- **Closing.** Background Ctrl+W does not reach Zen (key chords sent this way
  are ignored), and Zen's tabs expose no close button. Closing a whole window
  through its title-bar Close button (UIA Invoke) works in the background. On
  Chromium, closing a window with several tabs opens a "Close all tabs?"
  dialog that blocks it. So `close` closes **windows** Pilot opened, and only
  when the window holds only Pilot's tabs.
- **Driver state outside Grain's folder:** `~/.cua-driver/` (update cache) and
  `%LOCALAPPDATA%\cua-driver\` (pid file, pipe locks). Disconnect removes them
  only if Grain created them.

**Zen tab sync.** Zen 1.24 syncs tabs across windows by default
(`zen.window-sync.enabled`). A plain `--new-window` is not separate: its tabs
appear in the user's main window, the main window's active tab followed a tab
Pilot opened, and Zen brought the new window to the front (Edge and Brave did
not). Test tabs were closed afterwards and the user's 9 tabs were checked
unchanged.

**Decision (user, 2026-11-16): Zen work happens in an unsynced window.**
Checked the same day:

- `zen.exe --blank-window <url>` (Zen's own command-line flag, read from its
  `BrowserContentHandler`) opens an **unsynced** window with only that tab.
  The user's windows kept their 9 tabs and their active tab. The unsynced
  window's read lists only Pilot's tab, so no other tab titles reach the
  model. A second `--blank-window` opens a second separate window.
- Zen brings the new window to the front every time. `launch_app`'s
  `start_minimized` breaks this launch (no window opens), so Grain fixes focus
  itself, once, as soon as the window appears (~2 s after launch):
  1. Minimise it (title-bar Minimise through UIA Invoke, or
     `ShowWindow(SW_MINIMIZE)`). Focus goes back to the user's window.
  2. Restore it without activating and park it just past the right edge of
     the virtual screen: `SetWindowPos(HWND_BOTTOM, right + 50, 0,
     SWP_NOSIZE | SWP_NOACTIVATE)`. Do not use `SetWindowPlacement`; Windows
     snaps that back onto a monitor.
  The window is visible for about 0.5–2 s before step 1.
- A minimised window can be read but not clicked (`window_minimized`), which
  is why step 2 exists.
- In the parked window, all of these worked in the background with the user's
  window kept in front: reads, clicks (UIA Invoke; a link and a submit
  button), typing (`type_text` with an element token; the value appears a
  moment after the call returns, so the settle-read must wait for it), and the
  UIA password check (its rectangle matched the driver's off-screen frame).
- `show` moves the window back on screen and brings it to the front.
- `close` (title-bar Close through UIA Invoke) closed the parked windows
  cleanly, after checking each one held only Pilot's tab.
- Zen restores unsynced windows after a restart
  (`zen.session-store.restore-unsynced-windows`), so a window Pilot leaves
  open can come back after a Zen restart.
- Still to check, in item 7: an unsynced window opened from the command line
  uses the default container. Sites the user signed in to only inside a Zen
  space container may not be signed in there.

**Item 7: the booking run (2026-11-16, Zen, BookMyShow, Cinepolis Viviana).**

| Step | What happened |
| --- | --- |
| `open` showtimes page | Parked unsynced window; loaded in ~5 s; 605 elements raw. Each show is a `Button "Book 10:15 PM"`. |
| Click a show | Seat-layout page with a "How many seats?" box (buttons `1`–`10`), then "Select Seats". |
| Seat map | Drawn (images only), as expected. **But** the page has `Button "Open accessibility seat selection"`: a form with drop-downs for tickets, category (with prices), row, then each seat as a button (`"Select seat 12 in row D"`). Pilot can book without seeing the map. |
| Drop-downs | The driver's click, UIA `SelectionItem.Select` and `set_value` all leave a web `<select>` unchanged (the page never hears the change). What works: expand it (driver click = UIA Expand) → `press_key down` × n → `return`. Expanding moves keyboard focus to Pilot's window, so Grain re-parks it right after; the user lost focus for ~1–3 s, the window stayed off-screen. |
| Seat | Seat buttons respond to a click (UIA Toggle). The page then shows `Button "Pay ₹ 340"`. |
| "Pay ₹ 340" | Not payment yet: it opens a Terms & Conditions box (`Cancel` / `Accept`). Stopped there. |

What this adds to the design:

- **New tool `choose`** (pick an option in a drop-down by its text). Firefox
  family: expand, arrow keys to the option, Enter, then re-park. Grain
  computes the number of arrow presses from the option list in the latest
  read. Chromium: not checked (background keys are refused there).
- **The Agent's stop rule fires on the first button that says pay, accept,
  confirm, place order or send**, even when it is only a step towards the
  real payment page. Here that is "Pay ₹ 340"; stopping one step early is the
  safe side. `show` hands the window over at that point.
- **Provider description hint:** "On a drawn seat map or picker, look for an
  accessibility or list alternative before handing over."
- Not confirmed: whether the default container is signed in to BookMyShow
  (the site had not asked for sign-in by that point).

**Phase 1: core** (`grain-core/src/pilot/`): tools, parsing, request mapping,
compaction, rendering, policy, manifest, with unit and golden tests.
**Done 2026-11-16**: 47 Pilot tests (218 in grain-core), clippy clean.
- `numbering.rs`: Pilot hands out its own control numbers, keyed by role,
  name and occurrence, so a number never points at a different control after
  the page changes. A click on a control that left the page is refused. This
  replaces "retry by role and name".
- `tools.rs`: 12 tools (adds `choose` for web drop-downs: expand, arrow keys,
  Enter, re-park). The tool list is ≤1.5k tokens.
- `policy.rs`: `is_payment_control` refuses Pay / Place order / Buy now and
  similar, by whole words; `show` hands those to the user.
- `compact.rs`: page subtree only (browser frame and other tabs dropped),
  `find` and the after-action `changes` diff run on Grain's side.
- Measured on the saved reads (tokens ≈ chars / 4):

  | Page | Raw | Controls | Page view | `find` |
  |---|---|---|---|---|
  | BookMyShow showtimes | ~15.9k | ~2.9k | ~3.2k | ~6 |
  | BookMyShow seat form | ~2.3k | ~0.14k | ~0.31k | ~11 |
  | Zomato booking | ~7.9k | ~0.63k | ~1.0k | ~31 |

**Phase 2: adapter and wiring**: install and verify, daemon, calls,
settle-read, cleanup; registry arms; setting, command, updater; the
no-account built-ins table; Extensions row and icon.

**Phase 3: acceptance in the real app** through `tests/agent-harness/`: open
BookMyShow → find a show → click → seat page → `show`; the same on Zomato; an
app flow (Notepad: open, type, read back). Each run records tokens per read
against the targets above. The user approves the Extensions row visually.

**Phase 4: after it works**: shrink older reads in the conversation if the
loop allows it cleanly; decide Agent Browser's removal; list the Reach
features not covered yet (media keys, show file in folder).

## Risks

- The driver ships several releases a day. Pilot pins one version and its
  hashes; moving the pin is a deliberate change with Phase 3 re-run.
- Background input into Chromium page content may be refused on Windows.
  Firefox-family worked. Phase 0 item 4 decides how much this matters.
- An accessibility read can be wrong or empty on some pages and apps; the
  fallback this round is `show`, not pixels.
- Drawn surfaces (seat maps, canvases) cannot be driven without screenshots.
- Bot checks do not apply to the user's own browser, but a site can still ask
  for a CAPTCHA; that is a `show` hand-off.

## Next round (not in this plan)

1. Screenshots for drawn surfaces, once tool results can carry an image.
2. Remembered recipes for repeated tasks.
3. Skipping a model round when the page already proves the step worked.
4. In-process driver instead of the sidecar.
5. macOS and Linux.
