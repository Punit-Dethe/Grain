# Pilot: plan

Status: **plan only, nothing built.** Written 2026-11-16. "Pilot" is a working
name (see Decisions).

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
- A raw page read is large (showtimes page ~16k tokens). The same data filtered
  to controls only is ~3.7k (Agent Browser: ~4.0k); seat page 0.26k; Zomato
  booking page 1.0k. A `find` read is ~0.5k.
- The driver's own tool list is ~45k tokens. Pilot never sends it; the model
  sees only Pilot's short list.
- A click returns before the page changes, so an immediate re-read says
  "no change". Pilot must wait for the page to settle (see Adapter).
- Not measured: sign-in, OTP, payment, Chromium browsers, driver RAM.

## Decisions (recommended; the first two are the user's to confirm)

- **Name / id:** "Pilot", extension id `grain.pilot`, Extensions page id
  `pilot`, setting `pilot_enabled` (default off). Renaming is one constant.
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
  `DO_NOT_TRACK=1`, `CUA_DRIVER_DISABLE_RUN_SCRIPT=1`, update check off,
  `CREATE_NO_WINDOW`.
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

**Phase 1: core** (`grain-core/src/pilot/`): tools, parsing, request mapping,
compaction, rendering, policy, manifest, with unit and golden tests.

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
