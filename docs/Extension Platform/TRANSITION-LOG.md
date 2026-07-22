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
| Phase 1 remainder | Pill-theme **rendering** (packs already import/store). |
| **Phase 2 — ALL STEPS** | **SHIPPED.** Steps 1–3 (`11173b4e`, `bbe6a60c`, `457d0f37`), step 4+6 (`b7a22a61`), step 5 (`ddf5f93e`), step 8 (`15d4d633`), Workers refactor + round-trip tests (`29da398d`), step 2's grant flow (the half the first step-2 commit left out). |
| **Phase 3 step 1** — manifest grows `surfaces`/`slots`/`contributes` | **SHIPPED**, then corrected against SPEC §4.1/§4.3 (see Phase 3 detail below). |
| **Phase 3 step 2** — the slots registry | **SHIPPED.** Exclusive positions, core defaults as occupants, explicit takeover. |
| **Phase 3 step 3** — schema settings render | **SHIPPED.** Levels 1–2, all five anchors mounted, auto-categorize dogfoods it. |
| **Phase 3 step 4a** — `contributes.shortcuts` | **SHIPPED.** Namespaced `ext:<id>:<sid>`, toggle-order arbitration, status rows. |
| **Phase 3 steps 5–10** | **SHIPPED 2026-07-22.** workspace (5a/b/c), overlay (6), pill theme (7a–d), embed/capture/doc (8), store shell (9), Grain Space Test walked (10, [PHASE3-REVIEW.md](PHASE3-REVIEW.md)). See detail below. |
| **Phase 3 step 4b** — chunk 2b (`sessionMode` + a working `session.start`) | **NOT STARTED — the one STRUCTURAL gap, now the top Phase 4 item.** Reserved + plumbed (returns "not implemented"); an extension can't start its own recording session yet. |
| **✅ GATE — distribution platform + developer mode** | **LIFTED 2026-07-22.** Designed in [DISTRIBUTION-PLAN.md](DISTRIBUTION-PLAN.md), evidenced by [DISTRIBUTION-RESEARCH.md](DISTRIBUTION-RESEARCH.md); requirements preserved in [GATE-DISTRIBUTION-AND-DEVMODE.md](GATE-DISTRIBUTION-AND-DEVMODE.md). **New build order: 3.5 (developer mode) → 4 → 5A (trust rails) → 5B (registry).** The Phase 3 store step 9 remains a SHELL, filled in 5B. |
| **Phase 3.5 — Developer Mode & SDK** | **IN PROGRESS. Steps 1–4 shipped 2026-07-22:** WS `Origin` hardening, `grain-ext init`, in-app load-unpacked/dev overrides, and `grain-ext dev` incremental build + authenticated hot reload. **Step 5 (source maps) is next.** |

**Phase 3.5 step 4 detail.** `grain-ext dev` performs one normal build, keeps
the project's build command alive in incremental `--watch` mode, watches its
`dist/` output and `manifest.json`, and reuses one developer WebSocket. The
credential is a role-bound dev-control token in
`<app-data>/extension-dev-token.json`; it is created only while Developer mode
is enabled (mode `0600` on Unix), revoked/removed on disable or exit, and cannot
invoke extension host APIs. Reload re-reads the already human-approved folder,
revalidates the pack, intersects grants, replaces a live worker generation
without tearing down its supervisor, and remounts sandboxed surface iframes.
Worker/surface/pill/dev-control roles are now explicit; stale worker sockets
cannot detach a replacement generation. Ten-generation tests hold worker and
token registry counts constant. The CLI prints reload latency and both counts;
the live gate passed in an isolated portable app: ten distinct edits reloaded
in 3–4 ms, worker/token counts stayed at 1/4, and the app process tree stayed at
11 processes. Private memory moved from 331.9 MB after the first stable reload
to 333.7 MB after ten, then settled flat at 333.7 MB (allocator noise, not
per-generation growth). The live pass also fixed two lifecycle defects it
exposed: esbuild now uses `--watch=forever` when the CLI has no interactive
stdin, and extension shortcut reconciliation retries after the core shortcut
backend initializes while remaining idempotent across every hot reload.

**Phase 2 is complete against the guide's definition of done.** What shipped,
beyond steps 1–3 detailed below:

- **Step 4** — `src-tauri/src/extension_host.rs`: hidden supervisor webview
  (`extension-host` label, created on first worker need, torn down with the
  last worker), a `Workers` registry, activation dispatch carrying the
  triggering event as the injected `activation` payload, host calls under a
  deadline, and a 30 s-tick reaper killing workers idle > 120 s (token revoked).
  `events_server::handle` now funnels **every** outbound frame through one mpsc
  so the socket has a single writer, and routes inbound `HostFrame`s —
  `Request` → `host_api::dispatch`, `CallResult` → its awaiter. Pill path
  untouched.
- **Step 5** — `extension-host.html` (a second Vite entry) + `src/
  extension-host.ts` (supervisor: one blob-URL Worker per extension, reports
  `ready`/`died`) + `src/extension-runtime.ts` (`GRAIN_RUNTIME_JS`: the worker
  shim that opens the extension's own WS, sends its own token, and exposes
  `grain.*`). **`csp` is `null` in tauri.conf.json**, so the guide's blob-worker
  / `connect-src` CSP pitfall does not apply here. Capability file
  `capabilities/extension-host.json` grants only `core:default`.
- **Step 6** — `run_transforms` in toggle order, 150 ms hard deadline per
  extension, cold workers skipped (never block the paste), empty reply
  suppresses, 3 strikes → auto-disable + the new additive
  `DaemonEvent::ExtensionDisabled`. One `[GRAIN]` call in
  `process_transcription_output`.
- **Step 7** — `session:start` is reserved in `KNOWN_CAPABILITIES`,
  capability-checked in the router, and returns a clean "not implemented yet".
  **The sessionMode slow stage itself is deliberately chunk 2b** (the guide
  permits this split; the capability name exists from day one so a Phase-3
  extension can't discover a gap).
- **Step 8** — `grain.auto-categorize` seeded at startup as the first scripted
  built-in (default OFF, pre-granted as first-party).
- **Step 2's grant flow** (missing from the original step-2 commit):
  `extension_set_enabled` holds a scripted pack at first enable and returns
  `{"needsPermissions":[…]}`; `extension_grant` records approval clamped to the
  manifest; Overview renders the plain-language permission sheet.

**Verification:** 243 src-tauri lib tests (7 in `extension_host`, incl. a
host-call round-trip against a Rust-level fake worker, deadline expiry, reaper
victim selection, strike threshold), `tsc` clean, `vite build` emits
`dist/extension-host.html`, ratchet green, all pushed.

**Live smoke test — DONE 2026-07-21** (user closed their Grain; `bun run tauri
dev`). Verified: app starts clean; `events WS: 'pill' authenticated` (the token
path works end to end); `grain.auto-categorize` seeded to
`<data>/extensions/` with `enabled:false` + pre-granted caps;
`extension-host.html` serves from the Vite dev server and its supervisor
transpiles with `@tauri-apps/api/event` resolved; **`bindings.ts` regenerated**
(+118 lines, all extension commands now typed — `OverviewSection` may switch to
typed bindings whenever convenient). **Still unproven: an actual worker spawn
end to end** — that needs a real dictation (mic + configured LLM provider).
Expect `ext-host: spawning 'grain.auto-categorize'` then a reap ~120 s later.

**Still open from Phase 2:** a real RAM measurement of the reap, and chunk 2b
(sessionMode slow stage, folded into Phase 3 step 4).

---

## 1b. Phase 3 progress + what the next session must know

**⚡ The performance rule now governs everything (user requirement, and a real
bug that shipped).** A feature must never be slower *because* it is an
extension. The smoke test exposed that `grain.agent-center-layout` ships
**enabled with no pack file**, so "nothing installed" is never the real runtime
state — there is always an enabled record to iterate. Consequently
`run_transforms` was reading+parsing a manifest from **disk on every
transcription**, and `on_event` did the same **plus** `serde_json::to_value` on
**every** broadcast including `AudioLevel` (many/second while recording).

Fixed by `extension_host::refresh_index()` building a cached index
(`by_event: variant → ids`, `transforms` pre-sorted by toggle order) behind
`HAS_ACTIVATIONS` / `HAS_TRANSFORMS` atomics. Both hot paths are now **one
relaxed load and a return** when nothing is enabled. The event tag comes from
`DaemonEvent::variant_name()` — an **exhaustive** match, so adding a variant is
a compile error rather than a silently dead activation. `refresh_index` is
called from `start()` and from every registry mutation
(enable/disable/grant/import/uninstall/auto-disable) — **if you add another
registry mutation, call it too, or the hot paths go stale.** Recorded as a
Phase 3 non-negotiable in the guide and as the `extensions-must-feel-native`
memory.

**Step 1 shipped, then corrected.** The manifest grew `surfaces` (workspace /
overlay), `slots`, and `contributes` (settings schema + shortcuts); `ANCHORS`
and `KNOWN_SLOTS` are pinned contract surface. A SPEC re-read then caught three
flaws worth internalizing, because all three are the *expensive* kind:
1. The v1 anchor list **contradicted SPEC §4.3** (invented `space.after`,
   omitted `dictation.pipeline.after` and `models.after`). Now verbatim, with a
   test pinning it.
2. Unknown anchors were **rejected**; §4.3 requires **fallback to the
   extension's own section** (settings are never lost). `ANCHORS` drives
   rendering, not validation.
3. An unknown setting `kind` failed to **deserialize**, killing the whole pack.
   `SettingKind` now has `color`/`slider` plus `#[serde(other)] Unsupported`.

**Step 2 shipped — the centre-variant gotcha is CLOSED.**
`grain.agent-center-layout` has a registry record but **no `.grainpack.json` on
disk**, so nothing manifest-derived can report that it competes for
`agent.reply-surface`. Resolved three ways, all verified against the live
registry file:
- `ExtensionRecord.slots` carries the declared slots, copied at install, so
  occupancy never needs a disk read. `load()` **backfills** the centre variant's
  slot (and heals registries written before `slots` existed).
- `load()` seeds every `KNOWN_SLOTS` entry with `CORE_DEFAULT` (`grain.core`),
  making SPEC §3.2's "core defaults are occupants" literally true in storage. A
  slot is therefore never *free*, so even a first claim prompts.
- The centre variant's **claim** is reconciled from `agent_panel_position`
  (`grain_commands::sync_agent_reply_surface_slot`, called at boot, on toggle,
  and on position change) because SPEC §10.2 says enabling it only adds it to
  the dropdown — *selecting* it takes the slot. That function never overwrites a
  third-party occupant.

`set_enabled` refuses a contested enable (belt and braces behind the command
layer's structured `{"slotConflict":…}`), and `take_slot` disables the incumbent
in the same transaction. **If you add a new slot consumer, release on disable
AND on uninstall** — `release_slots_locked` handles both today.

**Open contract question (still not decided):** `slots` validation accepts any
`overrides:<setting>`. SPEC §4.2 publishes an allowlist for core-setting
*reads*, but no allowlist exists yet for override *targets*. Left permissive
deliberately rather than inventing contract surface. Step 2 did not need to
decide it (nothing consumes `overrides:` yet); **the first consumer must**.

**Step 3 shipped — where the settings contract is actually enforced.**
`grain-sdk/settings_schema.rs` is the single rule table: `coerce` (write) and
`resolve` (read). It is called from **two** places and both matter —
`extension_setting_set` (the host's control) and `host_api`'s `settings.set`
(the extension writing to its own namespace over the WS). A schema enforced
only in React is not enforced: the extension can reach the same keys directly.
If you add a third writer, route it through `coerce` too.

`ExtensionSettingRow` is a deliberately **flat DTO**, not the manifest type —
`SettingKind` is internally tagged with per-variant fields and crosses the
bindings boundary badly. `specta` gained its `serde_json` feature so the value
itself crosses as `JsonValue`.

**Step 4a shipped — contributed shortcuts.** Registered through the *existing*
binding registry as `ext:<extension-id>:<shortcut-id>`; `handle_shortcut_event`
gets one `[GRAIN]` hook that prefix-matches before any `ACTION_MAP` lookup.
Arbitration lives in a **pure** `extension_shortcuts::plan()` (7 tests): core
always wins, then extensions in **toggle order**, loser inactive-until-rebound
with the holder named. `sync()` **always** defers onto the async runtime — see
the deadlock rule below — and is called from inside `refresh_index`, so every
registry mutation reconciles shortcuts for free.

**Manifest ids and shortcut ids may not contain `:`** (validated at import),
because that is what makes `ext:<id>:<sid>` unambiguously parseable. Do not
relax this without changing `parse_binding_id`.

**A slot bug the tests could not have caught — read before touching slots.**
Toggling the Agent centre layout on failed live with *"agent.reply-surface is
occupied by grain.core"*. Both halves of Step 2 were individually right and
together wrong: every slot is seeded with core as occupant (so no claim looks
uncontested), and `set_enabled` claimed every declared slot — which makes the
one pack whose design is "enabling only adds it to the dropdown" impossible to
enable. Fixed by splitting the record's declaration in two:
- `slots` are **claimed on enable** (a pill theme, an output destination);
- `variant_slots` are **offered** (SPEC §10.2 surface variants) — enabling adds
  the pack to a host-owned chooser and a core setting decides occupancy, so it
  changes no occupant and cannot be a takeover.

`slot_conflict` reads `slots` only; `take_slot` accepts either; release is by
**occupancy**, so a selected variant still hands the slot back on disable.
`heal_slots` migrates old registries. **No manifest syntax offers a variant slot
yet** — the name is reserved, the shape waits for a real third-party consumer,
so `install` preserves what `heal_slots` backfilled instead of clearing it.

**Step 5a shipped — `workspace` extracted, behaviour and RAM unchanged.**
`surfaces/workspace.rs` is the host-owned generic keyed by surface id;
`grain_space/window.rs` is a 105-line caller holding only Grain-Space facts
(geometry, event names, the `is_enabled` gate, "payload is a note id"). Carried
over *verbatim* because they are the load-bearing parts: the unmount-then-hide
handshake, **both** fallback timers, the stale-ack guard, the async-runtime hop
(tauri#3990), and the WebView2 TrySuspend work. Generalized: per-surface `AWAKE`
(was a module static) and a JSON payload stash (was `FOCUS_NOTE`). Grain Space's
embedding-engine teardown is now an `on_sleep` hook, so the generic knows nothing
about embeddings.

Verified by driving the real hotkey — the only way to check a RAM profile:
build 763→906 MB (+1 webview process), sleep 906→762.7 MB (process retained),
wake 763→826 MB (cheaper than a cold build), focused re-sleep 826→763 MB exact
baseline. **Zero "ack timed out" warnings**, which is the proof the frontend
handshake still round-trips rather than the fallback quietly covering for it.

**Step 5b shipped — the LRU cap.** `lru_victims` is pure (3 tests). Residency is
capped, access is not: at cap, the least-recently-used sleeps and the incoming
workspace always opens. Only `capped: true` surfaces count and **Grain's own are
not capped** — a core feature is never evicted for an extension.

**Step 5c shipped — extension-facing workspace + its own realm.** An extension
declares `surfaces.workspace` (now with a `ui_source` HTML field); the host opens
one through the same sleeping-window generic. The security is STRUCTURAL, not a
check: the window loads Grain's `extension-surface.html`, and the extension's
markup runs in a **sandboxed iframe** with `allow-scripts` but NOT
`allow-same-origin` → opaque origin, no Tauri IPC, no reach into the wrapper, no
shared global. The wrapper holds the surface token; the iframe reaches the host
only by postMessage-to-wrapper → host-call frame, and every capability is still
checked in Rust. **Identity is derived, never passed**: `workspace.open`/`close`
take no id (host reads the worker channel); the three surface commands read the
calling window's label. Each surface mints its OWN token (≠ the worker's),
revoked on window destroy; disable/uninstall destroy the surface. `min_size` is
clamped against an untrusted manifest; a `ui_source`-less surface is rejected at
import. **Live surface WINDOW e2e (open/iframe-mount/sleep) is deferred to
Step 10's Grain Space Test**, which builds a real workspace extension — the
guide's own sequencing, not a skipped check.

**Step 6 shipped — overlay (transient HUD).** Reuses the *realm* (wrapper +
sandboxed iframe + own revocable token) but not the lifecycle: no sleep, no LRU,
create-and-destroy. Host **budgets** enforce "cannot linger" (SPEC §1.2): size
clamped to a HUD (can't impersonate a window), lifetime a hard cap (auto-dismiss
timer + focus-loss dismiss; an extension that asks for no/too-long a timeout
still gets a self-removing HUD). Host calls `overlay.show(payload)` /
`overlay.dismiss`, capability `surface:overlay`.
- **Race fixed before it shipped:** overlays use UNIQUE per-invocation labels
  (`ext-overlay-<id>-<epoch>`) + a `CURRENT` ext→label map, because reusing one
  label across a replacing `show` would race Tauri's async `win.destroy()`
  against the rebuild. The auto-dismiss timer is epoch-guarded so a replaced
  overlay's timer can't kill its successor.
- **Payload delivery unified** across workspace + overlay: a label-keyed stash
  in `surfaces::extension`, collected by the wrapper via
  `extension_surface_payload` on every mount (fresh build + wake). A freshly
  built surface has no live listener yet, so the opening argument would
  otherwise never arrive; the live `payload_event` still covers an already-awake
  surface.

**Realm plumbing lives in `surfaces::extension`** and is shared: `stage()` (mint
token + park init + bind label→id), `take_init`/`take_payload`/`id_for_label`,
`revoke_for_label` (clears label, token AND payload). Overlay calls into it.

**Bug caught by tsc, worth remembering:** a `//` comment I added INSIDE the
worker runtime's backtick template (`GRAIN_RUNTIME_JS = \`…\``) used backticks of
its own and silently terminated the template string. The whole runtime shim is a
template literal — **no backticks anywhere in its comments or code**.

**Step 7 shipped — pill theme rendering (SPEC §9).** Declarative, data-only; no
extension code runs in the pill.
- **Contract** (`grain-sdk/pill_theme.rs`): `PillTheme` = optional `PillStateTheme`
  per state (idle/recording/processing/fallback); each has `background?`, `dot?`,
  `pattern` ∈ static/breathe/sweep + `#[serde(other)] Unsupported`. Every field
  optional → every gap falls back to Grain's look. **No theme can blank the
  pill** — that SPEC rule is structural, not a check.
- **Rendering** (`grain-pill`): theme mirrors Remote→App, read ONLY by the
  collapsed-pill roll (Studio/agent surfaces stay Grain's — "main pill only").
  `roll_themed_field` paints the whole inner field in the theme colour via three
  pattern renderers; `false` return routes every gap to the existing default
  rolls. Capsule background themed at one site. Verified by a **PNG render test**
  I eyeballed (all three patterns correct, silhouette respected).
- **Delivery** (`src-tauri/pill_theme.rs`): `current()` reads the `pill.theme`
  slot occupant's pack payload → `PillTheme`, `None` for core/garbage. Sent to
  the pill in its **welcome** (connects late, misses broadcasts) AND broadcast on
  change via `refresh_index` (every registry mutation, like the shortcut sync).
- **Pack format**: `payloads.pill_theme` stays opaque `Value` on the wire;
  `validate()` now rejects a malformed theme AND a theme with no `pill.theme`
  slot claim at import.
- **DELIBERATELY DEFERRED, name reserved** (capability-governance doctrine — no
  consumer yet): the per-dot **expression evaluator** and its **3-strike →
  default** (a named pattern can't error per-frame, so there's nothing to
  strike); and **`pill:slots` action chips** (`pill:slots` cap already in
  KNOWN_CAPABILITIES). Build these when a real theme/chip extension needs them.

**Step 8 shipped — the three Grain Space Test gaps closed.**
- **`embed`** now runs Grain Space's own on-device BGE model
  (`grain_space::embed::embed` on the blocking pool), batch-capped at 64. Was a
  clean "not available" stub; this is what makes third-party semantic recall
  buildable without shipping a model.
- **`capture:selection`** (new cap): reads the current selection via the Agent's
  `capture_selection` primitive (synthetic copy → poll → restore clipboard). Its
  own grant; meant to pair with a shortcut trigger.
- **Document store**: `doc.get/put/delete/list`, one file per key under
  `<id>.docs/`, shares the `storage` grant + 200 MB quota. Security-critical bit
  is `ExtStorage::safe_doc_name` — an ALLOWLIST (`[A-Za-z0-9._-]`, reject empty/
  over-long/all-dots/separators) so a key is always a filename, never
  `../secrets`; checked before any path touches disk, exhaustively tested.
- Worker runtime + surface bridge gained `grain.doc.*`, `grain.embed` (→ vectors
  array), `grain.captureSelection` (worker only). No new Tauri commands / no
  bindings change — all via the host-API WS dispatch. Ratchet untouched (no
  Handy-tree edits).

**Step detail for 1–3 (as originally recorded):**
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

## 1c. Session log — 2026-07-22, the gate design pass

No feature code was written this session (one bug fix aside). What happened:

1. **Phase 3 was closed out** — steps 5–10 shipped and pushed, the Grain Space
   Test walked in [PHASE3-REVIEW.md](PHASE3-REVIEW.md) with two gaps recorded
   honestly (`session:start` structural, `pill:slots` chips additive).
2. **The user lifted the hold on the gate** and asked for deep, multi-source
   research before any planning — covering where the store is hosted, how
   authors publish, how it is kept secure, our review dashboard, a way to get
   things verified quickly, a clean install path, and a genuinely good developer
   experience. They also asked that the research be turned back on our *own*
   prior decisions, and that anything we got wrong be corrected and placed in
   the plan.
3. **Research** (~25 sources, primary docs where they exist) is written up in
   [DISTRIBUTION-RESEARCH.md](DISTRIBUTION-RESEARCH.md) as 24 numbered findings:
   prior art compared across seven platforms, the 2023–2026 attack record
   (Open VSX token takeover, GlassWorm, Zed's Zip Slip, download pumping,
   domain-ownership "verified"), the mechanisms worth stealing (TUF's four
   properties, trusted publishing, registry-side builds, sparse static indexes),
   and what makes authoring enjoyable (Raycast, Zed).
4. **The design** is [DISTRIBUTION-PLAN.md](DISTRIBUTION-PLAN.md) — written to be
   read cold, with no prior knowledge of Grain assumed. It answers every
   question the gate raised and places the work as **3.5 → 4 → 5A → 5B**.
5. **One shipped bug was found and fixed** (C-1, `9e0d8db2`) plus nine further
   corrections recorded and placed (§3 above).

**The one judgement call worth knowing about:** developer mode was moved *ahead*
of Phase 4 rather than shipped alongside the store. The reason is C-1 — a defect
that survived Phase 3 because the only way to exercise an extension surface is
to be an extension author, and nobody can be one yet. Every later phase is
verified through that loop, so it goes first.

---

## 2. What was built, file by file

### `crates/grain-ext-cli` (NEW — author tooling)
- `grain-ext init <name> [--id <reverse-dns-id>]` creates a no-overwrite
  scripted project: normative `manifest.json`, `src/main.ts`, SDK-generated
  `grain.d.ts`, README, `.gitignore`, package/TypeScript configuration.
- The scaffold states the Node + esbuild requirement up front. A real generated
  fixture passes `tsc` and bundles with an external source map.

### Phase 3.5 step 3 — load unpacked
- `ExtensionsRegistry` persists one effective identity per id. A dev record
  parks any installed record verbatim and restores it on unload; replacing a
  dev folder never nests or loses that backup. Slots are released while the
  dev copy is disabled, and a restored installed copy never silently retakes a
  slot that another extension acquired meanwhile.
- `dev_extensions.rs` canonicalizes the human-picked folder, bounds manifest
  and entry sizes, enforces project containment and Grain API compatibility,
  then injects the real entry source through the same `GrainPack::validate`
  wall as installed extensions.
- The only Tauri load command takes **no path argument**. It checks the
  separately persisted developer-mode setting, opens a native folder picker,
  and loads only that result. Every mutating developer command also rejects
  callers outside Grain's main settings window. Turning developer mode off
  unloads every dev project and destroys workers, tokens, surfaces, and prompt
  payloads.
- Overview shows a persistent developer-mode chip, permanent `dev` trust
  badges, local paths with explicit unload controls, and the installed copy as
  **“Overridden by dev extension”** when ids collide. Dev extensions use the
  unchanged permission and slot-takeover flows.
- Verification: 29 grain-core tests, 272 src-tauri tests plus both streaming
  smoke tests, and a production frontend build. Repository-wide ESLint and
  `clippy -D warnings` remain red on pre-existing configuration/lints; filtered
  Clippy reported no warnings in the new load-unpacked code.

### `crates/grain-sdk` (NEW — the public contract, dependency LEAF)
- `authoring.rs` — source-project `ExtensionProjectManifest` (`manifest.json` +
  real entry path) and the author-facing `grain` API declaration copied by the
  CLI; reflected event types and the capability union come from SDK types/data.
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
- `extensions.rs` also owns `DevOverride`: the active local folder plus an
  optional boxed installed record, with tested load/replace/unload/uninstall
  semantics.
- `settings.rs` — new fields `snippets_enabled`, `agent_enabled`,
  `extensions_imported_v1`, and `extension_developer_mode` (all default
  **false** = new-install default OFF).
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

Phases 0–3 are shipped; the gate is lifted. **Everything below is specified
step-by-step in [DISTRIBUTION-PLAN.md](DISTRIBUTION-PLAN.md) §10.**

1. **Phase 3.5 — Developer Mode & SDK.** Steps 1–4 (`Origin` hardening,
   `grain-ext init`, load-unpacked, and authenticated hot reload) are shipped.
   The live step-4 latency/RSS gate passed. Continue with source maps →
   developer panel → typed errors → `doctor`
   → author docs → **verify the Phase 3 surface handshake end-to-end with a
   real dev extension** (this is what would have caught C-1, below).
   *Nothing blocks step 5.*
2. **Phase 4 — contract completion.** Top item is the one structural gap:
   `session:start` + `contributes.sessionMode` (chunk 2b, reserved and plumbed,
   currently returns "not implemented"). Then tier-C native, `settings-panel`
   iframes, pill action chips, re-platformed built-ins, plus C-7 (per-worker
   memory ceiling) and C-8 (secrets in the OS keychain).
   **Native extensions run in developer mode but are not distributable until
   5A** — trust rails ship before anything that executes a binary travels.
3. **Phase 5A — trust rails in the client.** Key ceremony → signed-index
   verification (rollback + expiry) → store data path → pack format v2 with
   path-safe extraction (**tests before the extractor**) → install/update/remove
   transaction → revocation UX.
4. **Phase 5B — the registry.** `grain-extensions` repo → three-job CI with the
   secret/egress boundary → risk lanes → publish pipeline → review dashboard →
   fast lane → store UI → public site → **one real third-party extension end to
   end**.

### Corrections carried into those phases

Found 2026-07-22 by researching our own code against the 2023–2026 incident
record (full ledger: DISTRIBUTION-PLAN §8).

- **C-1 — surface windows had no Tauri capability.** `ext-surface-*` /
  `ext-overlay-*` matched no capability file, so `listen()` would have been
  denied and the sleep/revive/payload handshake would have failed at runtime.
  App commands are allowed on every window by default, which is why the
  `invoke` calls looked fine and the gap hid. **Fixed** (`9e0d8db2`,
  `capabilities/extension-surface.json`, scoped to `core:event:default` only —
  a surface must never be able to move, resize or close its own window).
  Runtime proof is Phase 3.5 step 10.
- **C-2 — fixed in Phase 3.5 step 1.** The WS handshake now accepts only Grain's
  Tauri/loopback origins (or an absent `Origin` for native clients), rejects
  other browser origins with 403, and caps concurrent unauthenticated clients
  at 64 with drop-safe slot cleanup.
- **C-3/C-9 — trust and no-transitive-install are true by accident.** Both need
  to become tested invariants (5A steps 2 and 5).
- **C-4 — invisible/bidi Unicode is unscreened.** The exact GlassWorm hiding
  technique. → `doctor` lint (3.5) + CI gate (5B) + import path.
- **C-5 — archive extraction does not exist yet**, so its traversal/zip-bomb
  tests get written *first* (5A step 4). Zed shipped a CVSS 7.4 Zip Slip in
  exactly this code.
- **C-6 — `entry_source` as an embedded string** blocks source maps and hides
  payloads → registry-built artifacts + dev source maps.
- **C-10 — Grain's own updater is live with a pinned minisign key.** Reuse that
  crypto shape for the index; do not invent one.

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
