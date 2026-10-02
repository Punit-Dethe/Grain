# Linear browser consent preparation: focused audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Status:** authentication-only harness prepared; real browser consent/code exchange remains **Pending** until the account owner is present. The guarded no-browser prerequisite is verified. Numbered ledger stays **50 Pass / 3 Pending (9/12/17)**; whole B2/B3 and all seven release gates remain open. This unit does not execute tools, certify a live account, or begin broader extension implementation.

## Source and reuse decisions

[Linear's official MCP guide](https://linear.app/docs/mcp) documents its read-only endpoint and read OAuth scope. The fixed alias retains both restrictions. The lockfile already resolves **rmcp 3.1.4**, despite the manifest's compatible `3.0.0` range; the prior preflight audit's source reference is corrected. The [locked SDK source](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.1.4/crates/rmcp/src/transport/auth.rs) owns browser authorization, PKCE, issuer/state checks, code exchange, scope resolution, credential publication and refresh. No SDK version changes.

The SDK records requested scopes when a token response omits scope, following [RFC 6749 section 5.1](https://www.rfc-editor.org/rfc/rfc6749#section-5.1). The harness reports that source explicitly rather than labelling it an explicit token-response scope. Unknown expiry remains unknown. No API-lifetime assumption, time edit, credential import, new OAuth implementation or gateway is added.

An optional harness dependency names the already-locked `oauth2` **5.0.0** `TokenResponse` trait, permitting borrowed metadata inspection without serializing/cloning tokens. The lockfile changes only the application's dependency edge; no package version or new package is introduced. Standard/native production authentication remains unchanged. Linear's test DCR name is `Grain Agent Harness - Linear read-only`, making its eventual account authorization distinguishable from ordinary Grain.

## Implementation and reviewed boundaries

Graph-first context, call relationships, impact, change detection and review context identify shared harness/configuration/authentication consumers and incomplete graph test edges. Direct source tracing covers the SDK future and vault lease, marker, main-window guard, fixed consent slot, native opener, input/IPC lifetime, reporting and scoped cleanup. Graph risk/coverage summaries alone do not certify these paths.

`mcpLinearConsent` requires the existing exclusive `mcpLiveLinear` marker. Only the fixed debug alias receives Ready, Open Authorization and Grant Metadata controls. Missing mode, another target and missing consent are refused. The native opener consumes and revalidates the SDK URL; the runner never receives or prints it in human mode. State, PKCE, callback validation and exchange remain the actual production/SDK operations. The original test mode still refuses browser controls and all modes retain the HTTP **no-tool-execution** guard.

Every SDK vault publication for the alias validates exact issuer, SDK-granted `read` alone, any explicit response scope, bearer type/nonempty token, client bound and issued/lifetime arithmetic before storage. Refresh cannot publish wider access through this store. The RPC returns only fixed scope/source, issued/expiry seconds, expiry-known/expired flags and refresh availability. It never returns tokens, client values, or account/workspace identity. Positive/negative credential unit cases use disposable synthetic values and do not establish real-provider acceptance.

The separate `mcp-linear-sign-in --linear-sign-in` selection requires an interactive terminal; missing opt-in, an opt-in in another suite and noninteractive launches fail before profile/network/browser work. Connect has a 310-second IPC observation around the unchanged five-minute production deadline; discovery has 95 seconds around the unchanged 90-second operation budget. Other providers/operations retain ten seconds.

The human cancels or closes the first consent tab without approving, then types only `cancelled` in the runner. Evidence separately records that human acknowledgement and the precise production Denied/Cancelled outcome, released callback and zero grants. Approval during this step fails. The second browser flow is approved by the owner; the harness then validates the actual scoped grant, authenticated supported-tool count, real restart/persistence, disconnect and cleanup. Discovery returns only a supported-tool count, excluding names/digests/descriptions. It is not an Agent read or nested-tool test.

Cancellation input is bounded to four minutes, abortable, and releases its timer/interface/input listeners on success, wrong input, timeout, input close and abort. During implementation, the new oracle caught an actual cleanup defect: removing all `close` listeners before `readline.close()` removed Readline's own input-cleanup callback. Only this helper's named handlers are now removed. Listener counts return to zero in all five schedules, consistent with [Readline close](https://nodejs.org/api/readline.html#rlclose) and [EventEmitter listener ownership](https://nodejs.org/api/events.html#emitterremovealllistenerseventname). The defect was repaired before any human account run.

Human mode drops host output and model/event bodies, suppresses main text/snapshots/screenshots, and replaces private error bodies with fixed diagnostics. Reports retain only this procedure's metadata and counters. The controller unit test detects broadened scope, empty discovery, lost restart and accidental first-flow approval while proving final disconnect and no tool operation. These are controller tests without UI/browser replicas or Tauri visual shims; genuine consent is not claimed.

Cleanup uses the existing exact alias/run UUID Windows namespace, with independent metadata-only deletion after owned processes retire. The added consent-only invalid marker is refused by cleanup as well. No idle host/profile/grant is retained between coding phases. Local deletion does not prove remote token revocation; the owner can remove the clearly named test authorization in Linear's account settings. Unconsented DCR registrations remain server-owned.

## Verification

| Evidence | Result |
|---|---|
| Runner self-tests | **64 Pass**, including scope/lifetime/privacy, finite budgets/CLI admission, five input-cleanup exits, controller failure schedules and existing fixture/report oracles. |
| Rust harness guards | **12 Pass**. |
| Rust MCP tests | **51 Pass** in harness mode, including positive/negative grant inspection; **49 Pass** in normal configuration. |
| No-browser guard `run-XD6yuj` | Pass, 3.028 s: actual SDK readiness, absent URL refusal, cancellation, metadata absence and real restart. |
| Guard repeat `run-RwOXs1` | Pass, 2.334 s in another owned profile. |
| Preflight regression `run-vMPNh1` | Both existing cancellation/retry cases Pass, 3.764 / 3.640 s. |
| Full controlled OAuth regression `run-tKMuvY` | All ten cases Pass, including peer isolation, fixed-port refusal and actual expiry/rotation/refusal recovery. Cleanup Pass. |
| Final named-client guard `run-3xVcJm` / repeat `run-CFMkHz` | Both Pass, 2.260 / 2.239 s after the final rebuild. |
| Final preflight `run-kC7p9Z` | Both cancellation/retry cases Pass, 3.544 / 4.199 s. |
| Final Agent smoke `run-bTifY9` | Cold/warm and decline Pass, 1.164 / 0.944 s; new controls remain refused outside the consent marker. |
| Rust lint | Clippy passes with the existing 74 warnings. |
| Real human scenario | **Not run**. No consent page, personal token exchange or live tool result is asserted from these tests. |

The maintained builder passes CLI build, TypeScript, actual frontend build and debug native host/stamp. A final alias-only DCR display-name adjustment is followed by rebuilding and rerunning affected no-browser guards/preflight; the controlled-provider regression uses the same unchanged production/client behavior. Final binary identity: `af7e98f72cae1557886880b67c8c15d27728cdc42f871c93ed2a7fad40a2cf82`; source fingerprint `a3a2eb86de73da8055aacd02eb3ff745c5be5f84f77fb8bed5999b55e8f9abec`; built at `2026-10-02T18:17:34.081Z` from base `001d24a95d343456b780f3e45a478a9070b50a49` plus scoped dirty sources. Runner fingerprint: `8943f68dcd76db21a74f00f9196d96e3ad9b3d2cec2b4ebc2a658368d1794b2b`. Candidate and final reports remain ignored under `tests/agent-harness/.runs/`.

All reported runtime cleanup in this unit is Pass. Final guards/preflight show zero OAuth/client-secret entries remaining and removed owned profile/fixture trees. Independent final process/listener inspection confirms no harness host or 17124 event listener remains. Ordinary Grain and browser processes are preserved. The unrelated generated bindings remain outside this commit. Previously policy-blocked scratch cleanups are not revisited or certified here.

## Next step and retained work

Ask whether the owner is ready, then run the explicit human procedure and guide both browser steps. Keep private sign-in details in Linear's browser page. Review the real outcome before changing any numbered verdict. A discovered tool count does not close check 9's full usability/recovery requirements, check 12's actual expiry/refresh requirement, or check 17's nested schema/result requirement. A fixed harmless Agent read allowance must be reviewed against real authenticated schema before tool execution is enabled. All three numbered checks stay Pending in this preparation commit.

Keep the finite marker, native handoff, typed metadata, privacy rules and named scenarios as maintained test infrastructure. No temporary product scaffold or authentication engine requires later removal. Broader library simplification, physical retirement, outstanding release/input findings and the two previous policy-blocked scratch cleanups retain their existing order/status.
