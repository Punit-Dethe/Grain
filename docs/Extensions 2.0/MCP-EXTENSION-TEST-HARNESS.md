# Persistent extension acceptance harness

**Created:** 1 October 2026. **Status:** design; no harness runner or application automation has been implemented by this document.

Companion to [the execution plan](MCP-EXTENSION-PLAN.md) and [the progress ledger](MCP-EXTENSION-PROGRESS.md). The user requested reproducible automation of extension tests, with human participation where account sign-in or practical judgment is needed. This work supports the existing sequential acceptance blocks; it does not create another extension feature roadmap.

## What the harness should do

Keep the test definitions and runner in the repository. Create a fresh disposable working directory for each run, containing fixtures, an isolated application profile and bounded evidence files. A run should set up a known state, perform a named scenario, assert specific results, clean up everything it owns and produce a readable report. Adding another scenario must not require a new testing architecture.

The target is Grain's real implementation: actual extension validation, registry, native workers, pinned MCP SDK, action executor and Agent approval/continuation paths. A real-application suite must run the real Tauri host and WebView workers. An existing Rust unit/protocol test remains valuable, but its success alone is not a real-application result.

Opening the app and watching for an error is insufficient. For example, the reload test must prove which generation produced a result, whether the interrupted call dispatched, whether it was automatically repeated, whether the replacement stayed available and whether the old worker/token was retired.

## Verified starting point

- `scripts/run-rust-test.ps1` supplies the Windows activation manifest needed by the real Tauri library test executable. `scripts/run-tauri.ts` handles this checkout's Windows native-build workaround. Reuse these rather than inventing conflicting launch behavior.
- Existing extension-host, Agent, capability, action-executor and MCP tests already exercise important production helpers. Local MCP/OAuth fixtures provide deterministic failures without requiring external accounts.
- `grain-sdk::DevControlFrame` and the developer branch in `events_server.rs` allow an authenticated developer client to reload an already approved project and return enabled/worker/token information. They do **not** let the client install fixtures, execute tools, approve calls or operate Agent. Do not expand this token into a general privileged test controller.
- The current disposable native fixtures live in a user temporary directory. Their definitions need to become versioned, harmless harness fixtures before another developer can reproduce the same run.
- Complete application-profile, vault-namespace and single-instance isolation has not been established for harness use. Setting `APPDATA` or a temporary working directory is not sufficient proof of isolation: the launcher explicitly notes that Tauri runtime paths use Windows known-folder APIs.

## Test ownership and reports

Each scenario has a stable ID, suite, prerequisites, setup, actions, expected observations, cleanup and links to relevant existing numbered checks. A scenario may support only part of a numbered manual procedure; record that limitation explicitly.

Reports distinguish:

| Evidence | Meaning |
|---|---|
| Production-logic automated | Real helpers/SDK exercised in deterministic Rust or protocol tests; no claim about the complete application. |
| Real-application automated | Real host/workers and the stated application path executed and independently asserted. |
| Human-assisted | The harness collected evidence, with a named human step such as browser consent. |
| Manual observation | The user performed and reported the procedure. |

Each result is Pass, Fail, Blocked or Not run. Missing binaries, account registration, required model access, denied UI automation or unavailable diagnostics must never become Pass. Reports identify commit, dirty working-tree state, executable/build identity, platform, adapter/provider, attempt count, monotonic timings and prerequisite failures. A report from a different build must not silently certify the current build.

Store machine-readable JSON plus a short Markdown summary. Retain bounded redacted diagnostics and counter deltas, not tokens, full user transcripts or personal account data. Failure evidence should say what was expected and what was actually observed. The maintained progress document links to accepted evidence; an automated run does not silently rewrite the existing manual Pass/Pending ledger.

## Isolation and control requirements

1. Establish and verify an isolated profile covering registry, settings, extension storage, artifacts, logs, credentials and the instance lock before automating any mutation. Use an explicit distinct vault namespace; a scratch filesystem directory does not isolate the OS vault. Refuse a mutation suite if it could target the user's normal profile.
2. Launch and clean up only owned processes. Use process ownership/IDs and bounded shutdown, not executable-name-wide termination. Attach to a user's current app only for explicitly supported observation; destructive scenarios use a harness-owned instance.
3. Exercise the production admission and approval paths. Any in-process test entry point must call the same validated operations and preserve enabled-state, generation, schema, account, confirmation and dispatch checks. Never use direct handler execution as evidence for Agent-to-extension behavior.
4. Prefer existing tests and narrow diagnostics. If an application integration runner needs test-only hooks, put them behind an explicit compile-time feature plus opt-in launch configuration, disabled in ordinary builds. Keep the runner fixture-scoped, short-lived and unable to read real credentials or grant arbitrary extension privileges. Review the feature's exclusion from release builds.
5. Do not add an unrestricted production HTTP/WebSocket test API. A controller's knowledge of a developer token must not imply authority to approve calls or access first-party context. Any control design needs its own focused security review.
6. Await observable readiness and completion conditions with deadlines. Use monotonic clocks for idle tests and record actual elapsed time. Timing assertions for real cleanup use production thresholds; accelerated unit tests are a separate evidence class.
7. Clean up listeners, worker tokens, processes, temporary grants and test credentials on success, failure and cancellation. A cleanup failure fails the run. Preserve bounded diagnostic artifacts after failure without retaining live services. Repeating a scenario starts from a verified baseline.

## Application windows and repository rules

`AGENTS.md` currently prohibits browser/computer control for Grain UI development or visual evaluation, and prohibits browser-only UI replicas and mock Tauri visual harnesses. This design complies: initial work covers deterministic production tests and non-visual runtime acceptance. It does not authorize controlling the user's windows.

If a later suite needs automatic clicks, Escape, dialog interaction or Agent text entry through the real windows, agree a narrow exception for real-application acceptance automation before implementing that adapter. Tauri documents real-application WebDriver testing; this is a possible technical route to evaluate after that exception, not an existing Grain integration. Visual approval remains the user's responsibility. Never substitute a rendered browser copy for the real application.

References: [Tauri test modes](https://v2.tauri.app/develop/tests/), [Tauri WebDriver implementation](https://github.com/tauri-apps/tauri/tree/dev/crates/tauri-driver), [official real-application examples](https://github.com/tauri-apps/webdriver-example). Tauri distinguishes its mock runtime, which does not execute native WebView libraries, from end-to-end WebDriver tests. Driver/platform and fork compatibility must be verified against Grain's pinned dependencies before selection.

## Small implementation sequence

### H0 — Isolation, inventory and reproducible runner

Inventory the existing production tests against checks 1–53, recording partial coverage and remaining manual steps. Add a checked-in runner, scenario manifest, harmless fixture sources and JSON/Markdown reporting. Run the focused native lifecycle suites using the existing Windows test runner. Establish profile/vault/instance isolation and owned-process cleanup before attempting real-host mutations.

**Acceptance:** a fresh checkout can list and run the native production-logic suite; a missing prerequisite reports Blocked; a failing assertion produces a failed process exit and useful evidence; cleanup is tested; no real-application or manual check is falsely marked complete. Review this foundation before adding runtime control.

### H1 — Real native lifecycle, current Block 2B first

Launch an isolated real Grain host and load the versioned harmless native fixture through the validated extension path. First implement greeting, warm reuse, ten replacement cycles, real idle cleanup and developer reload. Observe generation-specific dispatch/results and worker/token retirement. Add slow-call interruption and disabled-state preservation through the same production operations. Include resource baselines and bounded return-to-baseline assertions rather than assuming a fixed total worker count in a shared app.

Map to checks 3–6, 25, 27, 35 and the runtime part of 37. Shared socket/recording/pill behavior in check 37 and window-close checks 29–30 remain partial until the actual corresponding paths are exercised. Explicitly distinguish a directly prepared executor call from a call discovered and requested by Agent.

**Acceptance:** run twice from clean profiles, with reproducible results and no automatic repeat of an interrupted dispatched call. Inject a known fixture failure and verify the harness detects it. Finish the focused native block audit against this evidence. Check 37 remains Pending until its whole procedure is covered or the user completes it.

### H2 — One controlled MCP, then two extensions

Reuse local protocol fixtures and the pinned SDK to test discovery, selective loading, schema refusal, actual calls, error outcomes, pagination, transport loss, teardown and reconnection. Expose harmless read/write tools backed by a disposable operation journal so a lost write reply can be proven to have dispatched once. Any loopback HTTP/TLS test allowance stays in test configuration and must not weaken production network/OAuth policy.

Then run one native and one MCP together with colliding tool names, separate storage/accounts, independent disablement and partial failure. Test cross-extension attribution before growing to three/five extensions. Keep real-host and protocol-only reports distinct.

### H3 — Authentication with human handoff

Automate controlled authorization metadata, PKCE/state/resource checks, callback cancellation, listener teardown, timeouts, exchange/refresh failure, stale account replacement and disconnect using disposable credentials. Exercise the real OS vault in the isolated namespace separately from memory-store tests.

Live certification pauses for the user to sign in and consent with a selected provider. The harness resumes only after observing verified completion, then checks the applicable restart/refresh/disconnect behavior. It must report unsupported registration modes or missing prerequisites. One provider's success certifies that provider/configuration, not every MCP. Never capture passwords, automate MFA or put account secrets in reports.

### H4 — Agent workflow and release regression

Add deterministic model responses to force exact search/load/approval/cancellation cases, while keeping a separate real-model suite for relevance and actual tool use. Deterministic responses are not proof of live-model selection. Test that approval resumes the original task, denial blocks repeats, independent verification can proceed and returned results stay bound to the right call/account/generation.

Grow the matrix to upgrade/restart recovery, memory/handle trends and the existing one-to-five-extension ladder. Run the appropriate small suite after each module change; use the full regression matrix at release gates. Keep each block's focused source/security audit after acceptance testing.

## What still needs a person

Live provider browser consent/MFA, the usefulness of actual model responses, microphone/audio quality, final appearance/accessibility judgment and practical shortcut behavior in applications the harness cannot faithfully reproduce remain human checks. Some may become assisted tests later. The runner should present only the necessary human steps in a batch and collect their outcomes beside automatic evidence.

The expected benefit is fewer repeated manual cycles and better diagnosis, not a promise that automation certifies the entire product. The harness itself needs review, intentional failing cases and repeatability checks because it can contain mistakes too.
