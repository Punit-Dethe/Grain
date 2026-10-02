# MCP response bounds and close/cancellation audit

Date: 2 October 2026. Branch: `extensions/tool-only-retirement`. Precommit base: `2d881045b90ba60454f9786dd97219e5653405fb`.

## Scope and verdict

Three new separately reported real-app procedures extend the existing controlled HTTPS peer. They exercise production Agent search, selected loading, approval, revalidation, MCP HTTP wrappers, execution, model continuation and actual panel closure. No production transport/authentication behavior, dependency version, ordinary UI or Handy code changes in this unit. The Rust change adds two finite debug-harness instructions under the existing window/profile/marker guard.

Controlled response-bound and close/cancellation coverage passes. This supplies further evidence for checks **20 and 31**, but does not complete their entire live/account requirements. Ledger remains **39 Pass / 14 Pending**: B2 has four pending numbered checks, B3 seven, B4 three. Whole B2 and all R0-R6 release gates remain open. No new human testing batch is assigned here.

## Independent procedures

| Stable ID | Evidence and recovery | Explicit exclusions |
| --- | --- | --- |
| `mcp.response-preview` | JSON and SSE each return a large multi-byte text plus oversized structured JSON. Actual model input remains within 16 KiB, preserves UTF-8/trust labeling and explicit truncation notice, and excludes oversized structured data. Actual UI shows the host-authored size-limit omission detail. Each variant is followed by an approved ordinary read. | No heap measurement, binary rendering support or live provider certificate. |
| `mcp.transport-bounds` | Stateless and legacy each receive six distinct conditions: declared-length JSON overflow, chunked JSON overflow, SSE data overflow, many small SSE comments exceeding the raw stream budget, oversized HTTP error body, and connection loss after actual receipt. Each has one approved provider call, an honest unknown outcome in model/UI, no raw failed body or tool replay, session cleanup and a successful fresh read. | No official conformance claim, whole-operation/catalog flood or real deadline measurement. |
| `mcp.close-cancellation` | Four approved held reads: stateless/legacy crossed with JSON/SSE. Provider receipt is the barrier before closing the actual Agent panel. HTTP response closure and zero protocol sessions/held calls are required. A deliberately late completion is attempted on the destroyed response; no result reaches the model. Fresh sessions verify current nested results and exactly one additional call. Pending approval closure is tested separately for both lifecycles with zero provider calls and fresh recovery. | The peer stores no account. This cannot prove credential persistence, production provider work being undone, global Escape behavior or arbitrary late push/subscription handling. |

Overflow payloads are valid JSON-RPC/SSE envelopes, rather than malformed substitutes. Comments are many small events; the test distinguishes the raw stream bound from a single large parsed event. HTTP errors remain HTTP errors independently of their body; existing normal-build tests supply the lower-level bounded-body oracle. Limits are unchanged: JSON 2 MiB, SSE stream/event ceiling 512 KiB, error body 16 KiB, retained model result 16 KiB. Deliberate fixture sizes exceed these boundaries. No production clock is shortened.

Stages run serially with distinct names and recorded outcomes. A first failure stops the batch and leaves subsequent scenarios Not run. Call receipt, returned result and model verdict are independent observations. A generic success, pre-dispatch refusal or model error cannot satisfy an unknown-result or bounded-preview oracle.

## Evidence and reproducibility

Build once after Rust/assets stabilize:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.response-preview --fault short-mcp-preview
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/runner.test.mjs
```

Run suites serially and after native compilation finishes. The deliberate fault must fail with a missing truncation-notice model observation, exactly one provider call and cleanup Pass; exit nonzero alone is insufficient.

Final binary SHA-256: `cb419822e40462a453bcde59034eaa4afe97a57afeef083193e24f7d89f8c83b`. Source fingerprint: `6d771db6d3f123424c96a99530ae07a8861bac50ea23a8058ffec34081940aa6`. Final runner fingerprint: `085f4b428d70a2b85b2490b082989e24491ef76c9f89d3c776e804219e3a3ed0`. Build commit is the precommit base above; fingerprints identify the tested edits. The unrelated dirty generated `src/app/bindings.ts` is preserved and fingerprinted, not included in this commit.

| Report under `tests/agent-harness/.runs/` | Verdict |
| --- | --- |
| `run-mnHybt/evidence/report.json` | All five MCP procedures Pass; 52 supporting stage observations, 46 actual tool calls, four held-response closures and four discarded late replies; cleanup Pass. New procedure timings: 1,888 / 11,087 / 5,261 ms. |
| `run-Rc2B9N/evidence/report.json` | Final identical-runner clean-profile repeat: all five Pass, 46 calls, zero sessions/held calls, cleanup Pass. New procedure timings: 1,885 / 11,186 / 5,215 ms. |
| `run-dhzma2/evidence/report.json` | Intentional short-preview fault: Fail / exit 1, missing truncation notice captured in model evidence, one actual call, no replay, cleanup Pass. |
| `logic-0ihUWH/evidence/report.json` | Normal production MCP group: 46 Pass, zero failures. |
| `run-fEMcZr/evidence/report.json` | Ordinary native cold/warm and declined-approval regression: both Pass, cleanup Pass. |

The identical-runner final runs each finish all five case bodies in about 24 seconds, without reducing production deadlines. This is a batch infrastructure saving, not a claim that all MCP foundation requirements are complete. Both final runs, the deliberate fault and smoke use the identities above. Earlier `run-hqoGuk` and `run-Nmi4z8` pass all five cases but predate the final failed-report scope fix; their different runner identities are retained, not represented as identical final runs.

Additional verification: 22 runner self-tests Pass; four feature-gated isolation/auth guard tests Pass; actual frontend type check/assets build and Rust feature build Pass; scoped Prettier/rustfmt/diff checks Pass. Existing Rust warnings remain. No new UI design or ordinary microphone/shortcut test is claimed.

## Findings resolved in this unit

1. `run-8P5eIE`: preview model oracle passed, but the UI assertion expected the model's full truncation wording. Source tracing shows the UI bounds its result body and preserves a separate omission detail. The assertion now checks that explicit production detail; the model still independently checks the original truncation notice. This is an oracle correction, not an application fix. Later cases remained Not run; cleanup Pass.
2. Fault runs `run-W1JakT` and `run-8rsKU6`: adding failed-model evidence exposed a JavaScript scope error, masking the original assertion as a prerequisite failure. Move the model journal baseline into the scenario scope before `try`. Final deliberate fault retains the original substage failure and model errors; cleanup Pass in every run. These earlier fault runs are not accepted fault-oracle evidence.
3. Source review: snapshot fixture mode per request, use valid oversized envelopes, and bound retained closed response handles. Late completion now actually invokes `end()` on the destroyed response and immediately releases the handle. It is not a fabricated journal-only late-reply observation.

Graph change/review tools report 13 changed entities and 30 impacted nodes across 13 files, with incomplete flow/test edges and high aggregate review risk. Zero discovered flows is not zero impact. Scoped source review traces the finite enum through the guarded submit command and production Agent, bounds fixture memory/requests/journals, checks TLS and exact endpoint admission remain unchanged, and verifies failure reporting/owned cleanup. Normal production MCP tests, the feature build, real-app matrices and deliberate failure support the audit; graph gap counts are not treated as test certificates.

## References and maintenance

The [2026-07-28 cancellation specification](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/patterns/cancellation.mdx) describes HTTP response-stream closure as cancellation, with timeouts preventing hung requests. Legacy behavior is tested separately; no assertion assumes identical wire notifications across protocol generations.

The [locked Rust SDK transport source](https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.1.4/crates/rmcp/src/transport/streamable_http_client.rs) owns request streams and cancellation tokens. The [TypeScript SDK client documentation](https://ts.sdk.modelcontextprotocol.io/v2/api/index/@modelcontextprotocol/client/) independently describes per-request abort signals and protocol-dependent cancellation. These support observing actual transport closure rather than accepting only an inactive Agent flag.

[Goose's MCP client](https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/agents/mcp_client.rs) separates cancellation/timeouts and active-call cleanup, with distinct modern/legacy request paths. Grain retains its narrower tool-only, on-demand policy; this batch does not import Goose's broader extension capabilities or add another framework.

The [harness retention/cleanup inventory](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) names retained regression assets, per-run disposable resources, feature-gated seams and retirement conditions. No unused future scaffold is left by this unit. Production orphan reconciliation and physical privilege retirement remain explicit later work.

Next B2 work: catalog/whole-operation budgets and real timeout schedules, the pinned guarded production-wrapper conformance entry, and eligible live read/account-persistence evidence. Complete and audit those before B3 authentication or broader platform improvements. No live account is certified by the fixed unauthenticated peer.
