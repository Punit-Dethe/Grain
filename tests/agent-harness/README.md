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

# Legacy upgrade/restart preservation and typed native tool contracts.
npm run test:agent -- --suite native-foundation

# Native OAuth, scoped Windows vault, approved A/B reads and restart.
# This suite additionally requires Python with cryptography installed.
npm run test:agent -- --suite native-auth

# Signed local store, real Store page, offline and interrupted downloads.
npm run test:agent -- --suite store

# Existing registry state, invalid startup and real failed saves.
npm run test:agent -- --suite registry-recovery

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

`smoke` contains the first two; `lifecycle` includes smoke and the original remaining non-idle lifecycle cases; `native-failures` contains the six Block 2C scenarios above. `native-installation` contains six native consent/package/restart cases. `native-foundation` contains the two B1a cases below. `native-auth` contains seven guarded native-account cases below; `native-auth-schedules` selects the four B1c cases. `store` contains three signed-store cases below. `registry-recovery` contains the existing `native.registry-refusal` plus two new cases below. `all` includes all thirty-nine distinct cases; overlapping suite selections are not additive. `idle` runs only the long timing case. Scenarios set up their own fixture baseline and clean it up. A failed scenario stops the batch; later scenarios are Not run rather than being assessed against contaminated state.

### Native foundation (B1a)

| ID | Actual application assertions |
|---|---|
| `native.typed-contract` | Seven text/number/entity and optional/null/zero cases round-trip through the actual worker and are verified by the scripted model. Each requires approval and one dispatch. Changing a valid declaration without reload refuses the old approval with zero dispatch; a fresh approved call works. Native declarations retain the four-parameter limit. |
| `native.legacy-migration` | Legacy capture grants, edited prompts/bindings and an interrupted archive checkpoint migrate through actual startup. Six restart transitions preserve inert edits/artifacts/user data, refuse re-enable and start no worker. A real Windows registry lock preserves the owner after failed uninstall; terminal installed/developer removal and fresh valid tools recover. |

The [focused audit](../../docs/Extensions%202.0/NATIVE-FOUNDATION-AUDIT.md) records checks 2/22, the reproduced stale-quarantine repair, review and repeated evidence. Legacy profile fixtures are changed only between owned host lifetimes; the changed-declaration test edits only its owned manifest. This is not power-loss, ordinary-profile upgrade or authenticated-account certification. The permission-free fixture gate remains intact; authentication uses the separate fixture below.

```powershell
npm run test:agent -- --scenario native.typed-contract --fault stringified-number
npm run test:agent -- --scenario native.legacy-migration --fault lost-migration-archive
```

Both deliberately corrupt actual disposable output/state and must give Fail, exit 1 and cleanup Pass. Each fault is accepted only with its named scenario. Typed verification is derived from real returned JSON; diagnostic journals retain a verification flag, not the argument/result payload.

### Guarded native account fixture (B1b prerequisite)

`native.auth-fixture` imports only `com.grain.harness.auth` from its owned package and uses the actual permission dialog. It runs production OAuth/PKCE, the loopback callback, token exchange, run-scoped Windows credential vault and host-authenticated network broker. The controlled HTTPS provider supplies accounts A/B. Actual returned JSON verifies an approved A read, stale approval refusal after switching to B, a fresh B read and B restoration after an owned restart. Disconnect must retire its worker; uninstall and end-of-run vault cleanup must return to baseline. The [focused audit](../../docs/Extensions%202.0/NATIVE-AUTH-FIXTURE-AUDIT.md) records the reproduced disconnect-worker repair and final evidence.

This suite requires Python with `cryptography`; missing prerequisites report Blocked. The runner generates a short-lived certificate/key inside its disposable root. Trust is scoped to the fixture's exact localhost token/API endpoints; normal certificate/hostname validation stays enabled, and no OS trust store is changed. Only runs selecting the native account scenarios set `authPort`. Other suites explicitly verify that the auth control refuses access. The original permission-free loader still refuses authentication and nonempty permissions.

```powershell
npm run test:agent -- --scenario native.auth-fixture --fault wrong-account
npm run test:agent -- --scenario native.auth-fixture --fault abandoned-auth
```

Both commands must report Fail, exit 1 and cleanup Pass. The first corrupts the actual API reply so the account oracle fails. The second deliberately abandons one real test credential; independent cleanup must report `nativeVaultCleanup.deleted: 1` and `remaining: 0`. Cleanup enumerates only metadata in the exact run service and fixed fixture key family; it never reads/prints credential blobs. Provider journals contain bounded account/stage/counter information, not tokens, authorization URLs or private payloads. TLS files and owned scratch state are removed after every run, including failures.

The prerequisite alone credits **no numbered checks**. The subsequent [binding/owner audit](../../docs/Extensions%202.0/NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md) accepts 41/45/53 using the two additional scenarios below. Checks 42–44/50–51 callback/refresh/logout, failed-switch, scope and expiry schedules remain pending. Controlled consent does not certify browser UX, live-provider compatibility or other OS vaults.

| Scenario | Precise real-app coverage |
|---|---|
| `native.auth-binding` | Expired unbound session and historical id-only vault grants refuse tool loading/read/refresh before and after restart. Explicit reconnect yields actual verified reads across restart. Four reviewed declaration changes (client/token endpoint/scopes/API hosts) refuse old approvals, require reconnect and use fresh correctly configured grants. |
| `native.auth-owners` | Actual Grain CLI pack/doctor for developer A/B, installed A versus developer B accounts, same-directory registration and production WebSocket reload, new-directory disconnect, stale approval refusal, restart and installed restoration. Parked uninstall independently deletes exactly one credential while the developer account remains usable. Actual returned JSON verifies both account and source owner. |

```powershell
npm run test:agent -- --scenario native.auth-binding --fault unchanged-auth-declaration
npm run test:agent -- --scenario native.auth-owners --fault wrong-owner
```

These new faults must give Fail/exit 1/cleanup Pass. The first reimports an unchanged declaration and must fail the expected reconnect assertion. The second changes actual compiled developer output and must fail source-owner verification. Faults are limited to their named scenarios. The model accepts a missing action schema as a negative result only for the exact actual native account refusal; generic unavailable output is never accepted.

Historical credential setup is fixed to the installed fixture in the explicitly enabled, idle harness; it never accepts or returns tokens/keys/namespace/path. It uses the actual run vault and expires the deliberately unbound grant so a mistakenly accepted refresh would be detectable. Finite declaration variants add only a fixed second public client/token path/read scope or matching localhost API/network permission. No arbitrary endpoint/client/scope or normal trust exception is allowed. Vault inventory reports only a bounded count in the exact run namespace, not keys or blobs.

The accepted combined runs still needed final independent deletion of three discarded developer grants. Their old sessions are not available to the selected owner, but ordinary-app orphan reconciliation remains open in R3. End-of-run harness cleanup does not certify production crash/orphan recovery. The next unit is B1c, followed by its focused audit and repeated acceptance.

### Native sign-in/switch/expiry schedules (B1c partial)

| ID | Actual application assertions |
|---|---|
| `native.auth-cancellation` | Actual CLI developer project and WebSocket reload, disable and Disconnect at callback/held exchange boundaries; six production cancellations, exact vault counts, actual late callback refusal/reusable ports, fresh reads/restarts. |
| `native.auth-switch` | Disconnect A/connect B refuses the old Agent approval without dispatch; fresh actual B read/restart succeeds. |
| `native.auth-failed-switch` | Explicit pending-flow cancellation, provider denial and actual token HTTP failure preserve A/count across reads/restarts; subsequent successful B switch refuses stale A approval. |
| `native.auth-expiry` | Actual partial consent rejected without replacing A; real two-second grant without refresh requires reconnect before/after restart; actual expiry with refresh performs one exchange/read, zero new logins and no extra restart refresh. |

The [focused audit](../../docs/Extensions%202.0/NATIVE-AUTH-SCHEDULE-AUDIT.md) accepts checks 42/43/50/51 only. Check 44 still needs refresh/logout and two independent native provider identities. Live-provider/browser, orphan reconciliation and whole authentication certification remain open. No clock/production-timeout override or vault edit is used to accelerate these cases; observed barriers and actual short-lived provider grants retain the required assertions. `native.auth-cancellation` requires the stamped actual CLI, like the owner case.

```powershell
npm run test:agent -- --suite native-auth-schedules
npm run test:agent -- --scenario native.auth-cancellation --fault uncancelled-login
npm run test:agent -- --scenario native.auth-expiry --fault accepted-partial-consent
```

The subset uses the same real-app runner/cleanup and avoids repeating unrelated cases. Both faults must give Fail/exit 1 and cleanup Pass. The harness-only Cancel operation calls production cancellation for the fixed fixture and preserves the prior selected account; it exposes no arbitrary target or approval bypass. The unchanged-installed-reimport driver failure is retained; developer reload is exercised through the actual CLI/project/WebSocket path.

Reports identify supporting numbered checks in the existing extension plan. These links do not certify the whole numbered procedure or rewrite its ledger automatically. After reviewing complete real-app procedure coverage and its focused audit, record an automated Pass explicitly with its evidence class. Human results remain separately identified. The harness still excludes microphone/native-pill observations, including the ordinary-app portion of check 34; the user has separately completed that [2C audit handoff](../../docs/Extensions%202.0/NATIVE-FAILURE-AUDIT.md). The current ordinary-app handoff is in the installation audit.

The first five installation cases cover real permission-dialog Cancel/Allow, cancelled/enabled/disabled state across actual restarts, stale reviews and call approvals, same-version imported bytes, developer A/B replacement and installed restoration, and preserved/refused invalid registries. The [first-unit audit](../../docs/Extensions%202.0/NATIVE-INSTALLATION-AUDIT.md) records their evidence and the user's accepted ordinary-app check 40. Native imported fixtures go through the actual SDK and guarded production import command; that setup bypasses the OS file picker, not the permission dialog or Agent approval.

The sixth case, `native.cli-package-ownership`, builds maintained fixture projects through the real `grain-ext pack` command and runs `doctor`. Its fixed npm script uses the repository's existing esbuild; it installs no dependencies and starts no watcher. The builder also compiles/stamps `grain-ext.exe`; this case refuses a missing, changed or obsolete CLI stamp before launching the host. Reports bind the CLI binary, source, package and icon hashes. All other suites remain independent of that stamp.

This case replaces a package without changing its id/version, checks actual distinct greetings, preserves an installed approval through a developer override/restoration and refuses it, then verifies developer A/B and installed ownership across restarts. Old flat and version-only package files are seeded only between owned host lifetimes and must neither shadow current bytes nor be deleted. See the [packaging audit](../../docs/Extensions%202.0/NATIVE-PACKAGING-AUDIT.md) for acceptance and exclusions. Signed-store schedules, OAuth/vault state, power-loss durability and nonempty extension-settings preservation are not certified by this permission-free fixture.

Restart launches a new owned host only after the previous one exits with code zero and both its event/CDP endpoints close. Session-tagged host observations preserve evidence across process lifetimes with a 4096-event aggregate ceiling. The test-build onboarding resolver skips ASR setup only in the guarded isolated host, so these tests need no model download or microphone. Tests navigate the existing management route because embedded production assets hide its developer nav item. Normal onboarding/navigation and UI components are unchanged.

```powershell
npm run test:agent -- --scenario native.consent-persistence --fault skip-restart
```

This withholds the actual restart. Expect the new-host-session assertion to fail, exit 1 and cleanup Pass. It is accepted only for this scenario; no normal tool or scenario retries are added.

The user has completed the ordinary-app import/consent batch. For reproduction, `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/prepare-installation.ps1` prints a permission-free package with one confirmed tool; the audit retains the three procedures. This disposable manual package intentionally lacks a submission icon; it is not a CLI/store packaging certificate.

```powershell
npm run test:agent -- --scenario native.cli-package-ownership --fault stale-package
```

This deliberately imports the original package instead of the compiled replacement. Expect the old greeting to fail the replacement-result assertion, exit 1 and cleanup Pass. The fault is accepted only for this fixed scenario; it adds no automatic product/tool retries.

The production-logic runner filters the normal backend library tests into `agent`, `native`, `execution`, `mcp` and `auth` groups, plus the core registry tests in `registry`, imported-update boundaries in `imported`, developer-project validation in `developer` and signed-store boundaries in `store`. `all` includes all nine. It requires a successful Cargo exit **and at least one actual passing test**; a zero-test filter fails. The registry group includes real Windows file-sharing locks: a short lock must recover without replaying serialization; a persistent lock must fail within the bounded retry policy, preserving saved state and unowned recovery files. These tests use production helpers/SDK and local fixtures, not complete real-application or live-account acceptance. The Windows manifest runner is configured as a Cargo argv array so repository paths containing spaces remain usable.

### Signed-store scenarios

| ID | What is asserted |
|---|---|
| `store.close-offline` | Ten actual Store loading/route-close/reopen cycles cancel held refresh HTTP responses and drop the resident index. The real Install button installs a verified package disabled, real consent and Agent greeting work across restart, cached offline browsing shows its notice and disables Update, and a direct backend install is also refused. The installed greeting remains usable. |
| `store.pending-mutations` | During held real artifact downloads, production disable and remove operations supersede the pending update. Disabled/removed choices survive actual restart. Both schedules repeat under a developer override; the parked installed record is preserved on disable and removed without breaking the active developer. Explicit subsequent installation/consent returns the new greeting across restart. |
| `store.integrity-close` | A same-length corrupt artifact fails its hash check; closing cancels a held artifact request; an invalid catalogue signature becomes offline and admits no artifact fetch. Corrected signed bytes install and produce the actual approved greeting. |

The runner starts an owned ephemeral loopback HTTP fixture only when a store scenario is selected. Its fixed public test seed signs modern minisign `ED` messages using Blake2b-512 prehash and a signed trusted comment. The separate debug host uses the matching **test publishing anchor only inside its validated isolated profile**. Normal publishing roots, mirrors and release initialization are unchanged. Cached index loading still passes through the production signature verifier and rollback-floor logic. No arbitrary endpoint/key or ordinary account is admitted. Model/store fixture ports cannot target the ordinary event listener on 7124 or collide with owned event/model listeners.

This certifies signed fixture installation and ownership through the production verifier/installer, not a published production extension or production root rotation. The fixture returns no roots-chain/revocation document. Accounts, OS vault recovery, root rotation, live publishing, power loss and physical file-picker/mouse behavior remain separate. The [focused store audit](../../docs/Extensions%202.0/NATIVE-STORE-AUDIT.md) records acceptance and exact build/run evidence.

```powershell
npm run test:agent:logic -- --group store
npm run test:agent -- --scenario store.close-offline --fault unclosed-store
```

The deliberate fault withholds navigation away from the real loading Store page. Expect the cancelled-response assertion to fail, exit 1 and cleanup Pass, including release of the owned fixture listener and held socket. It is accepted only for this scenario and never counts as a passing product run. No ordinary-app microphone or external sign-in step is needed for these fixed permission-free store cases.

## Verify the harness detects a real failure

### Registry recovery scenarios

| ID | What is asserted |
|---|---|
| `native.registry-refusal` | Malformed, future-version, inconsistent-identity and oversized registry startup preserve the file, refuse extension operations, record a diagnostic and spawn no worker. Restored bytes recover actual greetings through real restarts. |
| `native.registry-preservation` | Installed/disabled records and a synthetic opaque account pointer survive restart. A stopped-host corrupt artifact triggers actual startup quarantine; restored bytes do not silently clear it. Explicit validated import and real consent/greeting recover across restart. |
| `native.registry-save-failure` | Real Windows publication-sharing locks fail enable/disable saves. Enable rolls back; disable takes effect live but honestly remains unsaved. Actual restarts distinguish those outcomes. A successful explicit disable/re-enable recovers. |

The owned hidden `registry-lock.ps1` helper admits only the marked profile's fixed registry and refuses links. It permits read/write sharing and denies DELETE; it changes no file permissions/content. Its 30-second bound, parent-owned release/exit checks and cancellation cleanup are tested. The synthetic pointer has no vault entry and does not certify OAuth/account switching. This unit changes only test tooling; production application sources are unchanged. See the [focused registry audit](../../docs/Extensions%202.0/NATIVE-REGISTRY-AUDIT.md).

```powershell
npm run test:agent -- --scenario native.registry-preservation --fault lost-pointer
```

This deliberately removes the synthetic pointer only from the disposable stopped-host startup fixture. Expect pointer-preservation Fail, exit 1 and cleanup Pass. It is accepted only for this scenario and is not product Pass. `npm run test:agent:logic -- --group registry` separately covers production persistence helpers, including real Windows locks; it does not replace the real-application procedures above.

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
- Native/MCP credential service names receive a per-run harness suffix. No production credentials are copied. Ordinary fixtures create no OAuth credentials. The explicit `native-auth` fixture writes only its run-scoped native credential and independently verifies vault cleanup; broader crash/recovery and live-account certification remain future work.
- The harness skips the native pill supervisor, single-instance forwarding, autostart application and automatic updater checks. It clears default shortcut bindings and disables Agent context/screen capture/autocopy in its own profile. It uses no microphone, selection capture or external account. This bounds interference with the user's ordinary app and defines what the test excludes.
- The test controls admit only fixed, permission-free harness fixture projects A/B and a fixed package under the run root. Installation cases register/import without granting; their actual permission sheets handle installed approval. They use production validation, registration, grant/enable/disable/unload operations. The fixture's installation approval is explicit test setup; **individual action approval still goes through the actual Agent UI and host executor**. There is no control to run an arbitrary action, provide an arbitrary path, read credentials, or approve a tool directly.
- A separate debug-only auth control additionally requires the isolated main window/profile and explicit marker port. It fixes the package ID/path and one read tool, with finite permission/client/scope variants and exact owned HTTPS endpoints. Authorization handoff belongs to one pending flow and clears on completion/cancellation/drop. Its connect/disconnect/remove operations delegate production paths; actual permission and action review are never bypassed.
- Developer reload uses the existing role-bound DevControl token and protocol. That token remains reload-only; it was not given extra powers. It is read only from the owned profile and is never included in the report.
- The WebView2 debugging endpoint is loopback-only and confined to the child process. Playwright activates the real enabled DOM buttons through their production handlers to avoid coordinate races during native resize/entrance animation. This verifies functional activation, not physical mouse targeting or accessibility quality.
- Escape is a native shortcut, so `native-input.ps1` sends one Windows key down/up pair after verifying the visible foreground `Grain Assist` window belongs to the owned host PID. Test startup owns the production HandyKeys manager with no ordinary accelerators registered and keeps explicit blank binding entries to prevent default fallbacks. The normal initialization command is checked for idempotence. It refuses native input if ownership/focus cannot be established. Keep the ordinary app's Agent closed to avoid competing transient shortcuts.
- The Escape adapter refuses held Shift/Ctrl/Alt/Windows, Escape or left mouse input; it never releases user keys to force a test. `-ProbeOnly` reads modifier states without sending input. An idle/unlocked desktop is a prerequisite. If Windows denies focus, it can activate the owned Agent with one actual click on its empty header, only after checking that the header is unobstructed and its top-level window is the owned target. It rechecks owned PID/title and point/root ownership immediately before clicking and verifies foreground ownership before Escape. A temporary per-monitor DPI context and absolute virtual-desktop mouse coordinates keep the click location consistent; the helper restores its thread context afterward. The pointer remains on that header; no click is sent to another application's window. A partial down-only insertion attempts a matching synthetic release as cleanup and still fails. No key/call/scenario is replayed.
- Each Escape attempt records bounded owned-focus booleans, held modifier codes, Escape state, accepted click/key counts and elapsed time. Failure diagnostics survive host-status failure. No foreign titles, PIDs, cursor coordinates or typed content are recorded. `SendInput` acceptance is not proof that Grain received the event; actual panel destruction, approval release and zero unwanted dispatch remain mandatory. Foreground/input failures stay recorded even if a later run passes.
- `node tests/agent-harness/run.mjs --scenario agent.reopen-escape --focus-click` exercises one guarded native header click for each of the ten pairs and asserts two accepted click events. This is explicit test setup; it does not certify the application's normal foreground policy. The flag is refused for other scenarios. The separate `--fault missing-escape` still withholds Escape and must fail even with a visible usable panel.
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
