# MCP SDK OAuth fixture and late-login ownership audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Status:** focused audit, final repeated acceptance and affected regressions complete. Check **10** accepted: **42 Pass / 11 Pending** (15 human, 27 reviewed automated). Whole B2/B3 and all seven release gates remain open. No new manual batch.

## Scope and evidence boundary

This testing unit adds three cases to the maintained real-application harness. The real Grain host, Agent WebView, approval/executor, locked **rmcp 3.1.4**, production callback listener and run-scoped Windows vault execute the flow. The external fixture supplies a controlled HTTPS OAuth issuer and MCP resource; it does not inject a token, substitute the SDK, change the clock or launch a replica UI.

| Scenario | Independently asserted real-app path | Requirement boundary |
| --- | --- | --- |
| `mcp.auth-fixture` | SDK protected-resource/issuer discovery, public DCR, S256 PKCE, resource-bound exchange, actual scoped grant, one selected nested tool, approval before dispatch, exact A result/restart, successful B replacement, exact stale A refusal with zero calls, fresh B/restart and disconnect | Prerequisites for 12/13/17/31. No refresh, registration variants, config-change or live nested-provider acceptance |
| `mcp.auth-denied-cancelled` | Provider `access_denied` callback and actual Cancel sign-in command finish, exact callback port rebinds, zero exchanges/grants, fresh approved A/B reads | Controlled part of 9; live browser denial/cancellation remains required |
| `mcp.auth-late-callback` | Cancel sign-in, Disable and Developer Mode off each release the old listener; the actual issued old callback is refused, with zero exchange/grant; no resurrection after actual restart; fresh B login/read and another restart work | Full controlled callback/account ownership procedure for 10; external browser UX remains separate |

The runner obtains the SDK's actual authorization URL privately and sends it to the owned issuer, then sends its actual issued redirect to Grain's actual callback listener. This replaces only external browser consent. It is **not** human-assisted consent or evidence for a live provider. Accounts are identified from actual bearer-token lookup and actual returned nested JSON, not a stored badge, model claim or injected result.

## Source ownership and focused review

The original no-account `/mcp` endpoint remains strict and rejects authorization headers. Only the additional fixed `grain-harness-auth` debug catalog entry uses `/account-mcp`, the same owned HTTPS peer and an explicit `mcpAuth` marker requiring `mcpPort` and excluding public live mode. The two adapters coexist in the twelve-case `mcp-foundation` selection. Ordinary/release providers do not gain a localhost endpoint, fixture certificate, consent handoff or extra process.

Production integration is limited to selecting the existing HTTP client and effective endpoint. Normal providers still clone the same managed HTTP client and use their unchanged catalog endpoints. The auth fixture uses its owned CA client for SDK metadata, DCR, exchange and authenticated MCP transport. Certificate/hostname validation and no-redirect policy remain active; OS trust is unchanged. The normal SDK auth manager, public registration, production callback state/issuer validation, session tickets/leases and real credential store remain authoritative.

The finite control accepts Tools/Account and existing management operations only. Main-window, app-ID/profile and fixed-provider guards remain. The private consent slot is bounded to one URL, exact owned HTTPS origin/port/path, no userinfo/fragment, at most 8 KiB, and a unique owner. Its lifetime guard clears only its own handoff on success/failure/cancellation/drop; an older guard cannot clear a replacement. Reading consumes the slot. URLs, state, codes, verifiers, bearer tokens and credential blobs are not status/report data.

The issuer bounds request bodies to 8 KiB, registration/code/token/callback entries to 16 each and its privacy-safe journal to 256. It requires a registered public client, exact owned callback/resource, S256 challenge/verifier and one-use code. Only issued callback destinations are exercised; no arbitrary socket/navigation is accepted. Fixture maps, sockets, callbacks, held responses, profile/TLS material and host processes are destroyed at teardown. There is no new background engine, cache or persistent server in Grain.

Independent vault inventory/deletion extends the existing `auth-cleanup.ps1` with `-Mcp`. It matches only the fixed MCP fixture key in this run's exact UUID-scoped MCP service, checks enabled marker ownership, enumerates metadata and never reads blobs. Native cleanup retains its original namespace/key family. Normal account data and client secrets are excluded. A deliberate abandoned grant must be independently deleted with one deletion and zero remaining.

Graph-first exploration was followed by impact, change detection and minimal review. The initial change call without `repo_root` inspected another configured repository and supplied no Grain evidence; the corrected call and review used this checkout. The graph reported broad/truncated impact and zero affected-flow edges; its baseline included previous committed files and omitted the two untracked fixture modules. Those limitations required direct source/diff tracing of client/endpoint selection, SDK/vault lifetime, callback admission, fixed controls, actual model oracle, rollback/cleanup and the new modules. Absence of graph edges is not whole-flow assurance.

**Audit repair:** the first account oracle accepted any `Failed (...)` as stale refusal. It now requires the exact production Cancelled/not-dispatched account-generation message, zero provider calls and a successful replacement account read. Self-tests reject unrelated network failures, expired confirmations, invented/wrong/coerced results, credential context and replay. An intermediate combined run exposed a missing production “The action was not dispatched.” prefix in that stricter oracle; it failed, cleaned up and credited nothing. The exact observed message is now required.

**Combined-run repair:** the existing transport-only final report retained 2,017 entries against the peer's 2,048-entry bound. Adding OAuth account evidence exceeded that bound during a genuine timeout. The catch block tried to record another error into the full journal and crashed the Node runner before teardown. The journal now has a fixed 4,096-entry ceiling with one reserved terminal-error slot. Overflow records a constant failure without throwing again or copying rejected bearer/parameter details. Every scenario independently rejects peer/issuer error entries before Pass. A self-test fills the boundary, verifies explicit failure and unchanged capacity on repeated errors, and refuses private rejected data. No production budget, request limit or clock is enlarged.

## Verification

Reports are retained under `tests/agent-harness/.runs/<run>/evidence/`; raw artifacts are ignored. Final combined coverage retains **145 separate observations, 2,146 bounded request entries and 94 actual tool calls**, with zero retained sessions/held replies/delay timers/grants and cleanup Pass. The audited OAuth repeat and both fault oracles use the same binary/source/runner identities.

| Run | Actual verdict and evidence | Acceptance / cleanup |
| --- | --- | --- |
| `run-4JRUXP` | All twelve combined transport/OAuth cases Pass, including four genuine 45s call deadlines and two 90s discovery deadlines | Final combined acceptance; cleanup Pass |
| `run-dTmIAr` | All three audited OAuth cases Pass again in another clean profile, including all three late-login boundaries | Final clean repeat; zero grants, cleanup Pass |
| `run-EeGMP1` | Actual A grant produces deliberately wrong B result; exact account assertion Fail, exit 1 | Required negative oracle; cleanup Pass, zero remaining |
| `run-z9xkzX` | One actual grant deliberately abandoned; exact assertion Fail, exit 1 | Independent MCP vault deletion 1, remaining 0; cleanup Pass |
| `run-iRUtI9` | All eight native-auth cases Pass, including account owners/cancellation/expiry/refresh/logout | Four discarded scoped native grants independently deleted, remaining 0; cleanup Pass |
| `run-zpbRjC` | Both ordinary Agent smoke cases Pass | Cleanup Pass |
| `run-3psc3S` | Both actual public MCP live cases Pass | No account written; cleanup Pass |
| `conformance-BoW3Ri`, `conformance-3P05Bw` | Legacy/modern tools each Pass with two actual official checks; standalone init remains Blocked with zero checks | All case/child cleanup Pass; final direct exit 2 |
| `logic-Rxjtgx` | All 48 normal-build MCP tests Pass | Production logic only; cleanup Pass |
| Feature guards / self-tests | Eight feature-gated guard tests and 34 harness self-tests Pass, zero skipped | Isolation/origin, exact refusal/result and journal-exhaustion checks |

| Final identity | SHA-256 |
| --- | --- |
| Real host `C:/gt/debug/grain-agent-harness.exe` | `9d1cbc468908f476b53358e8be1a769cdea09d2b5bb04915dca1ebd73a635ef2` |
| Build source | `c96c755296f5994dbd5b193ec11b5a46cc8838c5ecad97e6d585994223248964` |
| Final runner | `0a97a2e0cae1186c13556851ff848b2444e2b7076b33833a2bab85589c31647b` |

Independent inspection of all seven named final runtime roots found no retained data/fixture/TLS scratch directories, no private MCP token marker in reports, and matching source/binary/runner identities. The two official tool children and blocked init child each report cleanup Pass and zero retained sessions. The event listener 17124 is closed. This applies to these completed runs; the two separately policy-blocked interrupted roots below retain their exact limitations.

Initial supporting runs: `run-MuaCIk` passed the actual A/B fixture; `run-ZZmQWo` passed all three initial cases; `run-oVzT94` passed the expanded three cancellation boundaries. Their definitions predate the final strict oracle. `run-rUzdt0` failed that oracle's incomplete prefix; subsequent cases were Not run and cleanup passed. None substitutes for the final repeated unit.

`run-36JHWk` passed the preceding ten cases then failed the first genuine HTTP-deadline classification after about 21 seconds. The user reported accidentally cancelling Agent during this batch; the failed run remains explicit with cleanup Pass and the final uninterrupted batch supplies acceptance. This observation is not classified as a reproduced product bug. `run-t1ux8t` subsequently crashed at journal exhaustion; its incomplete checkpoint and policy-blocked scratch cleanup remain separate below. All required clocks/cases were repeated; no timeout or classification assertion was relaxed.

The normal-build MCP group `logic-Rxjtgx` passed **48** tests. Eight feature-gated harness guard tests passed, including exact account marker/origin guards. The real app/CLI build, frontend type/build, feature backend Clippy, scoped Rust/JS formatting and **34** runner self-tests passed; existing warnings remain and are not broadly cleaned up. `src/app/bindings.ts` remains the pre-existing 211-addition/206-deletion diff and is excluded. Build/run source fingerprints separate this source state from the parent commit. The shell initially normalized the full official nonzero exit to 1; repeating with `exit $LASTEXITCODE` verified the unchanged Node aggregator's correct exit 2 for the retained Blocked fixture, without changing cases or assertions.

## References and reuse

The [official Rust SDK OAuth guide](https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md), [MCP authorization contract](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) and [OAuth protected-resource metadata RFC 9728](https://www.rfc-editor.org/rfc/rfc9728.html) informed fixture metadata, discovery, PKCE and resource binding. Actual behavior was traced in the installed locked `rmcp-3.1.4/src/transport/auth.rs`; current upstream documentation is not proof of a different SDK version. No dependency upgrade or homemade production OAuth implementation is introduced.

## Retention, remaining work and order

Keep the three cases, exact oracles and shared external issuer as regression infrastructure. Keep finite debug endpoint/CA/consent seams only while needed for equivalent real-app acceptance; retire them with replacement coverage and remove references together. `mcp-foundation` is a selection of existing cases, not another engine or duplicate test suite. The shared metadata-only vault cleaner replaces duplicate cleanup code. No unused refresh, confidential-client or arbitrary-provider scaffold is added.

This unit does not certify live browser consent, real provider expiry/refresh, public/confidential registration variants, preservation after failed account replacement, changed-client invalidation, two authenticated MCP providers/fixed-port conflict, authenticated slow-close/shutdown, live nested tools or multi-step Agent work. Failed replacement must be tested with an existing A grant; disconnected denial is insufficient to certify preservation. Controlled native auth results do not certify those MCP paths.

The repeated verification/audit accepts only **10**. Supporting stages for 9/12/13/17/31 remain partial and do not change their Pending verdicts. Next use this fixture for authenticated slow-close/account preservation (31), then remaining auth/config/refresh/independence schedules and eligible live/nested prerequisites. Identify one eligible live provider for retained human-assisted requirements. No broad platform improvement starts before B1–B4 acceptance. All seven release gates, official standalone-init blockage, retained input findings and production discarded-grant reconciliation remain separate.

The earlier interrupted `run-FCSQDD` and this unit's journal-overflow `run-t1ux8t` have Pending scratch cleanup after automatic approval review rejected deletion with “blocked by policy”. For the latter, read-only inspection found no owned host/WebView or event/model/MCP listener; independent scoped MCP vault cleanup reports zero remaining. Its `evidence/interruption.md` preserves the absent CDP checkpoint and retained `data/fixture/mcp-tls` limitations. Neither rejected deletion is retried through another route; neither incomplete run supplies final acceptance/cleanup Pass.
