# Transition Log — Extension Platform implementation handoff

Written 2026-07-21 at the end of the design + Phase 0 + Phase 1 session, for
whoever (human or agent) continues in a fresh context. Read this, then
[SPEC.md](SPEC.md) (the single normative doc), and you have everything.

---

## 1. Where things stand

| Piece | State |
|---|---|
| Design docs (7) | **Complete and internally consistent.** SPEC.md is normative; PLAN / STRESS-TEST / CASE-HEYCLICKY / CAPABILITY-GOVERNANCE / FREEDOM-LADDER are rationale-only (bannered); README is the plain-language entry. |
| **Phase 0** — secure transport + grain-sdk | **SHIPPED** (`f05d73a0`). |
| **Phase 1 chunk 1** — registry, built-ins, Overview tab | **SHIPPED**. |
| **Phase 1 chunk 2** — .grainpack import/export, prompt packs, centre-variant gating | **SHIPPED**. |
| Phase 1 remainder | Pill-theme **rendering** (packs already import/store); permission sheet (deliberately deferred until something requests permissions). |
| **Phase 2 steps 1–3** — protocol frames, extension tokens + scripted manifests, host API router | **SHIPPED** (`11173b4e`, `bbe6a60c`, `457d0f37`). |
| **Phase 2 steps 4–8** — read-loop refactor + extension_host lifecycle, JS supervisor+worker, transform hook, session:start plumbing, dogfood | **NOT STARTED.** Guide is complete and prescriptive for all five. |

**Phase 2 progress detail (start the next session at Step 4 of
[PHASE2-GUIDE.md](PHASE2-GUIDE.md)):**
- **Step 1 done** — `grain-sdk/protocol.rs`: `ClientRequest`/`ServerResponse`/
  `HostCall`/`HostCallResult` wrapped in `HostFrame` (externally-tagged →
  `{"req":…}`/`{"res":…}`/`{"call":…}`/`{"callres":…}`). The
  `protocol_frames_are_mutually_exclusive` test guards discrimination.
- **Step 2 done** — `events_server::mint_extension_token(id, caps)` +
  `revoke_token` (both `#[allow(dead_code)]` until Step 4 calls them).
  `manifest.rs`: `activation` + `entry_source` fields; `validate()` branches by
  tier (Scripted requires entry_source + `KNOWN_CAPABILITIES`; Native rejected;
  Pack rejects entry_source). `is_scripted()`. `KNOWN_CAPABILITIES` lists
  `session:start` though undogfooded.
- **Step 3 done** — `src-tauri/host_api.rs`: `dispatch(app, identity, method,
  params)`, capability check first. Methods: log.info/warn, storage.*,
  settings.* (own `ext.<id>.*` namespace, no AppSettings path), llm.complete
  (via new `grain_post_process::complete_for_extension` — keys stay host-side),
  embed + session.start return clean "not implemented". Pure/tested:
  `has_capability`, `required_capability`, `ExtStorage` (quota, settings
  isolation).
- **Step 4 is next.** Its two hard parts, both in the guide: (a) the
  `events_server::handle` read-loop refactor to a **single-writer mpsc** so
  responses/host-calls and event broadcasts share the `write` half without
  borrow fights (add a third select arm); route `HostFrame::Request` →
  `host_api::dispatch` (spawn, reply via the mpsc). (b) `extension_host.rs`:
  supervisor-webview lifecycle, worker registry, activation dispatch (carry the
  triggering event as `activation_event`), the 120 s idle reaper. Rate-limiting
  lives here (per-connection frame counter), NOT in host_api.

Verification state at handoff: 233 src-tauri lib tests, 5 workspace suites
(grain-core 14 · grain-pill 13 · grain-sdk 4 · provider-router 22 ·
rolling-window 62), `tsc --noEmit` clean, ratchet green, everything pushed.

## 2. What was built, file by file

### `crates/grain-sdk` (NEW — the public contract, dependency LEAF)
- `event.rs` — `DaemonEvent` (~39 variants) + `PillAction` + `SessionMode` +
  `AgentInputKind` + `OverlayPosition`, moved **verbatim** from grain-core.
- `protocol.rs` — `ClientHello` / `ServerWelcome` / `GRAIN_API_VERSION`
  ("1.0"). First-frame WS auth; `client` field is a log label, **never
  identity**.
- `manifest.rs` — Phase 1 subset: `ExtensionManifest`, `Tier`,
  `PromptPackEntry`, `PackPayloads` (`pill_theme` opaque JSON), `GrainPack`
  (one-file `.grainpack`), `validate()` (reverse-dns id, `grain.` prefix
  reserved, tier-A only, **no permissions on inert packs**).
- Depends only on serde/serde_json/specta. grain-core re-exports everything at
  old paths (`pub use grain_sdk as event;`), grain-pill depends **only** on
  grain-sdk.

### `crates/grain-core`
- `extensions.rs` (NEW) — `ExtensionsRegistry`: owned JSON `extensions.json`
  (separate from AppSettings). Pack records + **toggle order** (SPEC §4.4:
  enable assigns `next_toggle_seq`; re-enable moves to end; built-ins tracked
  via `builtin_toggle_seq`; never-toggled = `u64::MAX`). Corrupt file →
  reinitialize, never brick. Constants: `BUILTIN_SNIPPETS/CONTEXT/AGENT`,
  `AGENT_CENTER_VARIANT_ID` (`grain.agent-center-layout`).
  `apply_prompt_pack`/`remove_prompt_pack`: `ext:<extid>:<id>` namespacing,
  idempotent apply, selection-healing removal.
- `settings.rs` — new fields `snippets_enabled`, `agent_enabled`,
  `extensions_imported_v1` (all default **false** = new-install default OFF).
- `context.rs` — `import_extension_flags_v1` inside `load_settings`: the
  **§10.1 upgrade rule**, once per install. Existing users (file pre-existed):
  snippets on iff any configured; agent on always. **`file_preexisted` is
  captured BEFORE the fresh-install branch persists defaults** — moving that
  check later reintroduces a bug that was caught in review.
  `settings_file_exists()` exported for the shell.

### `src-tauri`
- `src/events_auth.rs` (NEW) — pure: `TokenRegistry` (token→identity→caps),
  `authenticate()` (first frame), `allows_event()` (granularity:
  `events:transcripts` / `events:audio-levels` / `events:sessions`),
  `allows_reverse()`. 4 security tests incl. "A's token cannot act as B".
- `src/events_server.rs` — mints a 244-bit token per run (2×uuid), injects
  `GRAIN_EVENTS_TOKEN` into the pill spawn env, authenticates first frame on a
  3s deadline, sends `ServerWelcome`, filters every outbound event, gates the
  reverse channel. **Debug builds also accept token `"grain-dev"`** (manual
  `cargo run -p grain-pill`); release accepts only the minted token.
- `src/grain_commands.rs` — `ExtensionCard`, `extensions_overview`,
  `extension_set_enabled` (built-ins → settings flags + `touch_builtin_toggle`;
  agent toggle re-registers its binding; centre-variant disable falls
  `agent_panel_position` back to `side`; packs → registry + payload
  apply/remove), `extension_import_pack` / `extension_export_pack` /
  `extension_uninstall` (keep-by-default, `purge` flag). Pack files:
  `<data>/extensions/<id>.grainpack.json`.
- `src/lib.rs` — registry constructed at startup (`settings_preexisted`
  captured BEFORE `AppContext::new`); 6 commands registered.
- `src/agent.rs` — `summon()` guards on `agent_enabled` (Recall/Capture stay
  under `grain_space_enabled` — deliberate).
- `src/handy/shortcut/{handy_keys,tauri_impl}.rs` — `summon_agent` skipped at
  registration when `!agent_enabled` (the `grain_space_enabled` pattern).
- Snippet gates: `grain_actions.rs`, `stt_router.rs` (empty slice when
  disabled), `handy/audio_toolkit/grain_text.rs` (`finalize_batch_text`).
- `src/grain_actions.rs` also holds `mirror_stream_text` etc. from earlier
  isolation work — unrelated to extensions but shares the file.

### Frontend (`src/components/settings/experimentations/`)
- `OverviewSection.tsx` (NEW) — SPEC §5.1 master list: toggle-order sort,
  inline switch, hover description, tier/version chip, repo link, disabled
  "Browse extensions — coming soon" store affordance. **Uses raw
  `invoke()` + a local `ExtensionCard` mirror type** — bindings.ts is
  generated; regenerate on the next dev run, then optionally switch to typed
  bindings. `toggle_seq` crosses as a **string** (u64 vs JS numbers).
- `ExperimentationsSettings.tsx` — tabs are now Overview (default) / Snippets /
  Context / Agent. **Actions merged under Snippets** below a thin divider
  (SPEC §5.4 — the exact `snippets.after` anchor position).
- `AgentSection.tsx` — "Center panel" dropdown option only while
  `grain.agent-center-layout` is enabled.

## 3. Next work, in order

1. **Live smoke test on a dev run** (blocked this session: a second app
   instance fights the user's running Grain for port 7124). Verify: pill
   connects through the token path (log line `events WS: 'pill'
   authenticated`), Overview tab renders/toggles, tab merge looks right,
   bindings.ts regenerates (then optionally swap OverviewSection to typed
   bindings).
2. **Phase 1 finish — pill-theme rendering** (SPEC §9): named patterns first
   (`breathe`/`sweep`/`static` + per-state backgrounds/dot colours), the
   expression evaluator later. Delivery route to decide: likely a
   `DaemonEvent::PillTheme` (additive) emitted on connect + change, or theme
   JSON via env at pill spawn + event for live switch. Missing state → Grain's
   default FOR THAT STATE; 3 strikes → default theme. `pill.theme` slot
   occupancy on enable (slots machinery can start minimal: one registry field).
3. **Phase 2** (re-review first per §10.3): supervisor webview + **one Worker
   + one WS connection + one token per extension** (SPEC §7.1 — NEVER a shared
   realm; that was a caught security flaw), extension tokens in
   `events_auth::TokenRegistry` (revocation already implemented), host API
   bridge, transform hook (activates at **session start**, not
   pipeline-reach — 300ms wake vs 150ms budget), idle reaper, **the two
   structural capabilities** (`session:start` + `contributes.sessionMode` with
   its slow stage), dogfood = port auto-categorization.
4. Phase 3+: per SPEC §8 conformance table.

## 4. Gotchas that will bite you (all learned the hard way)

- **Never launch the app / tauri dev while the user's Grain is running** —
  port 7124 + global shortcuts + `C:/gt` target-dir lock collide. Build with
  `CARGO_TARGET_DIR=C:\gtc`; never kill the user's app.
- **Windows build env**: `env -u LOCALAPPDATA -u TEMP TMP='C:\Windows\Temp'
  CARGO_TARGET_DIR='C:\gtc' cargo …` (transcribe-cpp junction workaround).
- **Ratchet measures HEAD, not the working tree**: commit code first, then
  `python Upstream/ratchet.py`; on justified growth `--update` + follow-up
  commit. Budgeted-file hooks so far: lib.rs 631, Cargo.toml 86,
  handy_keys 52, tauri_impl 44 — keep hooks tiny and `[GRAIN]`-marked.
  **Never add features inside `src-tauri/src/handy/`.**
- **`bindings.ts` is generated — never hand-edit.** New commands → raw
  `invoke()` + local mirror types until a dev run regenerates it.
- **Sync bot pushes to main every ~2h** → on rejected push:
  `git pull --no-rebase` (NEVER rebase — flattens the graft merge), push again.
- **Upstream sync discipline**: prefer merge over cherry-pick; ancestry drift
  is auto-detected (`python Upstream/sync_upstream.py`); close out with
  `git merge -s ours` then `ratchet.py --update`. Runbook: `Upstream/UPSTREAM.md`.
- Commit messages: clean, human, no AI attribution. Commit each verified chunk
  promptly (the user rebases/resets main; uncommitted work gets wiped).
- Every phase: **re-review before building** (SPEC §10.3), record
  kept/changed/why in the phase commit. Scope trims so far: manifest structs
  waited for their consumer (twice), pill-theme rendering deferred to its own
  session — all recorded.

## 5. Decisions you must not re-litigate (user-confirmed)

Quick Panel is being retired — build nothing on it. Extensions never own
windows (surfaces, host lifecycle). Rust WS boundary is the security wall;
one Worker/connection/token per extension. Toggle order (not install order).
Overview = first tab; tab bar never grows. Store = slide-in inside settings
window, Zen-style. Core = basic snippets + basic context awareness + sidebar
agent, all OFF for new installs, upgrade rule for existing users. Centre
agent layout = installable pack (the surface-variant dogfood). Pill themes:
main pill only, all-four-states with per-state fallback, `reactive:false`
allowed, size/interactivity locked v1. Providers implement host-defined
interfaces (never own APIs — the Thunderbird/XUL lesson). Freedom ladder
rung 4 (companion binaries) is the "build it yourself" answer.

## 6. Memory files

`extension-platform.md` (the platform, current), `handy-isolation.md`
(src/handy layout + ratchet), `upstream-merge-strategy.md` (sync + ancestry
drift). All current as of this handoff.
