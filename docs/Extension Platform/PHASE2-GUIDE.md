# Phase 2 Implementation Guide — the scripted runtime

A prescriptive, step-by-step guide for implementing Phase 2 in THIS codebase.
Written because Phase 2 is the hardest phase: it turns the contract into a
running JS runtime. Follow the steps **in order** — each compiles and tests on
its own, and later steps assume earlier ones. Where a decision looks open,
it isn't: the resolution is written here or in [SPEC.md](SPEC.md), which wins
on any conflict. Read [TRANSITION-LOG.md](TRANSITION-LOG.md) first for the
state of the world.

**What Phase 2 delivers (SPEC §8 row 2):** supervisor webview + **one Worker,
one WS connection, one token per extension**; capability-checked host API
(storage / llm / embed / log to start); the transform hook with timeout +
strikes; the idle reaper; the two structural capabilities (`session:start` +
`contributes.sessionMode` slow stage); one dogfooded scripted built-in.

**Non-negotiables (do not "simplify" these away):**
- NEVER a shared JS realm or shared token for extensions (SPEC §7.1 — a
  shared realm makes identity forgeable; this was a caught security flaw).
- The security wall is the Rust WS boundary. The JS side is convenience.
- No feature code inside `src-tauri/src/handy/` — hooks only, marked
  `[GRAIN]`, budget-accepted individually (ratchet will fail your push
  otherwise; that is it working, not breaking).
- Additive protocol changes only (`DaemonEvent`/protocol enums grow; nothing
  is renamed or removed).

---

## Step 0 — Re-review (SPEC §10.3)

Before coding, re-read SPEC §7.1, §3.1, §1.3 and this guide end to end. Record
"kept / changed / why" for the phase plan in your first commit message. If you
change an approach in this guide, say so there too.

## Step 1 — Protocol additions (grain-sdk)

File: `crates/grain-sdk/src/protocol.rs`. Add four types (serde, tested like
the existing ones):

```rust
/// Worker → server: an API call. `id` correlates the response.
pub struct ClientRequest { pub id: u64, pub method: String, pub params: serde_json::Value }
/// Server → worker: the response.
pub struct ServerResponse { pub id: u64, pub ok: Option<serde_json::Value>, pub err: Option<String> }
/// Server → worker: a host-initiated call (transform, session result). The
/// worker must answer with `HostCallResult` before the host's deadline.
pub struct HostCall { pub call_id: u64, pub method: String, pub params: serde_json::Value }
/// Worker → server: answer to a HostCall.
pub struct HostCallResult { pub call_id: u64, pub ok: Option<serde_json::Value>, pub err: Option<String> }
```

**Pitfall — frame discrimination.** All frames share one duplex text channel.
Existing shapes: `DaemonEvent` (externally-tagged enum), `PillAction`
(`{"action": …}`), `ClientHello` (`{"token": …}`), `ServerWelcome`
(`{"grain_api": …}`). Give the new types unambiguous top-level markers by
wrapping: serialize as `{"req":{…}}`, `{"res":{…}}`, `{"call":{…}}`,
`{"callres":{…}}` (one wrapper enum in protocol.rs with `#[serde(rename)]`
variants is the clean way). Add a test proving none of the five shapes parses
as another. Do NOT try to retrofit a tag onto `DaemonEvent` — that breaks the
pill.

## Step 2 — Extension tokens + the permission sheet

1. `src-tauri/src/events_server.rs`: add
   `pub fn mint_extension_token(ext_id: &str, caps: HashSet<String>) -> String`
   (same 2×uuid recipe as `pill_token`; registers
   `ClientIdentity { id: ext_id, caps: CapabilitySet::Named(caps) }`), and
   `pub fn revoke_token(token: &str)`. Tokens are minted at **worker spawn**
   and revoked at reap/disable — not long-lived.
2. Grants: `ExtensionRecord.granted` already exists. For tier-B (`scripted`)
   packs, `extension_set_enabled`'s first enable must compare
   `manifest.permissions` against `granted`; if grants are missing, return a
   structured error `{"needsPermissions": [...]}` so the frontend shows the
   **permission sheet** (plain-language list + Approve/Cancel); on approve the
   frontend calls a new `extension_grant(id, permissions)` command that writes
   `granted` and retries enable. Plain-language strings for each capability
   live in one frontend map — copy the wording style from SPEC §1.3's table.
3. Capability names for events are already enforced server-side
   (`events_auth::allows_event`). The `Named` set for a worker = its `granted`
   list verbatim.

**Pitfall:** do not grant implicitly on enable "to keep it simple" — the
Chrome-model hold-until-approved behavior is SPEC §6 and the whole point.

## Step 3 — The host API router (Rust)

New file `src-tauri/src/host_api.rs` (Grain-owned; declare in lib.rs — one
budgeted line, accept it). Signature:

```rust
pub async fn dispatch(app: &AppHandle, identity: &ClientIdentity, method: &str, params: Value) -> Result<Value, String>
```

Methods for Phase 2 (each behind a capability check FIRST — return
`"capability 'X' not granted"` errors, never partial data):

| Method | Capability | Implementation notes |
|---|---|---|
| `log.info` | (none) | `log::info!("[ext:{id}] {msg}")` — rate-limit 20/s |
| `storage.get` / `storage.set` / `storage.delete` | `storage` | one JSON file per extension: `<data>/extensions/<id>.storage.json`, loaded/saved whole (fine at this scale); key→Value map; 200 MB quota check on set |
| `settings.get` / `settings.set` | `settings` | ONLY the extension's own `ext.<id>.*` namespace — store inside the storage file under a reserved `"__settings"` key. NO path to AppSettings. |
| `llm.complete` | `llm` | call `crate::grain_post_process`'s single-provider path with the user's active provider; attribute + rate-limit (10/min default). Do NOT expose provider ids or keys in errors. |
| `embed` | `embed` | wire to the grain_space embedding entry point if trivially reachable; otherwise return "unavailable" and note it — do not force it |

Wire into `events_server::handle`'s read loop: parse the wrapper enum; `req`
frames → `dispatch` (spawn a task; send `res` back on the write half via an
mpsc so the select loop stays single-writer). Unit-test dispatch directly with
a `Named` identity: wrong capability → error; storage roundtrip works.

**Pitfall — the write half.** The current `handle()` loop owns `write`
exclusively. Responses and host-calls must be funneled through ONE mpsc
channel consumed by the select loop (add a third select arm), or you will
fight the borrow checker and end up with interleaved partial writes.

## Step 4 — The extension host (Rust side)

New file `src-tauri/src/extension_host.rs`. Owns:

- **Supervisor webview lifecycle**: a hidden `WebviewWindow` labeled
  `"extension-host"`, URL `extension-host.html` (Step 5). Create on first
  worker need (`tauri::async_runtime::spawn` — window creation must NOT run on
  a shortcut/event thread; see the tauri#3990 freeze rule), destroy when the
  last worker dies. Talk to it via Tauri events:
  `ext-host://spawn {ext_id, token, code_url, activation_event}`,
  `ext-host://kill {ext_id}`; it reports `ext-host://died {ext_id}`.
- **Worker registry**: `HashMap<String, WorkerState>` (spawned_at,
  last_activity, strikes, token). `touch(ext_id)` called from events_server on
  any frame from that identity (expose a hook or a shared `Arc`).
- **Activation dispatch**: subscribe to `ctx.subscribe()` on a task; for each
  event, for each ENABLED tier-B extension whose manifest `activation`
  contains `onEvent:<VariantName>` (match on the serde variant name of the
  event) → if worker not running, spawn it **carrying the triggering event as
  `activation_event`**. This matters: the broadcast is already past when the
  worker connects — without the carried payload the wake reason is lost.
  `onTransform` extensions activate on `RecordingStarted` (SPEC: session
  start, because a ~300ms cold wake cannot fit the ~150ms transform budget).
- **Reaper**: a 30s-interval task; worker idle (no frames, no pending host
  calls) for > 120s and not `resident` → kill + `revoke_token`. Log every
  spawn/reap at info with the reason — this is how "destroy if not in use" is
  audited later.

Manifest source: Phase 1 packs are tier-A only. Give tier-B manifests the same
`.grainpack.json` storage (validate() must now ACCEPT `tier: "scripted"` with
an `entry` field — update `manifest.rs` + its tests; keep rejecting `native`).
`entry` names a JS file placed next to the pack file:
`<data>/extensions/<id>/<entry>` (import copies it from a sibling of the
imported file, or embed the code as a string field `entry_source` in the pack
JSON for v1 — **choose the embedded-string route**: one-file packs stay
one-file, no path-copying logic, and blob-URL workers don't care).

## Step 5 — Supervisor page + worker (the JS side)

The supervisor is Grain's own code; the worker is Grain's shim + the
extension's `entry_source`. **No extension code runs in the supervisor global.**

1. `src/extension-host.html` (bundled with the frontend; a real route so
   Tauri can load it as a webview URL). Its inline script:
   - `import { listen, emit } from "@tauri-apps/api/event"` (or the global
     `window.__TAURI__` if the host window has no bundler context — prefer a
     tiny standalone `.html` in `public/` that pulls `@tauri-apps/api` via the
     injected global; verify which is available in THIS app's webview setup
     before writing — check how the pill/agent windows do it).
   - `listen("ext-host://spawn", ({payload}) => spawnWorker(payload))`
   - `listen("ext-host://kill", ({payload}) => killWorker(payload.ext_id))`
   - `spawnWorker({ext_id, token, entry_source, activation})`: build the worker
     source = `GRAIN_RUNTIME_JS + "\n" + entry_source`, wrapped so the runtime
     gets `__GRAIN_TOKEN__`, `__GRAIN_ACTIVATION__` injected as consts at the
     top. `new Worker(URL.createObjectURL(new Blob([src], {type:"text/javascript"})))`.
     Track `workers.set(ext_id, w)`. On `w.onerror` / worker-reported fatal →
     `emit("ext-host://died", {ext_id, reason})`.
   - `killWorker(id)`: `workers.get(id)?.terminate(); workers.delete(id)`.

2. `GRAIN_RUNTIME_JS` — the worker-side shim, authored as a plain string
   constant (a `.ts` that exports the string, or a `.js` in `public/` fetched
   at build). It:
   - opens `new WebSocket("ws://127.0.0.1:7124")`; on open sends
     `{"token": __GRAIN_TOKEN__, "client": ext_id, "grain_api": "1.0"}`.
   - maintains a request-id counter + a `Map<id, {resolve,reject}>`.
   - exposes the global `grain` object the extension calls:
     `grain.log.info(msg)`, `grain.storage.get/set/delete(k[,v])`,
     `grain.settings.get/set`, `grain.llm.complete(prompt, opts)`,
     `grain.embed(texts)` — each sends a `{"req":{id,method,params}}` frame and
     returns a Promise resolved by the matching `{"res":{id,ok|err}}`.
   - dispatches incoming `{"call":{call_id,method,params}}` (HostCall) to the
     handler the extension registered (`grain.onTransform(fn)`,
     `grain.onEvent(fn)`, `grain.onSessionResult(fn)`), then replies
     `{"callres":{call_id, ok|err}}`. A handler that throws → `err`; the host
     applies its pass-through/strike policy.
   - `grain.activation` = the injected `__GRAIN_ACTIVATION__` (the event that
     woke this worker — see Step 4; may be null for shortcut/manual spawns).

**Pitfall — WebSocket IS available in Worker scope** (unlike DOM). Good. But
`@tauri-apps/api` is NOT — the worker reaches Rust only through its WS, never
Tauri IPC. Only the supervisor page uses Tauri events.

**Pitfall — blob workers + CSP.** Tauri's default CSP may block
`worker-src blob:`. Add `worker-src blob:` (and keep `connect-src ws://127.0.0.1:7124`)
to the extension-host window's CSP, scoped to that window only if possible.
Test that a trivial worker can open its WS before wiring anything else.

## Step 6 — The transform hook (SPEC §3.1, §3.3)

Where: the finalize path, AFTER the raw transcript and Grain's own
fast text stages, BEFORE the slow stage (post-processing) and paste. In Grain
that junction is `process_transcription_output` (actions.rs, `[GRAIN]`-hooked)
— add ONE marked call:

```rust
// [GRAIN] extension transform pipeline (SPEC §3.1)
let text = crate::extension_host::run_transforms(app, text).await;
```

`run_transforms(app, text) -> String`:
- collect enabled tier-B extensions whose manifest `activation` includes
  `onTransform`, in **toggle order** (registry `toggle_seq`);
- for each: issue a `HostCall{method:"transform", params:{text}}` to that
  worker with a **150 ms deadline**; on reply use the returned string; on
  timeout/err keep the input unchanged and record a **strike** (3 strikes →
  auto-disable the extension + user notice, exactly like SPEC §3.3);
- an empty-string reply **suppresses** (the transcript becomes empty →
  nothing pastes), matching CASE-HEYCLICKY's "output suppression" note;
- return the final text.

Workers for `onTransform` extensions must already be warm — they activated on
`RecordingStarted` (Step 4). If a worker isn't connected when the transform
fires, skip it (do not block the paste path waiting for a cold spawn).

**Pitfall — never block paste on an extension.** The deadline is hard; the
timeout path returns the untransformed text. A transform extension can slow a
paste by at most 150 ms × (number of enabled transforms). Show per-step ms in
the pipeline UI later (Phase 3); for now just log slow steps.

## Step 7 — session:start + contributes.sessionMode (the structural pair)

This is the reason Phase 2 owns these (SPEC §CAPABILITY-GOVERNANCE: structural
= land early or never). Scaffold the plumbing even though no built-in dogfoods
it yet — the shape must exist so Phase-3+ extensions don't force a contract
break.

- `contributes.sessionMode` in the manifest: `{id, binding?, slow: true}`
  declares a named capture mode the extension owns.
- Host API `session.start(mode)` (capability `session:start`): asks the
  `TranscriptionCoordinator` for a session (it SERIALIZES — an extension
  session and core dictation can never overlap; the loser is rejected exactly
  as two core bindings are, SPEC chokepoint #1). The pill shows the owner
  ("<name> is listening").
- On stop, the transcript is delivered to the extension as a
  `HostCall{method:"sessionResult", params:{text}}` with a **long deadline**
  (this is the slow stage — seconds, not 150 ms). The worker runs its
  multi-second LLM work and returns text; the host pastes it through the
  normal output path.
- Wire the mode's binding through the binding registry if `binding` is set
  (reuse the `contributes.shortcuts` path from later; for Phase 2 a manual
  `session.start` call from the worker on its own activation is enough to
  prove the loop).

**Scope note (allowed):** if session:start proves large, split it into chunk
2b — but land `session:start` as a *named, capability-checked, rejected-when-
unimplemented* method in Step 3's router first, so the capability exists in the
vocabulary from day one (reserving the name is the SPEC rule). Do NOT ship
Phase 2 without at least the reserved capability + a returning-"unimplemented"
stub, or a Phase-3 extension will discover the gap the hard way.

## Step 8 — Dogfood: port auto-categorization

The proof the runtime works end to end. Auto-categorization (currently
piggybacking the Grain Space capture LLM call) becomes the first **scripted
built-in**: manifest `tier: "scripted"`, `permissions: ["storage", "llm"]`,
`activation: ["onEvent:TranscriptionComplete"]` (or the Grain Space capture
event if more apt), `entry_source` = the JS that, on the event, calls
`grain.llm.complete(categorizePrompt)` and `grain.storage.set(...)`.

- Ship it as a built-in `.grainpack.json` under `<data>/extensions/` at
  startup if absent (like the centre variant), default **OFF**.
- This exercises: worker spawn on an event, the injected activation payload,
  `llm` + `storage` host calls, capability enforcement, and the reaper (it
  should die ~120 s after the last event). Watch the logs for spawn/reap.
- It does NOT exercise transform or session:start — that is fine; those have
  their own minimal tests. The dogfood's job is the headless happy path.

**Definition of done for Phase 2 (SPEC §8 row 2 + this guide):**
supervisor+worker with one WS/token per extension (proven: two workers, each
sees only its own grants); host API storage/llm/log capability-checked
(dispatch unit tests); transform hook with timeout+strikes (one integration
test with a stub worker or a Rust-level fake); idle reaper (RAM returns after
120 s); `session:start` capability reserved+plumbed; auto-categorization runs
as a scripted built-in. `tsc` clean, all Rust tests green, ratchet green.

## Appendix — file map for Phase 2

New: `crates/grain-sdk/src/protocol.rs` (+4 types), `src-tauri/src/host_api.rs`,
`src-tauri/src/extension_host.rs`, `src/extension-host.html` +
`GRAIN_RUNTIME_JS` source, a built-in auto-categorize `.grainpack.json`.
Touched (hooks, budget-accepted): `events_server.rs` (write-half mpsc, request
routing, `mint_extension_token`/`revoke_token`), `actions.rs` (one
`run_transforms` call), `lib.rs` (module decls + host-window CSP + startup
seed), `manifest.rs` (accept `tier:"scripted"` + `entry_source`),
`grain_commands.rs` (`extension_grant`, needsPermissions error).