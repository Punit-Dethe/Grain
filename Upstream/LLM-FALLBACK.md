# Ordered LLM fallback

## Scope and plan (2026-10-04)

1. Remove `provider-router`, `post_process_router`, `rotation_state`, retained
   health trackers, cooldowns, token/headroom estimates and daily quotas.
2. Keep one optional Fallback setting and existing provider participation.
   Try enabled, configured providers in their persisted list order.
3. Add a six-dot grip immediately left of Edit. Support vertical pointer drag
   and Up/Down keyboard movement; save through normal backend settings commands.
4. Verify failure paths, settings persistence, generated bindings, frontend and
   upstream gates; independently audit the result.

Speech-to-text remains local only. This change concerns AI text processing and
Agent LLM requests. It adds no service, worker, polling, retained tracker or
routing engine.

## Runtime contract

- Fallback off: only `post_process_provider_id` is attempted, even if its
  fallback participation checkbox is off. Failure retains the existing raw
  dictation/error behavior.
- Fallback on: `grain-core/providers::fallback_pool` filters the saved list
  without sorting. Skip disabled entries, missing models and unconfigured
  providers. Nonblank keys configure HTTP providers; the existing Custom local
  endpoint and Apple Intelligence can be configured with a model without a key.
- Each call starts at the top. Try the next provider after a failed, empty,
  rate-limited or timed-out attempt. Return immediately on success. No previous
  request changes future priority.
- Existing transport compatibility retries (reasoning rejection, structured to
  legacy output, image to text) stay within each provider's attempt. Existing
  time budgets remain: post-processing 120 seconds; Agent uses its existing
  `AGENT_LLM_TIMEOUT`. No additional cooldown or retry service is introduced.
- Agent plain, image and tool paths use the same saved ordering. Apple
  Intelligence is skipped when tools are required. A reply containing only
  tool calls is successful and its exact winning reply goes to the existing
  bounded Agent tool loop.
- Extension `llm.complete`/image APIs retain their existing selected-provider
  contract and credential boundary. They did not participate in rotation.

## Settings and UI seam

Transcription settings now combine Models, Configure (Standard, Streaming,
model unload) and AI processing. The `capture` route owns this pane and the
`models.after` extension anchor. AI processing availability stays enabled:
Grain core settings loading restores `post_process_enabled` to true, and the
former master-toggle command is no longer exposed. Do not restore that toggle
when adapting upstream shortcut changes. This does not send every recording to
AI: the existing capture policy settings still decide when processing runs.

`post_process_fallback_enabled` is a backend-owned boolean, default false.
Priority is the order of existing `post_process_providers` records; no duplicate
order map is maintained. Each record's `enabled` flag controls participation.
Keys remain in `grain.secrets.json`; the renderer receives configuration
presence, provider records and model names through `pp_get_pool`.

`grain_provider_commands.rs` owns the pool API, outside the Handy tree.
`pp_reorder_providers` validates an exact permutation under AppContext's normal
settings/persistence locks, preserving full records, keys, models and selection.
Unknown, duplicate, dropped or stale provider lists are rejected. Existing
smart-rotation/quota fields are ignored and omitted on subsequent saves; no
user migration was requested during development.

`ppPoolStore` reloads the canonical backend view after writes and exposes
write/refresh failures. `PostProcessingPool` shows configured rows in backend
order, including keyless local providers. Pointer capture belongs to the stable
list container so moving row DOM nodes cannot interrupt dragging. Preview
order lives only during a gesture/save; cancellation, lost capture, settings
changes and unmount clear it. No global listener is added. A single save occurs
at drop; keyboard changes use the same command and restore grip focus after
the saved list renders. Unconfigured template slots
stay intact while visible providers are reordered. Disabled fallback entries
remain visible and reorderable but are skipped at runtime.

## Upstream review

Handy's `llm_client.rs` remains inert; its relevant transport fixes must be
ported into `grain_llm_client.rs` and reviewed against `grain_llm_fallback.rs`,
`grain_post_process.rs` and Agent. Handy settings changes need review against
Grain core settings/provider policy and commands. These destinations are in
`relocations.json` and its generated divergence table. Never restore a retired
router/tracker or quota field while adapting upstream. Frontend remains
Grain-owned under the existing freeze.

## Verification and acceptance

Automated checks cover saved priority/filtering, exact-permutation rejection,
settings and credential preservation, retired field omission, HTTP 429/503 and
empty response progression, exhaustion, reordered first attempt, repeated
requests starting at priority one, and Agent plain/tool-only winning replies.
Frontend tests cover visible ordering with hidden templates and store
write/refresh errors. Compile/export checks cover command/types registration.

Real application visual acceptance is still required. Close Grain, then run:

```powershell
cd C:\Users\watrm\.codex\worktrees\handy-webview-overlays\grain
$env:CARGO_TARGET_DIR='C:\gtc'
bun run dev:asr
```

In AI post-processing settings, enable Fallback; drag the grip left of Edit,
check top-to-bottom order, reload settings/restart and confirm persistence.
Try Up/Down on the focused grip. Turn Fallback off and confirm selection uses
one provider. With a failing first endpoint and working second endpoint,
verify dictation and Agent succeed through the second. No browser-only visual
harness was used; real-device visual/audio and non-Windows acceptance remain
user checks.

### Recorded results (2026-10-04)

- 212 grain-core unit tests and four capability benchmark tests pass, including
  priority validation and saved fallback/order/credential preservation.
- 28 targeted Tauri tests pass: ordered HTTP fallback 3, LLM compatibility 6,
  post-processing prompt framing 2, Agent 5, coordinator 8 and overlay 4.
- Full Tauri test-library compilation, Specta export, TypeScript/Vite production
  build, lint, 120 frontend tests, settings parity (84 fields), changed-source
  formatting and upstream preflight pass.
- Independent read-only audit found two UI edges: pointer capture on a moving
  row and focus loss after keyboard reorder. Both were fixed and independently
  rechecked; no outstanding actionable finding remains.
- Shared-code costs decrease: Cargo.toml 139→138, commands/mod.rs 22→21 and
  lib.rs 1260→1259. The deleted routing crate/state and moved Grain commands are
  outside that shared-code metric. These reductions are locked in budget.json.

Rust verification ran on Windows. A command-local Tauri resource override
avoided copying DLLs locked by the running app; production configuration is
unchanged. HTTP failure tests use owned local test servers, not external
providers. The independent audit was static; real-app interaction and
macOS/Linux runtime behavior were not exercised.
