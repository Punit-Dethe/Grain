# MCP live reads and HTTP version negotiation: focused audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Base:** `aa9c8a4f8ea8cc48d38d0697b17339fb0852d8a1`. **Status:** focused audit and final verification complete; checks 8/20 accepted. Ledger **41 Pass / 12 Pending** (15 human, 26 reviewed automated). Whole B2 and all seven release gates remain open.

## Scope and acceptance mapping

This unit supplies actual public-service evidence through Grain's real application, selected-tool search/load, Agent confirmation, production MCP SDK and result handling. It adds no provider to the ordinary catalog and no broader extension capability. The model remains scripted to make requests and assertions reproducible; it cannot manufacture the provider response.

| Requirement | Prior supporting evidence | Missing procedure supplied here | Acceptance condition |
| --- | --- | --- | --- |
| 8 | Controlled modern/legacy JSON/SSE, catalog filtering and disable checks | Public discovery/read, stale and fresh disabled refusal, re-enable and actual restart recovery | Pass: final live procedures, repeat, audit and cleanup |
| 20 | Controlled declared/chunked JSON/SSE/error/result limits, bounded previews and recovery | Public ordinary read → large documentation request → fresh ordinary read | Pass: honest bounded unknown outcome, no replay, genuine recovery, repeat and cleanup |
| 17 | Controlled nested argument/result values, including legacy HTTP rejection | Not supplied: this public provider has a flat `repoName` parameter | Remains Pending for a suitable real nested read |
| 31 | Controlled approved/pending close, response/session release and fresh recovery | Not supplied: public provider has no account to preserve | Remains Pending for account-preserving slow-read acceptance |

Whole B2, all seven release gates, MCP OAuth checks 9–15 and Agent workflows 46–48 remain open. Passing public reads does not certify account sign-in, native+MCP integration or live-model judgment. No requirement is removed.

## External provider and evidence boundary

The debug-only explicit marker selects only `https://mcp.deepwiki.com/mcp` and public repository `modelcontextprotocol/rust-sdk`, using normal TLS trust. The post guard admits only protocol discovery/initialization/list/ping plus `read_wiki_structure` or `read_wiki_contents` with the exact fixed argument. It refuses questions, other repositories, extra arguments and credentials before dispatch. The fixed provider remains absent from ordinary/release builds. No OAuth flow, personal account, production vault entry or system trust-store change is used.

The live catalog observed on 2 October reports DeepWiki 2.14.3 with three tools: `ask_wiki_question`, `read_wiki_contents`, `read_wiki_structure`. Only the two documentation reads can execute through this test guard. The question tool is never loaded for execution. The [provider's documentation](https://docs.devin.ai/work-with-devin/deepwiki-mcp) and [owner-published registry entry](https://github.com/mcp/cognitionai/deepwiki) independently establish public no-auth documentation access and the HTTP endpoint. Actual tool metadata is checked rather than assuming older documented names remain current.

The first successful run received 2,498-byte model-facing structure results identifying the fixed repository and real Overview/Transport topics. The large request returned a 209-byte honest unknown/unusable outcome, followed by a successful ordinary read. This is neither a successful large-document claim nor proof of exact remote payload size. Exact overflow/budget behavior is established by separately controlled fixtures on the same adapter. Evidence stores byte counts/result kinds and fixed metadata, not the documentation body. Client attempt observations are conservative pre-send backend observations, not independent remote receipts.

Fresh disabled search retains an unrelated harmless native directory entry so Grain enters its tool-search route. No native action runs. Empty-directory text-only model fallback is excluded from this procedure. Closing the previous panel before submitting a fresh request is required by the normal harness lifecycle; failing to do so produced no new model turn and is retained as a harness setup failure.

## Reproduced compatibility failure and narrow repair

An actual public discovery attempt failed before dispatch. DeepWiki rejects the 2026-07-28 discovery probe with HTTP 400 and a typed legacy `-32600` error using generic ID `server-error`; legacy initialization at 2025-11-25 succeeds. The pinned `rmcp` 3.1.4 Auto client accepts correlated nonmodern discovery errors for its own fallback, but rejects this uncorrelated error.

The [current MCP HTTP versioning contract](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/versioning.mdx) permits inspecting the initial HTTP 400 body to distinguish protocol eras before a legacy initialize. The [TypeScript SDK protocol-version guidance](https://github.com/modelcontextprotocol/typescript-sdk/blob/main/docs/protocol-versions.md) supplies an independent official SDK reference; the actual Rust behavior was traced in the pinned local `rmcp-3.1.4/src/service/client.rs` and its HTTP transport. Grain's exact shim is an application choice informed by those sources, not a claim that an incorrect JSON-RPC ID becomes valid.

The bounded HTTP client records a per-operation flag only for HTTP 400, typed `server/discover`, an uncorrelated typed JSON-RPC error, and legacy `INVALID_REQUEST`/`METHOD_NOT_FOUND`. The original reply is forwarded unchanged; SDK correlation is not relaxed or rewritten. If initial negotiation fails with that flag and time remains, the host drops the first transport and starts exactly one fresh legacy initialize. The same cloned bounded client shares the byte counter; the same absolute operation deadline, endpoint, account manager and generation guards apply. No tool has run at this point, so there is no action retry.

Success responses, tool calls, authorization statuses, server/internal errors and modern protocol-error codes do not arm the shim. Correlated legacy errors continue to use the SDK's existing fallback. The official raw initialization fixture's malformed HTTP 200 empty discovery reply does not qualify and remains Blocked. No SDK upgrade, new engine, session cache, idle service or blanket fallback is introduced. The extra state is one small atomic flag per existing operation.

## Focused source audit

Graph-first exploration covered context/search, file summary, impact, then change detection/review and the reproduced failure's focused impact. Seven calls were used because the live failure added a production-boundary repair to the original harness-only scope. The initial impact returned 89 direct/36 impacted/17 additional entities; focused two-file impact returned 93 direct and no transitive edges; review reported 35 nodes/16 files/167 gaps. Incomplete indexing and absent edges are not complete-flow evidence.

Direct review traced both actual service-open callers, shared byte/absolute budgets, failed-transport ownership, cancellation, SDK lifecycle selection, auth-client reuse, marker/profile/window guards, finite instructions, public argument restriction, selected-schema loading, stale/fresh refusal, exact result oracle and final owned cleanup. The public mode and local TLS peer are mutually exclusive. Failure paths explicitly close the panel and disable the test provider; the runner independently tears down its owned host/profile. Reports keep real-service and controlled fixture evidence separate.

Pure tests reject actions/success/auth/modern errors as fallback triggers and enforce the public fixed-repository guard. An actual local SDK fixture asserts exactly one rejected probe, one fresh initialize, one tool call and clean service close. The real-app JSON/SSE counterpart independently checks each operation's handshake count, exact nested result, session deletion and zero retained sessions.

## Verification and evidence

Reports are retained under `tests/agent-harness/.runs/<run>/evidence/`; raw reports are ignored, not committed. Both final live runs share matching binary/source/runner identities. Each has five genuine 2,498-byte ordinary results and one 209-byte large-read unknown outcome; stale/fresh disabled calls have zero backend attempts. The full nine-case regression passed with 120 separate observations, 82 tool calls, zero retained sessions/held calls/delays, and owned cleanup Pass.

| Run | Observed result | Credit / cleanup |
| --- | --- | --- |
| `run-m1nS13` | Both live cases Pass before the final visible fresh-refusal assertion | Supporting first run; cleanup Pass |
| `logic-lVNq3D` | 48 production MCP tests Pass, including exact new handshake counters | Production logic; cleanup Pass |
| `run-xeltrm` | All nine real-app MCP cases Pass, including unchanged 45/90-second deadlines | Controlled regression; cleanup Pass |
| `run-JMf70A` | Both final live cases Pass, including visible fresh disabled refusal | First final acceptance run; cleanup Pass |
| `run-itGxYT` | Both final live cases Pass in another fresh profile | Post-audit clean repeat; cleanup Pass |
| `run-rzZSwL` | Genuine approved read followed by deliberately missing result evidence; exact assertion Fail, exit 1 | Required negative oracle; cleanup Pass, no acceptance credit |
| `run-GX2rOw` | New generic-ID HTTP rejection JSON/SSE case Pass again | Focused clean repeat; cleanup Pass |
| `conformance-J438Ym` | Legacy/modern tool cases each Pass with two official checks; initialize still Blocked with zero checks | Correct nonzero complete entry; cleanup Pass |
| `run-8YEN83` | All eight native-auth cases Pass | Four discarded scoped credentials independently deleted, zero remaining; cleanup Pass |
| `run-fkujds` | Both smoke cases Pass | Cleanup Pass |
| Feature-gated Rust guard run | Six tests Pass | Marker/public-argument and existing isolation/account guards |
| Runner self-tests | 31 Pass, zero skipped | Exact result, bounded/unknown output, replay and scenario inventory guards |

The real application/CLI build and frontend type/production checks passed. Scoped JavaScript/Rust formatting and whitespace checks pass. Existing unrelated Rust warnings remain. The user's pre-existing generated `src/app/bindings.ts` diff is excluded; its unrelated whitespace findings do not authorize editing it.

| Final identity | SHA-256 |
| --- | --- |
| Host `C:/gt/debug/grain-agent-harness.exe` | `3e6fa56fcc0f4e91c85af85df382cd7594ab947e21ca4418de393cd44d242ba5` |
| Build source | `41ccd1d7b3f75cfd6048c483e79b359f3c7af070b93599da0eecf01da03a4a23` |
| Final runner | `755239ed8cdcd82f13feebcc28ced567b028b20dfecb5ecd2b24cc3572ac3125` |

Independent final inspection found no retained data/fixture/TLS scratch roots in the eight named final runtime/report roots. The event listener 17124 and full-regression peer port 54644 were closed. Native vault cleanup retained zero credentials. Official tool validation reports 17 legacy and 14 modern wire messages, no violations, and one exact 5+3=8 tool result per case. Public mode writes no credential. Build/run commit and dirty source state are recorded separately; documentation updates do not alter these source identities.

Failed attempts remain explicit: `run-FrFinP` exposed an inappropriate local-fixture bootstrap assertion for the public mode; `run-lO7YVA` reproduced the production negotiation failure; `run-maaNbi` and `run-teWagv` both received real public reads and refused stale approval but timed out waiting for a fresh request because its previous panel had not been closed. All four completed owned cleanup. An intermediate runner self-test failed its old hardcoded eight-MCP inventory assertion after the ninth case was added; final inventory tests now pass. These failures receive no acceptance credit and are not erased.

Previous interrupted `run-FCSQDD` still has Pending scratch cleanup after automatic approval review rejected deletion with “blocked by policy”. This unit does not retry it through another tool or derive acceptance from it. Its exact inventory remains in [harness retention](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory). Every new run verifies its own cleanup independently.

## Reproduction, retention and next work

Build the stamped real application first and leave the Windows desktop idle. Run actual suites serially after native compilation:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp-live
node tests/agent-harness/run.mjs --scenario mcp.live-read-disable --fault missing-live-evidence
node tests/agent-harness/run.mjs --scenario mcp.legacy-http-probe
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/conformance.mjs
```

Live and controlled suites expect exit 0. The intentional evidence fault expects exit 1 after a genuine read and cleanup Pass. The full official entry still expects exit 2 for the retained initialize blocker. No timeout is shortened or replaced with a passing wait. External availability failures remain failures/prerequisites, not automatic retries.

Keep the nine controlled MCP cases, two explicit public cases, finite debug instructions and result oracles as regression coverage. Public tests are excluded from ordinary `all`. Destroy owned host/profile/model sockets and response handles each run; public mode creates no credential. Retire the fixed public marker/post guard only with equivalent real-application safe-service coverage. Retire the production HTTP-era shim after a reviewed SDK version supplies equivalent generic-ID HTTP rejection handling, with these positive/negative tests repeated; do not maintain dual negotiation engines.

Next remains foundation evidence: checks 17/31 need suitable nested/account-preserving MCP prerequisites; checks 9–15 need scoped MCP OAuth coverage and a supported live-account batch; checks 46–48 need complete Agent workflows. Fix the pinned official initialization prerequisite separately. No broad library replacement, physical legacy deletion or new product feature starts before baseline testing/audits are accepted.
