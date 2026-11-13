# Reach (formerly "Grain Control"): plan

Status: **approved, implementation started** 2026-11-13. Research date: 2026-11-13.

## 0. Decisions (user, 2026-11-13)

- **Name:** Reach. Extension id `grain.reach`.
- **Default:** off until the user turns it on from Extensions.
- **No confirmation dialogs while testing.** Every Reach call runs as soon as the
  Agent makes it, Enter included, the same as enabled MCP extensions today. The
  confirmation and taint rules in §6 are **deferred to the production decision**;
  they are kept here as the proposal for that decision. Hard refusals (§6.2) and
  the terminal/password/own-window limits still apply, because they are not a
  confirmation box.
- **Element tier (press buttons / focus fields by name):** out of v1, planned later.
- **Platforms:** Windows first. Link/app opening uses the cross-platform opener,
  so macOS/Linux get links; the other tools report "not available on this OS yet".

## 0.1 As built (v1, 2026-11-13)

| Piece | Where |
| --- | --- |
| Tool schemas, argument parsing, link policy (allow/refuse lists), site-search and app deep-link URL builders, path and file-type checks, app-name matching | `crates/grain-core/src/reach.rs` (pure, unit-tested) |
| OS adapters (opener links, Start-menu launch/switch, focus check via `context_detect`, paste via `utils::paste_with_options`, keys via enigo, Core Audio volume, window list), Agent catalogue, on/off command | `src-tauri/src/grain_reach.rs` |
| Agent wiring: directory entry, compiled catalogue, `grain.reach` dispatch (sequential, beside MCP batches), revalidation digest | `capability.rs`, `action_exec.rs` |
| Summon target (the window "type this" goes to when the notch has focus) | `agent.rs` `summon_inner` / `open_notch` → `grain_reach::remember_summon_target` |
| Setting `reach_enabled` (default off), command `change_reach_enabled_setting`, Extensions card (Add/Remove) | `grain-core settings.rs`, `settingsStore.ts`, `McpPresets.tsx` |

Ten tools: `open_link`, `search_site`, `open_app`, `open_in_app`, `type_text`,
`press_key`, `media_control`, `open_settings`, `show_file`, `list_windows`.
`search_site` and `open_in_app` replace §4's `search` and `open_app target`.
§5.1's folder layout became the two single files above.

Typing goes to the app in front. If Grain's own notch has focus, it goes to the
window the Agent was summoned from, which is brought back to the front first.
Typing is refused for password fields, Grain's own windows and terminals.
Enter is also refused in terminals. Replace mode needs a confirmed editable
field. Everything else in §6.2 is enforced in core.

## 1. What it is

A first-party, Grain-owned capability that lets the Agent act on the OS **without
moving the mouse or reading pixels**. Everything goes through the doors the OS
already has: URLs, protocol handlers (deep links), the Start menu / Launch Services /
`.desktop` catalogue, accessibility APIs (by element, never by coordinate), the
clipboard paste path Grain already ships, and a small allow-listed set of keys.

Examples it should handle:

- "Open YouTube and search lo-fi beats" → one URL.
- "Play the Veritasium video about the longest straight line" → Exa (already an
  extension) finds the watch URL → open it.
- "Open Spotify and play Daft Punk" → `spotify:search:` deep link (the app takes it
  from there).
- "Open Bluetooth settings" → `ms-settings:bluetooth`.
- "Reply to this saying I'll be there at 6" (summoned from Slack) → type the text
  into the focused field; **Enter only after the user says yes**.
- "Draft an email to sam@x.com about Friday" → `mailto:` prefill. The user presses Send.

The pattern is **"take the user to the right place and get it ready"**. Grain
navigates, prefills and pastes. Anything irreversible (send, submit, Enter, button
press) is the user's call or needs an explicit host-held confirmation.

## 2. How it fits Grain today (code findings)

| Fact | Where | Consequence |
| --- | --- | --- |
| Agent tools come **only** from MCP providers (`mcp.<id>`). `run_agent_calls`/`execute` refuse any other prefix ("Only MCP connections provide Agent tools.") | `src-tauri/src/action_exec.rs` | Needs one new provider kind: in-process first-party (`grain.<id>`). |
| Directory, lexical shortlist, `search_tools` recovery and schema budget are provider-agnostic once a `Catalog` of `(ActionInput, SessionAction)` exists | `src-tauri/src/capability.rs` (`open`, `fetch_catalog`, `dispatch_frame`) | A static, compiled catalogue slots in beside MCP. Discovery and token budget come for free. |
| The host owns risk: `prepare(..., RiskClass, SideEffect, digest)`. `Confirm` calls are held in `PendingCalls` (TTL 120 s, max 16), revalidated, resumed by `agent_confirm_action`. `AgentPanel.tsx` already renders `confirm_action` | `action_exec.rs`, `agent.rs` (`publish_pending`, `resume_pending_turn`), `AgentPanel.tsx` | Per-call confirmation exists. It is currently unused because enabling an MCP extension auto-approves its calls. Grain Control reuses it, only for its irreversible actions. |
| Platform docs: extensions never get screen/OCR/cursor/OS integration; "the Agent owns contextual features" | `docs/Extensions 2.0/EXTENSION-PLATFORM-HANDOFF.md` §1 | Grain Control must be **host code**, not a native/MCP package. It can still appear as a card on the Extensions page (like Exa: Add/Remove, no account). |
| `app_catalog.rs` already walks `shell:AppsFolder` (desktop exe + packaged AUMID). The macOS/Linux side is a stub | `src-tauri/src/app_catalog.rs` | App launch on Windows reuses this. One extra field is needed: the AppsFolder parsing name used for launch. |
| UIA focus probe: control type, editable, `is_password`, read-only, Value/Text/TextEdit patterns, terminal detection | `context_detect.rs` (`read_focus_probe`, `classify`), `agent.rs` (`foreground_is_terminal`) | "Is there a text box focused, and is it safe?" is already answered. |
| Paste path with Paste Catch pre/post-flight, `PASTE_LOCK`, clipboard restore. `send_return_key` (Enter / Ctrl+Enter / Cmd+Enter) via enigo | `handy/clipboard.rs` (`paste_with_options`, `send_return_key`), `paste_catch.rs` | Typing and Enter reuse upstream functions **by calling them**. No edits under `handy/`. Note: `paste_with_options` routes to the Agent panel if the panel has focus, so the target window must be focused first. |
| Summon snapshots the foreground HWND (the screen-image target) | `agent.rs` `summon_inner` → `foreground_hwnd()` | That HWND is the natural "type here" target, so keep it in `AgentState` for the task. |
| Earlier secure `open:url` (scheme allowlist http/https/mailto/tel, opener plugin, never a shell) and `open:app` (user-picked paths only), retired with the old host API | commit `0eb9ce667`, removed in `3eb5d5f6e` | Recover the validator and its RCE-vector tests instead of rewriting them. |
| `tauri-plugin-opener`, `enigo`, `windows` crate (UIA), `tauri-plugin-macos-permissions` already linked | `src-tauri/Cargo.toml` | v1 needs **no new heavy dependency** on Windows or macOS. Linux AT-SPI would add `atspi`/`zbus` (P3 only). |

## 3. Capability matrix

Reliability: **H** works almost everywhere; **M** works for most mainstream apps
with known gaps; **L** app-, version- or desktop-dependent. "v1" marks what the
recommended first release ships.

### 3.1 Open web pages and web "deep links"

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| Open http(s) URL in default browser | `ShellExecuteW` via opener plugin | `NSWorkspace.open` via opener | `xdg-open` / portal `OpenURI` via opener | H | ✔ |
| Open in a *specific* browser | opener `open_url(url, app)` (browser exe + URL as its only argument) | `open -a <bundle>` / `NSWorkspace.open(_:withApplicationAt:)` | `gio`/`.desktop` launch with URI | H | ✔ |
| Site search/compose templates (host-built URL, model fills only the query) | same | same | same | H (each template M: sites change params) | ✔ |
| "Go straight to the first result" | DuckDuckGo `!ducky <query>` redirect, or Exa search then open the URL | same | same | M | ✔ (Exa route preferred) |
| Anything *inside* the page (click, fill a web form) | Only UIA into Chromium/Edge/Firefox content. Chrome ≥138 exposes web content natively over UIA. Fragile per site | AX into WebKit/Chromium (needs `AXEnhancedUserInterface`/`AXManualAccessibility` for Chromium/Electron) | AT-SPI (Chromium needs `--force-renderer-accessibility`) | L | ✘ (later, element tier) |
| Browser JS / DevTools Protocol / AppleScript `do JavaScript` | CDP needs browser relaunch with a debug port, which is a severe security hole | Chrome "Allow JavaScript from Apple Events" is off by default | CDP same | — | ✘ never |

Seed template table (host-held, URL-encoded by the host, reviewed when sites change):

| id | URL |
| --- | --- |
| web (user's engine: Google/Bing/DDG/Brave/Kagi) | `https://www.google.com/search?q={q}` etc. |
| youtube search / watch | `https://www.youtube.com/results?search_query={q}` / `https://www.youtube.com/watch?v={id}` |
| maps search / directions | `https://www.google.com/maps/search/?api=1&query={q}` / `…/maps/dir/?api=1&destination={q}` |
| gmail compose | `https://mail.google.com/mail/?view=cm&to={to}&su={subject}&body={body}` |
| outlook web compose | `https://outlook.office.com/mail/deeplink/compose?to={to}&subject={s}&body={b}` |
| google calendar new event | `https://calendar.google.com/calendar/render?action=TEMPLATE&text={t}&dates={d}` |
| translate | `https://translate.google.com/?sl=auto&tl={lang}&text={q}` |
| wikipedia, amazon, github, reddit, spotify web, images, news | standard `?q=`/`?k=` forms |
| AI chats (prefill) | `https://claude.ai/new?q=`, `https://chatgpt.com/?q=`, `https://www.perplexity.ai/search?q=`. Behaviour undocumented: ChatGPT may **auto-submit**, Claude prefills. Treat as M and test |

### 3.2 Protocol handlers / app deep links

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| Launch a URI | `ShellExecuteExW` (opener) | `NSWorkspace.open` | `xdg-open` / `gio` | H | ✔ |
| Is a handler installed? (before offering a deep link) | `AssocQueryStringW(ASSOCF_IS_PROTOCOL, ASSOCSTR_FRIENDLYAPPNAME, scheme)` | `NSWorkspace.urlForApplication(toOpen:)` | `gio::AppInfo::default_for_uri_scheme` / `xdg-mime query default x-scheme-handler/<s>` | H | ✔ |
| Enumerate all handlers | `SHAssocEnumHandlersForProtocolByApplication` (packaged + desktop); classic `HKCR\<scheme>` with `URL Protocol`; packaged-only `Launcher.FindUriSchemeHandlersAsync` | `NSWorkspace.urlsForApplications(toOpen:)` (12+); `LSCopyAllHandlersForURLScheme` (deprecated) | `MimeType=x-scheme-handler/*` in `.desktop` files | M | ✘ (only for the settings UI) |

Curated deep-link registry (host data, one entry per app action, typed params,
risk tier, handler check). Seed, all of which **navigate or prefill, never send**:

| App | Link | Effect |
| --- | --- | --- |
| Windows Settings | `ms-settings:<page>` (display, bluetooth, network-wifi, sound, notifications, defaultapps, windowsupdate, privacy-microphone, …) | open page |
| Mail (any client) | `mailto:{to}?subject=&body=` | prefilled compose |
| Phone / SMS | `tel:` / `sms:` | dialer / compose |
| Spotify | `spotify:search:{q}`, `spotify:track:{id}` | search / open |
| WhatsApp | `whatsapp://send?phone={e164}&text={t}` | prefilled chat |
| Telegram | `tg://resolve?domain={user}` / `tg://msg?text=` | open chat / prefill |
| Teams | `msteams:/l/chat/0/0?users={email}&message={t}` | prefilled chat |
| Zoom | `zoommtg://zoom.us/join?confno={id}&pwd={p}` | join meeting (Confirm: joins with mic/cam) |
| Slack | `slack://open?team={T}`, `slack://channel?team={T}&id={C}` | navigate |
| Discord | `discord://-/channels/{g}/{c}` | navigate |
| VS Code / Cursor | `vscode://file/{path}:{line}` / `cursor://file/…` | open file. **Confirm**: opening a folder can trigger workspace-trust prompts and tasks |
| Obsidian | `obsidian://open?vault=&file=`, `obsidian://new?vault=&name=&content=`, `obsidian://search?query=` | open / create note |
| Notion | `notion://www.notion.so/{page}` | open page |
| Steam | `steam://rungameid/{id}` | launch game |
| Calculator / Clock / Snip | `calculator:`, `ms-clock:`, `ms-screenclip:` | open |
| Claude desktop | `claude://…?q=` (prefill, documented) | prefill |
| macOS | `x-apple.systempreferences:<pane>`, `shortcuts://run-shortcut?name=` | settings / run a user Shortcut (Confirm) |

### 3.3 Launching and switching apps

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| App catalogue | `shell:AppsFolder` walk (exists: `app_catalog.rs`) | `/Applications`, `~/Applications`, `/System/Applications` + bundle ids (`NSWorkspace`), optionally `mdfind kMDItemContentType==com.apple.application-bundle` | `.desktop` in `$XDG_DATA_DIRS/applications` + Flatpak/Snap exports (`gio::AppInfo::all`) | H | ✔ |
| Launch by name | `ShellExecuteExW("shell:AppsFolder\<parsing name>")`, one path for desktop and packaged; or `IApplicationActivationManager::ActivateApplication(AUMID)` | `NSWorkspace.openApplication(at:configuration:)` / `open -b <bundle>` | `gio::DesktopAppInfo::launch` (never hand-parse `Exec=`; field-code injection) | H | ✔ |
| Bring running app to front | `EnumWindows` → match exe/AUMID → `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`. Allowed while Grain owns the foreground (the notch was just used); otherwise re-launching a single-instance app activates it | `NSRunningApplication.activate` | X11 `_NET_ACTIVE_WINDOW`; **Wayland: no generic API** (GNOME none, KDE via KWin script, wlroots foreign-toplevel) | M (Win/mac), L (Wayland) | ✔ |
| List open windows (title, app) | `EnumWindows` + UIA window names | `CGWindowListCopyWindowInfo` (titles need Screen Recording since 10.15, so app names only) / AX windows | X11 `_NET_CLIENT_LIST`; Wayland: no | M | ✔ (Windows) |
| Minimize / maximize / close window | UIA `WindowPattern` | AX `AXMinimized`, `AXCloseButton` press | X11 EWMH | M | close ✘ (data loss); min/max later |
| Run with arguments / arbitrary executable path | possible | possible | possible | — | ✘ never (arg injection, e.g. Chromium `--gpu-launcher`, is a known RCE) |

### 3.4 Text into apps (no mouse)

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| Read focus: app, control type, name, editable, password, read-only | UIA (exists: `read_focus_probe`/`classify`) | `AXUIElementCreateSystemWide` → `kAXFocusedUIElementAttribute`, `AXRole`, `AXSubrole=AXSecureTextField` | AT-SPI `Accessible`/`StateSet` (`EDITABLE`, `PASSWORD_TEXT` role) | M–H | ✔ |
| Insert at caret | clipboard paste (exists, with Paste Catch) | clipboard paste (exists) **or** AX set `kAXSelectedTextAttribute` (inserts at caret, no clipboard; Cocoa H, Electron M) | paste (exists: wtype/ydotool/xdotool) or AT-SPI `EditableText.InsertText` | H (paste) | ✔ |
| Replace whole field | UIA `ValuePattern.SetValue` (single-line edits incl. Chrome/Electron inputs; not contenteditable/rich editors) → fallback Ctrl+A + paste after verifying focus is an editable field | AX set `kAXValueAttribute` | AT-SPI `SetTextContents` | M | ✔ |
| Focus a field by name ("the search box") | UIA `FindFirst` (bounded, Edit/Document + Name/AutomationId/HelpText) → `SetFocus` | AX children walk → set `kAXFocusedAttribute` | AT-SPI `Component.GrabFocus` | M | ✘ v1, ✔ v1.1 (Windows) |
| Invoke a button by name ("press Send") | UIA `InvokePattern` / `LegacyIAccessible.DoDefaultAction` (MSAA) | `AXUIElementPerformAction(kAXPressAction)` | AT-SPI `Action.DoAction` | M | ✘ (element tier, always Confirm) |
| Elevated / admin windows | UIPI blocks both SendInput and UIA writes from a non-elevated Grain | n/a | n/a | — | refuse with a clear message |

### 3.5 Keys (no mouse)

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| Enter / Ctrl+Enter / Shift+Enter / Tab / Esc | `SendInput` via enigo (`send_return_key` exists) | `CGEventPost` (needs Accessibility, which Grain already asks for to paste) | X11 XTest; Wayland: wtype (wlroots), ydotool (uinput daemon), or RemoteDesktop portal (prompt; KDE/GNOME; keysym bugs) | H Win/mac, M Linux | ✔ (Enter variants always Confirm) |
| Arbitrary shortcuts (Ctrl+K, Ctrl+L, …) | same | same | same | M | ✘ (fixed allowlist only; open question Q7) |

### 3.6 Other system surfaces

| Capability | Windows | macOS | Linux | Rel. | v1 |
| --- | --- | --- | --- | --- | --- |
| Media play/pause/next/prev | `SendInput` `VK_MEDIA_*` (system-wide SMTC) | `NX_KEYTYPE_PLAY` system-defined `CGEvent` (Accessibility) | MPRIS over D-Bus (`org.mpris.MediaPlayer2.Player`) | H | ✔ |
| Volume set/up/down/mute | Core Audio `IAudioEndpointVolume` (exact %, `windows` crate) | CoreAudio `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` | `wpctl` / `pactl` | H | ✔ |
| Settings pages | `ms-settings:` | `x-apple.systempreferences:` | `gnome-control-center <panel>` / `systemsettings kcm_*` (desktop-specific, L) | H/H/L | ✔ |
| Reveal file / open folder | `SHOpenFolderAndSelectItems` (opener `reveal_item_in_dir`) | `NSWorkspace.activateFileViewerSelecting` | `org.freedesktop.FileManager1.ShowItems` | H | ✔ |
| Open a document with its default app | `ShellExecuteW` + **refuse** if `AssocIsDangerous(ext)` (exe, lnk, bat, ps1, hta, js, msi, …) | `NSWorkspace.open` + UTType executable check | `xdg-open` + executable bit/MIME check | H | ✔ Confirm |
| Copy to clipboard | clipboard plugin (`agent_copy` exists) | same | same | H | ✔ |
| Windows Search | `search-ms:` is an abused handler (UNC lure, 2022) | Spotlight has no URL to drive it | — | — | ✘ |
| Notifications / toasts | Agent already replies in the notch | | | — | ✘ |
| Office COM, PowerShell, AppleScript/JXA, shell commands | possible, but equal to arbitrary code execution | AppleScript needs per-app Automation (TCC) prompts | | — | ✘ v1. macOS Shortcuts by **user-allowlisted name** is the safe escape hatch (P4) |
| Lock / sleep / shutdown | `LockWorkStation` etc. | | | — | ✘ (not asked for; easy later) |

## 4. Recommended v1 action set

Keep the tool count small: each schema costs first-round tokens, and
`capability.rs` already budgets them. Eight tools, compact enums:

| Tool | Args | Does | Host risk |
| --- | --- | --- | --- |
| `open_link` | `url`, `browser?` | Open a URL. Allowed: `https`, `http`, `mailto`, `tel`, `sms`, curated deep-link schemes, user-added schemes | Safe for http(s)/mailto/tel/sms and curated navigate/prefill links. **Confirm** for user-added schemes and *any* link while the task is tainted (§6.3). Refuse the denylist |
| `search` | `site` (enum of templates), `query`, extra fields per template (`to`, `subject`, …) | Host builds and encodes the URL. The model never assembles it | Safe (Confirm when tainted and the template carries free text to a third party: compose / AI prefill) |
| `open_app` | `name`, `target?` (deep-link action id + params) | Fuzzy-match the catalogue; focus if running, else launch. With `target`, open a curated deep link in that app | Safe. **Confirm**: terminals, shells, script hosts, Registry Editor, installers (LOLBin list) |
| `type_text` | `text`, `mode` (`insert` / `replace`) | Focus the task's target window (summon HWND or the app this task opened), probe focus, then paste (insert) or `SetValue` (replace) | Safe when the target is an ordinary editable field in the app the user summoned from. **Confirm** for terminals/consoles, another app than the summon target, or text over ~2 KB. Refuse password fields, non-editable focus, elevated targets and Grain's own windows |
| `press_key` | `key` (`enter`, `ctrl_enter`, `shift_enter`, `tab`, `escape`) | Same target rules as `type_text` | **Confirm** for every Enter variant (it sends/submits). Tab/Esc Safe |
| `media` | `action` (`play_pause`, `next`, `previous`, `volume_up`, `volume_down`, `mute`, `set_volume`), `level?` | System media/volume | Safe |
| `open_settings` | `page` (enum, mapped per OS) | Settings page | Safe |
| `show_file` | `path`, `open?` | Reveal in folder, or open with its default app | Reveal Safe. Open is **Confirm** and refused for dangerous types (`AssocIsDangerous`) |

Plus one read tool, folded in to save a schema: `open_app` with `name: "?"` or a
small `list_windows` returns open windows and the current focus (app, control
type, field name, editable). This lets the model check "is there a text box?"
before typing. No screen pixels, no field contents.

Cut from v1 on purpose: invoking buttons by name, focusing fields by name, window
close/minimize, arbitrary shortcuts, AppleScript/Shortcuts, file writes, anything
running a command.

## 5. Architecture

### 5.1 Where the code lives

```
crates/grain-core/src/control/          (pure, no Tauri, unit-tested)
  policy.rs     scheme allow/deny, URL validation, taint rules, risk table
  templates.rs  web search/compose templates, encoding
  deeplinks.rs  curated app deep-link registry (data + typed params)
  tools.rs      ToolDef schemas + catalogue digest

src-tauri/src/grain_control/            (OS adapters, thin)
  mod.rs        provider: directory entry, catalogue, dispatch, enable setting
  windows.rs    ShellExecuteEx, AppsFolder launch, EnumWindows/focus,
                UIA focus/SetValue (reusing context_detect), Core Audio, media keys
  macos.rs      NSWorkspace, AX insert/focus, CGEventPost, CoreAudio   (P2)
  linux.rs      gio launch/handlers, MPRIS, wpctl, AT-SPI              (P3)
```

Nothing under `src-tauri/src/handy/` changes. `clipboard::paste_with_options` and
`send_return_key` are called as-is.

### 5.2 How the Agent sees and calls it

1. **Directory.** `capability::open` appends `grain_control::directory(app)` (one
   entry, `extension_id = "grain.control"`) when enabled. The model sees it
   alongside MCP providers. `search_tools` and the lexical shortlist work
   unchanged.
2. **Catalogue.** `capability::fetch_catalog` gets a `grain.` branch that returns
   the compiled `ToolDef`s. The digest is a hash of the schemas plus the policy
   version, so a build that changes policy invalidates held confirmations, as an
   MCP tool change does today. No network, no cache file.
3. **Prepare.** `prepare_action` → `action_exec::prepare(...)` with a risk class
   **computed by `grain_core::control::policy` from the tool and its actual
   arguments plus task state** (target app, taint). Never from the model.
4. **Dispatch.** `action_exec::run_agent_calls` / `execute` /
   `current_manifest_digest` / `is_enabled` get a `grain.` arm calling
   `grain_control::call(app, action, args)` on a blocking worker (UIA/COM and
   input injection must stay off the async runtime). Calls in one frame run
   **sequentially** for this provider (focus-dependent), unlike MCP's concurrent
   batches.
5. **Confirm.** For `Confirm` results, `run_agent_calls` routes this provider
   through `run_or_confirm` instead of the auto-approve policy. The existing
   `ToolResult::Confirm` → `publish_pending` → `AgentPanel` card →
   `agent_confirm_action` → `resume` path is used. `resume` revalidates
   time-of-use: the target window still exists, focus is still an editable
   field, the digest is unchanged, and the TTL (shorten to ~30 s for keys) has
   not expired.
6. **Result.** Plain-text outcomes ("Opened YouTube search for 'lo-fi beats' in
   Chrome", "Pasted 214 chars into Slack: #general message box"). Receipts flow
   into Agent history as today.

### 5.3 Target window rule (no mouse, no guessing)

The Agent only types into one of two windows: **(a)** the foreground window
snapshotted at summon (already captured), or **(b)** a window this task opened or
focused with `open_app`/`open_link`. Before typing, the host restores that window
(`SetForegroundWindow`; Grain is the foreground owner at that moment) and waits
up to ~500 ms for focus. It then re-reads the UIA focus and runs the policy check.
It never types into whatever happens to be focused.

### 5.4 Enabling and settings

- An Extensions-page card like Exa ("Add" → on, no account), backed by a host
  setting `grain_control_enabled` (default **off**, Q2). Presentation only: it is
  not a package and cannot be replaced by a store update.
- Detail page toggles per group: *Open links & apps*, *Type into apps*,
  *Press Enter* (off = never offered), *Media & volume*, *Settings pages*.
  Disabled groups are removed from the catalogue, not just refused at call time.
- "Allowed custom link types" list: the user adds schemes; the denylist still wins.
- macOS: the card shows Accessibility status (reusing
  `tauri-plugin-macos-permissions`) and the Settings deep link to grant it.

### 5.5 Resource rules (AGENTS.md)

No background thread, watcher, hook or daemon. Every call is on-demand. The UIA
client and COM apartment live only for the call, and the app catalogue is resolved
per task and dropped with the `CapabilitySession`. Static tables are compiled in.
Idle RAM cost: zero when disabled; a few KB of tables when enabled.

## 6. Security model

### 6.1 Principles

1. **The host decides risk** from the tool, its exact arguments and task state.
   The model can't downgrade it. A tool description is not authority.
2. **Allowlists, not blocklists**, for schemes, keys, templates and app targets.
   Denylists are only defence in depth.
3. **No shell, no arguments, no paths to executables.** URLs go through the OS
   default handler (opener plugin). Apps launch by catalogue identity only. This
   is the lesson of a decade of `shell.openExternal` RCEs and Tauri's own
   `shell.open` scope advisory (already recorded in the Phase 5C log).
4. **Navigate and prefill freely; commit only with the user.** Enter, send,
   submit, join a meeting, run a Shortcut or open a document: each needs a
   host-held confirmation showing the exact target app/window and the full text.

### 6.2 Hard refusals (no confirmation can unlock them)

- Schemes: `file`, `javascript`, `vbscript`, `data`, `blob`, `shell`, `ms-msdt`
  (Follina, CVE-2022-30190), `search-ms`/`search` (UNC lure), `ms-officecmd`,
  `ms-appinstaller` (disabled by Microsoft after abuse), `ms-word`/`ms-excel`/
  `ms-powerpoint` and other Office `ofe|u|` handlers (remote documents), `hcp`,
  `its`/`ms-its`/`mk`, `jnlp`, `ldap`, `res`, UNC `\\host\…` in any form,
  credentials in URLs (`user:pass@`).
- Executables and script types opened as documents (`AssocIsDangerous` on
  Windows, executable UTI/bit elsewhere).
- Password fields, secure desktop/UAC, elevated windows, Grain's own windows.
- Launching by path or with arguments.

### 6.3 Prompt injection (the main risk)

The Agent reads untrusted text: web pages via Exa, MCP results such as issues,
email and Notion pages, and the user's screen. Grain Control turns words into
OS actions, so a poisoned page could try to make it:

- open a phishing URL, or a URL whose query **exfiltrates** private data the
  Agent read (`https://evil.example/?d=<notes>`). This is the "lethal trifecta":
  private data + untrusted input + an outbound channel;
- paste attacker text into a terminal and press Enter (command execution);
- launch an installer or LOLBin app; spam tabs.

Mitigations:

- **Taint flag per task.** Once any non-Grain-Control tool result (MCP/Exa) or a
  screen image enters the task, every Grain Control action that carries
  model-composed free text to a destination becomes **Confirm**: `open_link` with
  a non-template URL, compose/prefill templates, deep links with text,
  `type_text`. Pure navigation (search YouTube for the user's own words, open an
  app, media) stays Safe. User-typed URLs are not tainted.
- Terminals/consoles always Confirm for typing and refuse Enter without a fresh
  confirmation. Detection already exists (`foreground_is_terminal`).
- Per-task caps (for example 5 launches/links, 3 Enter presses) and Esc cancels
  as today.
- Confirm cards show the **full** URL (scheme and host highlighted) and the full
  text, never a model summary.

### 6.4 OS permission prompts

| OS | Needed for | Prompt |
| --- | --- | --- |
| Windows | nothing beyond today | none (UIPI silently blocks elevated targets, so report it) |
| macOS | AX insert/focus, `CGEventPost` (Enter, media keys) | **Accessibility**, already requested for paste |
| macOS | AppleScript to specific apps (only if P4 adopts it) | **Automation**, per target app (TCC), needs `NSAppleEventsUsageDescription` |
| macOS | window titles of other apps | Screen Recording (avoid: use app names only) |
| Linux Wayland | key injection | RemoteDesktop portal consent, or a ydotool daemon setup (already Handy's paste story) |
| Linux | AT-SPI | accessibility bus enabled (`toolkit-accessibility`); Chromium/Electron need a flag |

## 7. Risks and limits

| Risk | Note / mitigation |
| --- | --- |
| Focus stealing | Windows' foreground lock can refuse `SetForegroundWindow` if Grain is no longer the foreground process (voice summon without the notch taking focus). Fallbacks: AppsFolder re-activation; report "couldn't bring X to front" rather than type blind |
| Paste into the wrong place | Typing is gated on a re-probed editable focus in the *target* window. Paste Catch's post-flight still runs and holds the text on a miss |
| Chromium/Electron accessibility | Chrome ≥138 has native UIA. Older Electron apps expose little until an AT is detected, which makes `SetValue` (M) weaker than paste (H), so paste stays the default |
| Deep links rot | Curated registry entries carry a "last verified" date and handler check; unknown apps fall back to `open_app` + `type_text` |
| AI-chat URL params | Undocumented: ChatGPT `?q=` may auto-submit (Tenable TRA-2025-22 flagged it). Mark those templates Confirm when tainted, or omit them |
| Wayland | No generic window focus or listing; key injection needs portal/ydotool. Linux reach is mostly links, apps, media and AT-SPI text |
| Model over-reach | "Open the bank site and pay the bill" stops at navigation. There is no click tool, so payment, purchase and send remain the user's |
| Agent policy change | MCP extensions currently auto-approve. Grain Control reintroduces per-call confirmation for its own irreversible actions only (Q3) |

## 8. Phasing

| Phase | Scope | Done when |
| --- | --- | --- |
| P0 Plumbing | `grain.` provider kind in `capability.rs`/`action_exec.rs`; host risk from policy; confirm path for this provider; enable setting + Extensions card; `grain_core::control` with policy/template tests (recover `validate_open_url` tests from `0eb9ce667`) | Agent lists and calls a stub tool; Confirm card round-trips |
| P1 Windows v1 | The 8 tools on Windows; AppsFolder launch/focus; UIA focus probe reuse; Core Audio; curated deep links + handler check; taint flag | Real-app acceptance via `tests/agent-harness/`: YouTube search, open app, type into Notepad, Enter confirm/decline, refusal cases |
| P2 macOS | NSWorkspace links/apps, catalogue, AX focus/insert, `CGEventPost`, CoreAudio, media | Same scenarios on macOS |
| P3 Linux | xdg/gio, `.desktop` catalogue, MPRIS, wpctl, AT-SPI insert, existing paste paths | Links, apps, media, type (X11 + one Wayland DE) |
| P4 Element tier (optional) | `focus_field(name)`, `press_button(name)` (always Confirm), macOS Shortcuts by user-allowlisted name, user custom schemes | Decided after v1 use |

## 9. Open questions for the user

1. **Name:** "Reach" (recommended), "Grain Control", or something else?
2. **Default:** off until enabled from the Extensions page (recommended), or on for everyone?
3. **Enter/send:** always ask (recommended), or let the user mark specific apps
   (for example Slack) as "send without asking"?
4. **Typing into the app you summoned from:** no confirmation (recommended), or
   always confirm?
5. **Taint rule:** OK that after web/MCP content is read, links and typed text
   need a tap to confirm? This is the main defence against prompt-injected exfiltration.
6. **Which deep links matter most to you?** The seed list is in §3.2. Contacts
   aren't known (WhatsApp needs a number, Teams an email). Should a later step read
   contacts?
7. **Shortcut keys:** v1 has only Enter/Tab/Esc. Do you want app shortcuts
   (Ctrl+K, Ctrl+L) or a user-defined allowlist?
8. **Element tier (P4):** press buttons and focus fields by name. Is that within
   "no computer control" for you, or out of bounds?
9. **Platforms:** Windows-only v1, with macOS/Linux after?
10. **Browser beyond navigation:** accept "navigate + prefill only" (recommended),
    or explore UIA/AX inside web pages later?
