# Extension implementation: progress, changes and user testing

**Updated:** 29 September 2026. **Implementation snapshot:** `10d777b1`, on `extensions/tool-only-retirement`.

This is the maintained progress report beside the [execution plan](MCP-EXTENSION-PLAN.md). The plan owns the intended architecture, phase gates, research references and exact test procedures. This document explains the current implementation, records changes from that plan and keeps the user-facing test results together. It is not a second execution plan.

## The current position in plain language

Grain extensions now have one job: provide tools that the agent can call. A tool might be implemented directly as a native Grain extension, or supplied by an MCP server. Both go through Grain's checks before execution.

Much of the foundation is implemented. Old extension privileges are refused, workers have clearer start/stop ownership, inputs and results have limits, sign-in has stronger account separation, and the agent can find selected tools and continue a task after approval. Installation and account changes also have stronger save-before-activation behavior.

The complete product is still unfinished. We have not certified live providers, measured the real application's memory baseline, or completed the one-to-five-extension workflow ladder. The user has started real-app testing: **2 of 53 numbered manual checks have passed; 51 remain Pending.** Ordinary dictation was also reported working, while its broader numbered procedure remains open. **All seven phase acceptance gates remain open.** This means seven phases have unfinished requirements, not that seven phases are untouched.

The latest recorded implementation verification passed **447 automated tests**, with one separate live-store test ignored. Those tests exercise substantial production logic and local protocol fixtures; they do not certify real accounts, the real OS credential vault or the complete real-model workflow.

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

The next retirement unit should remove obsolete extension dispatch implementations and their exclusive wiring/helpers, while preserving first-party context features and the minimal metadata needed to reject/migrate old packages. Keep the host's tool-only allowlist and negative access tests after deleting those implementations. Source inspection and automated production-path tests can proceed without user testing; real-app checks close acceptance rather than blocking all independent development.

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

**Current focus: Block 1 only.** This track organizes the existing execution plan into reviewable product behaviors; it does not replace its R0–R6 requirements. Complete each block's real-app checks, fix the failures, perform a focused source/security/lifecycle audit of that block, rerun its relevant automated checks and repeat the affected manual checks before calling it accepted. Do not spread implementation across later blocks merely because their unit tests already pass. A missing external account or provider is a recorded prerequisite, not a pass.

| Block | Behavior to finish | Real-app acceptance to collect | Focused audit after user feedback | State |
|---|---|---|---|---|
| 1. Tool-only boundary and one native tool | Ordinary Grain features survive; retired extension capabilities are absent/unreachable; a newly authored tool loads and actually runs through Agent. | Checks 1, 3, 4, 7, 16; check 2 if a legacy installation exists. Confirm the real greeting tool result, not a model-written greeting. | Remove unreachable retired backend handlers/wiring while retaining first-party context and migration/refusal tombstones; inspect declaration, host preflight, Agent offering and core-feature independence. Fix/retest observed failures. | **Active; user tests pending** |
| 2. Native execution, consent and persistence | The same native tool survives ordinary reload/restart, stops safely and cannot execute an old approval against a new owner. | Checks 5, 6, 18–19, 21, 24–30, 32–40, 49, 52–53 as applicable; use disposable packages for installation cases. | Audit worker/resource ownership, timeout/result certainty, package publication, approval identity, registry recovery and teardown. Fix/retest. | Queued |
| 3. One MCP transport and tool contract | A controlled MCP provider is discovered, loaded, called and disconnected with honest results, independently of OAuth. | Checks 8, 17, 20, 23 and 31 with a safe read and a provider exposing the relevant shapes; mark unavailable scenarios Blocked rather than guessing. | Audit actual SDK transport, pagination/schema limits, dispatch/outcome classification and cleanup against deterministic fixtures and the selected provider. Fix/retest. | Queued |
| 4. Authentication for one certified provider | One real MCP account connects, survives restart/expiry and disconnects; stale sessions cannot act for a replacement. Then certify native account behavior with a suitable fixture. | MCP checks 9–15, then native checks 41–45 and 50–51 when the native connection fixture/UI exists. Record registration mode, scopes and provider identity. | Audit callback, issuer/resource/client/scope mapping, vault publication/refresh/revocation, cancellation and orphan cleanup; fix/retest. A provider-specific compatibility matrix is required. | Queued |
| 5. Agent tool selection and full task | Agent finds only relevant schemas, confirms a disposable write once, resumes the original task and verifies the result. | Checks 22, 46–48, plus stale/changed-definition outcomes from 19 and 23; use real model tool calls and test objects. | Audit selection freshness/cache, exact call IDs, account/schema revalidation, approval/auth continuation, denial and receipt preservation; fix/retest. | Queued |
| 6. Two complete extensions together | One native and one MCP tool both work in the same task, including name collisions, disablement and partial failure. | Run the R5 one-native, one-MCP and two-extension ladder with recorded real calls, account routing and cleanup. | Audit cross-extension data boundaries, isolation, lifetime and result attribution; fix/retest. | Queued |
| 7. Scale and release | Three- and five-extension workflows, compatibility, measured resources, upgrade and release behavior are documented and accepted. | Complete the remaining R5/R6 gates, including real Tauri visual review and any still-open numbered checks. | Final focused integration/security review, compatibility/CI matrix and resource measurements. | Queued |

The order deliberately puts **one working native tool before broad authentication work**. MCP transport is checked with a controlled safe provider before auth is called complete; if that provider itself requires login, its credential setup is a prerequisite, while the auth module remains unaccepted until Block 4. One certified provider is an honest release milestone; it is not a claim that all MCP servers work. The R5 two-extension demonstration is the first cross-extension acceptance milestone, with three/five tested afterward.

### One test at a time: current handoff

**Step 1A — observed, partial check 16.** The user's real-app screenshot shows **Load unpacked** and **Hosted MCP validation** with Linear, Notion, Atlassian, GitHub, Slack and Google Calendar rows. It does not show Recommendation Lab/Core 6/Stress 24 in the visible area. A red **“MCP protocol negotiation failed. Check the account and provider availability.”** banner is visible above Linear; its cause is unknown and is recorded for the later focused MCP block. No provider login, Test call, native tool execution or full-store review was performed. Check 16 therefore remains Pending rather than Pass.

**Step 1B — check 3 Pass.** The user's 30 September real-app screenshot shows a developer row for `com.example.tool-smoke` pointing to the prepared disposable folder; the user reports enabling it and then confirms the tool works in Agent. The generated fixture's inspected manifest declares the `Say hello` action and zero permissions/activation. No unexpected permission request was reported. The Developer list shows the extension ID rather than a separate tool button. The MCP negotiation banner is a separate unresolved observation; no provider action was requested.

**Step 1C — check 4 Pass; check 1 partial.** The user reports that Agent's Tool Smoke tool works and ordinary dictation works in the real app, then explicitly confirms the greeting also worked both immediately and after about three minutes without tool activity. This completes the user-facing cold/warm/idle greeting procedure. Worker-reap log and measured resource evidence remain independent Block 2/phase-gate work. Check 1 remains Pending because Snippets/Context/Agent settings and restart persistence were not reported. Do not connect, test, disable or reconfigure hosted MCP providers to make this native test pass.

**Step 1D — next Block 1 batch, checks 1, 16 and 7.** (a) Open Snippets, Context and Agent, change one ordinary core setting, restart the real app and verify that setting plus dictation still work. (b) Look through Extensions/Developer and Store without signing in: retired Recommendation Lab/Core 6/Stress 24, prompt packs, screen/OS/Space integration controls should not be offered; an empty eligible store is acceptable. (c) Load the separate invalid folder `C:\Users\watrm\AppData\Local\Temp\grain-retired-tool-smoke-bbde33a1-c1da-4ecb-8ad1-094fc03995a4\tool-smoke-retired` through **Load unpacked → Choose folder…**. Its manifest requests the retired `capture` permission; expect an explicit refusal before activation, and the original valid Tool Smoke greeting should still work afterward. Do not approve a capture permission or edit the working fixture. The invalid fixture was independently checked with the real extension CLI: it reports exactly one manifest error for retired `capture`, without an unrelated icon error. Report the three results together, including exact refusal wording and any existing MCP banner changes; no hosted-provider action is required.

For each step, ask for **Pass / Fail / Blocked**, the visible result and any exact error. Do not require the user to run the whole 53-check list at once. Update the existing numbered ledger and the active block only after its full numbered procedure has evidence; if a failure appears, stop this block's acceptance, reproduce/audit/fix it and retest before moving to the next block.

## User testing: seven groups, all 53 checks

The numbered checks below are the same checks in the execution plan; **1–8 have not been renumbered**. This is a short result ledger, not a replacement for the plan's [full procedures and disposable Tool Smoke setup](MCP-EXTENSION-PLAN.md#combined-real-app-test-checklist-retirement-lifecycle-and-authentication-phases).

**Status vocabulary:** Pending = full numbered procedure not yet verified; Pass = user observed the expected real result; Fail = attempted and wrong; Blocked = an actual missing prerequisite prevented the check. Two entries are now Pass and 51 remain Pending. Local automated coverage alone does not turn a manual row into Pass.

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
| 1 | Dictation, Snippets, Context and Agent still work; core settings and user prompts survive restart. | Pending |
| 2 | Old privileged extensions stay disabled across restart; archived edited prompts/bindings survive without duplication. | Pending |
| 3 | Load Tool Smoke with tool-only consent; no screen, OCR, prompt or transcript permissions requested. | Pass |
| 7 | A disposable package requesting retired permissions/startup activation is explicitly refused. | Pending |
| 16 | Store/developer controls offer tool-only capabilities; retired recommendation/lab/integration controls are absent. | Pending |
| 40 | Cancel consent leaves the tool off; one fresh Allow enables it and a real greeting works, with usable controls. | Pending |

### B. Native calls, stopping and recovery — 12 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 4 | The actual greeting works cold, warm and again after idle worker cleanup. | Pass |
| 5 | Ten unload/reload cycles work without duplicate replies or an old error killing the replacement. | Pending |
| 6 | Unload during a five-second greeting stops/invalidates it; reload restores normal calls. | Pending |
| 24 | A 25-second greeting times out, does not replay or disable the tool, and a normal call recovers afterward. | Pending |
| 25 | Disable/reload during execution invalidates the old call without destroying the new worker. | Pending |
| 26 | Rebuild a warm tool without Reload; obsolete source is refused and a fresh request uses the changed source. | Pending |
| 27 | Even an unchanged reload invalidates an earlier pending confirmation. | Pending |
| 29 | Close Agent or use Escape during a native call; no late result/replay and a fresh session works. | Pending |
| 30 | Close with approval/model response pending; no old reply or approval appears in the replacement session. | Pending |
| 32 | Measure the native absolute timeout from invocation/approval; expect about 20 seconds under normal local I/O. | Pending |
| 35 | Idle cleanup and requests near its boundary preserve an executing/current replacement worker. | Pending |
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

Follow the [current handoff](#one-test-at-a-time-current-handoff) and the sequential acceptance track. Do not run this ledger as a single 53-check assignment. Tool Smoke loaded and worked cold, immediately again and after idle. Continue Block 1 retirement/core checks. Block 2 later tests reload, cancellation and persistence with that same fixture. MCP/authentication steps need their actual provider/registration prerequisites; missing ones should be recorded as Blocked. Check 47 needs harmless test objects and a model that actually calls the tools; read confirmations are currently expected too. Some checks overlap intentionally to exercise different cancellation, deadline and ownership boundaries.

## Evidence and maintenance

| Evidence | Recorded result | What it does not establish |
|---|---|---|
| Latest core suite | 258 unit + 4 integration tests passed. | Live app appearance, real accounts or a measured RAM baseline. |
| Latest affected backend suites | 185 tests passed across auth, worker host, Agent, executor, capability, MCP, imported updates, developer projects, host RPC, event socket and store. Combined with core: 447. | A single complete live-model/native/MCP workflow or real OS-vault acceptance. |
| Latest static checks | Core Clippy with warnings denied, backend check/Clippy and scoped formatting/diff checks passed; unrelated backend warnings remain. | Whole-release acceptance or unrelated warning cleanup. |
| Earlier slices | SDK, CLI, checker and affected frontend type/build/test checks are recorded in the plan's dated evidence. | They were not all rerun in the latest backend slice; historical overlapping counts must not be added together. |
| Manual/live checks | Checks 3 and 4 Pass; 51 Pending. Dictation reported working as partial check 1. | No live provider certification or achieved workflow-completion percentage is claimed. |

Representative recent commits: `10d6990d` (native sign-in ownership/binding), `cbeccbb5` (native account isolation), `3232b53c` (selected tools/approval continuation), `4da6c745` (registry preservation/account publication), `ff015f94` (grant validation), and `10d777b1` (activation publication). Earlier implementation evidence and multi-source research remain in the execution plan.

For subsequent implementation work:

1. Update this document's date and implementation snapshot; revise the cumulative behavior/remaining limits instead of only appending a chat history.
2. Add or resolve a divergence row when delivery differs from the authoritative plan. Distinguish user-approved scope changes, temporary choices and unfinished requirements.
3. Keep full procedures and newly numbered checks in the plan. Add each new check exactly once to an appropriate group here, retaining existing numbers; update both totals.
4. Record actual user results by changing the row status and adding evidence below. Do not infer Pass from automated tests, credential badges or a model's prose response.
5. Close a phase only when its gate evidence is recorded. Record deterministic checks, live acceptance and measured resources separately.

### Manual result record

**Check 3 — Pass, 30 September 2026, Windows real app:** the prepared tool-only native project was loaded/enabled, and the user reports its Agent tool works. Its manifest has zero permissions/activation; no unexpected consent was reported. **Check 4 — Pass:** the user explicitly confirms a working first call, immediate repeat and another after about three idle minutes; worker-reap logs were not supplied. **Check 1 — partial:** ordinary dictation works; restart/core settings were not reported. **Check 16 — partial:** Developer UI seen, full store/control review pending. The existing red MCP negotiation banner is recorded separately for later diagnosis. For each completed report, record: **check number, date, tested commit, platform, provider/model when relevant, Pass/Fail/Blocked, observed result and next action**. For failures, include the exact visible error and redacted developer-log timestamps. Never include credentials or private prompt contents.

### Progress log

- **29 September 2026:** Created this consolidated report through `10d777b1`; reconciled all 53 pending manual checks into seven groups and documented scope changes, implementation differences and the seven open phase gates. This update changes documentation only.
- **29 September 2026:** Verified the API-versus-implementation distinction against the production worker, generated authoring API and host preflight/dispatch: legacy backend branches remain unreachable behind refusal and still require physical removal. Recorded the next retirement unit and a minimum user acceptance checkpoint; no implementation or manual-test status changed.
- **29 September 2026:** Added sequential acceptance blocks and a single active real-app handoff. Block 1 starts with reduced extension controls, then a real native tool call; all 53 manual checks remain Pending until the user reports results.
- **29 September 2026:** Received the Developer-screen screenshot, recorded check 16 as partially observed and noted the MCP negotiation error without attributing a cause. Prepared disposable, bundled Tool Smoke locally; Step 1B is to load its folder. No provider connection or application-code change was made.
- **30 September 2026:** The user showed Tool Smoke loaded in the real Developer view and reports turning it on. Individual tools are not shown as separate buttons there; Step 1C batches cold, immediate repeat and idle-recovery Agent calls. Checks 3, 4 and 16 remain Pending until their complete outcomes are reported.
- **30 September 2026:** The user reports the tool and ordinary dictation work. Marked check 3 Pass (one completed manual check), recorded partial check 1 and 4 evidence, and requested clarification before closing the immediate/idle repeat procedure. No code, provider or credential change was made.
- **30 September 2026:** The user confirms both immediate and three-minute repeat tool calls succeeded. Marked check 4 Pass; 2 of 53 numbered manual checks are complete and 51 remain Pending.
- **30 September 2026:** Prepared a separate disposable negative package with a retired `capture` permission; the real CLI doctor reports only that intended error. The next user batch covers core settings/restart, retired management/store controls and refusal of this package while preserving the valid greeting. No app code or provider credentials changed.
