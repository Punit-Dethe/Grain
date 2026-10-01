# Persistent Agent acceptance harness

**Created:** 1 October 2026. **Status:** initial Agent harness implemented; current verification evidence is recorded in the progress companion. [Maintained runner instructions](../../tests/agent-harness/README.md) describe actual commands, coverage and limitations.

Companion to [the execution plan](MCP-EXTENSION-PLAN.md) and [the progress ledger](MCP-EXTENSION-PROGRESS.md). The user requested reproducible **Agent** testing, initially covering native extension lifecycle/tool calling and later deeper Agent, MCP and authentication behavior. Human participation remains where account sign-in or practical judgment is needed. This work supports the existing sequential acceptance blocks; it does not create another extension feature roadmap.

## What the harness should do

Keep the test definitions and runner in the repository. Create a fresh disposable working directory for each run, containing fixtures, an isolated application profile and bounded evidence files. A run should set up a known state, perform a named scenario, assert specific results, clean up everything it owns and produce a readable report. Adding another scenario must not require a new testing architecture.

The target is Grain's real implementation: actual extension validation, registry, native workers, pinned MCP SDK, action executor and Agent approval/continuation paths. A real-application suite must run the real Tauri host and WebView workers. An existing Rust unit/protocol test remains valuable, but its success alone is not a real-application result.

Opening the app and watching for an error is insufficient. For example, the reload test must prove which generation produced a result, whether the interrupted call dispatched, whether it was automatically repeated, whether the replacement stayed available and whether the old worker/token was retired.

## Verified starting point before implementation

- `scripts/run-rust-test.ps1` supplies the Windows activation manifest needed by the real Tauri library test executable. `scripts/run-tauri.ts` handles this checkout's Windows native-build workaround. Reuse these rather than inventing conflicting launch behavior.
- Existing extension-host, Agent, capability, action-executor and MCP tests already exercise important production helpers. Local MCP/OAuth fixtures provide deterministic failures without requiring external accounts.
- `grain-sdk::DevControlFrame` and the developer branch in `events_server.rs` allow an authenticated developer client to reload an already approved project and return enabled/worker/token information. They do **not** let the client install fixtures, execute tools, approve calls or operate Agent. Do not expand this token into a general privileged test controller.
- The current disposable native fixtures live in a user temporary directory. Their definitions need to become versioned, harmless harness fixtures before another developer can reproduce the same run.
- At design time, application-profile, vault-namespace and single-instance isolation had not been established. The initial implementation below now supplies explicit isolation. Setting `APPDATA` or a temporary working directory alone is insufficient: the launcher notes that Tauri runtime paths use Windows known-folder APIs.

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

On 1 October the user explicitly superseded the obsolete automation prohibition and authorized a real-application Agent harness. `AGENTS.md` now records that authorization. Browser-only replicas and mock Tauri visual harnesses remain inappropriate; visual design approval remains the user's responsibility.

The Windows adapter uses Playwright's documented CDP connection to the actual WebView2 processes created by the owned Tauri host. It activates production DOM button handlers and sends follow-up text through the real Agent WebView. For Escape it initializes the production shortcut manager and uses Windows `SendInput` only after verifying the foreground Agent window belongs to the owned host PID. This checks that specific transient Escape path; physical mouse targeting, ordinary summon shortcuts and microphone behavior remain outside the initial suite. Tauri WebDriver remains another potential platform adapter; this implementation does not require it. Never substitute a rendered browser copy for the real application.

References: [Playwright WebView2](https://playwright.dev/docs/webview2), [Microsoft WebView2 environment options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl), [Tauri test modes](https://v2.tauri.app/develop/tests/), [Tauri WebDriver implementation](https://github.com/tauri-apps/tauri/tree/dev/crates/tauri-driver). Tauri distinguishes its mock runtime, which does not execute native WebView libraries, from end-to-end testing. Other platform adapters still need compatibility verification against Grain's pinned dependencies.

## Initial implementation and scope

`tests/agent-harness/` owns the runner, stable scenario definitions, source/binary build stamp, permission-free native fixture, scripted HTTP model, authenticated developer-socket client and JSON/Markdown reporting. A separate debug-only `grain-agent-harness` executable compiles test controls under the opt-in `agent-harness` feature; release compilation with that feature is forbidden. Ordinary application builds do not include those controls or observations.

The Grain-owned profile adapter requires a bounded marker/run UUID before startup and redirects filesystem state without editing `src-tauri/src/handy/`. The compiled application identifier, event port, WebView2 cache and per-run credential-service namespace are separate from normal Grain. Test startup skips the pill supervisor, autostart application, single-instance forwarding and automatic updater. Installation approval is fixture-scoped setup through production validation/grant operations; individual tool confirmation is still performed through the actual Agent UI and host executor.

The runtime suite contains thirteen scenarios, covering cold/warm calls, decline, typed approval, stale approval, native Escape, ten rapid close/reopen pairs, window close at three lifecycle stages, ten replacements, disable/re-enable, production developer reload and real idle timing. It starts with a typed-instruction seam, so summon-time capture and microphone/pill behavior are explicitly excluded. The separate production-logic runner covers core registry and normal-build Agent/native/MCP/native-auth tests; these remain a distinct evidence class. No real OAuth account or live model is certified by the local scripted provider.

Build freshness is verified before acceptance: executable SHA-256 and a source fingerprint must match the build stamp. Inputs changing during a build refuse publication of a new stamp. The run also records commit/dirty state, an independent runner/fixture fingerprint and the actual WebView adapter version. Cleanup failures fail the batch. Missing prerequisites are Blocked; later cases after failure are Not run. Manual checks remain unchanged until their full acceptance procedures have corresponding evidence.

## Small implementation sequence

**Delivery status:** H0's initial native inventory/runner/isolation is implemented; its coverage map is intentionally partial rather than a certification of all 53 checks. H1's native runtime scenarios are implemented, with repeatability and negative-oracle evidence recorded in the progress companion. Block 2B is accepted after its focused source audit, automated retests and the user's ordinary microphone/pill/Agent batch. Native 2C is accepted after its focused audit, automated retests and the user's ordinary-app shared-listener check 34. Native 2D first-unit coverage now includes five consent/package/restart cases, with its focused audit and repeated evidence in the [installation companion](NATIVE-INSTALLATION-AUDIT.md). Its ordinary-app and signed-store/account requirements, measured resource trends and whole-native-block acceptance remain open. H2 and H3 are future suites. H4 has an initial deterministic Agent subset only; its live-model and release matrix remain future work.

**First sprint evidence:** twelve real-application scenarios passed in one batch, eleven short cases passed again from another clean profile, and the deliberately incorrect greeting produced the required failure with successful teardown. The nine harness self-tests, one isolation-marker test and 133 focused normal-build backend tests passed. Earlier registry-save failures and native Escape observations remain recorded for focused investigation; successful batches do not establish universal reliability. Read the progress companion's evidence table for run identities and exclusions.

### H0 — Isolation, inventory and reproducible runner

Inventory the existing production tests against checks 1–53, recording partial coverage and remaining manual steps. Add a checked-in runner, scenario manifest, harmless fixture sources and JSON/Markdown reporting. Run the focused native lifecycle suites using the existing Windows test runner. Establish profile/vault/instance isolation and owned-process cleanup before attempting real-host mutations.

**Acceptance:** a fresh checkout can list and run the native production-logic suite; a missing prerequisite reports Blocked; a failing assertion produces a failed process exit and useful evidence; cleanup is tested; no real-application or manual check is falsely marked complete. Review this foundation before adding runtime control.

### H1 — Real native lifecycle, current Block 2B first

Launch an isolated real Grain host and load the versioned harmless native fixture through the validated extension path. First implement greeting, warm reuse, ten replacement cycles, real idle cleanup and developer reload. Observe generation-specific dispatch/results and worker/token retirement. Add slow-call interruption and disabled-state preservation through the same production operations. Include resource baselines and bounded return-to-baseline assertions rather than assuming a fixed total worker count in a shared app.

Map to checks 3–6, 25, 27, 35 and the runtime part of 37. Shared socket/recording/pill behavior in check 37 and window-close checks 29–30 remain partial until the actual corresponding paths are exercised. Explicitly distinguish a directly prepared executor call from a call discovered and requested by Agent.

**Acceptance:** run twice from clean profiles, with reproducible results and no automatic repeat of an interrupted dispatched call. Inject a known fixture failure and verify the harness detects it. Finish the focused native block audit against this evidence. Check 37 requires coverage of its whole procedure, including the human portion; that portion is now user-reported Pass and recorded in the progress companion. This accepts 2B without certifying the later native sub-blocks.

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

The [Block 2B focused audit](NATIVE-LIFECYCLE-AUDIT.md) adds `agent.reopen-escape` (ten immediate close/reopen pairs with actual Windows Escape), a deliberate `missing-escape` failure oracle and the maintained `registry` logic group. There are now thirty real-app scenarios and nine logic groups. The registry-recovery subset reuses the installation refusal case and adds preservation/failed-save cases; suite sizes overlap and are not additive. The user completed the ordinary recording/pill/Agent batch and 2B is accepted. The [2C focused audit](NATIVE-FAILURE-AUDIT.md) adds six failure/deadline/input scenarios, outcome/timing evidence and a deliberate error-to-success oracle. All nineteen cases passed and the six new cases repeated. The user completed check 34's ordinary-app microphone/pill/greeting observations; 2C is accepted. The first three 2D units are accepted; no repeat of 2B or hosted-provider sign-in is requested.

Live provider browser consent/MFA, the usefulness of actual model responses, microphone/audio quality, final appearance/accessibility judgment and practical shortcut behavior in applications the harness cannot faithfully reproduce remain human checks. Some may become assisted tests later. The runner should present only the necessary human steps in a batch and collect their outcomes beside automatic evidence.

The expected benefit is fewer repeated manual cycles and better diagnosis, not a promise that automation certifies the entire product. The harness itself needs review, intentional failing cases and repeatability checks because it can contain mistakes too.

The [2D first-unit audit](NATIVE-INSTALLATION-AUDIT.md) documents real main-window consent, actual owned host restarts, developer A/B/installed restoration and preserved invalid-state startup. It also records the guarded onboarding exception, intentional-quit seam repair and setup route remount. These are actual application paths, with explicit exclusions for file-picker/CLI packaging, signed store, accounts and ordinary input.

The user confirms all three ordinary-app first-unit procedures passed, completing check 40. The [second 2D packaging audit](NATIVE-PACKAGING-AUDIT.md) adds actual CLI build/doctor/pack, same-version package replacement, preserved legacy artifact conflicts, original installed approval refusal through an override/restoration round trip and exact saved installed-record preservation. Final installation cases passed twice after audit; the deliberate stale-package fault was detected with cleanup Pass. Checks 28/39 are reviewed automated Pass, bringing the ledger to 25 Pass / 28 Pending. The maintained builder stamps the CLI separately; only this scenario requires that verified stamp. That packaging snapshot left signed-store schedules and native account/registry recovery open; the signed-store unit below supplies subsequent evidence. No further microphone batch is requested now.

The [third 2D signed-store audit](NATIVE-STORE-AUDIT.md) adds an owned ephemeral signed catalogue/artifact fixture and a fixed test publishing anchor confined to the guarded isolated debug host. Three cases cover actual Store close/offline UI, signed installation/consent/Agent greeting, held disable/removal including post-mutation restart and developer override, and corrupt hash/signature/close refusal. All 28 cases passed before expanded mutation assertions; the three final cases passed twice afterward, and a deliberate unclosed Store failed with cleanup Pass. Checks 18/36 are reviewed automated Pass: **27 Pass / 26 Pending**. Whole 2D remains open for remaining registry/account recovery; production roots rotation/live publishers/vaults remain excluded. No new microphone/sign-in batch is needed now.

**Testing-first correction (1 October):** further platform feature implementation is paused until testing/auditing of the existing implementation is complete. Expand this harness only to exercise existing behavior and fix reproduced failures. The [registry audit](NATIVE-REGISTRY-AUDIT.md) adds normal/disabled/quarantined state and opaque-pointer restart assertions, malformed/future/identity/oversized startup refusal, and actual Windows failed-save schedules. Account pointers are synthetic nonsecret metadata without credential entries; they do not close live OAuth/account tests. The actual lock-helper cancellation self-test raises harness self-test coverage to 16 cases.

The final registry unit passed all 30 real-app cases in `run-I1qAql`, followed by all three registry cases in clean profile `run-CfCnvH`. Deliberate pointer loss was detected with cleanup Pass; check 49 is reviewed automated Pass: **28 Pass / 25 Pending**. The [audit](NATIVE-REGISTRY-AUDIT.md) preserves failed oracle assumptions and two unattributed intermittent Escape observations, despite later successful Escape runs. Whole 2D remains open for native account-owner testing; this testing sprint adds no application feature and does not resume platform improvements.
