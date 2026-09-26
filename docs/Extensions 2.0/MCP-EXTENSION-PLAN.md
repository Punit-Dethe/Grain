# Native and MCP extensions: executable delivery plan

Date: 26 September 2026. Status: researched implementation plan; implementation has not begun in this task.

Evidence: [research and architecture decisions D1–D9](MCP-EXTENSION-RESEARCH.md). Reproducible external references: [source ledger](MCP-EXTENSION-SOURCES.json).

## Outcome and scope

Grain will offer native Grain extensions and MCP extensions through one product experience. A user should be able to enable extensions, connect the required accounts, request a task, let the model discover relevant tools, approve consequential actions, and receive an accurate result after the agent completes the workflow.

The first supported release includes the existing native runtime and a certified subset of curated remote MCP servers. It must handle authentication expiry, provider unavailability, schema changes, user cancellation and partial workflow completion. It must keep schema context and idle resources bounded.

The release does not depend on arbitrary local executable installation, every MCP optional feature, or all six currently listed providers being certified simultaneously. Those capabilities remain explicit later milestones. “Works reliably” means passing declared scenarios and reporting failures truthfully; it cannot promise upstream services never fail.

## Architecture commitments

1. Preserve the existing Rust agent, native extension host, OS vault and `rmcp` adapter. Refactor boundaries incrementally inside the active path.
2. Introduce one extension-instance/capability contract with native and MCP implementations. Preserve supported native actions, auth, data contributions, shortcuts and host-rendered settings. Keep Grain's visual ownership; do not restore extension-authored views or panels.
3. Separate installation, enablement, account authorization, catalog discovery, model exposure and runtime activity.
4. Let the model choose extensions and search tools. Host discovery may fetch catalogs that never enter the model context.
5. Make the host authoritative for identity, schema validation, permissions, approval and dispatch. Metadata and model output never grant permission.
6. Preserve the original agent run across approval and authorization. Continue using real tool-call IDs and actual execution results.
7. Keep write outcome certainty explicit. Reconnection, request IDs and local idempotency keys do not imply safe replay.
8. Add no always-on worker or service merely to support remote MCP. Bound caches, pending state, listeners and transport work.

This plan proposes replacing the development-only product scope in “Amendment E — Remote MCP development providers (2026-08-31).” It preserves the separate visual-ownership Amendment E dated 2026-09-08. It adds bounded per-tool discovery to Amendment D's model-selected extension approach; it does not restore the superseded mandatory pre-router. The prior `PLAN.md` remains historical/contextual guidance until each implementation change is landed and its status updated.

## Dependency order

```mermaid
flowchart LR
    P0[P0 contracts and baseline] --> P1[P1 connector and outcomes]
    P1 --> P2[P2 authentication]
    P2 --> P3[P3 resumable agent]
    P3 --> P4[P4 selective discovery]
    P4 --> P5[P5 one to five extensions]
    P5 --> P6[P6 production rollout]
```

Run a small fixture-backed vertical workflow at every phase. P5 broadens coverage; it is not the first integration test. Provider registration paperwork can begin during P0 without changing this dependency order. Do not expand the live-provider list to hide a failed earlier gate.

## P0 — Freeze contracts, support boundaries and baseline

**Purpose:** establish one meaning for extension identity, execution state and completion before rearranging modules. Evidence: D1, D6, D8, D9.

### Work

- [ ] Record current runtime flow and update the architecture decision record to reference this plan and the two revised amendments.
- [ ] Define `ExtensionInstanceId`, `ToolKey`, `AuthIdentity`, descriptor generation/digest and per-instance policy. Multiple configured accounts must not collide even if the UI initially supports one account per instance.
- [ ] Specify `ToolDescriptor`, `PreparedCall`, `ExecutionResult` and `RunContinuation` contracts. Preserve existing types where they already meet the requirement.
- [ ] Split grant status from last-known availability. Define UI states and timestamps without background health polling.
- [ ] Freeze a release matrix: native + remote HTTP; modern and claimed legacy protocol versions; supported auth modes; output types; model/provider combinations; explicit optional-feature exclusions.
- [ ] Create deterministic native and MCP test adapters that run through the production bridge and executor. Use a local protocol test server for transport tests. Do not create an alternate UI or mock Tauri visual harness.
- [ ] Capture current request counts, schema tokens, cold/warm latency, idle resource counts and memory on the actual target machine. Record environment and model identifiers.
- [ ] Create a provider readiness checklist covering app identity, redirect registration, scopes, test tenant/account and distribution constraints. Start externally gated registration work now.

### Ownership

`crates/grain-core/src/capability_agent.rs`, existing execution types, `src-tauri/src/capability.rs`, and `docs/Extensions 2.0`. Transport and vault handles stay outside the pure core types. New backend modules remain Grain-owned; do not add features to `src-tauri/src/handy/`.

### Exit gate

The same deterministic read and approved write can be represented through both adapter kinds with identical policy/outcome semantics. Existing core capability tests stay green. The support matrix and baseline contain explicit unknowns instead of assumed compatibility.

**Suggested change sets:** P0.1 contract/documentation; P0.2 shared fixture adapters and baseline harness.

## P1 — Make connection, cancellation and outcomes trustworthy

**Purpose:** know what happened to an action before introducing recovery or larger workflows. Evidence: D3, D6, D7, D8.

### Work

- [ ] Replace the universal MCP “did not run” failure mapping with typed execution certainty. Track the boundary between preparation and dispatch conservatively.
- [ ] Preserve `isError`, protocol failures, transport failures and output-conversion failures as distinct facts. A server tool error may still follow partial effects.
- [ ] Add deadlines and a cancellation token from run/UI stop through discovery and tool dispatch. Define behavior before send, during send and after response.
- [ ] Bound complete discovery by elapsed time, pages, tools and bytes. Detect repeated cursors and repeated empty pages. Return explicit incomplete catalog state.
- [ ] Keep modern request-scoped HTTP and legacy fallback. Prove resource ownership on success, timeout, cancellation and app shutdown. Use a short legacy lease only if measurements justify it.
- [ ] Preserve text formatting and structured JSON in a typed result. Report truncation. Separate rendering support from execution success; introduce bounded retained artifacts only where required by a result-size limit.
- [ ] Restrict automatic retries to operations known to be safe and within an operation budget. Respect rate limiting; do not retry uncertain writes. Make connection reconnection and business-action replay separate functions.
- [ ] Revalidate enablement/configuration generation after asynchronous setup. Disabled or removed extensions must not be resurrected by late work.

### Ownership

`src-tauri/src/grain_mcp.rs`, `action_exec.rs`, `capability.rs`, and the existing core execution result types. Avoid a second independent executor.

### Required tests

| Failure injection | Required behavior |
|---|---|
| Invalid schema/arguments before send | No dispatch; actionable validation error |
| Network unavailable before send | No false success; accurate non-dispatch only when known |
| Server writes then drops response | Unknown outcome; zero automatic write replays |
| Response arrives with unsupported content | Execution/result status retained; no claim it never ran |
| Cancel before dispatch | No action sent |
| Cancel after dispatch | UI stops waiting; remote outcome may remain unknown |
| Repeated cursor or infinite empty pages | Bounded failure, cancellation works |
| Disable during connect/discovery | No late runtime/catalog resurrection |

### Exit gate

All above fixtures pass through production code. After 100 connect/discover/cancel/dispose cycles, no owned listeners or tasks remain after completion and no steadily growing resource count is observed. Compare memory with P0 measurements; document allocator noise separately from retained handles.

**Suggested change sets:** P1.1 typed outcomes and result envelope; P1.2 deadlines/cancellation/pagination; P1.3 lifecycle and fault-injection tests.

## P2 — Make authentication recoverable and provider-aware

**Purpose:** connect and recover accounts without conflating login, connectivity or write permission. Evidence: D4 and the provider compatibility table in the research.

### Work

- [ ] Add an auth coordinator façade over native and MCP strategies. Preserve resource/audience separation and existing OS vault use.
- [ ] Define versioned credential keys by extension instance and grant identity. Migrate old provider-keyed grants safely: write new entry, validate metadata, then retire the old entry; failures must not discard the only usable grant.
- [ ] Support optional secrets for public preregistered clients, provider-required confidential integrations, CIMD where supported, and legacy DCR. Publish a Grain-owned CIMD only after its stable URL, client identity and redirect policy are settled.
- [ ] Preserve SDK PKCE/state/issuer/resource checks; add application tests for challenge handling and metadata/redirect network policy.
- [ ] Coalesce login/refresh per identity and make logout/config-change win over late completion. Avoid a global lock unnecessarily blocking unrelated providers.
- [ ] Handle access expiry, refresh rotation, revoked refresh tokens, absent refresh tokens, insufficient scope and denied consent distinctly.
- [ ] Own callback listeners through a single coordinator. Use a dynamic port only for providers supporting it; otherwise serialize fixed-port login flows.
- [ ] Expose actionable auth states to the agent and UI. Browser authorization requires an explicit user interaction; a model-generated request cannot silently start an unbounded login loop.
- [ ] Finish a live Linear restricted-read connection, disconnect and reconnect in a test account. Record the actual endpoint, transport/protocol, scopes and auth mode observed.

### Ownership

`src-tauri/src/grain_mcp.rs`, `grain_auth.rs`, existing settings Tauri commands/events and their real UI components. Regenerate bindings only if commands change. Do not merge arbitrary native and MCP tokens into one reusable credential.

### Required tests

Valid login; wrong state/issuer; duplicate/late callback; callback port occupied; cancellation; two simultaneous providers; parallel refresh; refresh without a refresh token; revoked grant; step-up scope denial; disconnect during refresh; wrong-account isolation; no secrets in logs or model messages.

### Exit gate

Applicable pinned MCP authorization conformance scenarios pass for claimed modes, with skips listed separately. Linear login → read → expiry/recovery → disconnect succeeds, and the app releases the callback listener on every path. Other providers remain “not yet certified” until their own checks pass.

**Suggested change sets:** P2.1 identity/storage migration; P2.2 coordination/recovery; P2.3 registration modes and provider certification.

## P3 — Resume the actual agent after approval or login

**Purpose:** complete workflows rather than returning only the first approved tool's receipt. Evidence: D5, D6.

### Work

- [ ] Introduce bounded `AgentRun` ownership in the existing runner. Retain messages, call IDs, offered tools, selected descriptors, remaining hop/call budgets and pending interaction.
- [ ] Change confirmation handling to resume the owning run after execution and append the real tool result. Preserve the immutable prepared-call contract and exact-call validation.
- [ ] Atomically consume approval tokens. Reject duplicate, expired, wrong-run and already-consumed decisions.
- [ ] Define denial, cancellation and changed-intent behavior. Denial resumes with a denial result; cancellation terminates the run. A new unrelated request must not accidentally approve old work.
- [ ] Resume after explicit auth completion only after account, scope, policy and schema revalidation. Changed arguments or a changed consequential target require a new approval.
- [ ] Preserve valid assistant/tool message pairing for multi-call model responses. Defer remaining calls or resolve them explicitly, following the selected model protocol.
- [ ] Track dispatch/completion in a bounded ledger. Add a minimal durable journal for consequential dispatch if needed for crash ambiguity; do not silently claim full durable workflow resumption.
- [ ] Release network resources while waiting for the user. Expire pending state using the existing bounded/TTL approach, extended to the owning run.
- [ ] Keep sequential execution initially. Add parallel independent reads only after dependency and cancellation tests pass.

### Ownership

`src-tauri/src/agent.rs`, `action_exec.rs`, `capability.rs`, `grain_llm_client.rs`, and existing confirmation UI commands/events.

### Required tests and demonstration

Read → prepare write → approval → write result → second read → final answer, through native and MCP paths. Repeat with denial, auth expiry in the middle, duplicate approval delivery, schema change while waiting, account switch, extension disable, two model calls in one response, and timeout after a write.

### Exit gate

The complete workflow returns a final answer without asking the user to restate the task. Every tool-call ID has the correct outcome message. All deterministic duplicate-delivery fixtures produce at most one dispatch of the approved write. A crashed/interrupted run never labels a possibly completed write “not run.”

**Suggested change sets:** P3.1 run continuation; P3.2 approval/auth integration; P3.3 multi-call and crash-boundary tests.

## P4 — Bound discovery, schemas and model exposure

**Purpose:** scale tool count without losing selection quality or consuming the model context. Evidence: D2, D3, D7.

### Work

- [ ] Introduce a bounded metadata catalog keyed by instance, auth identity, configuration/protocol and generation. Coalesce fetches; prevent late responses overwriting newer state.
- [ ] Honor server-declared TTL/cache scope, and invalidate on auth, configuration and catalog changes. Bound retained metadata by bytes as well as entries.
- [ ] Stop unconditional full catalog listing before every call. Revalidate the selected descriptor against a fresh valid catalog entry; refresh when stale, invalidated or challenged.
- [ ] Preserve extension-level model choice. Load a small extension's schemas directly only when they fit; return a summary/search affordance for larger catalogs.
- [ ] Add host tool search with extension filters, explicit coverage and pagination. Start with the existing lexical index after adapting metadata and authorization filters.
- [ ] Enforce model-schema token/size budgets. Pin pending-call descriptors and allow controlled eviction of unused exposed schemas between rounds.
- [ ] Preserve the per-round offered-tool snapshot; search cannot permit a same-round unoffered call. Resolve every model alias through the authoritative map.
- [ ] Compile/validate full supported input schemas at execution. Reject unsupported or over-budget constructs clearly. Disable arbitrary external schema reference resolution.
- [ ] Add provider-specific schema presentation tests without weakening the authoritative schema. Preserve falsy values and distinguish absent fields from null.
- [ ] Bound and paginate the extension directory; communicate omitted coverage instead of silently excluding installed extensions.

### Ownership

`crates/grain-core/src/capability_agent.rs`, existing capability index/evaluation modules, `src-tauri/src/capability.rs`, `grain_mcp.rs`, `grain_llm_client.rs`.

### Evaluation corpus

Use separate development and held-out sets. Include similar tool names across providers, aliases and vocabulary mismatch, no-match requests, wrong-account/private tools, schema-heavy tools, disabled providers, changed catalogs, and server-side discover/execute wrappers. Scale to 1,000 tools without injecting 1,000 schemas into the prompt.

### Exit gate

Proposed initial targets: held-out Recall@8 ≥95%; zero exposure of disallowed-account or disabled tools; 100% schema-budget compliance; all invented/unoffered-call fixtures blocked; no silent incomplete catalog. Track top-1 precision, no-match behavior, task completion and extra model turns alongside recall. Adjust limits only from evidence and record the tradeoff.

Warm calls must eliminate the unconditional `tools/list` observed in the baseline. Measure cold/warm latency and memory; a retrieval benchmark alone is not acceptance for real model performance.

**Suggested change sets:** P4.1 catalog cache; P4.2 selected schema exposure/search; P4.3 full validation/provider adapters; P4.4 held-out evaluations.

## P5 — Prove one, two, three and five extensions

**Purpose:** demonstrate interoperability and failure isolation. Evidence: D1–D9 together. Each rung includes deterministic fixtures, then live test-account runs. A fixture-only pass must never be labeled live provider certification.

| Rung | Composition | Required demonstration | Pass condition |
|---|---|---|---|
| One | Linear MCP; native conformance tested separately | Find/read items, approved disposable write, continued answer, expiry/reconnect/cancel | Complete one-provider behavior and truthful failures |
| Two | Linear + Notion MCP | Read from both; propose a write to one; approve and continue | Correct account/tool routing; failure in one preserves the other's result |
| Three | Linear + Notion + one existing native extension | Move authorized information through both adapters and produce a combined result | Identical policy/continuation behavior; no cross-adapter identity leakage |
| Five | Prior three + certified Atlassian + certified GitHub | Similar names, chained reads/writes, one denied action, one unavailable provider | Bounded schema context, no duplicate writes, accurate partial completion |

Select an existing native extension with suitable declared read/write capabilities during P0. Do not invent a production native API merely to complete this table. If an external provider's registration is blocked, continue fixture testing but leave its live certification incomplete; do not substitute a fake success. Slack and Calendar enter later through the same certification checklist.

### Scenarios at every rung

- [ ] Successful multi-step task with receipts and a final grounded answer.
- [ ] Wrong/ambiguous tool names and a request that none of the extensions can satisfy.
- [ ] One authorization expiry; user reconnects and the original run resumes.
- [ ] Approval denied or expired; no unauthorized action follows.
- [ ] One transport timeout before dispatch and one ambiguous write outcome after dispatch.
- [ ] One extension disabled or reconfigured while work is pending.
- [ ] Tool list/schema changes between discovery and approval.
- [ ] Malicious tool description/result attempting to request other credentials or bypass policy.
- [ ] Large output, structured output and an unsupported content block.
- [ ] Rate limiting, bounded retry budget, user cancellation and app shutdown.

### Exit gate

All deterministic isolation, policy and replay tests pass. For live model evaluation, choose at least two declared tool-capable model/provider combinations and repeat each selected workflow. Initial release target: ≥95% task success when prerequisites and upstream services are healthy. Publish sample size, model version, prompts, tool counts and failure categories; do not hide infrastructure failures inside the success denominator.

Five enabled extensions must not imply five idle workers, permanent auth listeners or mandatory long-lived MCP connections. Native extensions explicitly declaring supported background activity remain subject to their existing lifecycle contract.

**Suggested change sets:** one test/evidence change set per rung; fix regressions in the owning P1–P4 component before moving on.

## P6 — Turn the certified subset into a product release

**Purpose:** expose the proven system with clear status and support boundaries. Evidence: D1, D4, D8, D9.

### Work

- [ ] Present native and MCP extensions in the same management experience, with adapter-appropriate configuration.
- [ ] Display enabled state, account, granted scopes where useful, last-known availability and actionable recovery separately.
- [ ] Add bounded diagnostics keyed by run/instance/call/attempt. Record cache, latency, auth and outcome transitions without raw secrets or sensitive payloads by default.
- [ ] Publish a provider compatibility ledger with tested endpoint, protocol, auth, scopes, supported tools/result types, last verification date and known limitations.
- [ ] Add conformance and regression suites to CI, pinned by dependency/version. Keep scheduled live account checks separate from deterministic CI.
- [ ] Roll out behind one execution-path feature flag with per-provider enablement. Do not create a shadow executor capable of duplicate effects.
- [ ] Migrate existing configuration/vault entries with rollback-compatible versioning. Disable new entry points without deleting credentials if rollout is rolled back.
- [ ] Update the older extension/auth documentation to point to the implemented decisions, and mark every phase with evidence.
- [ ] Run the actual Tauri app for final manual UX review. Follow repository policy: the user owns visual approval; no browser-only harness or UI automation.

### Exit gate

The support matrix matches reality, P0–P5 evidence is attached, no critical correctness/auth/isolation defect remains, and manual real-app UX review passes. A rollback drill prevents new dispatch while preserving truthful receipts and recoverable configuration.

## Release-blocking invariants

| Area | Required invariant |
|---|---|
| Identity | Every call resolves to the intended instance and account; aliases alone grant nothing |
| Exposure | Only offered, enabled, authorized tools can reach dispatch |
| Schema | Full supported input validation runs after model output and before dispatch |
| Approval | Approval binds an immutable exact call and is consumed once |
| Auth | No token enters model context, another extension's vault identity or diagnostic output |
| Replay | Uncertain writes are never automatically replayed |
| Continuation | Approval/auth completion resumes the original run with valid call/result pairing |
| Outcome | Completed, partial, failed and unknown effects remain distinguishable |
| Resources | Bounded state and deterministic cleanup; disabled instances stay disabled |
| Compatibility | Unsupported protocol/features are visible and do not masquerade as execution failure |

## Verification commands and evidence

Baseline commands already passed during research:

```powershell
cargo test -p grain-core --locked --offline capability_
cargo test -p grain-core --locked --offline --test capability_benchmark -- --nocapture
```

Results: 36 core capability tests and four retrieval benchmark tests passed. This does not certify Tauri, OAuth, live providers or the proposed architecture.

During implementation, run the repository's existing relevant Rust, TypeScript, lint and production-build checks for changed components. Add focused integration test targets as the production adapter is made testable. Prefer the real SDK with a controlled protocol server over mocks that merely repeat implementation logic.

The [MCP conformance runner](https://github.com/modelcontextprotocol/conformance) supports a client driver and versioned suites. Pin its release in CI, adapt Grain's connector as the driver, and invoke the required core/auth/backcompat suites using the runner's documented interface. A useful command shape is `npx @modelcontextprotocol/conformance client --command "<grain-driver>" --suite auth`; the actual pinned command and spec-version flags become part of P2 evidence. Skipped/non-applicable cases are not passes.

For each phase record: commit, checks run, environment, observed results, outstanding limitations, and the gate decision. Do not mark a checkbox complete on code inspection alone when it explicitly requires a live scenario.

## Implementation sequence and sizing

Use the suggested change sets as reviewable units, not a promise that each is one day. Scope P0 first, then estimate P1–P4 from the confirmed refactor surfaces and provider setup. Authentication approvals and marketplace eligibility have external lead time. Do not give those dependencies an engineering-only deadline.

Recommended first implementation session:

1. Define the outcome contract and add the regression fixture for a server that writes and drops its response.
2. Correct the misleading MCP failure mapping while preserving existing native behavior.
3. Define the minimal continuation contract and specify the approval-resume regression cases for P3 through the real runner seam.
4. Land the P0 contracts and P1 outcome foundation as small reviewable changes; complete the remaining lifecycle/auth gates before broadening workflows.

Keep all initial work focused on correctness. Introduce per-tool discovery after the run can reliably pause, execute and continue. Introduce more providers after that complete path passes.

## Explicit later backlog

- Local stdio with trusted/pinned installation, minimal environment, owned process trees and OS-level containment.
- User-configured remote endpoints with endpoint/discovery trust policy and enterprise-network behavior.
- MRTR/elicitation, tasks, subscriptions and richer resources/media as separate capability-tested additions.
- General durable resume across app restarts, with a carefully scoped persistence and recovery design.
- Vendor-native search or programmatic tool execution only if evaluations show a benefit over portable host search.
- Slack and Google Calendar consumer onboarding after their provider integration/distribution requirements are met.

These items must use the same executor, auth identity, outcome and continuation contracts. They should not create parallel extension systems.
