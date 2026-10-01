# Extension implementation: progress, changes and user testing

**Updated:** 1 October 2026. **Implementation snapshot:** `extensions/tool-only-retirement` through the partial Block 2B audit and initial Agent harness delivery.

This is the maintained progress report beside the [execution plan](MCP-EXTENSION-PLAN.md). The plan owns the intended architecture, phase gates, research references and exact test procedures. This document explains the current implementation, records changes from that plan and keeps the user-facing test results together. It is not a second execution plan.

## The current position in plain language

Grain extensions now have one job: provide tools that the agent can call. A tool might be implemented directly as a native Grain extension, or supplied by an MCP server. Both go through Grain's checks before execution.

Much of the foundation is implemented. Old extension privileges are refused, workers have clearer start/stop ownership, inputs and results have limits, sign-in has stronger account separation, and the agent can find selected tools and continue a task after approval. Installation and account changes also have stronger save-before-activation behavior.

There is now a maintained Agent test harness in `tests/agent-harness/`. It opens a separate real Grain instance, performs repeatable native-tool and Agent approval/cancellation checks, and saves readable results. Its disposable settings, extension state, credentials namespace and WebView caches are separate from the ordinary app. It can grow into MCP/account and deeper Agent suites. The initial scripted model makes tool decisions reproducible; it does not replace live-model or real-account acceptance.

The complete product is still unfinished. We have not certified live providers, measured the real application's memory baseline, or completed the one-to-five-extension workflow ladder. **12 of 53 numbered manual checks have passed; 41 remain Pending.** Block 1's tool-only boundary and Block 2A's stop/replacement behavior passed their real-app checks and focused audits. **Block 2B close/repeat cleanup is active; all seven whole-phase acceptance gates remain open.** The separate R1 legacy-migration/upgrade gate and release security review are not implied by these smaller acceptances.

An earlier broad implementation sweep passed **447 automated tests**, with one separate live-store test ignored; focused checks for the current audit patch are recorded below. These tests exercise production logic and local protocol fixtures; they do not certify real accounts, the real OS credential vault or the complete real-model workflow.

## What has been implemented

### 1. Extensions have a much smaller permission boundary

Extensions can declare tools/functions and narrowly scoped supporting access: approved network hosts, their own storage and scoped authentication. They cannot obtain Grain's screen, OCR, selection, cursor, whole transcript, Space, prompt replacement, prompt packs or priorities, shortcuts, resident activation or other retired internal capabilities.

This is enforced at package validation and host dispatch, including attempts to use historical grants or call old APIs directly. Hiding a control in the interface is not the only protection. Mixed packages requesting a tool plus retired capabilities are refused rather than silently running with a different meaning.

**Physical-removal clarification:** the production worker interface and generated authoring API no longer expose capture/context/prompt/session services. However, `src-tauri/src/host_api.rs` still contains legacy implementation branches such as `capture.selection`, `capture.app`, `capture.screenText`, `capture.screenImage`, LLM and semantic services behind the tool-only preflight allowlist. Requests to these methods are refused before those branches run, including with historical or `All` grants. Their presence means extension access is retired, but physical backend removal is unfinished. Retired manifest fields/types also remain for compatibility, refusal and migration; that does not authorize a new extension to use them.

The agent's own context access remains a first-party responsibility. This work does not build new agent screen/OCR features. Ordinary dictation, user prompts, Snippets, Context and Agent settings must continue to work independently.

### 2. Existing installations have a preservation-oriented migration

Incompatible legacy extensions are quarantined and disabled. Obsolete grants and historical slot ownership are cleared, including parked installed copies underneath developer overrides.

Edited extension-owned prompts and custom bindings are archived as inert JSON before removal from active use. Repeated migration avoids duplicate archive entries. Existing package artifacts and user storage are preserved. The migration checkpoint is saved only after the preceding migration work succeeds, and startup still revalidates packages.

**Still needed:** broader interrupted-upgrade, cleanup, restart and rollback acceptance. Migration scaffolding is implemented; every failure boundary is not yet certified.

### 3. Authoring and management have been reduced to tools

New CLI projects expose an explicit action through `grain.actions`; their generated API no longer advertises the retired host services. Import, developer loading and signed store downloads validate the reduced contract. Store filtering excludes retired integrations, and direct installation has independent checks so bypassing the catalog does not bypass policy.

Retired recommendation controls and extension contributions have been detached from the relevant management/core surfaces. An empty store is valid when its published catalog contains no eligible tool-only packages.

**Still needed:** historical example/documentation cleanup, remaining unreachable code and catalog registrations, and real-app visual confirmation. Necessary parsing/migration tombstones are not working legacy APIs.

### 4. Native workers have clearer ownership and cleanup

A cold call starts the needed native worker; a warm worker can be reused and is later reaped when idle. Unique generations distinguish each supervisor and worker lifetime. Late callbacks, socket closure, idle cleanup or memory-observer results belonging to an old worker cannot legitimately destroy its replacement.

Unload, disable and replacement invalidate queued or pending work. Cleanup rejects outstanding requests, removes listeners/callbacks and retires owned workers and resources. Calls are serialized per current worker with bounded admission. Changed source bytes cannot silently keep executing through an obsolete warm worker.

Native calls have a single 20-second operation budget covering validation, wake and reply, with a shorter readiness ceiling. Blocking filesystem or vault operations can still delay some cleanup; no hard storage deadline is certified.

**Still needed:** real-app cancellation/recovery evidence and measured memory/handle baselines. Enabling/listing an extension alone does not start its runtime; an ordinary recently used native worker can remain warm until idle cleanup.

### 5. Both adapter paths check the exact call

The host checks the enabled instance, current account/configuration identity, selected tool definition and arguments before execution. A confirmation prepared for an older account, package, developer project or schema is refused after that owner changes, even when the visible tool name stays the same.

Native parameter contracts are validated. MCP input validation uses an offline schema evaluator with bounded complexity, nested constraints and a documented supported profile. It does not fetch arbitrary remote schema references. Unsupported individual schemas can be excluded while supported tools remain usable; malformed or incomplete provider discovery still fails.

**Current policy:** native and MCP calls conservatively require confirmation, including reads. Automatic reads need a host-reviewed policy and restricted provider scopes; an extension's own claim that a tool is harmless is insufficient.

### 6. Results describe what actually happened

Execution distinguishes: nothing was sent, success, a tool-reported error, an unknown outcome after sending, and a received result that cannot be used. If the response disappears after a call was sent, Grain must not pretend the operation never happened or repeat it automatically.

Text/JSON previews retain useful formatting with explicit truncation or omission. Raw responses, catalog discovery, argument structures, decoded results and retained model previews have separate limits. Private error details are redacted rather than copied blindly into the model-facing result.

Actual SDK HTTP and legacy SSE fixtures cover lost responses, bounded discovery, pagination loops and cleanup. These are local protocol tests, not a universal transport/provider compatibility claim.

### 7. Sign-in belongs to Grain and credentials stay out of model context

Grain owns browser authorization, callback listeners, cancellation and credential storage. MCP uses the pinned official Rust SDK with additional bounded host checks. Public-client and confidential-client configuration are supported where the provider's registration permits them. Cancellation and the whole-flow timeout release the owned callback listener; an old browser completion must not reactivate a cancelled account.

Refresh ownership is scoped so one slow provider's network request does not impose a global provider-network lock. Configuration changes, logout and account replacement invalidate late work. The OS vault itself may serialize access.

**Still needed:** live provider/registration certification and complete verified issuer/resource/client/scope mapping. Authentication is not yet seamless across every MCP. Some registrations require a fixed callback port and cannot sign in concurrently on that port.

### 8. Native accounts are separated and validated more carefully

Native credentials are bound to the declared client, endpoints, scopes and allowed API hosts. Old unbound credentials require reconnect. Installed and development copies have separate account sessions; reloading the same developer directory preserves its session, while changing the project owner requires the appropriate separate grant.

Exchange/refresh responses are checked before publication: supported token type, credential grammar, declared scope coverage, expiry and bounded encoded storage. An expired credential without a usable refresh token reports that reconnect is needed. Tokens are not added to tool descriptions, model context or ordinary diagnostics.

A new native login stores its candidate credential first, then commits the selected account pointer. If pointer persistence fails, the prior selected account is retained and only the unreachable candidate is retired. Successful publication cleans up prior credentials afterward.

**Still needed:** real native connection fixtures/UI prerequisites, provider-specific scope equivalence, vault crash/orphan reconciliation and live account tests. Native API credentials and MCP grants remain separate. Preserving an old local token cannot guarantee its issuer has kept it valid.

### 9. The agent loads selected tools instead of every schema

The initial agent context contains a bounded directory and host-owned search/load tools. Search returns small metadata pages; loading requests exact tool IDs. A later load can add another tool from the same unchanged extension without forgetting the first. Unrelated schemas are not automatically offered.

The directory reports incomplete coverage beyond 100 extensions. Each explicit load selects one to eight tool IDs, and all offered definitions share a 32 KiB serialized budget. Fabricated tools and tools loaded too late for the current model response cannot bypass the offered-tool checks.

**Still needed:** a bounded metadata cache, more precise freshness rules and measured search relevance. Current selected discovery is fresh on each search/load, and unrelated catalog changes can conservatively invalidate a selection. This is selective loading, not a finished caching system.

### 10. Approval can continue the original task

The agent retains the original call IDs, messages, selected definitions and remaining budgets while approval is pending. Approving consumes the exact prepared call once, appends its actual result to the original call and resumes the remaining task. The user should not need to dictate the whole request again.

Calls after the first withheld call in a batch receive honest unexecuted results. Duplicate call IDs/approval tokens are refused. Declining or failing a tool blocks another attempt at that same tool within the task, while an independent verification tool may still run. Completed receipts survive a later model failure.

Pending approval expires after two minutes and releases retained state. Close, Stop, replacement and cancellation own the run's cleanup. Transports are not kept open while waiting for the user. Restart ends pending runs; there is no durable restart-resume promise.

**Still needed:** explicit continuation after an authentication interruption and combined real-model/native/MCP read → approved write → verification evidence. The existing continuation tests and adapter tests are separate evidence, not that complete live demonstration.

### 11. Saving now precedes exposing important activation changes

Registry loading refuses malformed, unreadable, oversized or future-version state while preserving the existing file. It does not silently overwrite damaged state with an empty installation list. Saves use bounded streamed JSON and an owned temporary file before replacement.

Enablement, permission grant-and-enable, enabled package replacement and developer publication/restoration keep tentative changes hidden until the save succeeds. Failure restores the edited owner rather than exposing an unsaved enabled extension. Same-version package replacements use content-owned artifacts, and pending store downloads cannot undo a later disable/removal choice.

Explicit disablement has different failure behavior: it still invalidates the live instance even if saving fails. That failed save must be reported; it cannot alone guarantee the disabled state survives restart. JSON registry state and OS-vault state do not form one complete crash-proof transaction.

**Still needed:** remaining refusal/cleanup mutation recovery, artifact/orphan reconciliation and real-app damaged-state recovery acceptance. There is no automatic repair UI, power-loss certification or multiple-process writer guarantee.

## Where execution changed from the plan

The initial broad extension concept was deliberately replaced by the user's tool-only scope correction. That is an approved product change. Separately, some implementation choices differ from the target, and some promised pieces simply remain unfinished. The table keeps those cases distinct.

| Change or difference | Classification | Current behavior and reason | Follow-up |
|---|---|---|---|
| Broad internal extension features removed | User-directed scope replacement | Screen/OCR/selection, Space, prompts, OS hooks and other retired integration features are refused. Context remains agent-owned. | Keep retirement enforced; do not put these capabilities back on the extension backlog. |
| R1–R4 work interleaved instead of closing each whole phase in sequence | Delivery-order deviation | Worker identity, account ownership, persistence and continuation depend on one another. Deterministic work continued while real-app acceptance was unavailable. | Close the outstanding gates explicitly. Do not use implemented slices to declare R1 finished or expand uncertified providers. |
| Every tool currently needs confirmation | Conservative interim policy | Host-reviewed automatic-read classification is unfinished. Provider annotations cannot self-authorize it. | Implement and certify the reviewed read policy before promising fewer approvals. |
| Fresh selected discovery; no descriptor cache | Incomplete planned optimization | It avoids retaining stale runtimes/descriptors but repeats selected discovery. Whole-catalog identity can invalidate a selection after an unrelated change. | Add bounded generation-aware metadata reuse and finer selected-tool freshness. |
| Approval resumes a task; authentication interruption does not yet have equivalent continuation | Unfinished planned capability | Original task/call identity survives approval. Explicit agent auth interruption handling remains open. | Add bounded auth continuation with the same account/replay guards. |
| Provider and schema support is a documented subset | Compatibility limitation | Curated remote HTTP and tested legacy paths, provider-supported registration modes and bounded offline schemas are the current scope. | Publish a dated tested matrix; complete needed registration/identity work. Do not claim universal MCP support. |
| Targeted JSON/vault publication instead of a new database or journal | Implementation choice with remaining recovery work | Important activation/account changes commit before exposure, using existing low-overhead storage. Cross-store crash recovery is incomplete. | Reconcile orphan credentials/artifacts and finish interrupted-mutation evidence without overstating durability. |
| Retired compatibility markers and historical examples remain | Unfinished planned cleanup | Some markers preserve migration/refusal explanations and user data; some unreachable implementations/examples still need review. | Remove truly dead code/examples after ownership checks, retaining only necessary migration tombstones. |
| Native workers retained browser network access | Focused audit bug and fix awaiting live retest | The Rust host refused ungranted `grain.net.fetch`, but Blob Web Workers could call browser networking directly. A supervisor-page CSP now narrows their browser connections; the supervisor's Tauri capability is limited to listen/unlisten/emit events. | Retest Tool Smoke in the real WebView, exercise an unbrokered network probe, and keep residual IPC/local-socket risk in the security review. |

Physical removal of obsolete backend dispatch implementations and exclusive wiring/helpers remains a later cleanup unit, as requested by the user. The current Block 1 gate is that retired calls remain unreachable, with negative access evidence; first-party context and the metadata needed for refusal/migration must survive eventual cleanup.

Native currently uses the existing embedded tool execution path. Arbitrary native executables, unrestricted local MCP commands and OS sandbox certification are outside the initial release; they are not completed capabilities. Agent context expansion is also outside this implementation effort.

## Remaining phase gates

| Phase | What the unfinished work means in ordinary language |
|---|---|
| R0 — Contract and baseline | Finish the versioned contract/inventory and measure resource/latency baselines. |
| R1 — Retirement and migration | Finish interrupted cleanup/refusal recovery and confirm that upgrades preserve user data without restoring old access. |
| R2 — Execution | Finish reviewed read policy and certify cancellation, outcomes and cleanup through both real adapter paths. |
| R3 — Authentication | Finish provider identity/grant verification, vault recovery and live native/MCP login/refresh/disconnect certification. |
| R4 — Agent workflow | Add metadata caching/freshness and auth continuation; measure retrieval and prove the complete real-model task flow. |
| R5 — Multiple extensions | Certify one native, one MCP, then two, three and five mixed extensions, including failure/partial-completion cases. |
| R6 — Release | Finish cleanup/examples/bindings, CI and compatibility reporting, upgrade acceptance and the user's real-app review. |

Some of this can continue with deterministic tests without the user. Real account compatibility, application appearance, actual model selection, upgrade behavior and measured real-app resource usage need their corresponding live acceptance evidence.

## Sequential acceptance track

**Agent harness, 1 October:** the user authorized real-application automation and clarified that this is an Agent harness, initially exercising extension/tool lifecycle. The [architecture and delivery record](AGENT-TEST-HARNESS.md) and [runner instructions](../../tests/agent-harness/README.md) cover the implemented isolated real Tauri/WebView2 adapter, versioned native fixture, scripted HTTP model, production developer-socket reload, build identity, cleanup and evidence reports. `AGENTS.md` now permits this acceptance automation. Runtime scenarios and production-logic suites remain distinct evidence classes. Check 37 remains Pending because its recording/pill portion is not automated; the original 12 manual Pass results are preserved.

**Open findings discovered by repetition:** real unload/reload/setup runs failed while persisting `extensions.json` (`run-JXIoXD`, `run-0ucmuu`), and native Escape sometimes failed to destroy the panel after Windows accepted the key pair (`run-uCkVSM`). Focused/full reruns have also passed. The registry failure's cause is not established. Earlier Escape findings need attribution after the test-profile shortcut correction; passing reruns alone do not establish their cause. The harness preserves failed reports, stops subsequent cases and now includes underlying developer registry/approval errors and shortcut-manager diagnostics. Keep these findings open for the focused native-block investigation; do not add retries or change production timing merely to obtain a green report.

**Harness corrections:** the real first-run profile does not initialize shortcuts through the ordinary settings page, and clearing its binding map makes the normal initializer fall back to default accelerators. Test startup now owns the existing production HandyKeys manager with no ordinary accelerators registered, retains explicit blank binding entries and marks initialization complete; the normal command is checked for idempotence. Only Agent's transient Escape is exercised. Follow up is opened before typed approval; its production React Enter handler is activated directly because CDP key delivery during native resize was intermittent (`run-jrIG7K`). These changes are confined to test setup/driving and do not fix or certify general production shortcut behavior. A separate Windows DLL lock prevented compiling another configuration while the runtime was open; build/backend compilation must precede real-app runs in the shared debug target.

**Current focus: Block 2B native close/repeat cleanup.** This track organizes the existing execution plan into reviewable product behaviors; it does not replace its R0–R6 requirements. Complete each block or explicitly named sub-block's real-app checks, fix the failures, perform a focused source/security/lifecycle audit, rerun relevant automated checks and repeat affected manual checks before calling it accepted. Do not spread implementation across later units merely because their unit tests already pass. A missing external account or provider is a recorded prerequisite, not a pass.

| Block | Behavior to finish | Real-app acceptance to collect | Focused audit after user feedback | State |
|---|---|---|---|---|
| 1. Tool-only boundary and one native tool | Ordinary Grain features survive; retired extension capabilities are absent/unreachable; a newly authored tool loads and actually runs through Agent. | Checks 1, 3, 4, 7, 16; check 2 if a legacy installation exists. Confirm the real greeting tool result, not a model-written greeting. | Verify declaration, host preflight, worker network boundary, Agent offering and core-feature independence. Catalog dead handlers for later physical removal; fix and retest audit findings. | **Accepted 30 September: five checks and audit retest Pass** |
| 2. Native execution, consent and persistence | The same native tool survives ordinary reload/restart, stops safely and cannot execute an old approval against a new owner. | Checks 5, 6, 18–19, 21, 24–30, 32–40, 49, 52–53 as applicable; use disposable packages for installation cases. | Audit worker/resource ownership, timeout/result certainty, package publication, approval identity, registry recovery and teardown. Fix/retest. | **2A accepted; 2B active** |
| 3. One MCP transport and tool contract | A controlled MCP provider is discovered, loaded, called and disconnected with honest results, independently of OAuth. | Checks 8, 17, 20, 23 and 31 with a safe read and a provider exposing the relevant shapes; mark unavailable scenarios Blocked rather than guessing. | Audit actual SDK transport, pagination/schema limits, dispatch/outcome classification and cleanup against deterministic fixtures and the selected provider. Fix/retest. | Queued |
| 4. Authentication for one certified provider | One real MCP account connects, survives restart/expiry and disconnects; stale sessions cannot act for a replacement. Then certify native account behavior with a suitable fixture. | MCP checks 9–15, then native checks 41–45 and 50–51 when the native connection fixture/UI exists. Record registration mode, scopes and provider identity. | Audit callback, issuer/resource/client/scope mapping, vault publication/refresh/revocation, cancellation and orphan cleanup; fix/retest. A provider-specific compatibility matrix is required. | Queued |
| 5. Agent tool selection and full task | Agent finds only relevant schemas, confirms a disposable write once, resumes the original task and verifies the result. | Checks 22, 46–48, plus stale/changed-definition outcomes from 19 and 23; use real model tool calls and test objects. | Audit selection freshness/cache, exact call IDs, account/schema revalidation, approval/auth continuation, denial and receipt preservation; fix/retest. | Queued |
| 6. Two complete extensions together | One native and one MCP tool both work in the same task, including name collisions, disablement and partial failure. | Run the R5 one-native, one-MCP and two-extension ladder with recorded real calls, account routing and cleanup. | Audit cross-extension data boundaries, isolation, lifetime and result attribution; fix/retest. | Queued |
| 7. Scale and release | Three- and five-extension workflows, compatibility, measured resources, upgrade and release behavior are documented and accepted. | Complete the remaining R5/R6 gates, including real Tauri visual review and any still-open numbered checks. | Final focused integration/security review, compatibility/CI matrix and resource measurements. | Queued |

The order deliberately puts **one working native tool before broad authentication work**. MCP transport is checked with a controlled safe provider before auth is called complete; if that provider itself requires login, its credential setup is a prerequisite, while the auth module remains unaccepted until Block 4. One certified provider is an honest release milestone; it is not a claim that all MCP servers work. The R5 two-extension demonstration is the first cross-extension acceptance milestone, with three/five tested afterward.

Block 2 is intentionally divided before its whole acceptance: **2A stop/replacement** (checks 6, 25, 27), then a focused audit; **2B repeated/close cleanup** (5, 29, 30, 35, 37), then an audit; **2C deadlines, results and input** (21, 24, 26, 32–34), then an audit; and **2D consent, installation and persistence** (18–19, 28, 36, 38–40, 49, 52–53), then an audit. Missing prerequisites are recorded rather than forcing test mutations into a real profile. Other checks stay in their numbered ledger and phase gates. Each audit unit can span several small user batches, but one unit must be accepted before the next starts.

### Current small-batch handoff

**Step 1A — observed, partial check 16.** The user's real-app screenshot shows **Load unpacked** and **Hosted MCP validation** with Linear, Notion, Atlassian, GitHub, Slack and Google Calendar rows. It does not show Recommendation Lab/Core 6/Stress 24 in the visible area. A red **“MCP protocol negotiation failed. Check the account and provider availability.”** banner is visible above Linear; its cause is unknown and is recorded for the later focused MCP block. No provider login, Test call, native tool execution or full-store review was performed. Check 16 therefore remains Pending rather than Pass.

**Step 1B — check 3 Pass.** The user's 30 September real-app screenshot shows a developer row for `com.example.tool-smoke` pointing to the prepared disposable folder; the user reports enabling it and then confirms the tool works in Agent. The generated fixture's inspected manifest declares the `Say hello` action and zero permissions/activation. No unexpected permission request was reported. The Developer list shows the extension ID rather than a separate tool button. The MCP negotiation banner is a separate unresolved observation; no provider action was requested.

**Step 1C — check 4 Pass.** The user reports that Agent's Tool Smoke tool works and ordinary dictation works in the real app, then explicitly confirms the greeting also worked both immediately and after about three minutes without tool activity. This completes the user-facing cold/warm/idle greeting procedure. Worker-reap log and measured resource evidence remain independent Block 2/phase-gate work. Do not connect, test, disable or reconfigure hosted MCP providers to make this native test pass.

**Step 1D — checks 1, 16 and 7 Pass.** The user reports that the ordinary settings/core-feature and restart check worked, old extension controls and old extensions are no longer visible, and the invalid retired-capability package was rejected. The exact rejection text and any change to the pre-existing MCP banner were not provided; neither is needed to claim provider compatibility. The original native greeting had already worked cold, warm and after idle. No hosted-provider action was required.

**Step 1E — focused audit completed for Block 1.** Source review confirmed package validation rejects retired declarations and host preflight refuses retired RPC methods even with historical grants. It also found that extension Web Workers retained standard browser `fetch`/XHR/WebSocket access because the hidden host had no Content Security Policy. The hidden `extension-host.html` now restricts browser connections to Tauri supervisor IPC and Grain's local authenticated tool socket; extension HTTP requests are intended to use the exact-host `grain.net.fetch` broker. The hidden host's Tauri capability was narrowed from `core:default` to only event listen/unlisten/emit. [MDN's worker guide](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Using_web_workers) describes worker networking and Blob-worker CSP inheritance; [MDN `connect-src`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src) documents the network restriction, and [Tauri's CSP guide](https://tauri.app/security/csp/) documents its IPC exception. The production build, focused worker/supervisor tests and extension contract/host tests pass. The user then observed direct browser fetch blocked while the native tool and dictation still worked. IPC/local-socket exposure remains a residual boundary for release security review, not a claim of complete process sandboxing.

**Step 1F — Pass, 30 September.** The user supplied actual Agent tool results: Tool Smoke returned `Hello from this tool.` and Network Boundary Probe returned `PASS: unbrokered browser fetch was blocked.` The user also reports normal dictation is working. The probe directly requested `http://localhost:1420/` without a network permission; that live dev-server endpoint independently answered HTTP 200 during the handoff. The disposable probe's CLI doctor reported zero findings. These post-patch observations close Block 1's focused audit. Unloading the probe was requested but not reported; it is a cleanup step at the start of Block 2. No new numbered check was added or marked Pass, so the ledger remains 5 Pass / 48 Pending. This does not test check 2's legacy-profile migration, all possible browser/IPC escape routes, or a live MCP provider.

**Step 2A — Block 2A user batch, checks 6, 25 and 27.** Unload Network Boundary Probe if it is still listed. Load and enable the CLI-validated disposable `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a` through Developer → Load unpacked. It declares no permissions and has a fast `Say lifecycle hello` tool and a fifteen-second `Say slow lifecycle hello` tool; both request confirmation. (1) For check 6, ask Agent to use the slow tool, approve it, then unload Lifecycle Smoke *before* it finishes. This first call starts cold. The old call must not show `Slow lifecycle hello completed.` after unloading; reload the same folder and confirm a new fast call returns `Lifecycle hello completed.` (2) For check 25, approve another slow call, disable it while running, immediately re-enable it, and make a fresh fast call. Expect no late success from the old call and one new `Lifecycle hello completed.` result. Repeat after a reload/cold start if the UI permits; report whether that subcase was reached. (3) For check 27, start a fast call but leave its confirmation unanswered; load the same folder again without editing it. The old confirmation must disappear or be refused if approved. A fresh confirmed call must return one `Lifecycle hello completed.` result. If the UI prevents navigating to Developer while a call or confirmation is open, report that exact obstruction as Blocked for that part; do not infer a result. If a slow call finishes before the disable/unload click, repeat that attempt. Report the three numbered outcomes together, with any exact error. No provider login or source editing is needed. After these three, audit and repair Block 2A before starting 2B.

**Step 2A — Pass and audited, 30 September.** The user reported all three sequences worked. The supplied results include an uncertain outcome for the interrupted slow call (with no automatic retry), fresh `Lifecycle hello completed.` tool results after replacement, and a refusal of the old confirmation after reload. The exact cold-start repeat for check 25 was not separately narrated, so this is recorded as the user's aggregate Pass rather than an independently timed trace. Source review followed the actual unload/disable commands, token-scoped worker removal, pending-reply drainage, registry generation fingerprint and Agent approval resume. Focused checks passed: **258 core unit + 4 integration, 48 native-host, 4 action-executor and 15 Agent tests**. A visible encoding defect in the stale-approval refusal (`â€”`) was corrected to readable punctuation; that copy-only correction does not change approval or dispatch behavior. Native call interruption still cannot prove an already-started handler had no effects. Block 2A is accepted for its tested stop/replacement behavior; release-wide lifecycle/resource certification remains open.

**Step 2B — completed user batch, check 29 and the pending-approval part of check 30.** Keep the disposable Lifecycle Smoke extension enabled; its slow tool waits fifteen seconds. (1) Ask Agent to use `Say slow lifecycle hello`, approve, then close Agent with its **×** while it is running. Reopen Agent and make a fresh `Say lifecycle hello` call; expect no late slow success, no replay and one fresh fast result. (2) Repeat the slow-call sequence, but close Agent with **Escape** while normal dictation is not recording. Reopen and verify the fast call again. (3) Repeat once more, answering the slow tool's confirmation by typing a clear `yes` instead of clicking Approve; then close Agent while it runs and verify a fresh fast call after reopening. These three modes together complete check 29 if they pass. (4) Leave a fast greeting confirmation pending, close Agent and reopen it; the old approval must be gone, and a new fast request must work. This starts check 30; its separate slow-model-response case remains for a later batch. If a call finishes before you close Agent, repeat that attempt. Report the four observations together with any exact error. No source edit or provider account is needed.

**Terminal retest before Step 2B — user-reported Pass, 1 October.** The user reproduced a `0xc000013a` exit by summoning Agent while the PowerShell terminal running Grain was focused. Agent's invisible selected-text capture synthesized Ctrl+C into that terminal, which Windows delivers as a console interrupt. The Windows capture path now skips synthetic copy when a recognized standalone terminal host is in front, leaving Agent's dictation, screen and other context paths available. The user reports trying both the PowerShell shortcut and a normal text-selection surface after the fix, with expected behavior. Embedded terminals inside editors are not yet classified and need separate validation. Windows `cargo check --lib`, the focused terminal guard test and all 16 scoped Agent tests passed; the Tauri tests used the repository's Common Controls manifest runner. This retest does not itself change a numbered manual check.

**Step 2B — observed, check 29 Pass; check 30 partial.** The user reports all four prior close/reopen modes behaved as expected: closing an executing slow Lifecycle Smoke call with ×, Escape, and after conversational `yes` stopped the old result while a fresh fast call worked; closing with a confirmation still pending removed the old approval and a new request worked. The report is aggregate rather than individual timestamps or logs. Check 29 is Pass. Check 30 remains Pending until its separate slow-model-response/replacement case is observed. A focused source review traced X through window destruction to `clear_pending_action`, Escape through `global_close`, and both through `AgentRunControl` cancellation and token discard; all 16 scoped Agent tests pass. This is a partial Block 2B audit, not acceptance of the whole block.

**Step 2B — completed user batch, checks 30, 5 and 35.** (1) For the remaining part of check 30, ask Agent a harmless question that makes it show its loading state; close it immediately while the model is still replying. Reopen at once and ask a *different* harmless question. The old answer or approval must not appear in the new session, and the new question must work. If the first answer finishes before closing, repeat that attempt. A brief “already running” refusal while cleanup finishes is acceptable; retry after it clears. (2) For check 5, use the disposable Tool Smoke folder: unload, reload, approve if prompted and call its greeting **ten times**, recording whether every cycle produces exactly one fresh `Hello from this tool.` and no old error breaks the replacement. (3) For check 35, call a fast native greeting, leave tools idle for more than **2 minutes 30 seconds**, then call it again. After another fast call, wait about **1 minute 50 seconds** and start the fifteen-second Lifecycle Smoke call, approving promptly. It should finish; an idle cleanup must not kill an executing call. Disable/re-enable or reload once afterward and verify a fresh fast call still works. Report these three numbered results together with exact errors, if any. Do not count check 35 Pass merely from the earlier three-minute Tool Smoke repeat: it also requires the near-boundary/in-flight and replacement cases.

**Step 2B — observed, checks 30, 5 and 35 Pass.** The user explicitly confirms all three full procedures worked, including ten unload/reload-and-greeting cycles and both timed idle cases. No visible error was reported. The report is aggregate, without per-cycle timestamps or worker/RAM measurements. Source review found idle removal rechecks the current token, activity time and pending calls under the queue-admission lock; cancellation/reload removes only the owned generation and drains its pending replies. All 48 `extension_host::tests::` tests pass, including idle selection, replacement, duplicate-socket and ten-generation cases. This is a focused partial Block 2B audit; its final shared-socket/reload check 37 remains.

**Step 2B — final small batch, check 37.** With Lifecycle Smoke enabled, run `C:\t\debug\grain-ext.exe dev` from `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a` in a separate PowerShell, after putting `C:\Projects\Grain\grain\node_modules\.bin` on that shell's `PATH`. The disposable project builds and `grain-ext doctor` reports zero findings. A byte-identical backup of `src/main.ts` is at `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-main-baseline-20261001.ts`. (1) Warm the fast greeting, then append a harmless new comment to `src/main.ts`; the watcher should rebuild/reload, and one fresh fast call should work. Repeat once. (2) Start and approve the fifteen-second slow greeting, append another new comment before it finishes, and wait. The old call must not claim success after reload; one newly requested fast call must work. (3) Disable Lifecycle Smoke, append a further new comment, and confirm it stays disabled until you explicitly enable it; then call the fast greeting. (4) Check ordinary dictation and a pill/Agent action still work after those reloads, and stop the watcher. Report these four observations together. Restore `src/main.ts` from the backup after the test; the comment edits do not alter handler behavior. Check 37 requires this developer hot-reload path, not another unload/reselect cycle.

For each step, ask for **Pass / Fail / Blocked**, the visible result and any exact error. Do not require the user to run the whole 53-check list at once. Update the existing numbered ledger and the active block only after its full numbered procedure has evidence; if a failure appears, stop this block's acceptance, reproduce/audit/fix it and retest before moving to the next block.

## User testing: seven groups, all 53 checks

The numbered checks below are the same checks in the execution plan; **1–8 have not been renumbered**. This is a short result ledger, not a replacement for the plan's [full procedures and disposable Tool Smoke setup](MCP-EXTENSION-PLAN.md#combined-real-app-test-checklist-retirement-lifecycle-and-authentication-phases).

**Status vocabulary:** Pending = full numbered procedure not yet verified; Pass = user observed the expected real result; Fail = attempted and wrong; Blocked = an actual missing prerequisite prevented the check. Twelve entries are now Pass and 41 remain Pending. Local automated coverage alone does not turn a manual row into Pass.

Run the real app from the repository root:

```powershell
cd C:\Projects\Grain\grain
bun run dev:asr
```

Use existing ASR configuration and a tool-capable Agent model. Tool Smoke requires no external account; its build/load instructions are in the plan. A model merely writing a greeting does not prove the tool ran. MCP checks need an eligible configured provider; native account checks need a separate authenticated disposable native fixture and supported registration, which may not yet be exposed by the reduced UI.

Use disposable objects/packages for test writes. Do not damage the normal registry, change its permissions, edit vault contents or expose credentials to force failures. Deterministic fixtures cover those schedules. Where a catalog, provider, registration, isolated profile or observable timing is unavailable, record the specific prerequisite as Blocked.

### A. Everyday behavior and removal of old privileges — 6 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 1 | Dictation, Snippets, Context and Agent still work; core settings and user prompts survive restart. | Pass |
| 2 | Old privileged extensions stay disabled across restart; archived edited prompts/bindings survive without duplication. | Pending |
| 3 | Load Tool Smoke with tool-only consent; no screen, OCR, prompt or transcript permissions requested. | Pass |
| 7 | A disposable package requesting retired permissions/startup activation is explicitly refused. | Pass |
| 16 | Store/developer controls offer tool-only capabilities; retired recommendation/lab/integration controls are absent. | Pass |
| 40 | Cancel consent leaves the tool off; one fresh Allow enables it and a real greeting works, with usable controls. | Pending |

### B. Native calls, stopping and recovery — 12 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 4 | The actual greeting works cold, warm and again after idle worker cleanup. | Pass |
| 5 | Ten unload/reload cycles work without duplicate replies or an old error killing the replacement. | Pass |
| 6 | Unload during a slow greeting stops/invalidates it; reload restores normal calls. | Pass |
| 24 | A 25-second greeting times out, does not replay or disable the tool, and a normal call recovers afterward. | Pending |
| 25 | Disable/reload during execution invalidates the old call without destroying the new worker. | Pass |
| 26 | Rebuild a warm tool without Reload; obsolete source is refused and a fresh request uses the changed source. | Pending |
| 27 | Even an unchanged reload invalidates an earlier pending confirmation. | Pass |
| 29 | Close Agent or use Escape during a native call; no late result/replay and a fresh session works. | Pass |
| 30 | Close with approval/model response pending; no old reply or approval appears in the replacement session. | Pass |
| 32 | Measure the native absolute timeout from invocation/approval; expect about 20 seconds under normal local I/O. | Pending |
| 35 | Idle cleanup and requests near its boundary preserve an executing/current replacement worker. | Pass |
| 37 | Repeated reloads preserve socket recovery, disabled state, recording and ordinary pill actions. | Pending |

### C. Installations, permission reviews and saving — 8 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 18 | Repeated store close/reopen does not resurrect old requests; offline cached browsing cannot install. | Pending |
| 28 | Installed → development override → installed restoration preserves the installed owner and rejects old approvals. | Pending |
| 36 | A pending store download cannot reinstall/re-enable something subsequently removed or disabled. | Pending |
| 38 | A permission review becomes invalid when its package/declaration changes; a fresh sheet matches the new tool. | Pending |
| 39 | Reimporting changed bytes with the same id/version uses the new package after restart and override restoration. | Pending |
| 49 | Ordinary restart preserves registry state; damaged-state refusal is tested only with an already isolated profile. | Pending |
| 52 | Consent cancellation, enablement and disablement persist after ordinary successful saves/restarts. | Pending |
| 53 | Developer folders A/B and the restored installation retain the correct owner, output and account after publication. | Pending |

### D. Inputs, outputs and truthful failures — 7 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 17 | A supported nested MCP input produces a real harmless read result. | Pending |
| 20 | Ordinary and safely large MCP reads complete or refuse within bounds; a fresh ordinary read recovers. | Pending |
| 21 | Lost native replies, tool errors and malformed envelopes are classified honestly; private error markers stay hidden. | Pending |
| 22 | Native declared parameters preserve types/optional values; a changed contract invalidates an old approval. | Pending |
| 23 | Supported tools remain usable in a mixed MCP schema catalog; excluded schemas are not offered or executed. | Pending |
| 33 | An oversized decoded native result is unusable rather than successful; a normal result still works afterward. | Pending |
| 34 | An oversized raw native message rejects/cleans up correctly, with no blind replay or shared-listener regression. | Pending |

### E. MCP connection and accounts — 9 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 8 | Test/discover a configured MCP, perform a real read, then disable it and verify refusal. | Pending |
| 9 | Denied or cancelled login finishes cleanly; the callback listener becomes reusable. | Pending |
| 10 | Completing an old browser login after cancellation cannot resurrect the account; fresh login works. | Pending |
| 11 | Supported public/confidential registration works; changing client configuration invalidates the old grant. | Pending |
| 12 | Real login survives restart and refresh after documented expiry; a stored-credentials badge alone is insufficient. | Pending |
| 13 | An MCP approval from an old account/enablement/configuration cannot run under the replacement. | Pending |
| 14 | Supported providers operate independently; fixed-port login conflicts are reported, not treated as success. | Pending |
| 15 | Disable/provider-mode shutdown during MCP work cleans up and allows a fresh call after re-enable. | Pending |
| 31 | Close Agent during a slow MCP read or pending approval; account remains intact and a fresh read works. | Pending |

### F. Native sign-in and account isolation — 7 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 41 | Unbound or declaration-changed native credentials require reconnect; a supported fresh grant works across restart. | Pending |
| 42 | Reload/disable/disconnect during native sign-in prevents old callbacks selecting an account. | Pending |
| 43 | Switching native account A → B refuses A's pending approval; a new real read identifies B. | Pending |
| 44 | Logout defeats an old refresh; unrelated native providers are not blocked by its network request. | Pending |
| 45 | Installed/developer copies have separate accounts; same-directory reload preserves the appropriate session. | Pending |
| 50 | Cancelled/failed account switching preserves the prior selected account; a successful switch selects the new one. | Pending |
| 51 | Partial consent is refused; expiry without refresh requires reconnect, while supported refresh recovers. | Pending |

### G. Tool selection and complete agent tasks — 4 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 19 | A native confirmation becomes invalid after disable/re-enable or source replacement; a fresh call works. | Pending |
| 46 | Search/load offers only selected schemas; adding another tool preserves earlier selections and obeys limits. | Pending |
| 47 | Read → approve a disposable write → verify continues without re-dictation; duplicate/stale approvals do not execute. | Pending |
| 48 | Denial/expiry/Stop prevents dispatch; a later model failure preserves completed receipts and unfinished-step information. | Pending |

### Testing order

Follow [Step 2B](#current-small-batch-handoff) and the sequential acceptance track. Do not run this ledger as a single 53-check assignment. Block 1 and Block 2A are accepted; Block 2B now tests Agent close/cancellation and repeated worker cleanup in small batches. MCP/authentication steps need their actual provider/registration prerequisites; missing ones should be recorded as Blocked. Check 47 needs harmless test objects and a model that actually calls the tools; read confirmations are currently expected too. Some checks overlap intentionally to exercise different cancellation, deadline and ownership boundaries.

## Evidence and maintenance

| Evidence | Recorded result | What it does not establish |
|---|---|---|
| Latest core suite | 258 unit + 4 integration tests passed. | Live app appearance, real accounts or a measured RAM baseline. |
| Latest affected backend suites | 185 tests passed across auth, worker host, Agent, executor, capability, MCP, imported updates, developer projects, host RPC, event socket and store. Combined with core: 447. | A single complete live-model/native/MCP workflow or real OS-vault acceptance. |
| Latest static checks | Core Clippy with warnings denied, backend check/Clippy and scoped formatting/diff checks passed; unrelated backend warnings remain. | Whole-release acceptance or unrelated warning cleanup. |
| Block 1 audit patch | Frontend production build and 9 focused supervisor/runtime tests pass; 105 SDK/checker/CLI, 16 host API and 4 developer-loader tests pass; backend library check passes. | Real WebView CSP enforcement, actual direct-network refusal or a complete worker/IPC sandbox. |
| Block 2A focused audit | 258 core unit + 4 integration, 48 native-host, 4 executor and 15 Agent tests passed. Token-scoped teardown, generation changes and stale approval refusal were reviewed against production calls. | Guaranteed rollback of an already-started tool, all cold-start races, measured RAM/handles or every future backend path. |
| Earlier slices | SDK, CLI, checker and affected frontend type/build/test checks are recorded in the plan's dated evidence. | They were not all rerun in the latest backend slice; historical overlapping counts must not be added together. |
| Manual/live checks | Checks 1, 3, 4, 5, 6, 7, 16, 25, 27, 29, 30 and 35 Pass; 41 Pending. Block 2A audit passed; 2B is active. | No live provider certification or achieved workflow-completion percentage is claimed. |
| Initial Agent harness, Windows | All 12 real-app scenarios Pass in `run-q7NuS5`, cleanup Pass. Its eleven short cases also Pass from another clean profile in `run-L9Rzrk`. Normal-build focused suites: 16 Agent + 48 native-host + 46 MCP + 23 auth = 133 Pass. Harness assertions/reporting: 9 Pass; test-feature isolation marker: 1 Pass. | No live-model/account, OS-vault write, microphone/pill, measured RAM/handle trend or whole-block acceptance. Earlier intermittent findings remain open. |
| Harness failure detection | `run-NYtjox` intentionally returns an incorrect fixture greeting: expected assertion Fail, exit 1, cleanup Pass. Missing-binary self-test verifies Blocked/exit 2 rather than success. | A deliberately failed oracle check is not a passing product scenario. |
| Harness build checks | Application and Vite-config type checks, normal/harness frontend builds, separate-port assertions, scoped Rust/JS formatting and staged whitespace checks pass. Test hooks are feature-gated; the Handy tree is unchanged. | Existing unrelated generated-binding changes and backend warnings are outside this patch. |

Representative recent commits: `10d6990d` (native sign-in ownership/binding), `cbeccbb5` (native account isolation), `3232b53c` (selected tools/approval continuation), `4da6c745` (registry preservation/account publication), `ff015f94` (grant validation), and `10d777b1` (activation publication). Earlier implementation evidence and multi-source research remain in the execution plan.

For subsequent implementation work:

1. Update this document's date and implementation snapshot; revise the cumulative behavior/remaining limits instead of only appending a chat history.
2. Add or resolve a divergence row when delivery differs from the authoritative plan. Distinguish user-approved scope changes, temporary choices and unfinished requirements.
3. Keep full procedures and newly numbered checks in the plan. Add each new check exactly once to an appropriate group here, retaining existing numbers; update both totals.
4. Record actual user results by changing the row status and adding evidence below. Do not infer Pass from automated tests, credential badges or a model's prose response.
5. Close a phase only when its gate evidence is recorded. Record deterministic checks, live acceptance and measured resources separately.

### Manual result record

**Checks 1, 3, 4, 7 and 16 — Pass, 30 September 2026, Windows real app:** the prepared tool-only native project loaded/enabled without retired consent and its Agent greeting worked cold, immediately again and after about three idle minutes. The user then reported all three remaining Block 1 checks worked: normal core settings/features and restart, absence of old extension controls, and rejection of the separate package requesting `capture`. Exact refusal text, worker-reap logs and MCP banner status were not supplied; no live provider was certified. **Block 1 audit retest after `6e02748b` — Pass:** the user provided actual tool results for Tool Smoke (`Hello from this tool.`) and Network Boundary Probe (`PASS: unbrokered browser fetch was blocked.`), and reports that dictation works. Check 2 remains Pending because an actual legacy installed profile and archive/restart migration were not specifically exercised. For later reports record: **check number, date, tested commit, platform, provider/model when relevant, Pass/Fail/Blocked, observed result and next action**. For failures, include the exact visible error and redacted developer-log timestamps. Never include credentials or private prompt contents.

**Checks 6, 25 and 27 — Pass, 30 September 2026, Windows real app:** the user reports all three Lifecycle Smoke modes worked. Supplied tool output shows an interrupted slow call reported as uncertain rather than undone/retried, successful fresh fast calls on the current instance, and refusal of the old approval after an unchanged reload. The refusal included a malformed dash (`â€”`); its string literal was fixed during the focused audit. A separately timed cold-start repeat for check 25 was not described. No native write, installed-package rollback or measured worker memory claim follows from these observations.

### Progress log

- **29 September 2026:** Created this consolidated report through `10d777b1`; reconciled all 53 pending manual checks into seven groups and documented scope changes, implementation differences and the seven open phase gates. This update changes documentation only.
- **29 September 2026:** Verified the API-versus-implementation distinction against the production worker, generated authoring API and host preflight/dispatch: legacy backend branches remain unreachable behind refusal and still require physical removal. Recorded the next retirement unit and a minimum user acceptance checkpoint; no implementation or manual-test status changed.
- **29 September 2026:** Added sequential acceptance blocks and a single active real-app handoff. Block 1 starts with reduced extension controls, then a real native tool call; all 53 manual checks remain Pending until the user reports results.
- **29 September 2026:** Received the Developer-screen screenshot, recorded check 16 as partially observed and noted the MCP negotiation error without attributing a cause. Prepared disposable, bundled Tool Smoke locally; Step 1B is to load its folder. No provider connection or application-code change was made.
- **30 September 2026:** The user showed Tool Smoke loaded in the real Developer view and reports turning it on. Individual tools are not shown as separate buttons there; Step 1C batches cold, immediate repeat and idle-recovery Agent calls. Checks 3, 4 and 16 remain Pending until their complete outcomes are reported.
- **30 September 2026:** The user reports the tool and ordinary dictation work. Marked check 3 Pass (one completed manual check), recorded partial check 1 and 4 evidence, and requested clarification before closing the immediate/idle repeat procedure. No code, provider or credential change was made.
- **30 September 2026:** The user confirms both immediate and three-minute repeat tool calls succeeded. Marked check 4 Pass; 2 of 53 numbered manual checks are complete and 51 remain Pending.
- **30 September 2026:** Prepared a separate disposable negative package with a retired `capture` permission; the real CLI doctor reports only that intended error. The next user batch covers core settings/restart, retired management/store controls and refusal of this package while preserving the valid greeting. No app code or provider credentials changed.
- **30 September 2026:** The user reports all three remaining Block 1 checks worked. Marked checks 1, 7 and 16 Pass, bringing the ledger to 5 Pass / 48 Pending. The focused audit found direct browser networking available to native workers, added a hidden-host CSP and narrowed its Tauri capability to event operations. Automated build, contract, host and worker tests pass. Prepared a second disposable, CLI-validated tool that attempts a direct fetch of the dev server; real WebView retest remains before accepting Block 1. Physical deletion of unreachable retired backend handlers is deferred as a separate later cleanup unit.
- **30 September 2026:** The user supplied Tool Smoke and Network Boundary Probe tool results after the audit patch and confirms dictation works. Accepted Block 1; the numbered ledger stays at 5 Pass / 48 Pending because these were retests. Prepared a CLI-validated Lifecycle Smoke fixture with fast and slow confirmed tools for the first Block 2 batch (checks 6, 25 and 27). No code or hosted-provider change was made in this update.
- **30 September 2026:** The user reports all three Block 2A Lifecycle Smoke modes worked and supplied the interrupted/uncertain, stale-refusal and fresh-success tool results. Marked checks 6, 25 and 27 Pass (8 Pass / 45 Pending). Audited worker/Agent ownership and relevant tests, corrected the visible stale-refusal encoding, and accepted Block 2A. The next four observations exercise Agent close/Escape/conversational approval and pending-confirmation teardown in Block 2B.
- **1 October 2026:** Investigated the Agent summon failure reported as `STATUS_CONTROL_C_EXIT`. The user confirmed it occurs after using the Agent shortcut in the terminal, rather than at app startup. Guarded the Windows selection-copy path against terminal foreground windows; real-app terminal and nonterminal retests remain before resuming Block 2B. No numbered check was marked Pass.
- **1 October 2026:** The user confirms both terminal-fix retests and all four prior Block 2B close/reopen modes worked. Marked check 29 Pass (9 Pass / 44 Pending); check 30 has pending-approval evidence but still needs its slow-model-response case. Reviewed the X/Escape/window-destroy paths and reran all 16 Agent tests. Block 2B remains active; the next batch covers checks 30, 5 and 35.
- **1 October 2026:** The user explicitly confirms the remaining model-response close case, all ten Tool Smoke unload/reload cycles and both idle timing cases worked. Marked checks 30, 5 and 35 Pass (12 Pass / 41 Pending). Reviewed token-scoped reload and reaper removal; all 48 extension-host tests passed. Verified the disposable Lifecycle Smoke build and CLI doctor before handing off final Block 2B check 37. No application code changed.
- **1 October 2026:** Added the persistent acceptance-harness design and linked it to the execution plan. Verified that the developer socket offers reload only and that a temporary working directory/environment alone cannot establish application/vault isolation. Defined H0–H4 delivery, evidence classes and human sign-in handoffs; 12 Pass / 41 Pending remains unchanged. This update changes documentation only.
- **1 October 2026:** Implemented the persistent **Agent** harness and renamed its design companion to `AGENT-TEST-HARNESS.md`. Added the isolated feature-gated real host, scoped fixture controls, production worker observations, selective scripted model, actual Agent UI driver, authenticated production reload, real idle timing, stable scenarios, build/runner identities, bounded evidence and owned teardown. User authorization supersedes the obsolete automation rule in `AGENTS.md`.
- **1 October 2026:** Ran all twelve scenarios successfully with cleanup Pass, then repeated the eleven short cases from a clean profile. The real idle case retired after about 126.6 seconds and a slow call dispatched after another 110-second wait completed on the same worker; its whole case took about 253 seconds. The deliberately wrong greeting failed as required. Reports identify the precommit dirty working tree based on `55133ae5`, executable SHA-256 `33b68063383faaec7d78b3ec5f548dc81fdc26897cfc4b6e564f1243ec0e06cb`, source fingerprint and runner fingerprint. Local full reports live in ignored `tests/agent-harness/.runs/`; use the maintained commands to reproduce them on another checkout. Recorded the registry-save and earlier Escape findings instead of masking them with retries. Manual ledger remains 12 Pass / 41 Pending; check 37 and the focused whole-block audit remain open.
