# Linear SDK consent preflight: focused audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Verdict:** no-account preflight accepted after source review, negative oracle, clean repeat and affected regressions. Numbered ledger remains **50 Pass / 3 Pending (9/12/17)**. This does not certify browser consent, a usable live account, refresh, private results or the whole B2/B3 blocks. All seven release gates and previously recorded findings remain open.

## Purpose and researched boundary

The user selected Linear and authorized preparation until personal consent is needed. [Linear's official MCP documentation](https://linear.app/docs/mcp) supports Streamable HTTP, Dynamic Client Registration and a read-only endpoint. The [pinned official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.0/crates/rmcp/src/transport/auth.rs) supplies `AuthorizationRequest::with_scopes`, DCR, PKCE, resource binding, callbacks and credentials. This unit uses those existing SDK and Grain ownership paths; it adds no OAuth implementation, gateway, background worker or product capability.

Anonymous machine probes of [Linear issuer metadata](https://mcp.linear.app/.well-known/oauth-authorization-server) and [read-only resource metadata](https://mcp.linear.app/.well-known/oauth-protected-resource/mcp/readonly) identify the exact issuer, authorization/registration/token endpoints, S256 and read-only resource. The prior probe observed the unauthenticated read-only endpoint challenge requesting `read write`, despite the resource metadata advertising `read`. Therefore the test requests and validates **exactly `read`**, independently of the route name or challenge. Provider behavior can change; a changed contract fails this finite test rather than widening its scope.

The actual preflight reaches SDK-created authorization URLs with exact resource `https://mcp.linear.app/mcp/readonly`, `scope=read`, S256, state, client identity and a dynamic loopback callback. It never follows those URLs or submits an authorization code. DCR can leave server-owned client registrations; local cleanup does not establish their remote deletion. There is no API-key fallback or ordinary account-grant import.

## Reviewed implementation and impact

Graph-first exploration preceded raw reads. Change detection and review context identify shared harness/configuration/MCP consumers, high structural impact and incomplete graph test edges; graph edges are not a coverage certificate. Direct tracing covers the marker, catalog, SDK request, consent guard, HTTP gate, runner, vault cleanup and normal-build exclusion.

The debug-only `grain-harness-linear` alias is distinct from production `linear`. Its exact endpoint is available only with `mcpLiveLinear`; marker validation refuses mixed controlled MCP, native-account, store and DeepWiki modes. The production catalog entry, ordinary profile and Handy tree are unchanged. Real smoke and all ten controlled OAuth cases verify alias refusal without its marker. The normal-build catalog and production transport tests also pass.

The existing bounded consent handoff gains one fixed generation-owned slot. Taking/cancelling it cannot consume another provider's slot. The real production Connect future retains state/PKCE/exchange ownership. Both Rust and runner validate the exact origin/path, resource, read scope, eight unique required parameters, state/client bounds, S256 challenge and loopback callback restrictions. The URL remains private in memory; reports contain bounded flags, not URL/state/client/grant strings. The preflight HTTP adapter refuses **every tool call**, even if an unexpected callback completes. No authenticated read allowlist exists in this unit.

Each case performs initial consent creation, the named cancellation, exact cancellation classification, callback port reuse, late connection refusal, disconnected/disabled state and zero run-scoped vault entries. A real restart precedes a fresh SDK attempt and repeats the same operation. Disable and Disconnect are separate report IDs. Cancellation settlement has a five-second observation bound; production authentication remains five minutes. The existing ten-second IPC wrapper is adequate for these observed short DCR attempts, **not for future human sign-in**.

The existing metadata-only Windows cleanup gains an explicit `-McpLinear` mode. It requires the exact exclusive marker, run UUID service and alias target. Wrong modes/mixed markers are refused; no credential blob is read by cleanup. Cleanup runs after owned host/WebViews retire, including the missing-cancellation fault. Controlled MCP cleanup still passes its full regression suite. Existing unrelated generated bindings remain outside this commit.

## Verification and retained evidence

| Run/check | Result |
|---|---|
| First preflight `run-Vf86c7` | Disable 3.729 s / Disconnect 3.516 s, both Pass. Each includes actual SDK creation, restart and fresh retry. |
| Clean repeat `run-xJ9ggc` | Disable 3.650 s / Disconnect 3.674 s, both Pass in another empty profile. |
| `skip-linear-cancel`, `run-Aopx1h` | Expected Fail: cancellation did not settle after 5.040 s. Final disconnect retires the flow; cleanup Pass. No false acceptance. |
| OAuth regression `run-5z3hHA` | All ten cases Pass, including ordinary/preregistered, denied/late/closed/shutdown, independent-provider/fixed-port and actual expiry/rotating refresh/refusal cases. |
| Agent smoke `run-VUv6aB` | Both native cold/warm and Agent decline cases Pass; off-marker Linear authorization is refused. |
| Runner self-tests | **60 Pass**, including broad-scope/wrong-resource/duplicate/private-diagnostic refusal, exact cancellation, finite fault admission and Windows inventory marker refusal. |
| Rust guarded harness tests | **12 Pass**. |
| Rust MCP tests | **50 Pass** with harness feature; **49 Pass** in normal configuration. |
| Rust lint | Harness-feature Clippy passes with the existing **74 warnings**; no new warning appears in the changed preflight code. |
| Build | Maintained builder passes CLI build, TypeScript checking, actual embedded frontend and debug native host build/stamp. Existing warnings retained. |

All runtime cleanup reports in this unit are Pass. Preflight/fault reports show zero remaining OAuth/client-secret entries; owned `data` and `fixture` trees are removed. A bounded scan of their JSON/log evidence detects no authorization URL, OAuth query client/PKCE fields or token field strings; this scan supplements direct source review and is not a universal privacy proof. Independent final inspection finds **zero `grain-agent-harness.exe` processes and zero listeners on 17124**. Ordinary Grain processes/profiles are not terminated or modified; the two previously policy-blocked scratch cleanups remain separately Pending.

Build identity: `0a9ea92176051b3c430006e2c44cceb5aa39471c3485d9a4bb5c5684c6b9fac7`; source fingerprint `fc1ac3477149ab461a7efe85b958c0798e432370af1874e60b6e45692957ac81`; built at `2026-10-02T17:31:00.888Z` from base `f917c40003593705e53bbe921c55f3391a6d0904` plus scoped dirty application sources. All final runtime evidence uses runner fingerprint `1f808783ea4577aa14033806a11190ccccaba198aa07ee1f193133d0aada23c7`. Reports remain ignored under `tests/agent-harness/.runs/` rather than committed.

## Next account unit and maintenance

Keep these two cases and their finite guards as regression infrastructure. The next unit must prepare private browser handoff, a bounded human-wait IPC path, actual granted-scope/lifetime checks and suppression of private workspace data in failure diagnostics before inviting sign-in. It must validate actual authenticated tool metadata, then permit only a fixed harmless read through real Agent search → selected schema → approval → result. The preflight-only blanket tool refusal will need a narrowly reviewed read allowlist at that stage; do not weaken it merely to make discovery or a model response look successful.

Check 9 still needs real browser cancellation/consent and recovery. Check 12 still needs expiry/refresh on an actual MCP-issued grant with its real lifetime; API OAuth lifetime documentation or edited timestamps cannot substitute. Check 17 needs a suitable real provider nested schema/result; if Linear's available tools do not supply one, keep that requirement Pending and explicitly identify the additional provider prerequisite. Ordinary reads alone cannot close it. No sign-in is assigned to the user by this preflight unit, and no unused product scaffold is introduced.
