# Agent Browser: plan

Replaces Reach (removed 2026-11-15). Reach was Grain's own OS-automation tool
set: maintaining a computer-use system is not where a dictation-first app
should spend its time. The Agent now browses through
[`agent-browser`](https://github.com/vercel-labs/agent-browser) by Vercel Labs
(Apache-2.0), which its authors maintain. Grain owns only a thin, fixed
mapping from Agent tools to CLI commands.

## Decisions

- **Name / id:** "Agent Browser", extension id `grain.agent_browser`, Extensions
  page id `agent-browser`, setting `agent_browser_enabled` (default off).
- **Compiled provider, not stdio MCP.** `agent-browser mcp` exists, but Grain's
  extension platform deliberately has no local stdio/process MCP transport
  (`docs/Extensions 2.0/EXTENSION-ECOSYSTEM-BLAST-RADIUS-AUDIT.md`). One
  first-party provider with a fixed command allowlist gives the process
  boundary that milestone asks for (no shell, fixed argv, owned lifetime)
  without opening a general transport.
- **User-installed CLI.** Grain does not bundle or download the binary or a
  browser. The user runs `npm install -g agent-browser` and
  `agent-browser install`; `agent-browser upgrade` updates it. Connect checks
  the install and says exactly what to run when it is missing or too old.
- **Minimum version 0.43.0**, the contract verified on 2026-11-15:
  `{success,data,error}` JSON envelope, `--` ending options for `fill`, `tN`
  tab ids. Older releases (e.g. 0.31) type a literal `--`.
- **Headless, own session, ephemeral profile.** Every call passes
  `--session grain`, never the machine-wide default session. No profile or
  saved auth: the Agent's browser never sees the user's own cookies.
- **No per-call confirmations**, matching the current Agent policy (enabling a
  tool extension authorises its calls). Read-only tools are classed `Safe`,
  the rest `Confirm`, so a future confirmation policy has the data.

## Layout

| Piece | Where |
| --- | --- |
| Tool schemas, argument parsing, argv, reply rendering, version gate, binary candidates (pure, unit-tested) | `crates/grain-core/src/agent_browser.rs` |
| Link policy shared with Reminders (moved out of Reach) | `crates/grain-core/src/links.rs` |
| Binary discovery + version cache, one CLI process per call, session close on switch-off / exit | `src-tauri/src/grain_agent_browser.rs` |
| One registry for compiled providers (enabled, directory, catalogue, digest, call) | `src-tauri/src/grain_compiled_tools.rs` |
| Agent wiring (asks the registry, names no provider) | `capability.rs`, `action_exec.rs` |
| Extensions page row (Connect / Disconnect / switch) | `grain_mcp.rs` (`AGENT_BROWSER_ID`), `McpPresets.tsx` |

## Tools

`open`, `snapshot`, `read`, `click`, `fill`, `press`, `select`, `check`,
`scroll`, `navigate` (back/forward/reload), `wait`, `tabs`
(list/new/switch/close), `close`. Each maps to one `agent-browser` subcommand.

Deliberately not exposed: `eval` (arbitrary JavaScript), `upload`, cookies,
storage, `state`, `auth`, network routing, `--cdp`/attach to the user's
browser, screenshots. Add one only with a reason and a test.

## Safety rules (enforced in `agent_browser.rs`)

- Navigation is web-only: `open` and `tabs new` take `http(s)` links that pass
  `links::validate_link` (no credentials in URLs). `agent-browser` itself
  opens `file://`, so this check is the boundary.
- Refs (`@eN`), tab ids (`tN`) and keys are closed forms; values that sit where
  the CLI parses flags may not start with `-`; fill text follows `--`.
- Lengths are bounded and control characters refused.
- Page text reaches the model clipped to 12 000 characters and prefixed as
  untrusted content.

## Lifetime

- Grain keeps no thread, socket or child between calls; each call is one CLI
  process with a timeout (40 s, 60 s for navigation), then killed.
- `agent-browser`'s daemon keeps the browser between calls. Grain sets
  `AGENT_BROWSER_IDLE_TIMEOUT_MS` to 10 minutes on every call, runs `close`
  when the extension is switched off or disconnected, and at app exit
  (bounded to 4 s, skipped if a call is in flight).
- The first command starts the daemon, which inherits the CLI's stdout. A pipe
  would never reach end-of-file, so stdout goes to a per-call temp file and
  Grain waits on the CLI process, not the pipe.

## Follow-ups (not built)

1. Optional "show the browser window" (`--headed`); headed browsers are not
   idle-closed upstream, so it needs its own close rule.
2. Optional persistent profile for sites the user wants the Agent signed in to.
3. Optional Grain-managed install (pinned npm platform package + integrity
   hash) for a one-click Connect.
4. Screenshots into the Agent's vision path.
