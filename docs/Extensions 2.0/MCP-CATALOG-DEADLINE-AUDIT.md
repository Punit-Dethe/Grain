# MCP catalog budgets and real deadline audit

Date: 2 October 2026. Branch: `extensions/tool-only-retirement`. Precommit base: `788256e640665dce17dc6e8a60dc81af942a20a0`.

## Scope

This B2 unit adds three independent maintained real-app scenarios to the existing controlled HTTPS fixture. Production transport limits, SDK version, clocks, authentication, ordinary UI and Handy code remain unchanged. One finite debug-only instruction lets the scripted model verify actual failed discovery without requesting a tool; it uses the existing application/window/profile/marker guard.

| Stable ID | Assertions |
| --- | --- |
| `mcp.catalog-budgets` | Six cases crossed with modern/legacy and JSON/SSE: 129 tools including unsupported definitions; duplicate identity across rejected/supported pages; oversized cursor; 32 incomplete empty pages; cumulative catalog metadata; and cumulative raw response bytes. Both management Test and actual Agent refuse with the expected reason/page count. The model never receives action definitions; no approval or tool dispatch occurs. Every combination has a fresh approved read. |
| `mcp.http-deadline` | Four approved held reads: modern/legacy crossed with JSON/SSE. One actual tool receipt, honest unknown outcome, no replay, actual response/session closure, measured unchanged 45-second HTTP budget, attempted late reply and successful fresh read. |
| `mcp.discovery-deadline` | Modern/SSE and legacy/JSON discovery each deliver two pages after 35 seconds apiece. A third delayed page is cancelled by the unchanged 90-second whole-operation deadline. Exact timed-out/incomplete refusal, no action exposure or dispatch, timer/session cleanup and fresh approved read are required. |

Catalog and whole-response tests keep every individual JSON/SSE reply within its transport cap. They therefore exercise distinct limits rather than merely re-triggering the previous oversized-body tests. Catalog metadata uses two JSON pages or seven SSE pages. Unknown padding in eight JSON pages or 24 SSE pages crosses the raw 8 MiB operation budget without inflating the actual tools catalog. Empty catalogs hit exactly 32 pages; no unbounded server loop is introduced. Host identity/count checks cover rejected schemas too.

The discovery deadline is tested across successive completed requests, independently of a single HTTP request timeout. Measurements use monotonic clocks. The HTTP assertion permits 44-55 seconds from actual provider receipt, including result presentation; discovery permits 89-100 seconds from the driver start and separately verifies three attempts/two completed pages. These are acceptance tolerances; production clocks are not overridden. No inference that provider effects were undone follows from transport cancellation.

## Reproduction and evidence

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.catalog-budgets --fault accepted-mcp-catalog
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/runner.test.mjs
```

Run serially after native compilation finishes. The full eight-case MCP suite now includes about six minutes of genuine timeout waits per run. Repeat in a new owned profile after audit. The deliberate fault supplies an ordinary complete catalog where rejection is required: it must fail its first precise management refusal assertion, with zero tool calls and cleanup Pass. Do not accept exit nonzero alone or rerun scenarios automatically after failure.

Binary SHA-256: `c330bbdfb59f20e116e9a5fd0c00b80e354c9dfeece9a25387afa1a8929d4a69`. Source fingerprint: `6d6c4af7c8145ae4ab64a62d13e79cb6866dd28fb53a3fa61f5604b1467b214f`. Build commit is the precommit base above. The unrelated dirty generated `src/app/bindings.ts` is preserved and fingerprinted, not committed with this unit.

Final runner fingerprint after adding case checkpoints: `d0c3f21c89c50666510dc8d96315f3b92375659262b4db92f9d107840f46f129`. Both final full MCP runs below use this identical runner/binary/source identity. Reports are local under `tests/agent-harness/.runs/<run>/evidence/report.json` and `report.md`; no private keys, profiles or credentials are committed.

| Run | Reviewed verdict |
| --- | --- |
| `run-lsAhcb` | All eight cases Pass; 116 separately reported observations, 80 actual tool calls, 412,540 ms of scenario time; cleanup Pass. |
| `run-PfoAgA` | All eight cases Pass from a fresh profile after review; same 116 observations/80 calls, 412,211 ms; cleanup Pass. |
| `run-Lje41Z` | Deliberate accepted-catalog fault fails the first precise management refusal assertion: `stateless-json-tool_count: management Test accepted incomplete catalog`. Exit 1, zero tool calls and cleanup Pass. No prerequisite failure is credited as an oracle check. |
| `logic-e7G4Lv` | All 46 normal-build production MCP tests Pass. |
| `run-HXkox0` | All eight affected native-auth regression cases Pass, cleanup Pass, identical final identities. Independent scoped-vault reconciliation deletes four previously discarded test grants and confirms zero remaining; this retains the separate production orphan-reconciliation finding. |
| `run-wseoi8` | Both ordinary harness smoke cases Pass, cleanup Pass, identical final identities. No microphone/ordinary-user-profile certificate follows. |

The four HTTP measurements are **45,061 / 45,123 / 45,050 / 45,045 ms** in the first final run and **45,130 / 45,033 / 45,117 / 45,024 ms** in the repeat (modern JSON/SSE, legacy JSON/SSE). The two discovery measurements are **90,246 / 90,450 ms** and **90,189 / 90,333 ms** respectively (modern SSE, legacy JSON). Every deadline case asserts its own refusal/unknown outcome, no replay, resource release and fresh recovery; aggregate duration alone is not the oracle.

Final JSON review confirms eight Pass verdicts with no `activeScenario`, zero sessions/held replies/delay timers before fixture shutdown, and cleanup Pass in both complete runs. Disposable `data/`, `fixture/` and `mcp-tls/` are absent after both and the deliberate fault. During the repeat, the checkpoint correctly showed seven completed Pass cases, discovery active and cleanup Not run. This is distinct from the earlier interrupted run's Pending cleanup below.

All **23 runner self-tests** pass, including the independent catalog-refusal oracle's partial-success, oversized-error and action-exposure negatives. All **four feature guards** pass in the feature-enabled Rust lib target; existing backend warnings remain. The harness build passed CLI, application/config type checks, actual frontend assets and native host compilation. Scoped JS syntax/Prettier, Rust formatting and changed-file whitespace checks pass. The unrelated generated bindings remain unchanged at 211 added/206 removed lines. Native compilation and real-host suites ran serially; no shortened production clock or simultaneous test host was used to accelerate the deadline procedures.

## Review and maintenance

Graph entry/search/change/review calls identify ten changed entities and thirty impacted nodes in thirteen files; the subsequent blast-radius query reports 88 direct dependents and 32 impacted nodes with fifteen additional files. Test/flow edges are incomplete and aggregate review risk is high. Zero reported flows is not proof of zero impact. Source tracing checks the guarded enum, production `list_tools`/`discover_on_service`, cumulative bounded-client accounting, exact endpoint/CA admission, model no-action refusal, monotonic observation deadlines, failure evidence and owned cleanup. Common reporter/model dependencies prompted the full native-auth and ordinary-smoke regression above. No unresolved defect was found in this slice; the interrupted-run cleanup exception and older production findings remain explicit.

The fixture snapshots each request's mode. Delayed replies use a bounded set of at most four owned timer handles, release their close listener on completion/cancellation, and resolve abandoned waits during shutdown. `delayedRepliesBeforeShutdown` records remaining timers separately from held tool replies and protocol sessions. Retain these external regression modes; no extra production engine, arbitrary transport command or later-use scaffold is added. The [cleanup inventory](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) records retention/retirement rules. Per-run data, TLS private keys, credentials, listeners and owned processes must still be removed on every verdict.

**Interrupted initial run retained:** `run-FCSQDD` lost its terminal session during HTTP deadline checks, before final JSON/cleanup evidence. The first six cases had console-only Pass output; none is credited as final acceptance from this run. Read-only recovery found no active harness runner/application and closed model/MCP marker ports. Automatic approval review rejected deletion of the disposable profile/fixture/TLS directories with "blocked by policy" before that command ran. Cleanup remains Pending; no scenario was automatically retried. The local `evidence/interruption.md` and retained inventory identify the exact run. The new run uses a fresh root.

This exposed a reporting gap: evidence previously existed only at final teardown. The runner now saves reports before/after each case with an explicit `activeScenario` and cleanup Not run until teardown. A hard interruption preserves completed case observations without claiming the whole run or cleanup passed. The final JSON/normal shutdown still determines acceptance. No new background checkpoint service or timer is added.

The [current cancellation specification](https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/main/docs/specification/2026-07-28/basic/patterns/cancellation.mdx) recommends a maximum timeout even when progress is reported and ignoring cancelled replies. This supports separately observing absolute lifetime, request closure and later fresh results. This batch does not claim progress-notification coverage.

The [locked Rust SDK client](https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.1.4/crates/rmcp/src/service/client.rs) owns negotiated startup, response correlation and caches. Grain's host deadlines and received-byte budgets stay at the application boundary. [Goose's MCP client](https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/agents/mcp_client.rs) independently separates timeout/cancellation from response receipt and has different modern/legacy paths. These references inform coverage; no framework or dependency replacement occurs during baseline testing.

## Remaining acceptance

The maintained inventory becomes **48 runtime scenarios**, eight MCP cases, and 23 runner self-tests. Ledger remains **39 Pass / 14 Pending** until the full mapped live/account procedures are accepted. Controlled fixtures supply supporting coverage for checks 20/31 and further regression coverage for accepted check 23; they provide no live OAuth/account-persistence certificate.

Next B2 work is the guarded, pinned official conformance entry through Grain's production wrappers, followed by eligible live read/account evidence. All B3 authentication and B4 Agent workflow requirements, retained input findings, production orphan reconciliation and R0-R6 release gates remain open. No broader platform implementation begins before foundation acceptance.
