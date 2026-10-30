# Linear live browser acceptance and Agent observations

**Date:** 3 October 2026. **Branch:** `extensions/tool-only-retirement`. **Decision:** accept numbered check **9** after reviewing the user's actual isolated browser run and the existing controlled denial/cancellation evidence. Current ledger: **51 Pass / 2 Pending (12/17)**, comprising 15 user-reported and 36 reviewed automated results. Whole B2/B3 and all seven release gates remain open. No production code, SDK version, scope policy or task limit changes in this unit.

## Evidence and focused audit

The user ran the maintained real-application command `node tests/agent-harness/run.mjs --suite mcp-linear-sign-in --linear-sign-in` and reports completing both browser steps successfully. The actual report is `tests/agent-harness/.runs/run-Flbf3s/evidence/report.json`, run UUID `b4479c79-45e9-4683-a20e-03dab9263d01`, on Windows at commit `abaeac15294a82f0401039d6faf0175ab2e266df`. It ran from 00:28:39 to 00:29:15 on 3 October in Asia/Calcutta; the scenario itself took 34,428 ms. Its only recorded dirty file is the pre-existing generated bindings file.

| Stage | Verified result |
| --- | --- |
| First browser cancellation | Human acknowledgement plus actual production Cancelled outcome; callback port immediately reusable; disconnected/disabled state and zero scoped vault entries. |
| Fresh second sign-in | Actual SDK code exchange completes; exact explicit token-response `read` scope and verified Linear issuer; one isolated vault entry; callback port released. |
| Authenticated discovery | 38 supported tools from the fixed read-only endpoint; no tool executed. |
| Real host restart | Exact grant metadata restored with one scoped vault entry; no second browser sign-in. This stage inspects restoration, not an Agent read after restart. |
| Cleanup | Runner cleanup Pass, zero OAuth/client-secret entries remaining and owned profile/fixture trees removed. |

The report's binary SHA-256 `af7e98f72cae1557886880b67c8c15d27728cdc42f871c93ed2a7fad40a2cf82` was independently matched to the built host. Source fingerprint is `a3a2eb86de73da8055aacd02eb3ff745c5be5f84f77fb8bed5999b55e8f9abec`; runner fingerprint is `8943f68dcd76db21a74f00f9196d96e3ad9b3d2cec2b4ebc2a658368d1794b2b`. Independent post-run inspection found zero recorded owned hosts, zero 17124 listeners, and zero matching vault entries through metadata-only `-InventoryOnly -Mcp -McpLinear`. Only evidence and the run marker remain. Ordinary Grain/account/browser processes were preserved.

Graph-first review was supplemented with source tracing of the fixed alias, production OAuth/session/vault ownership, browser handoff, first-flow acknowledgement, callback reuse, fresh consent, metadata validation, discovery, restart and finally-disconnect. The [preparation audit](LINEAR-BROWSER-CONSENT-AUDIT.md) preserves the prior repeated actual-SDK no-browser cancellation and controlled OAuth regression evidence. The actual human case adds the missing browser cancellation and successful retry, rather than claiming another synthetic case certifies a live provider. Provider-side denial remains covered by the existing controlled issuer; this run's actual live outcome is cancellation.

All **64 runner self-tests** pass again, including the first-flow approval, broadened-scope, lost-restart and empty-discovery negative controller oracles and five terminal-cleanup exits. All **18 Agent production-logic tests** pass through the maintained `production-tests.mjs --group agent` runner (`logic-HY5nxn`), including exhausted budget dispatch refusal and completed-receipt preservation. These logic results are separate from real-browser acceptance. The prior audit's controlled/live prerequisite repeats remain applicable; the human run was not silently replayed.

Check 9's denial/cancellation, absent grant, released/reusable callback and fresh recovery requirements now have the required controlled and live-browser evidence. Checks 12 and 17 remain Pending. Discovery counts, grant restoration and refresh-token presence do not establish post-restart Agent reads, actual refresh or nested input/result compatibility.

## Actual grant lifetime and remaining tests

The actual MCP token response issued a lifetime of **86,100 seconds (23 hours 55 minutes)** and included refresh availability. The harness disconnected and removed that isolated grant at completion. Its recorded expiry cannot be used later to certify refresh against a grant that no longer exists, and it says nothing about the ordinary profile's token issuance time.

Next execute a narrowly reviewed live-read unit: obtain actual authenticated tool schemas, select a harmless supported read, validate its exact arguments, exercise Agent discovery/approval/dispatch, verify a genuine result, and repeat after a real restart. Record schema-only and bounded result oracles without retaining account identities, issue bodies or private provider descriptions. Check 17 needs an actual nested argument value accepted by a suitable live read tool; issue creation/assignment, nested output alone and flat filters cannot substitute. If Linear lacks that shape, record the exact compatibility prerequisite and choose a suitable supported provider without claiming Pass.

Then extend the same maintained runner for check 12's actual-clock expiry/recovery unit. Use a newly owned isolated read-only grant, its actual issued lifetime and an initial successful read. Release idle connections/host resources while preserving only the explicitly owned test profile/grant during that active procedure; after actual expiry, restore the real host, perform a harmless approved read, and verify successful scoped grant renewal without a fresh browser login. Record offline/recovery behavior, then retire owned processes and delete the exact test grant/profile. Do not edit the clock, forge expiry, import ordinary credentials or replace the issued lifetime with an API documentation assumption. This procedure is next work, not implemented or passed by this audit. It must support explicit cancellation and bounded final cleanup; no background service or persistent product engine is required.

[Linear's official MCP documentation](https://linear.app/docs/mcp) distinguishes its regular read/write endpoint from its read-only endpoint and `read` OAuth scope. Tool annotations remain hints rather than enforcement, as described by the [MCP maintainers](https://blog.modelcontextprotocol.io/posts/2026-03-16-tool-annotations/); a forthcoming read allowance must validate the actual contract and retain host approval and transport restrictions. Do not authorize arbitrary names solely from a provider's annotation or copy a third-party Linear server's schema. The [official Rust SDK OAuth guide](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.1.4/docs/OAUTH_SUPPORT.md) is a design reference; the installed source and actual token response remain authoritative for this test.

## Ordinary Agent results and deferred budget finding

The user separately reports that ordinary Grain, using the configured model and their empty read/write Linear test workspace, created issues and assigned them to the user successfully. This is useful human-reported live Agent write evidence; no independent issue receipt, exact request trace or build identity was supplied for those ordinary tasks. The user's authorization for disposable create/edit/delete tests is retained. It does not broaden the isolated authentication-only suite.

On some repeated requests the user saw:

> I couldn't finish the remaining steps: The model requested more tools after the task budget ended. Completed actions must not be repeated automatically

Retain finding **AGENT-BUDGET-01**, open for a focused Agent efficiency/completion investigation after the remaining baseline tests, unless evidence shows incorrect dispatch, duplicate writes or account mixing. Source `src-tauri/src/agent.rs` defines a task limit of eight admitted tool rounds or 24 calls; discovery/loading calls participate in the task budget. After exhaustion, the model receives no tools; if it nevertheless asks for calls, Grain returns this error before dispatch. Earlier completed actions remain completed. Existing budget and receipt tests pass. This verifies the stop path, not why this user's particular tasks reached the limit or whether the model/provider behaved optimally.

Before changing limits, capture a redacted failing task's discovery/load/action round counts, whether it is a fresh task or approval continuation, requested call names, confirmed completed receipts and unfinished work. Reproduce against the configured model with disposable objects; audit discovery repetition, tool selection, continuation accounting and final-response handling. Preserve no-replay guarantees and distinguish expected termination from an unexpectedly wasteful or failing task. Do not raise limits, automatically retry completed writes or mark the issue resolved merely because smaller tasks succeed. No production fix or additional numbered acceptance is claimed here.

## Maintenance

Only maintained documentation changes in this unit. No temporary scaffold, alternate UI, extra engine or new cleanup obligation is introduced. Historical snapshots retain their original counts; the progress ledger and reuse plan own the current 51/2 position. Prior policy-blocked scratch cleanup, retained input findings, discarded-grant reconciliation, official initialization prerequisite and all release gates keep their previous status.
