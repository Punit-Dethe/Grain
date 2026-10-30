# Agent workflow interruption and receipt audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Scope:** numbered baseline check 48. **Status:** accepted after focused audit, controlled and genuine-model repeats, detected faults, affected regressions and independent cleanup inspection. The ledger is **49 Pass / 4 Pending** (15 human, 34 reviewed automated); baseline B4 checks 46–48 are accepted. All seven release gates remain open.

## Contract and source checks

The [original procedure 48](MCP-EXTENSION-PLAN.md) requires denial without repeated approval or execution, a permitted independent verification, actual approval expiry or session closure with stale refusal, and a completed write receipt retained when the model cannot finish. Ambiguous tool results must never cause automatic write replay; restart ends pending workflows. There is no restart-resume promise.

The reference check used the [MCP tool/confirmation guidance](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/server/tools.mdx), [MCP cancellation requirements](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation) and [Goose Agent source](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/agent.rs). Cancellation can race completed work: stopping locally does not establish rollback of an external write. Grain therefore preserves confirmed receipts, distinguishes uncertainty and refuses automatic replay. No new protocol revision or third-party agent engine is adopted here.

Graph minimal context, file summary, impact, change detection and review context identified Agent and model/peer consumers. Graph flow and test edges are incomplete; the reported zero affected flows does not certify safety. Direct source/diff inspection supplements them, tracing `AgentRunControl`, actual `CONFIRM_TTL_MS`, `PendingToolTurn::finish`, `drive_tool_turn`, `continuation_reply`, the production React confirmation/close handlers, worker outcomes and independent peer counters.

## Implementation and ownership

Only maintained harness JavaScript and documentation change. Existing guarded finite workflow instructions, confirmation observation and the stamped real application are reused. No Rust, frontend, Handy, dependencies, production deadline, credentials policy or user model settings change.

Eight controlled cases share the existing owned host/model/HTTPS peer. Finite model modes reset only while no model request is active. All prompts and objects remain disposable. The model outage sends a real HTTP 503 through Grain's configured test-model boundary only after independently checking the actual successful write result. Stop holds one real model response after that receipt and observes its socket cancellation. The ambiguity peer performs exactly one write and drops its response; an independent verification reads the actual effect while a changed-argument write retry remains blocked.

Two opt-in genuine-model cases each exercise denial plus later model failure through native or MCP tools. The selected ordinary model chooses actual searches, schema loads and actions; it sees Grain's actual refusal and unchanged verification result. For later failure, the owned relay becomes unavailable after the real approved write result. This does not alter or disable the user's proxy, infer an external provider outage, or manufacture a tool receipt. Denied writes are recorded as `wf_write:declined`, never as successful write receipts. No raw prompts, keys, confirmation tokens or personal provider objects enter reports.

## Independent cases

| ID | Required proof |
|---|---|
| `agent.workflow-denial-native` | Read succeeds; the real write Cancel handler produces no write; a new-ID/changed-argument retry is refused; separate approved verification confirms unchanged state. |
| `agent.workflow-denial-mcp` | Same host behavior, plus peer wire receipts contain only read/verify and write count remains zero. |
| `agent.workflow-expiry` | MCP pending write remains unapproved for over the real 120-second deadline; old direct/UI approval cannot dispatch or resume the model. A second pending workflow ends at an actual host restart. |
| `agent.workflow-stop-native` | Close pending write, then separately close an active model request after a completed write; stale confirmations and cancelled response cannot resume either task. |
| `agent.workflow-stop-mcp` | Same schedules, with independent peer counts: zero pending write, one completed write and zero verification after Stop/close. |
| `agent.workflow-failure-native` | Exactly read/write dispatch, actual write receipt remains displayed with an unfinished-steps notice after real model HTTP 503; no verification or write replay. |
| `agent.workflow-failure-mcp` | Same failure boundary, independently checked actual peer write and zero subsequent tool calls. |
| `agent.workflow-unknown-mcp` | One actual write, lost response, honest uncertainty, changed-argument retry refusal and independent verification of one effect. |
| `agent.live-native-interruption` | Genuine selected-model denial/unchanged verification and later model-boundary outage after native write. |
| `agent.live-mcp-interruption` | Genuine selected-model refusal/verification and later outage after MCP write, with external peer counters. |

The real expiry is intentionally tested once per batch through MCP because its Agent timer/token admission is shared with native tools; native stale/close and existing timer logic tests remain separate evidence. No shortened timer, timestamp edit or fake clock stands in for this acceptance. Real uncertainty against a personal hosted provider is unavailable/Blocked: the original procedure reserves forced response-loss timing for deterministic fixtures. No live remote uncertainty certification is claimed.

The UI unfinished notice reports that remaining steps could not finish; it is not a new structured task planner. Completed receipts are checked independently of that notice. Closed panels are not expected to retain a visible conversation or resume it after restart.

## Negative oracles and retained candidates

`skip-workflow-denial` is admitted only for an isolated native/MCP denial case and approves the disposable write instead of cancelling; the exact refusal oracle must fail. `skip-workflow-expiry` is confined to the expiry case and skips the wait; the still-pending real token assertion must fail immediately. Neither fault is normal acceptance or an opt-in live fault.

Five new runner self-tests reject fabricated denial success, wrong verification state, missing blocked retry, generic failure substituted for uncertainty, absent completed receipt, false completion and extra dispatch. The genuine correction self-test uses an actual bounded local HTTP peer: one exact host validation refusal can reach the model without creating a receipt; a second refusal, another step or a generic execution failure is rejected. Ordinary/live scenario admission remains explicit, with new live cases excluded from `all`.

Candidate runs and repairs are retained below; no candidate result alone accepts the numbered requirement.

| Candidate | Finding / disposition |
|---|---|
| `run-ojZqUK` | All eight controlled cases Pass, including 120,248 ms observed expiry and actual restart; cleanup Pass. Subsequent live-only refinements did not change the controlled schedules; final stable-source repeat `run-QWLN3l` passed all eight again. |
| `run-tD5VV2` | Genuine native denial's verification supplied an undeclared parameter. The host refused it without dispatch. Clarify zero-argument read/verification instructions; no production validation is relaxed. |
| `run-uKM1da` | Genuine denial/unchanged verification passed, then a new task's initial read supplied an undeclared parameter. The relay incorrectly turned a legitimate non-dispatched validation result into a model outage. Permit one actual safe correction within unchanged production budgets, retain the refusal separately and still require exact actual receipts/counters. Generic failures, wrong steps, a second invalid correction and blocked write replays remain fatal. |

The correction self-test must begin with an actual initial model frame before metadata/result frames; reconstructing a transcript without its initial owner is correctly refused. The test fixture is repaired rather than weakening workflow ownership. Live modes record attempted tools and non-dispatched argument refusals separately from validated receipts. A declined write is not a receipt; an invalid argument does not imply a completed action.

## Evidence and disposition

The final runner fingerprint is `03d6384c3d501ae79af3be14f579692acade03f3221eda2f4343a148fbdcb730`. All ten final reports below match it. The stamped real binary `C:\gt\debug\grain-agent-harness.exe` has SHA-256 `fd0eeb2858cf8d92ea6c45c61ad9eea809da07545154d5ab571751f6057938ab`; its recomputed native/frontend source fingerprint remains `56f83a631d409e44e7be2b62b215df2388ce5487ec2dc1d812e108852eebe232`. Its recorded build commit is `d9e3aa2892408b234fe8572f5a53f45d4311e229`. Identical native inputs were reused, not rebuilt; prior Rust test and Clippy evidence is historical, not a fresh result of this unit.

| Evidence | Result |
|---|---|
| `run-QWLN3l` | All eight controlled cases Pass with cleanup Pass; approval expiry observed at 120,248 ms and actual host restart passed. |
| `run-ixvrBF`, `run-ECI3in` | Both genuine adapter cases Pass in each fresh profile, each running denial/unchanged verification and post-write model outage. Four cases/eight actual model tasks total; cleanup Pass throughout. |
| `run-IuXYEL` | Six staged-workflow regression cases Pass; cleanup Pass. |
| `run-LC1jD4` | Six native-failure regression cases Pass; actual absolute deadline observed at 20,009 ms; cleanup Pass. |
| `run-wgBCVD` | Two smoke cases Pass; cleanup Pass. |
| `run-kARHbU`, `run-lzqKxE` | MCP transport-contract and mixed-catalog regression cases Pass; cleanup Pass. |
| `run-hmecpQ`, `run-bhZT8c` | Native/MCP skipped-denial faults produce the expected exact-refusal failures and exit 1; cleanup Pass. No acceptance credit for these failures. |
| `run-ekbZae` | Skipped-expiry fault produces the expected still-pending assertion, “Real approval deadline did not expire”, and exit 1; cleanup Pass. |
| Runner self-tests | 49 Pass, zero failures/skips; retained `interruption-self-tests.log`. Seven changed JavaScript files pass Prettier; scoped diff whitespace check passes. |

The first genuine run's fingerprint precedes only the runner self-test's initial-owner transcript repair; no runtime behavior changed between that genuine pass and the final clean repeat. The first controlled pass precedes the live-only refinements and that test repair. Full runner hashes therefore differ for these retained first runs; final acceptance includes both stable-source repeats and the ten final reports, rather than claiming identical hashes for every historical run.

Each genuine denial task dispatched only read/verification, with zero writes and a refused consumed token. Each genuine outage task dispatched exactly read/write, performed one write, displayed its actual receipt and unfinished-work notice, and made no verification call. A single actual, non-dispatched invalid-argument refusal occurred in the first native outage and final MCP outage; each was returned to the actual configured model, corrected within normal host budgets and recorded separately from validated receipts. The selected model was `ag/gemini-3.8-flash` through the user's existing local proxy. Live uncertainty remains Blocked as stated above; no external authenticated MCP acceptance is implied.

Independent inspection covered all 14 retained roots (ten final, two supporting first passes, two failed candidates): every cleanup passed; each contains only its nonsecret marker and evidence. No fixture, settings, TLS, object or WebView profile artifacts remain. The selected model key is absent from retained files. Inspection found zero owned processes and zero listeners across the 26 recorded owned ports; callback port 17124 is free. The user's proxy is outside harness ownership. This is owned-resource cleanup evidence, not a production memory-baseline certificate. Two older policy-blocked scratch roots remain separately Pending and were not removed by another route.

Keep the fixed workflow modes and failure assertions as acceptance infrastructure. Held requests, state, timers, tokens, objects and run profiles remain scoped and disposable; only bounded nonsecret marker/evidence is retained. No future-feature scaffold or production service is added. See [harness retention inventory](AGENT-TEST-HARNESS.md).

Check 48 is accepted; **49 numbered checks have passed and four remain: 9/12/14/17**. Next complete independent MCP-provider check 14, then eligible live-account checks 9/12/17 with actual provider/account/schema prerequisites. No new manual batch is required for this completed unit. Baseline acceptance still precedes SDK/native-OAuth replacement or physical retirement; whole-phase release, official initialization, retained input and production vault reconciliation gates remain separate.
