# Native deadlines, inputs and failure results — Block 2C

Date: 1 October 2026. This focused audit follows accepted Block 2B and covers checks 21, 24, 26 and 32–34. It does not certify authentication, MCP transport, installation/update recovery, native companion executables, live-model judgment or the whole native/release gate.

## What changed and why

The maintained real-app harness now has six additional scenarios under `native-failures`. Its permission-free fixture supplies a real 25-second handler, declared/thrown errors, conflicting success/error envelopes, a reply lost by closing the actual worker socket, and replies beyond the existing decoded and wire budgets. Five model-authored input cases cover undeclared keys, missing required text, wrong types, malformed JSON and excessive bytes. The excessive model call is refused by the earlier model-message budget/identity boundary; the other cases reach native argument validation. Neither path requests approval or wakes a worker.

Debug-only evidence adds host invocation/startup and coarse outcome classifications. It retains no argument/result/error text or raw tokens. The harness distinguishes a delivered reply (`completed`) from a successful action (`outcome: succeeded`), checks visible notices and private-marker absence, counts dispatch/model calls, and verifies worker/token/supervisor cleanup and a successful fresh call. These hooks and fixed new instruction variants remain feature-gated. No control can execute a tool directly, approve a tool, select arbitrary code or access an account.

The production-logic runner now includes `execution`, so the native envelope/error mapping tests are maintained alongside worker-host tests. The runner still stops after a failure, leaves later scenarios Not run, and never retries a tool or a failed scenario within a run. `successful-error` deliberately changes the isolated error fixture into a success; it must fail the outcome assertion.

## Reproduced finding and repair

**Warm source drift, `run-Stew7J`:** the old approval was correctly rejected after changing source without Reload. However, the first fresh approval selected the obsolete warm worker. `wake_for_request` checked enablement and registry generation, while queue admission correctly refused its old source digest. The visible result said the action was not dispatched; the fresh greeting assertion failed. This was a product failure, with cleanup Pass, not successful source-drift acceptance.

Warm reuse now requires the exact native call/source digest as well as the current enabled registry generation. Before replacing anything, the wake path rereads the source, checks the approved identity under the registry revision guard, and refuses a stale request without retiring a newer worker. Only an obsolete token is retired; the new approved call starts a current worker. Final queue admission and reply revalidation remain unchanged. This does not replay the rejected call or reuse its approval. A production regression covers changed source with an unchanged registry generation, fresh identity, disabled state, changed generation and missing identity. The real-app test requires the first fresh request to succeed without Reload.

**Harness assumption, `run-QWY4Zt`:** the repaired drift case, deadline and size cases passed, but the input scenario waited for a fourth model request. An oversized model tool call is refused earlier, after three requests. Its visible refusal and zero dispatch were correct. The runner now waits for the selected-action attempt and host release, accepts the appropriate bounded refusal, and checks no worker/approval/private-key leakage. The failed report is preserved; a corrected single-case retest passed in `run-el1AoD`. This was an oracle repair, not a product input-validation repair.

## Focused source audit

- One monotonic 20-second operation budget covers validation, wake, queue and reply. Startup has the existing three-second sub-budget; reply waiting receives only the remaining operation budget. This is measured under normal local I/O, not a guarantee that synchronous filesystem work can be preempted during an OS stall.
- `NativeCallOwner` retires the exact token after startup/transport/budget/deadline failure or abandonment. Pending-call guards remove reply waiters; socket shutdown drains waiters and aborts owned host RPC work. Normal tool-reported errors keep the healthy worker, clear the pending call and preserve enablement.
- Pre-dispatch failures are failed actions. A lost reply/deadline/raw transport closure after dispatch has an unknown outcome. An invalid or over-budget decoded reply has an unavailable result. Declared/thrown tool errors remain tool-reported errors. None becomes a success or proof of rollback; raw private worker messages do not become UI/model notices.
- Native inputs/results have the existing 64 KiB encoded JSON, node and depth limits. The shared listener separately limits raw frames and assembled messages to 512 KiB before parsing. Oversized wire traffic closes its own socket; a fresh native greeting proves listener recovery. The normal microphone/native-pill path still needs the human observation below.
- Source verification follows registry revision → current enabled record → exact worker identity → final admission. File reading stays outside the registry lock; the bounded identity hash and worker snapshot are checked inside it. No new service, persistent source copy, background thread or retry mechanism was introduced. The extra source reread is transient and correctness-driven.

Graph change/review/impact tools were consulted, then the wake/admission/reply, host outcome, argument parser, socket limits and scoped test seams were reviewed directly. Structural graph summaries are navigation evidence, not proof that every runtime path is indexed.

## Research references and application

- [Tokio timeout and cancellation](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) and [absolute deadlines](https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html) support bounded waiting and dropping unfinished futures. Grain additionally owns external worker/socket cleanup: dropping a waiter alone cannot establish that a provider made no change.
- [MCP TypeScript SDK error guidance](https://github.com/modelcontextprotocol/typescript-sdk/blob/main/docs/servers/errors.md) separates protocol errors from tool-reported failures. Grain's native adapter uses its own envelope while preserving that distinction; uncertain transport loss is separately reported rather than turned into a tool success.
- [Tungstenite transport configuration](https://docs.rs/tungstenite/latest/tungstenite/protocol/struct.WebSocketConfig.html) distinguishes frame and message bounds. Grain sets both and independently bounds decoded result values; a decoded-value check cannot replace the raw transport limit.
- [VS Code's extension-host lifecycle](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/services/extensions/electron-browser/localProcessExtensionHost.ts) and [real extension-host restart smoke tests](https://github.com/microsoft/vscode/blob/main/test/smoke/src/areas/extensions/extension-host-restart.test.ts) provide an open-source reference for bounded termination/restart testing. Grain uses its existing token-scoped worker teardown and real application, with no automatic uncertain-action replay. These are design references, not claims that Grain shares VS Code's process sandbox or resource behavior.

## Evidence and acceptance state

**Block 2C accepted. Implementation, focused source audit, automated retests and the user's ordinary-app observations are complete.** Checks 21, 24, 26, 32 and 33 are explicitly recorded as automated Pass in the progress ledger after reviewing their complete numbered procedures. Their evidence class is real application with a scripted model; the original thirteen user-reported results retain their separate provenance. At 2C acceptance the ledger reached **19 Pass / 34 Pending** after the user's check 34 result; subsequent 2D evidence is recorded in the progress companion.

| Evidence | Result |
|---|---|
| `run-GIQ8JB` | All **19** real-app scenarios Pass, cleanup Pass. Existing lifecycle cases and the actual idle timing were rerun alongside the six new cases. Idle retirement: **143,852 ms**; whole idle case: **270,615 ms**. Native absolute deadline: **20,006 ms** from host invocation. |
| `run-vfq102` | All **six** Block 2C cases Pass from a new clean profile, cleanup Pass; native absolute deadline again **20,006 ms**. |
| `run-EGhvlm` | Intentional `successful-error` fault detected: actual succeeded differed from expected toolReportedError; exit **1**, cleanup Pass. This is successful failure detection, not a passing product scenario. |
| Normal-build production suites | **49** native-host, **4** executor and **18** Agent tests Pass. The native-host group includes the new warm source/generation regression. |
| Shared core/worker checks | **260** core unit + **4** integration tests, **11** harness self-tests and **9** focused frontend supervisor/runtime tests Pass. Counts overlap older evidence and are not cumulative product coverage. |
| Static/build | Maintained harness frontend type/build and native build Pass; backend library Clippy exits successfully with **68 unrelated existing warnings**; scoped Rust/JS formatting and diff checks Pass. The test-feature build has its expected unused ordinary-startup warnings. |
| Manual fixture preparation | Windows PowerShell creates valid UTF-8 JSON with two confirmed tools and no permissions. Store-oriented CLI doctor identifies only its intentionally absent submission icon; it is not claimed to be a clean submitted package. No ordinary profile or account was modified. |

Both accepted runtime batches identify executable SHA-256 **`0a8d6c0129d27ce259f3a26e5f76ee6d1171bb80ca6247e2f58f6fa06cc83121`**, source fingerprint **`b8025e0c753fdeab041fb7c1b0cf639df15c5ed9b2a69a5b90dad1b5ac5c8275`**, and runner fingerprint **`6d737cb05b69db8d9d47589582ffbf366776cb9ff8ca5a113e64126c52959e60`**. They record the dirty precommit tree based on `5800818d`; this document does not relabel them as postcommit runs. Local reports live in ignored `tests/agent-harness/.runs/`, with versioned commands in the maintained README for reproduction.

Earlier failures remain preserved. The ordinary-app portion of check 34 was separately reported Pass by the user after the `0884aca5` handoff: expected warning, dictation/pill recovery and actual fresh greeting. No live provider, microphone, pill, companion, RAM/handle trend or whole-release acceptance follows from this batch.

Reproduce the current runtime coverage after the maintained build:

```powershell
npm run test:agent -- --suite all
npm run test:agent -- --suite native-failures
npm run test:agent -- --scenario native.reply-failures --fault successful-error
npm run test:agent:logic -- --group native
npm run test:agent:logic -- --group execution
```

The third command must exit 1 with cleanup Pass; it is an intentional fault test. Do not count it as product acceptance.

## Accepted ordinary-app check 34 (procedure retained)

This is one numbered check with three observations. Use the ordinary Grain app and its existing microphone/model configuration. If rebuilding, run `bun run dev:asr` from the repository root; the isolated harness executable is not the ordinary app.

Prepare a disposable folder from the maintained fixture:

```powershell
cd C:\Projects\Grain\grain
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/prepare-manual.ps1
```

Developer → Load unpacked → select the printed folder → enable **Native Failure Smoke**. It declares only two tools, no permissions or external account. A store-submission icon is intentionally absent; this is a load-unpacked test fixture, not a submitted package.

1. Ask Agent: **“Use Native Failure Smoke's Return a deliberately oversized reply tool once.”** Approve it. Expect a warning that Grain could not confirm the result; no success and no automatic repeat. Confirm the actual tool was selected, rather than accepting a model-written explanation.
2. Close Agent and perform ordinary dictation into a scratch text field. Expect the recording pill, one pasted transcript and a clean stop. Start another recording, cancel it through the pill, then dictate once more. Expect no paste from the cancelled recording and normal recovery.
3. Ask Agent: **“Use Native Failure Smoke's Say test hello tool.”** Approve it. Expect the actual tool result **`Harness hello (one)`** from Native Failure Smoke. Unload the disposable fixture afterward.

The user reports all three observations Pass, with the expected warning and actual greeting supplied. No provider login, source edit or repeated timeout/size suite is required. 2C is now closed with its audit/retest evidence; continue **2D consent, installation and persistence**. Whole native and R0–R6 release gates remain separate.
