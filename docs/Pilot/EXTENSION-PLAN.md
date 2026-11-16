# Grain Pilot: browser extension plan

Status: **plan only, nothing built** (written 2026-11-17).
Replaces the Cua Driver build described in [PLAN.md](PLAN.md) (stack change
recorded at its top). Branch: `feat/pilot`.

## Goal

The Agent does things in the user's own browser, signed in as them, without
taking over the screen:

- Each Agent task gets its own **background tabs in a tab group** in the
  user's browser window. No new windows, no focus changes, no jumping.
- **Several tasks can run side by side**, each in its own group.
- **Only for doing things** (book, order, fill a form, change a setting).
  Looking things up stays with web search.
- **Browser control only.** No app or desktop control: `open_app` and the
  Cua Driver go.
- Chromium first (Edge, Chrome, Brave), then Firefox and Zen from the same
  code.

## How it fits together

```
Grain Agent ──► grain_pilot.rs ──named pipe──► grain-browser-host.exe ──stdio──► extension
 (LLM loop,      (tools, policy,                (tiny relay the browser          service worker
  tool calls)     compaction)                    starts; native messaging)        │
                                                                                  ▼
                                                              content script in Pilot's tabs only
                                                              (@page-agent/page-controller)
```

- **The model's tools and all policy stay in Rust** (`grain-core/src/pilot/`).
  The extension is a dumb executor: it owns tabs and runs page reads and
  actions. It never talks to an LLM.
- **Page reading and actions:** `@page-agent/page-controller` (Alibaba
  page-agent, MIT, pinned exact version, currently 1.12.4). It is
  browser-use's DOM serializer rewritten as in-page TypeScript:
  `getBrowserState()` returns the page's interactive elements as short,
  numbered, simplified HTML, and `clickElement(index)`, `inputText`,
  `selectOption` and `scroll` act on them. It uses plain DOM APIs only, so the
  same content script runs in Chromium and Firefox. Unused parts are not
  bundled: the mask/animation, `executeJavascript`, and the agent, LLM and MCP
  packages.
- **Native messaging, not a port.** The browser starts
  `grain-browser-host.exe` and talks to it over stdio. The host relays
  messages to Grain over a named pipe. Nothing listens on the network.

## Decisions taken in this plan

1. **The extension only touches tabs it created.** The service worker keeps
   the set of Pilot tab ids (mirrored in `storage.session` so a worker restart
   keeps it). Any message naming another tab is refused. The user's own tabs
   are never read.
2. **Content script on demand.** It is injected with `scripting.executeScript`
   into Pilot's tabs only, after each page load, never declared for all
   sites. It is re-injected after navigation.
3. **Stable control numbers.** page-controller renumbers on every read. The
   content script keeps a `WeakMap<Element, number>` and rewrites indexes to
   Grain's own numbers, so `[12]` keeps meaning the same control across reads
   (the rule Pilot already has in `numbering.rs`).
4. **Tab groups.** One group per Agent run, titled from the task (for example
   "Grain · Movie tickets"), created with `tabs.create({active: false})` in
   the user's last-focused normal window, then `tabs.group` and
   `tabGroups.update`. If no browser window is open, Pilot opens one minimised
   and unfocused. Pilot tabs are set `autoDiscardable: false` so Memory Saver
   does not unload them mid-task.
5. **End of a run.** Tabs handed to the user with `show` stay open. All other
   tabs of the run close. This matches HeyClicky: tabs used along the way
   close by themselves.
6. **Hand-over.** `show` makes the tab active and focuses its window, and the
   Agent stops there. It is the only call that takes focus. Payment, sign-in,
   terms boxes, codes and CAPTCHAs always go through `show`.
7. **Waiting for pages.** The service worker waits for `tabs.onUpdated`
   `complete`. The content script then waits until the DOM stops changing
   (a `MutationObserver` quiet window), bounded by `SETTLE_MS`. This replaces
   the read-again loop in the Cua adapter.
8. **Input fidelity.** page-controller fires synthetic DOM events. A site that
   ignores them is detected by a re-read showing no change. On Chromium the
   action is then retried once with `chrome.debugger`: `Input.dispatchMouseEvent`
   or `Input.insertText` at the element, with
   `Emulation.setFocusEmulationEnabled` so background tabs accept it. The
   debugger is detached right after. `debugger` is an *optional* permission,
   requested the first time it is needed. While attached, Chrome shows a
   "started debugging this browser" bar. Firefox has no debugger API, so it
   stays on synthetic events.
9. **The host is a separate tiny binary**, `crates/grain-browser-host`
   (std only, no Tauri), shipped as a Tauri `externalBin`. It must not start
   Grain's full app (the single-instance plugin and WebView would load).
10. **Keeping the connection.** An MV3 service worker stays alive while a
    native port is open. The extension connects at browser start-up. If Grain
    is not running, the host exits, the worker sleeps, and a `chrome.alarms`
    tick (1 minute) tries again. Grain's pipe server runs only while Pilot is
    enabled. The cost while idle (worker plus host memory) is measured in
    Phase 0; if it is too high, connect only during an Agent run and accept
    up to a minute's delay after Grain starts.
11. **Which browser.** Every connected extension reports its browser
    (`navigator.userAgentData` brands). Pilot uses the Windows default
    browser's connection, falling back to the most recently connected one.

## Model-facing tools

Built from the current `tools.rs`, with windows replaced by tabs. Budget stays
~1.5k tokens.

| Tool | Change from today |
| --- | --- |
| `open(url)` | New background tab in the task's group. Returns `t1` and a read. No window replacing. |
| `tabs` | Replaces `windows`: Pilot's tabs and titles. |
| `read(tab, find, detail)` | Same views (controls / find / page). |
| `click`, `type(submit)`, `choose`, `scroll`, `steps` | Same, by Grain control number. `type` with `submit` uses `form.requestSubmit()` when the field is in a form, otherwise a synthetic Enter. |
| `press(key)` | Synthetic key events, limited to the current key list. |
| `show(reason, message)` | Activates the tab and stops (hand-over). |
| `close(tab)` | Closes a Pilot tab. |
| `open_app` | **Removed** (no app control). |

The directory description keeps the "only for doing things, never research"
wording from 2026-11-16.

## Safety

Each check runs in the extension and again in Rust:

- **Passwords and cards.** Refuse typing into `type=password`, fields with
  `autocomplete` `cc-*`, `one-time-code` or `current-password`/`new-password`,
  and fields whose label matches the card/CVV/OTP words in `policy.rs`.
  The content script reports these field kinds with each read, and the model
  sees them marked.
- **Payment.** The existing `refuse_payment` rule moves over: buttons that pay
  or place an order are refused, and the model is told to `show`.
- **Links.** `policy::web_link` stays: https only (plus http for local test
  hosts).
- **Untrusted page text.** Reads keep the `render::UNTRUSTED` banner.
- **Pipe.** The pipe's ACL allows only the current user. Grain checks that the
  client process is its own `grain-browser-host.exe`
  (`GetNamedPipeClientProcessId` plus image path). The host manifest's
  `allowed_origins` lists only Grain's extension id.
- **Logs** never carry typed text or full addresses (rule kept from
  `describe_for_log`).

## Code changes

**New**

- `browser-extension/` (top level, bun + TypeScript, built to
  `browser-extension/dist/`):
  - `manifest.json`: MV3, fixed `key` (stable id), permissions `tabs`,
    `tabGroups`, `scripting`, `nativeMessaging`, `storage`, `alarms`;
    `host_permissions: ["<all_urls>"]` (needed to inject into the pages the
    Agent opens); `optional_permissions: ["debugger"]`.
  - `src/worker.ts`: native port, tab and group bookkeeping, message routing,
    load waits, debugger fallback.
  - `src/page.ts`: the content script around page-controller (reads, stable
    numbers, field kinds, settle wait).
  - `THIRD_PARTY.md`: page-agent MIT notice.
- `crates/grain-browser-host/`: stdio ⇄ pipe relay, about 200 lines.
- `src-tauri/src/grain_browser_link.rs`: the pipe server, client check,
  request/response matching, connected-browser registry, and registering the
  host manifest under `HKCU` for Chrome, Edge and Brave (and, in Phase 6,
  Mozilla), removed on Disconnect.

**Rewritten**

- `src-tauri/src/grain_pilot.rs`: drops the driver daemon, the Win32 window
  code (`win::park`, `send_back`, `windows_of`, `password_fields`…) and the
  Cua live tests. It keeps `run` dispatch, `act` (read → act → settle →
  report change), refusals and the per-run tab group.
- `grain-core/src/pilot/compact.rs`: parses page-controller's simplified HTML
  instead of Cua's markdown tree, with the same views, caps and change report.
- `grain-core/src/pilot/tools.rs`: tabs instead of windows, no `open_app`.

**Deleted**

- `grain-core/src/pilot/manifest.rs`, `request.rs`, and the driver parts of
  `render.rs` and `mod.rs` (`DRIVER_*`, `driver_asset`, `SESSION`).
- `src-tauri/src/grain_pilot_install.rs` (the Cua download).
- The Extensions row text "Connect downloads Cua Driver" in `grain_mcp.rs`.

**Kept as is:** `policy.rs`, `numbering.rs` (logic moves next to the
content script but the Rust rules stay for the change report), the
`BuiltIn::Pilot` entry, `pilot_enabled`, and `grain_compiled_tools.rs`
routing.

## Phases

### Phase 0: spike (throwaway, nothing committed but results)

An unpacked test extension in a **fresh Edge profile** (`--user-data-dir` in
the scratchpad). Never in the user's own browser profile, and never closing
any window except by the tab ids the spike created. Questions, each with a
measured answer written into this file:

1. Does page-controller read, click and type in a **background (inactive)
   tab** on iana.org, Wikipedia search, BookMyShow showtimes → seat page, and
   Zomato?
2. **Timer throttling.** Hidden tabs run timers at most once a second, and
   after 5 minutes Chrome's intensive throttling makes chained timers wait up
   to a minute. page-controller waits with `setTimeout`. Measure a click
   round-trip in a tab hidden for 1 minute and for 6 minutes. If it is slow,
   patch our bundle's `waitFor` to a `MessageChannel` tick.
3. **Lazy content.** Do scroll-to-load lists and `IntersectionObserver`-based
   lazy content load in a hidden tab? If not, what does the fallback cost
   (the debugger route, or `show`)?
4. **Tokens per read** versus the Cua numbers in PLAN.md (showtimes ~3.7k
   interactive-only, seat page ~260, Zomato ~1.0k).
5. **Which sites ignore synthetic events**, and does the debugger fallback
   click and type in a background tab **without activating it**?
6. **Native messaging.** Does the service worker stay up while the port is
   open? Host start-up time, and the memory of the host plus the worker when
   idle.
7. **Tab groups** in Edge and Brave: create, title, collapse, and close the
   group's tabs.

Exit: every question has a measured answer. A "no" on 1 or 2 without a
working fix stops the plan and comes back to the user.

#### Phase 0a results (2026-11-17): page-controller as-is, visible tab

The user asked to skip background tabs for now and see how page-controller
does on its own. Setup: page-controller 1.12.4 bundled as one script (106 KB),
injected by Playwright into **visible** tabs of a separate Edge profile. No
extension yet, so no background tabs.

| Site | Read time | Tokens, full page | Tokens, visible area | Action | Result |
| --- | --- | --- | --- | --- | --- |
| iana.org | 12 ms | ~480 | — | click link | ✅ navigated |
| Wikipedia main page | ~90 ms | ~4.5k | ~1.3k | type "Pune", click Search | ❌ as-is, ✅ with fix (below) |
| BookMyShow showtimes | ~120 ms | ~3.8k | ~1.2k | click a show | ✅ reached the seat-layout URL |
| Zomato Pune delivery | ~70 ms | ~2.1k | ~0.55k | click "Pizza" | ✅ navigated (next page ~1.8k) |

- **Reads are fast and small.** 10–120 ms per read. Full-page reads are about
  the size of Cua's after Grain's compaction (BookMyShow 3.8k vs 3.7k), and
  visible-area reads are 3–4× smaller. Clicks take about 0.6 s (built-in
  waits).
- **Typing failed silently on Wikipedia.** page-controller clicks the field,
  then sets the value on that element. Wikipedia replaces its search box on
  click, so the text went into a detached box: it reported success, and the
  search ran empty. **Fix in Grain's wrapper (tested ✅):** after the click,
  type into the field that has focus, then check its value and report the
  truth.
- **Reads do not show field values**, so the model cannot see what a box
  holds. The wrapper adds `value=` to input rows (never for password fields).
- **BookMyShow's seat page stuck on "Please wait, while we load the seats"**,
  and after a few runs Cloudflare blocked the test browser ("Sorry, you have
  been blocked"). This comes from the **test rig**: Playwright drives Edge
  through the debugging protocol and flags it as automated. A real extension
  in a normal browser does not carry that flag. BookMyShow testing waits for
  the real extension; no more Playwright runs against it.

Wrapper changes these add to Phase 2: type into the focused field and verify;
add field values to reads; prefer visible-area reads plus `find`, with the
full page on request.

### Phase 1: the link

The host crate, the `grain_browser_link.rs` pipe server, host manifest
registration (Chrome, Edge, Brave), and the extension worker with a
`ping`/`hello` round-trip. The Extensions row's Connect: register the host,
open the extension's store page (in development, the "load unpacked" steps),
and show Connected once the extension says hello.

Tests: host framing (4-byte length prefix, 1 MB message cap), pipe client
check, registry entries written and removed (the Disconnect path).

### Phase 2: tabs and pages

The worker's tab and group lifecycle, the content script around
page-controller, stable numbers, field kinds, and the settle wait. The new
`compact.rs` and the `tools.rs` changes. The `grain_pilot.rs` rewrite onto
the link.

Tests: `compact.rs` against saved page-controller output from the Phase 0
sites (fixtures, like the current Zomato and BookMyShow ones), the token
budget, the stable-number rules, and the tool parse tests carried over.

### Phase 3: safety, hand-over and fallback

Password, card and payment refusals on both sides, `show`, end-of-run
cleanup, and the Chromium debugger fallback behind its optional permission.

### Phase 4: remove the Cua path

Delete the files listed above, along with the Cua live tests and the
`agent-browser`/Cua references in the Extensions row. Update the memory file
and this plan.

The Agent Browser (Vercel `agent-browser` CLI) is removed in a follow-up
once Pilot passes Phase 5. It is not part of this plan.

### Phase 5: live acceptance (Edge, then Brave)

Run in the user's real browser only when they agree to it, through
`tests/agent-harness/` with recorded evidence:

- iana.org read and click; Wikipedia search (typing).
- BookMyShow to the terms box, then stop (hand-over point).
- Zomato to the cart, then stop before payment.
- **Two tasks at once**, in two groups, while the user keeps working in
  another tab: no focus change, no new window.
- Grain closed in the middle of a task: the extension goes idle, with no
  stuck tabs or group.

Then the user approves the real Extensions row visually
(`bun run tauri dev`).

### Phase 6: Firefox and Zen

The same `page.ts`. The worker uses the `browser.*` namespace through a thin
shim; Firefox MV3 runs the worker as an event page. Register the host under
`HKCU\Software\Mozilla\NativeMessagingHosts` with `allowed_extensions`.
Firefox requires a signed add-on even outside the store, so it gets AMO
unlisted signing. Check whether Zen reads Mozilla's registry key. Without a
debugger fallback, record which Phase 5 sites fail on synthetic events.

### Phase 7: publishing (needs the user)

Chrome Web Store and Edge Add-ons listings (unlisted to start), the AMO
listing, and the one-click Connect: Grain opens the store page, the user
clicks Add, and Grain shows Connected.

## Open decisions for the user

1. **Store accounts.** The Chrome Web Store needs a developer account (one-time
   fee), and Edge Add-ons and AMO need accounts too. Until then, development
   uses "load unpacked".
2. **The Chromium debugger fallback.** On by default (more sites work, Chrome
   shows a "debugging" bar while it acts), or off until the user allows it?
   The plan assumes it is asked for the first time it is needed.
3. **End-of-run tabs.** The plan closes everything not handed over. The
   alternative is to keep the whole group, collapsed.

## Risks

- **Background tabs are second-class.** Throttled timers, no rendering, and
  lazy content that may not load. Phase 0 questions 2 and 3 decide whether
  this works; the fallbacks are the debugger route and `show`.
- **Synthetic events.** Some sites (bot checks, some React inputs) ignore
  them. Chromium has the debugger fallback; Firefox does not.
- **page-controller changes upstream.** Pin an exact version, bundle it, and
  update deliberately. It is MIT, so a fork is possible if the project stops.
- **Store review** for `<all_urls>` plus `debugger`. Write a clear purpose
  statement; `debugger` being optional helps.
- **Brave and Zen specifics** (native-host registry locations, tab groups in
  Zen's workspaces) are unverified until Phases 5 and 6.
