# Persistent Agent acceptance harness

**Created:** 1 October 2026. **Status:** initial Agent harness implemented; current verification evidence is recorded in the progress companion. [Maintained runner instructions](../../tests/agent-harness/README.md) describe actual commands, coverage and limitations.

**B1a accepted (1 October):** `native-foundation` added actual legacy migration and typed native contract cases, bringing the inventory then to 32 distinct scenarios. [Focused audit](NATIVE-FOUNDATION-AUDIT.md) records seven verified typed payloads, stale-declaration refusal without reload, six actual migration restart transitions, the reproduced terminal-owner quarantine repair, repeated acceptance and negative oracles. Checks 2/22 are reviewed automated Pass.

**B1b fixture prerequisite accepted (2 October):** `native-auth` adds the separately guarded `native.auth-fixture`, bringing the maintained inventory to **33 distinct scenarios**. It exercises the real permission dialog, native OAuth/PKCE, run-scoped Windows vault, approved A/B API reads, stale approval refusal, restart and disconnect. The [focused audit](NATIVE-AUTH-FIXTURE-AUDIT.md) records the reproduced disconnect-worker repair, repeated acceptance and wrong-account/abandoned-credential oracles. The latter proves independent cleanup deletes an actual test credential. No normal credential or OS trust store is modified; the original permission-free fixture stays strict. Full 41/45/53 and B1c schedules remain pending: the ledger stays **30 Pass / 23 Pending**. No new human batch is required for this prerequisite. Historical inventories/results below retain their original counts.

Companion to [the execution plan](MCP-EXTENSION-PLAN.md) and [the progress ledger](MCP-EXTENSION-PROGRESS.md). The user requested reproducible **Agent** testing, initially covering native extension lifecycle/tool calling and later deeper Agent, MCP and authentication behavior. Human participation remains where account sign-in or practical judgment is needed. This work supports the existing sequential acceptance blocks; it does not create another extension feature roadmap.

**B1b numbered unit accepted (2 October, following the fixture prerequisite):** `native.auth-binding` and `native.auth-owners` bring the maintained inventory to **35 scenarios**. The [focused audit](NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md) accepts 41/45/53 after two combined final runs, actual CLI builds, exact account/source verification, real vault declaration/legacy refusal and independent parked-grant deletion counts. Both new negative oracles fail as intended; cleanup and existing hot-reload/smoke retests pass. Current ledger **33 Pass / 20 Pending**. No manual batch is needed for this controlled unit; live providers, browser UX, production discarded-grant reconciliation and B1c remain open. Earlier inventories below are historical.

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

## Native input follow-up — 1 October 2026

The [focused input investigation](NATIVE-INPUT-INVESTIGATION.md) records a reproduced foreground-request denial with zero key insertion, separate from the earlier accepted-key timeout. The adapter emits bounded owned-window/held-key/count/timing observations, including on failure, without foreign titles, PIDs or typed content. Before Escape it verifies owned foreground identity; when direct activation fails it can perform one real click on an unobstructed owned empty header. The helper uses physical virtual-desktop coordinates, rechecks PID/title/point ownership before input, and restores its DPI context. The pointer stays at that header. Held inputs are refused, partial synthetic holds get a matching release attempt as failure cleanup, and no key/tool/scenario replay is introduced.

Use `node tests/agent-harness/run.mjs --scenario agent.reopen-escape --focus-click` to explicitly test ten guarded header-click/Escape pairs. This test setup does not certify production foreground behavior. A missing-owned-window self-test verifies zero input plus diagnostic refusal. There are now seventeen self-tests, thirty real-app scenarios and nine production-logic groups. Retained failures and differing runner hashes remain explicit; the accepted-input timeout still needs attribution. No new human testing is requested for this harness-only follow-up.

**2 October B1c partial acceptance:** [Native sign-in/switch/expiry audit](NATIVE-AUTH-SCHEDULE-AUDIT.md) accepts 42/43/50/51 after six actual cancellation schedules, real account switch/failure/restart reads, partial-consent rejection and actual expiry/reconnect/refresh. The maintained inventory is now **39 distinct scenarios**; `native-auth` contains seven and `native-auth-schedules` selects the four new cases. Full affected suite, repeated new subset and both deliberate faults have expected verdicts and cleanup Pass. Current ledger: **37 Pass / 16 Pending**. Native check 44, live accounts, orphan reconciliation, retained input observations and all release gates remain open; no new manual batch is assigned.

**2 October B1c final numbered acceptance:** [Native refresh/logout audit](NATIVE-AUTH-REFRESH-AUDIT.md) accepts 44 using the actual production auth/vault path, Agent-approved interrupted reads and same-host independent provider reads. The fixed auth concurrency probe returns only null after dropping its zeroizing result; it does not enable parallel Agent sessions or direct tool execution. Exact peer admission/TLS routes and finite consent/receipt controls remain debug-only. The maintained inventory is **40 scenarios**, including **eight native-auth**; the schedules subset remains four. Full affected suite, final clean repeat and ordinary smoke pass; deliberate early release fails as required, cleanup Pass. Three earlier driver/oracle failures remain recorded. Current ledger: **38 Pass / 15 Pending**. All B1 numbered requirements are accepted; retained input findings, live accounts, production orphan reconciliation and all whole release gates stay open. No new manual batch; next B2.


## MCP catalog/transport unit - 2 October 2026

The [focused audit](MCP-CATALOG-TRANSPORT-AUDIT.md) accepts numbered check 23. The maintained inventory is **42 distinct scenarios**: two new MCP cases, eight native-auth cases, four in the native-auth-schedules subset, and the retained native/Agent/store cases. There are 21 runner self-tests. Current ledger: **39 Pass / 14 Pending**; whole B2 and every release gate remain open.

`--suite mcp` starts an owned HTTPS peer and drives the actual Agent search/load/approval/executor. The fixed debug-only provider requires an optional `mcpPort` marker field, exact loopback endpoint and separately generated CA/server leaf under the disposable `mcp-tls/` directory. It stores no grant and refuses account commands. The new TLS trust applies only to this fixed client; ordinary providers retain their defaults. Native auth still uses its separate established TLS fixture. All temporary keys/profiles and owned listeners are removed on success or failure.

- `mcp.transport-contract`: modern discovery and legacy handshake, JSON/SSE, two-page nested reads, selected-tool exposure, exact request/result types, exactly one approved provider call, session deletion, real restart and stale disable refusal.
- `mcp.mixed-catalog`: supported/excluded pages and developer warnings, an explicit excluded-tool load with zero dispatch, empty supported catalog, repeated cursor refusal and stale schema approval/fresh recovery.

```powershell
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.transport-contract --fault wrong-mcp-type
node tests/agent-harness/run.mjs --scenario mcp.mixed-catalog --fault supported-mcp-excluded
```

Build first using `tests/agent-harness/build.ps1`, with Python `cryptography` available; run real suites serially and after native compilation finishes. Both fault commands must fail with their intended assertion and cleanup Pass. Final accepted runs `run-gysFYv` / `run-C2iw9m`, fault runs, production tests and native/smoke regression identities are in the audit. This controlled peer supplies supporting evidence for 8/17 and full evidence for 23; it supplies no OAuth/account-persistence, oversized-response, slow-call, live-provider or official-conformance certification. Those are the next B2/B3 requirements. No additional manual batch is required now.

## Harness retention and cleanup inventory

**Current maintained coverage (2 October):** [MCP bounds/cancellation audit](MCP-BOUNDS-CANCELLATION-AUDIT.md) adds `mcp.response-preview`, `mcp.transport-bounds` and `mcp.close-cancellation`. There are **45 scenarios**, five MCP, and 22 runner self-tests. Final full MCP runs `run-mnHybt` / `run-Rc2B9N` each pass five separately reported procedures and 52 supporting stages, with 46 actual calls, zero retained sessions/held calls and cleanup Pass. The deliberate `short-mcp-preview` fault fails as required; ordinary smoke and 46 production MCP tests pass. The audit preserves the early UI-oracle and failed-report scope errors. Checks 20/31 remain Pending for their complete live/account requirements; **39 Pass / 14 Pending** is unchanged. No new user batch or broader platform work starts here.

```powershell
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.response-preview --fault short-mcp-preview
```

Retain `tests/agent-harness/` as regression infrastructure. Its scripted model and external native/OAuth/store/MCP fixtures exercise the real application; they are not alternate application engines or production dependencies. Retain stable scenario IDs, failure assertions and privacy-safe evidence when adapters change. Remove a fixture only after equivalent final-path coverage exists and its scenario/documentation references are removed together.

| Item | Lifetime / required cleanup | Retirement trigger |
| --- | --- | --- |
| Run-local profile, marker, TLS CA/leaf/private keys, scoped credentials, packages and registry locks | Owned by one run; runner `finally` removes them on success or failure and records cleanup failures. Never reuse a personal profile or credentials. | Every run, immediately after evidence collection. |
| Owned application, model/provider processes, sockets, listeners and timers | Explicit shutdown; owned ports must close. No surviving MCP sessions or held response handles. | Every run; cleanup failure makes the run unsuccessful. |
| Feature-gated Rust commands, fixed MCP endpoint/CA adapter and finite instructions | Debug harness only; preserve existing application/window/profile/marker guards. No arbitrary endpoint, direct executor, real-account bypass or release feature. | Replace/remove when maintained real-app automation can cover the same path without the seam. Until then these are intentional test support. |
| Closed MCP response handles used for deliberate late replies | Bounded to four handles; consumed immediately by `attemptLateReply()`. End-run cleanup drops handles even if an assertion fails. | After each late-reply assertion, or failed-run cleanup. |
| `.build/` assets, CLI/host identities and compiled executable | Local rebuild cache, ignored by Git; source and runner fingerprints distinguish evidence. | Regenerate when fingerprints change; removable when no owned test process is using them. |
| Privacy-safe reports and failed-run evidence | Retain for comparison and audit; no keys, grants, private prompts or personal content. | Explicit evidence-retention policy, never silently overwrite failures. |

No unused scaffold is intentionally deferred by the MCP bounds/cancellation batch. Known **production** cleanup work remains separate: discarded native grant/orphan reconciliation, retained input investigation and the later physical retirement sweep in the execution plan. A passing end-run credential cleanup does not close production orphan reconciliation.

Batching policy: compile stable edits once, then execute cases serially with distinct IDs and per-stage observations. Each affected case verifies a fresh request after failure/cancellation. Stop the batch on its first failure; preserve failed and Not run verdicts. Repeat the audited batch in a clean profile. Never shorten production deadlines, manufacture acceptance from aggregate results, or treat controlled fixtures as live-provider/conformance certificates.
