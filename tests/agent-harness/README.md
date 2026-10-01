# Grain Agent acceptance harness

This is a persistent **Agent** harness. Its first real-application suite covers native tools and lifecycle. It runs Grain's actual Rust/Tauri host, actual Agent React panel, real WebView2 extension workers, selective tool discovery, host confirmation and continuation. It connects Playwright to those owned WebViews; it never launches a browser replica or replaces Tauri APIs.

The local scripted model implements the normal OpenAI-compatible HTTP interface. It reliably requests search → selected schema load → one action → result, without a paid model or an external account. It checks that the initial frame contains no action schemas and exactly one requested action is offered after loading. This proves Agent mechanics with controlled decisions, not a live model's judgment.

## Quick start on Windows

Prerequisites: the repository's normal native build toolchain and dependencies, Node 22+ with WebSocket support, a 64-bit Windows desktop/PowerShell session, installed WebView2, and the Windows SDK `mt.exe` for backend library tests. Run `npm install` using the project's normal dependency instructions if `node_modules` is absent. No extra browser download, provider API key or OAuth sign-in is required.

From the repository root in PowerShell:

```powershell
# Build the separate debug-only host and its embedded application assets.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1

# List scenarios, then run the short smoke suite.
npm run test:agent -- --list
npm run test:agent -- --suite smoke

# All native lifecycle/Agent cases, including smoke.
npm run test:agent -- --suite lifecycle

# Native deadlines, reply errors, input refusal and result/wire budgets.
npm run test:agent -- --suite native-failures
npm run test:agent -- --suite native-installation

# Real production idle timing; allow about five minutes.
npm run test:agent -- --suite idle

# Target one scenario after a change.
npm run test:agent -- --scenario native.hot-reload

# Production-logic tests, separately classified from real-app evidence.
npm run test:agent:logic -- --group all
npm run test:agent:logic -- --group mcp
npm run test:agent:logic -- --group registry
npm run test:agent:logic -- --group execution

# Test the harness's own assertions/reporting.
npm run test:agent:harness
```

The builder prints the executable path. By default the runtime runner finds it through Cargo metadata. `--binary C:\path\grain-agent-harness.exe` selects another copy of the exact stamped binary; ordinary `handy.exe` is refused. Rebuild after changing application sources. The runner checks the executable hash and source fingerprint against `.build/host.json` before launching. A build's Git commit and the run's current dirty files are recorded independently.

Builds and backend compilation should finish before runtime suites start. On Windows, a running executable can lock native DLLs in the shared Cargo target directory; this was observed when compiling another test configuration while the harness was open. If a build reports `os error 32`, close the application using that debug target and rerun the build. Installed Grain instances using other directories can remain open. The runtime runner does not rebuild automatically or terminate ordinary application processes.

Grain's ordinary app may remain open: the harness has its own identifier/profile and event port. Run only one runtime harness at a time because its loopback event port is **17124**. If occupied, the run reports Blocked. The normal app continues to use **7124**. Harness windows appear and take focus during the suite; use an idle desktop or dedicated Windows CI desktop for uninterrupted runs.

Definitions remain in this folder. Each run creates `.runs/run-*` with disposable `data/` and `fixture/` directories. At completion those scratch directories are removed and `evidence/report.json`, `report.md`, a bounded redacted `host.log`, and any failure screenshot remain. `--output C:\path\reports` changes the parent; every run still gets a unique child. Build/run artifacts are ignored by Git. Do not commit them: future live-account evidence may contain private details despite redaction.

Runtime reports also identify the Node/WebView adapter version and an independent hash of runner/fixture definitions. Changing those definitions during a run invalidates its result. Changes to documentation alone do not require an application rebuild or invalidate a run.

## Scenarios and precise coverage

| ID                        | What is asserted                                                                                                                                                          |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.cold-warm`        | No action schema initially; selected action only; no dispatch before approval; one accepted result; subsequent call reuses the worker.                                    |
| `agent.decline`           | Clicking Cancel releases approval without native dispatch; a fresh request still works.                                                                                   |
| `agent.typed-approval`    | Typing `yes` into the actual follow-up field resumes the held call once.                                                                                                  |
| `agent.stale-approval`    | Changed source reloaded through the production developer socket invalidates the old displayed approval; a fresh approval calls the new code.                              |
| `agent.escape-slow`       | Native Windows Escape with the owned Agent focused closes it during a slow call, suppresses late success/replay and allows a fresh call.                                  |
| `agent.reopen-escape`     | Ten close/reopen pairs preserve native Escape and discard pending approvals without dispatch; a final fresh greeting works.                                               |
| `agent.close-pending`     | The real panel close button clears pending approval without execution.                                                                                                    |
| `agent.close-model`       | Closing during a delayed HTTP model reply releases the Agent run and cancels that provider request.                                                                       |
| `agent.close-slow`        | Closing during a dispatched slow tool retires its worker; no accepted late success or automatic second dispatch; fresh call recovers.                                     |
| `native.ten-replacements` | Ten unload/load/greeting cycles get distinct worker identities and return workers/tokens to the initial baseline.                                                         |
| `native.disable-slow`     | Disable/re-enable interrupts the old slow call without later cleanup destroying its replacement.                                                                          |
| `native.hot-reload`       | Two idle reloads, an in-flight reload, and a disabled reload use the actual authenticated developer WebSocket; new source results appear and disabled state is preserved. |
| `native.real-idle`        | The production 120-second idle threshold and 30-second reaper retire the worker; a new call recovers; a slow call started around 110 seconds survives.                    |
| `native.reply-failures` | Declared/thrown errors, conflicting result envelopes and an actual lost socket reply have distinct host classifications; private error text stays hidden, and fresh calls recover without replay. |
| `native.readiness-failure` | A real worker closes before authenticating; startup fails without dispatch, worker/token/supervisor retire, and corrected source works on a fresh request. |
| `native.source-drift` | Warm source changes with an unanswered approval, without reload; obsolete approval is refused and the first fresh approved call uses the new worker/source. |
| `native.absolute-deadline` | A real 25-second handler fails around the production 20-second absolute budget, measured from host invocation and approval; late success/replay is suppressed and recovery survives old cleanup. |
| `native.result-budgets` | Replies exceeding the 64 KiB decoded budget and 512 KiB raw wire budget fail at distinct boundaries; workers/tokens/supervisors retire, then fresh native calls recover. |
| `native.invalid-input` | Unknown keys, missing required values, wrong types, oversized arguments and malformed JSON fail before approval/startup; private rejected parameter keys stay hidden. |

`smoke` contains the first two; `lifecycle` includes smoke and the original remaining non-idle lifecycle cases; `native-failures` contains the six Block 2C scenarios above. `native-installation` contains five native consent/package/restart cases. `all` includes all twenty-four. `idle` runs only the long timing case. Scenarios set up their own fixture baseline and clean it up. A failed scenario stops the batch; later scenarios are Not run rather than being assessed against contaminated state.

Reports identify supporting numbered checks in the existing extension plan. These links do not certify the whole numbered procedure or rewrite its ledger automatically. After reviewing complete real-app procedure coverage and its focused audit, record an automated Pass explicitly with its evidence class. Human results remain separately identified. The harness still excludes microphone/native-pill observations, including the ordinary-app portion of check 34; the user has separately completed that [2C audit handoff](../../docs/Extensions%202.0/NATIVE-FAILURE-AUDIT.md). The current ordinary-app handoff is in the installation audit.

The installation suite covers real permission-dialog Cancel/Allow, cancelled/enabled/disabled state across actual restarts, stale reviews and call approvals, same-version imported bytes, developer A/B replacement and installed restoration, and preserved/refused invalid registries. [Block 2D first-unit audit](../../docs/Extensions%202.0/NATIVE-INSTALLATION-AUDIT.md) maps these five cases to their partial numbered coverage and remaining ordinary/store/account prerequisites. Native imported fixtures go through the actual SDK and guarded production import command; that setup bypasses the OS file picker and CLI packaging, not the permission dialog or Agent approval.

Restart launches a new owned host only after the previous one exits with code zero and both its event/CDP endpoints close. Session-tagged host observations preserve evidence across process lifetimes with a 4096-event aggregate ceiling. The test-build onboarding resolver skips ASR setup only in the guarded isolated host, so these tests need no model download or microphone. Tests navigate the existing management route because embedded production assets hide its developer nav item. Normal onboarding/navigation and UI components are unchanged.

```powershell
npm run test:agent -- --scenario native.consent-persistence --fault skip-restart
```

This withholds the actual restart. Expect the new-host-session assertion to fail, exit 1 and cleanup Pass. It is accepted only for this scenario; no normal tool or scenario retries are added.

For the ordinary-app import/consent batch, run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/prepare-installation.ps1`. It prints a permission-free package with one confirmed tool. Follow the three observations in the audit. This disposable package intentionally lacks a submission icon; it is not a CLI/store packaging certificate.

The production-logic runner filters the normal backend library tests into `agent`, `native`, `execution`, `mcp` and `auth` groups, plus the core registry tests in `registry`, imported-update boundaries in `imported` and developer-project validation in `developer`. `all` includes all eight. It requires a successful Cargo exit **and at least one actual passing test**; a zero-test filter fails. The registry group includes real Windows file-sharing locks: a short lock must recover without replaying serialization; a persistent lock must fail within the bounded retry policy, preserving saved state and unowned recovery files. These tests use production helpers/SDK and local fixtures, not complete real-application or live-account acceptance. The Windows manifest runner is configured as a Cargo argv array so repository paths containing spaces remain usable.

## Verify the harness detects a real failure

```powershell
npm run test:agent -- --scenario native.cold-warm --fault wrong-greeting
```

This deliberately changes only the disposable greeting fixture to return the wrong text. Expect **Fail**, process exit **1**, no subsequent scenario execution, and cleanup Pass. The same scenario without `--fault` should pass. Never count this intentional negative run as a passing product test.

To check the native Escape oracle itself:

```powershell
npm run test:agent -- --scenario agent.reopen-escape --fault missing-escape
```

This withholds the OS key event from the real app. Expect the eight-second window-destruction assertion to fail, exit 1 and cleanup Pass. The scenario does not approve any tool before that failure. This fault is accepted only for the named scenario. Ordinary runtime failures still stop the batch; the runner does not retry them.

To check the outcome oracle:

```powershell
npm run test:agent -- --scenario native.reply-failures --fault successful-error
```

This deliberately turns the disposable error tool into a success. Expect the host-classification assertion to fail, exit 1 and cleanup Pass. Only that named scenario admits this fault; normal tool calls are never retried.

For the residual ordinary-app check, run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/prepare-manual.ps1`. It prints a disposable, permission-free folder with only two declared tools. Load it through Developer in the ordinary app; the [accepted Block 2C procedure](../../docs/Extensions%202.0/NATIVE-FAILURE-AUDIT.md) retains the three observations for reproduction. The runner skips microphone/native pill, so a successful native recovery cannot certify that live input path.

Exit codes: **0** = every selected scenario and cleanup passed; **1** = assertion, application or cleanup failed; **2** = a prerequisite blocked execution or selected scenarios were not run. Unsupported operating systems report Blocked rather than using a browser substitute.

## Isolation and security boundary

- Cargo feature `agent-harness` is off by default. The maintained builder opts into it only for the separately named `grain-agent-harness` target. The feature adds limited controls to the shared backend library; startup refuses the ordinary app identifier and requires `com.grain.agent-harness` plus the isolated marker/profile. Debug assertions are compulsory, so default release compilation rejects this feature.
- Before Tauri starts, the host requires an absolute existing root containing a bounded versioned marker and valid run UUID. A Grain-owned portable-compatible module redirects application data, settings/secrets, model caches, history, logs and WebView caches into that run's `data/`. The Handy-derived portable module is unchanged.
- Native/MCP credential service names receive a per-run harness suffix. No production credentials are copied. This first runtime suite creates no OAuth credentials; OS-vault write/recovery certification remains future work.
- The harness skips the native pill supervisor, single-instance forwarding, autostart application and automatic updater checks. It clears default shortcut bindings and disables Agent context/screen capture/autocopy in its own profile. It uses no microphone, selection capture or external account. This bounds interference with the user's ordinary app and defines what the test excludes.
- The test controls admit only fixed, permission-free harness fixture projects A/B and a fixed package under the run root. Installation cases register/import without granting; their actual permission sheets handle installed approval. They use production validation, registration, grant/enable/disable/unload operations. The fixture's installation approval is explicit test setup; **individual action approval still goes through the actual Agent UI and host executor**. There is no control to run an arbitrary action, provide an arbitrary path, read credentials, or approve a tool directly.
- Developer reload uses the existing role-bound DevControl token and protocol. That token remains reload-only; it was not given extra powers. It is read only from the owned profile and is never included in the report.
- The WebView2 debugging endpoint is loopback-only and confined to the child process. Playwright activates the real enabled DOM buttons through their production handlers to avoid coordinate races during native resize/entrance animation. This verifies functional activation, not physical mouse targeting or accessibility quality.
- Escape is a native shortcut, so `native-input.ps1` sends one Windows key down/up pair after verifying the visible foreground `Grain Assist` window belongs to the owned host PID. Test startup owns the production HandyKeys manager with no ordinary accelerators registered and keeps explicit blank binding entries to prevent default fallbacks. The normal initialization command is checked for idempotence. It refuses native input if ownership/focus cannot be established. Keep the ordinary app's Agent closed to avoid competing transient shortcuts.
- Typed approval fills the real Follow up field and dispatches Enter to its production React key handler. Like button activation, this is a functional DOM test; it does not certify physical Enter delivery during native resize. Native Escape is tested separately at the OS boundary.
- Test-only observations record invocation/startup, queue admission (`dispatched`), accepted replies (`completed`), host outcome classification (`outcome`) and retirement. A completed reply is not necessarily a successful action; the outcome assertion distinguishes errors and unusable envelopes. Worker identifiers are one-way token digests; raw tokens/arguments/results are absent. Queue admission is conservative dispatch evidence, not proof of the extension's external effects. The finite buffer must not overflow.
- Each IPC operation and observation has a deadline. Cleanup requests app exit, falls back to the owned PID/tree if necessary, verifies listener/debug endpoint release, closes the model fixture and removes only owned scratch paths. Cleanup failures fail the run. Ctrl+C cancels the run and follows teardown; it must not be treated as a passing partial batch.

## Add another Agent scenario

1. Add a stable ID, suite, description and supporting-check links in `scenarios.mjs`. State precisely which paths are excluded.
2. Add its handler in `run.mjs`. Reuse `request`, `greeting`, `slowStart`, `status`, the production developer socket and scoped fixture operations. Assert an independent observable outcome; do not assert only that the request returned without throwing.
3. Keep fixtures harmless and versioned. If another adapter needs credentials or writes, introduce its isolated namespace and owned cleanup before automating it. Do not broaden the test controls to arbitrary application access.
4. Add an intentional failure case or other test of the new oracle. Run from two clean profiles and inspect JSON evidence as well as the readable summary.
5. Review the changed production paths and the harness's test assumptions. Update this README and the maintained plan/progress documents. Keep automation evidence distinct from human acceptance.

Next suites can cover controlled MCP transport, a native+MCP task, forced model misbehavior, human-assisted real OAuth consent, account replacement, live-model selection, actual summon/global shortcuts and microphone/pill behavior. The current runner is not a certification of those paths.

Implementation/design references: [Playwright WebView2 automation](https://playwright.dev/docs/webview2), [Microsoft WebView2 environment options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl), [Tauri's distinction between mock-runtime and real-application tests](https://v2.tauri.app/develop/tests/), [Windows SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) and [foreground window restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow). These support the adapter choice; Grain's isolation and evidence policies are local requirements.
