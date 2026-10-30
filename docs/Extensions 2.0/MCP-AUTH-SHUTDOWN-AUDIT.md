# Authenticated MCP shutdown audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Precommit base:** `a4ca1c08ac7734c4f949aff8ef7262c4ec158255`. **Status:** focused audit, final combined acceptance, clean repeat, negative oracle and affected regressions complete. Check **15 accepted**: **44 Pass / 9 Pending** (15 human, 29 reviewed automated). Check 13 remains Pending for client-configuration variants; check 17 remains Pending for eligible live nested-read evidence. Whole B2/B3 and all seven release gates remain open. No new manual batch.

## Scope and actual application evidence

`mcp.auth-shutdown` uses the maintained real Grain application, its actual Agent search/selected-schema/confirmation/execution path, locked `rmcp 3.1.4`, production OAuth callback and scoped Windows vault. The existing owned HTTPS issuer/resource provides a harmless read held at the actual response. Only browser consent is the private fixture handoff; this does not certify live-provider/browser authentication.

Provider Disable and Developer Mode off are each crossed with modern/stateless and legacy/session lifecycle, JSON and SSE: eight independent schedules. Each requires one actually approved account A read, boundary shutdown, zero retained sessions/held responses within ten seconds, an honest dispatched uncertainty, discarded late reply, unchanged stored grant/exchange count, no discovery while disabled, and fresh approved A recovery. An old pending confirmation must also be refused after disable/re-enable with zero wire dispatch; another fresh read and actual application restart/read must succeed.

Exact full-case totals are eight cancelled reads, eight stale confirmations with zero dispatch, and 24 successful fresh/restart A reads: 32 actual tool calls. Model receipts independently require eight exact uncertainty results, eight stale-refusal results and 24 genuine account successes. No stored badge, invented success, shortened deadline or fixture-supplied approval can pass.

## Reproduced failure and narrow production repair

`run-znE4bi` failed on authenticated provider-disable/legacy/JSON: the HTTP response closed and Agent reported uncertainty, but one remote protocol session remained. The scenario's final assertion then masked the primary cleanup wait failure; the failed report has cleanup Fail and receives no acceptance credit. The runner now preserves the primary error and records a separate scenario-cleanup failure.

Direct tracing of the locked SDK showed that authenticated `delete_session` obtains an access token through `AuthorizationManager`. Its credential store reload correctly rejects the old invalidated Grain account ticket after Disable. Local cancellation therefore worked while authenticated remote DELETE could not proceed. Weakening that ticket check would allow obsolete account access and is not the repair.

The bounded HTTP adapter now captures a private zeroizing credential owner only from an actual successful authenticated Initialize response with a protocol session. It binds the exact endpoint/session and original credential for one cleanup DELETE, using the SDK's existing plain HTTP implementation and protocol headers. It never reloads/refreshes the invalidated account, authorizes a replacement, or repeats a tool. Ownership cannot be overwritten, used for another endpoint/session or consumed twice, including through a clone. The snapshot expires with its operation; stateless/unauthenticated requests do not retain it. Existing one-second DELETE and service cleanup bounds remain.

SDK ownership of ordinary authorization, HTTP posts, correlation and lifecycle is unchanged; no dependency upgrade, idle account map, new engine, extension privilege or Handy change is introduced. The SDK HTTP API necessarily receives its normal temporary token string; the retained owner zeroizes on completion/drop. Errors are constant and never expose token/session payloads.

Remote revocation, an expired token, HTTP 401 or a server refusing DELETE may prevent remote disposal. This repair guarantees bounded local ownership and no refresh/replay, not remote undo or universal session deletion. Controlled accepting-server cleanup is tested separately from that limitation.

## Focused review

Graph overview/search/impact and changed-file/review context preceded direct source inspection. The graph returned no affected flows and truncated/unrelated hubs instead of the exact assertions, so direct tracing covered authenticated service construction, response-bound ownership, cancellation, cleanup routing, account-ticket invalidation and scenario oracles. Missing graph edges are not assurance.

The normal-build ownership test requires original credential retention, endpoint/session mismatch refusal, overwrite refusal and clone-wide single consumption. All 49 final normal-build MCP tests and eight feature guards pass; all 36 runner self-tests pass with zero skipped, including exact dispatched-uncertainty/no-success/no-replay refusal. Feature backend Clippy, scoped formatting/whitespace, frontend type/build and maintained real-host build pass. Existing Clippy warnings remain; the unrelated `src/app/bindings.ts` changes are preserved and excluded.

The first normal test build (`logic-aarte7`) failed because the internal protocol fixture lacked the new optional cleanup argument. It was corrected; intermediate `logic-e0Kjdq` is historical. Final `logic-2i4smH` supplies the 49-test result after zeroizing ownership was finalized. Failed/intermediate work is not final acceptance evidence.

## Verification records

| Run | Verdict / boundary |
| --- | --- |
| `run-W4YKpM` | Focused eight-schedule shutdown Pass, 33 observations, 342 OAuth journal entries, cleanup Pass and zero remaining MCP grant/session/held reply. Cleanup 386–423 ms on this machine. Supporting standalone result; final repeat/regressions required. |
| `run-280pfW` | All fourteen combined MCP cases Pass; 199 observations, 3,058 peer entries, 142 tool calls and 700 OAuth entries. Includes full four 45-second call deadlines and two 90-second discovery deadlines. Cleanup Pass, zero sessions/held replies/delay timers/scoped MCP grants. Shutdown matrix independently verifies 8 unknown, 8 stale-refusal and 24 account-success receipts. |
| `run-miGTTA` | All five OAuth cases Pass again in a fresh isolated profile, including eight complete shutdown/restart schedules and previous authenticated Agent-close regressions. Cleanup Pass; no remaining scoped MCP grant/session/held reply. |
| `run-kw9qRF` | Skip-disable fault fails immediately with `Provider disable did not take effect`/exit 1, before disabled discovery or a held-call timeout. Required negative oracle, cleanup Pass, zero remaining MCP grants. No acceptance credit. |
| `run-MTVhAc` | All eight native-auth cases Pass; cleanup Pass, four discarded scoped grants independently deleted, none remaining. This does not certify production orphan reconciliation. |
| `run-nsp1eS` | Both ordinary Agent smoke cases Pass; cleanup Pass. |
| `run-T8oyKx` | Both public live read/disable and large-response/recovery cases Pass; cleanup Pass. No live nested schema or OAuth acceptance is implied. |
| `conformance-rpu0Cp` | Legacy and modern official tools each Pass two official checks plus real Grain acceptance. Known standalone initialize remains Blocked with zero official checks; full entry exits 2. All case/run cleanup Pass. Blockage is not waived. |

Each final combined/repeat shutdown case has exactly 8 dispatched-uncertainty, 8 stale-refusal and 24 account-success model receipts, with 32 actual tool calls, no extra exchange and 33 independently recorded observations. Sixteen complete shutdown schedules across those runs clean up in **383–393 ms**, under the asserted ten-second ceiling; this is functional evidence on this machine, not a universal performance promise.

Final runtime identities: host `d4fcc4cd50bd70699744b7fe43863c3d2ab39d094ba67dff31e88a39a38c67ff`; source `14ef2f575df792ab64e6efd832757a2aa0eae1ad5121646fafc94b8ff52f4bf9`; runner `74f39533dc2b05a3fabc340435439f67cc1f6737c094846b45994c58a3348e5d`. Reports remain under `tests/agent-harness/.runs/<run>/evidence/`. Code and fixture definitions stay frozen through final runtime verification; Markdown changes do not alter those identities.

Independent inspection verifies matching final identities, cleanup Pass and no private MCP token marker across all seven completed runtime roots; each contains only its marker and evidence, with no retained profile/fixture/TLS scratch. All three official case identities/cleanup agree. No matching owned host/WebView remains and listener 17124 is closed. Failed `run-znE4bi` retains its cleanup Fail and receives no acceptance credit.

## Reproduction and maintenance

Run serially on an idle Windows desktop after `tests/agent-harness/build.ps1`, without compiling while runtime cases hold native files:

```powershell
node --test tests/agent-harness/runner.test.mjs
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-shutdown --fault skip-mcp-disable
node tests/agent-harness/run.mjs --suite native-auth
node tests/agent-harness/run.mjs --suite smoke
node tests/agent-harness/run.mjs --suite mcp-live
node tests/agent-harness/conformance.mjs
```

The deliberate skip-disable fault must fail immediately at the actual production-state assertion, before disabled discovery/queue behavior can obscure the cause. It receives no acceptance credit and must have independent cleanup Pass. Official initialization's known blockage is retained; tool-case results cannot waive it.

Retain this scenario/oracle, exact-owner regression and bounded cleanup adapter until an audited SDK/application ownership path provides equivalent authenticated cancellation without stale-ticket credential reload, refresh or tool retry. Then remove the superseded seam and its references together. The existing held-reply/issuer/restart/vault infrastructure is reused; no unused future fixture or alternate render path is added. Run-local profiles, TLS keys, grants, processes and response handles expire at teardown, while privacy-safe reports persist. Historical scratch roots `run-FCSQDD` and `run-t1ux8t` remain separately policy-blocked/Pending; no new route to their deletion or acceptance is attempted.

## Primary-source research and next live prerequisite

The [MCP HTTP transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports) recommends DELETE when a client no longer needs an assigned session and permits server refusal. The [current cancellation specification source](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/patterns/cancellation.mdx) defines cooperative cancellation and late-response handling; cancellation does not prove remote undo. The [official Rust SDK authenticated transport source](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/transport/common/auth/streamable_http_client.rs) and [authorization source](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/transport/auth.rs) provide upstream context. The actual repair was checked against the locally locked 3.1.4 sources, not assumed equivalent to today's upstream main.

[Context7's current tool implementation](https://github.com/upstash/context7/blob/master/packages/mcp/src/index.ts) uses flat string arguments and does not supply the missing live nested-input evidence. [Official hosted Notion MCP documentation](https://developers.notion.com/guides/mcp/mcp-supported-tools) documents search filters including nested `filters.teamspace_ids` and a tool-access discovery prerequisite. Its [hosted overview](https://developers.notion.com/guides/mcp/overview) requires an authorized account. This is a candidate for check 17, not acceptance: verify the actually exposed supported schema, plan/permissions and safely returned nested result before calling it. Unsupported advanced filters can be dropped with notices and must not silently broaden a planned test. Do not confuse the hosted service with the old open-source Notion API server. No personal account or additional provider is connected by this research.

Remaining live-account consent/expiry, public/confidential registration/client changes, two-MCP independence and staged Agent workflows retain their own requirements. Whole B2/B3 and all seven release gates remain open; baseline acceptance still precedes broad platform improvements.
