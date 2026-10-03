# Grain Agent acceptance harness

**Client metadata integration verified; production activation pending:** [Focused audit](../../docs/Extensions%202.0/MCP-CLIENT-METADATA-AUDIT.md) records the final thirteen-case auth batch, fresh metadata repeat, strict fault, fixed-port regression repair and scope limits. The maintained `mcp.auth-client-metadata` uses actual SDK/Tauri/vault/Agent reads and an owned HTTPS document resolved by the controlled issuer. It proves metadata selection with/without DCR, fallback/explicit credentials, separate wrong-ID/redirect/503 refusal, callback conflict/cancel and restart reads. It does not certify public hosting: the user confirms no official Grain website and production identity remains absent. Build once, run serially:

```powershell
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-client-metadata
# Expected Fail/exit 1 at wrong mechanism, cleanup Pass:
node tests/agent-harness/run.mjs --scenario mcp.auth-client-metadata --fault missing-mcp-cimd
```

The future public document uses the exact backend callback inventory and no secret. `scripts/check-mcp-client-metadata.mjs HTTPS_URL --file DOCUMENT.json` verifies a local artifact; `HTTPS_URL --live` verifies the direct public response. Do not use either checker result as a real-provider login or full forward-56 acceptance. The controlled document and strict fixture failures are maintained tests; ignored inspectors/reports are disposable. The three old policy-blocked scratch roots remain untouched.

**Authentication recovery/status verified:** [Focused audit](../../docs/Extensions%202.0/MCP-AUTH-STATUS-AUDIT.md) records the final twelve-case auth regression, fresh real-app recovery repeat, strict negative oracle and shared Agent/smoke checks. New `mcp.auth-temporary-recovery` separately tests real expired refresh under 503/malformed/dropped replies in modern/legacy and valid-grant metadata recovery. The retained account and real developer Test/status/sign-in controls are checked. No provider login is needed. Keep exact failure/count/cleanup assertions and fixture grant retirement after verified logout; it does not certify production orphan reconciliation. Build once, then run serially:

```powershell
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-temporary-recovery
# Expected Fail/exit 1 at Missing expected rejection, cleanup Pass:
node tests/agent-harness/run.mjs --scenario mcp.auth-temporary-recovery --fault missing-mcp-refresh-failure
```

**Configured-client issuer protection verified:** [Focused audit](../../docs/Extensions%202.0/MCP-REGISTRATION-ISSUER-AUDIT.md) covers actual cross-issuer disclosure reproduction and the fix. `mcp.auth-issuer-binding` reuses two owned HTTPS peers and the real SDK/settings/vault path; only its explicit marker admits the second exact consent origin. Registration ownership survives logout/restart, changed issuers are refused before consent/secret use, explicit re-save enables the new issuer, and failed discovery preserves the existing account. All eleven `mcp-auth` cases and the fresh repeat pass; the deliberate missing-rotation fault fails as intended. MCP teardown independently removes/asserts zero grants, client secrets and new registration records, including public clients. No ordinary credentials or provider writes are involved. See the commands and retention rules below.

After building the stamped host, run the focused case or its combined regression serially:

```powershell
node tests/agent-harness/run.mjs --scenario mcp.auth-issuer-binding
node tests/agent-harness/run.mjs --suite mcp-auth
# Expected Fail/exit 1 and cleanup Pass; never a positive acceptance run:
node tests/agent-harness/run.mjs --scenario mcp.auth-issuer-binding --fault missing-mcp-issuer-rotation
```

**Native OAuth reuse verified:** [B5b audit](../../docs/Extensions%202.0/NATIVE-OAUTH-REUSE-AUDIT.md) records exact `oauth2` 5.0.0 adoption with existing account/vault/cancellation guards. Run the unchanged `native-auth`, `mcp-auth` and `agent-workflow` suites against the stamped build. All final cases pass; the partial-consent fault fails as expected. The native peer treats state as an opaque long URL-safe nonce, retaining independent PKCE/receipt/ownership assertions. No IDs, clocks or retention rules are removed. `run-uZZ7J8` is an interrupted, nonaccepted run with separately cleaned credentials and policy-blocked scratch/TLS deletion; its limitation is retained in the audit and harness inventory.

**Current inventory, 3 October:** 91 distinct scenario IDs, 76 self-contained `all` cases, 73 harness self-tests; 13 `mcp-auth` and 22 `mcp-foundation` cases. Original baseline **52 Pass / 1 Deferred / 0 active Pending**, additional forward **54/55 Pass**, full **56 Pending for permanent hosting/provider acceptance**. The user defers live expiry/refresh check 12 and authorizes forward implementation; it is not passed. Obsolete-code removal stays on hold. The [client metadata audit](../../docs/Extensions%202.0/MCP-CLIENT-METADATA-AUDIT.md) records the current activation gate; older dated counts retain historical evidence.

**SDK regression accepted:** exact `rmcp` 3.5.0 passes two complete `mcp-foundation` runs, affected shared workflows/smoke, public-provider/Linear no-consent checks and the existing fault oracle. Scenario definitions/IDs, clocks and retention rules are unchanged. Official tools pass; the existing standalone initialization fixture remains Blocked. [Final evidence and reproducible commands](../../docs/Extensions%202.0/MCP-AUTH-LIFECYCLE-REUSE-AUDIT.md#final-evidence-and-scoped-acceptance). Native OAuth/issuer recovery/metadata integration will extend this maintained harness only as their next focused units require.

`mcp-linear-contracts` requires `--linear-sign-in`, an interactive terminal and one owner-approved READ-ONLY browser consent. It calls no account tool, inspects actual production-supported contracts, repeats discovery after restart, then disconnects and removes its scoped grant/profile. Unlike the counts-only sign-in suite, this report intentionally retains bounded tool/property names, primitive structural shapes, schema digests and nesting flags. Prose, defaults, enums, const/private literals and raw schemas remain omitted; the projection is explicitly **not an executable schema**. The inspected catalog had 38 supported reads and no nested object parameters.

`mcp-hf-live` uses fixed anonymous `https://huggingface.co/mcp`, selects only `hf_fs` and requires actual approval for one nested `cat` operation on the public `google-bert/bert-base-uncased/README.md`, limited to 2,048 bytes of file content. It verifies actual document/result bytes and repeats after a real host restart. The transport refuses other commands, URIs, batches, tools or credentials. Both suites are excluded from `all`; the local model scripts decisions and does not certify live-model judgment.

```powershell
node tests/agent-harness/run.mjs --suite mcp-hf-live
# Expected Fail/exit 1 with cleanup Pass and the missing-evidence assertion:
node tests/agent-harness/run.mjs --scenario mcp.hf-nested-read --fault missing-live-evidence
```

The finite Linear expiry preparation/resume/abort interface is deferred and does not exist yet. Completed runs delete their grants, so their expiry timestamps cannot be reused as refresh evidence. B5a's exact SDK upgrade has passed its affected suites; next reuse/extend those suites for the separately implemented native OAuth and authentication ownership/recovery units. Keep existing identities, clocks and oracles unchanged. No new manual batch is assigned.

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

# Actual SDK MCP OAuth, scoped vault and callback/account ownership.
npm run test:agent -- --suite mcp-auth

# Both controlled MCP adapters in one real app; retain real 45/90s deadlines.
npm run test:agent -- --suite mcp-foundation

# Opt-in actual Linear SDK/DCR consent preflight; no browser/sign-in/tools.
npm run test:agent -- --suite mcp-linear-preflight

# Browser/grant controls without opening a browser or signing in.
npm run test:agent -- --suite mcp-linear-guards

# Human-assisted Linear consent; run only when the account owner is present.
# Requires an interactive terminal. No credentials are typed in this terminal.
npm run test:agent -- --suite mcp-linear-sign-in --linear-sign-in

# One human sign-in; supported shapes/restart only, no account tools.
npm run test:agent -- --suite mcp-linear-contracts --linear-sign-in

# No account: fixed public nested text read before/after restart.
npm run test:agent -- --suite mcp-hf-live

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

| ID                         | What is asserted                                                                                                                                                                                  |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.cold-warm`         | No action schema initially; selected action only; no dispatch before approval; one accepted result; subsequent call reuses the worker.                                                            |
| `agent.decline`            | Clicking Cancel releases approval without native dispatch; a fresh request still works.                                                                                                           |
| `agent.typed-approval`     | Typing `yes` into the actual follow-up field resumes the held call once.                                                                                                                          |
| `agent.stale-approval`     | Changed source reloaded through the production developer socket invalidates the old displayed approval; a fresh approval calls the new code.                                                      |
| `agent.escape-slow`        | Native Windows Escape with the owned Agent focused closes it during a slow call, suppresses late success/replay and allows a fresh call.                                                          |
| `agent.reopen-escape`      | Ten close/reopen pairs preserve native Escape and discard pending approvals without dispatch; a final fresh greeting works.                                                                       |
| `agent.close-pending`      | The real panel close button clears pending approval without execution.                                                                                                                            |
| `agent.close-model`        | Closing during a delayed HTTP model reply releases the Agent run and cancels that provider request.                                                                                               |
| `agent.close-slow`         | Closing during a dispatched slow tool retires its worker; no accepted late success or automatic second dispatch; fresh call recovers.                                                             |
| `native.ten-replacements`  | Ten unload/load/greeting cycles get distinct worker identities and return workers/tokens to the initial baseline.                                                                                 |
| `native.disable-slow`      | Disable/re-enable interrupts the old slow call without later cleanup destroying its replacement.                                                                                                  |
| `native.hot-reload`        | Two idle reloads, an in-flight reload, and a disabled reload use the actual authenticated developer WebSocket; new source results appear and disabled state is preserved.                         |
| `native.real-idle`         | The production 120-second idle threshold and 30-second reaper retire the worker; a new call recovers; a slow call started around 110 seconds survives.                                            |
| `native.reply-failures`    | Declared/thrown errors, conflicting result envelopes and an actual lost socket reply have distinct host classifications; private error text stays hidden, and fresh calls recover without replay. |
| `native.readiness-failure` | A real worker closes before authenticating; startup fails without dispatch, worker/token/supervisor retire, and corrected source works on a fresh request.                                        |
| `native.source-drift`      | Warm source changes with an unanswered approval, without reload; obsolete approval is refused and the first fresh approved call uses the new worker/source.                                       |
| `native.absolute-deadline` | A real 25-second handler fails around the production 20-second absolute budget, measured from host invocation and approval; late success/replay is suppressed and recovery survives old cleanup.  |
| `native.result-budgets`    | Replies exceeding the 64 KiB decoded budget and 512 KiB raw wire budget fail at distinct boundaries; workers/tokens/supervisors retire, then fresh native calls recover.                          |
| `native.invalid-input`     | Unknown keys, missing required values, wrong types, oversized arguments and malformed JSON fail before approval/startup; private rejected parameter keys stay hidden.                             |

`smoke` contains two cases; `lifecycle` includes smoke and the original remaining non-idle cases. `native-failures` contains six cases; `native-installation` six; `native-foundation` two; `native-auth` eight, with four in `native-auth-schedules`; `store` three; `registry-recovery` three. `mcp` contains nine controlled transport cases; `mcp-auth` thirteen SDK OAuth cases; `mcp-independence` selects its two-provider cases; `mcp-refresh` selects its two expiry/recovery cases; `mcp-foundation` selects all twenty-two in one host. `agent-workflow` contains six separately reported controlled workflow cases; `agent-interruption` contains eight. `all` includes 76 distinct self-contained cases; overlapping suite selections are not additive. Three official, two public-live, two Linear consent-preflight, one Linear control-guard, one human Linear sign-in, one Linear contract inspection, one public nested-read and four configured-model cases (`agent-live` and `agent-interruption-live`) require explicit selection and their external prerequisites, bringing the complete inventory to 91 IDs. `idle` runs only the long timing case. Each scenario establishes and cleans up its own baseline. A failed scenario stops the batch; later scenarios are Not run instead of being assessed against a contaminated baseline.

### Linear consent preflight (opt-in, no account)

`mcp-linear-preflight` contains two separate cases: `mcp.linear-cancel-disable` and `mcp.linear-cancel-disconnect`. Each uses actual production `rmcp` metadata discovery and Dynamic Client Registration against Linear, validates the private SDK authorization handoff, cancels with the named host operation, verifies the exact cancellation, rebinds the retired callback port and verifies a late connection is refused. A real host restart and fresh SDK attempt repeat that case. The suite is excluded from `all` and does not open the browser, submit a code, import an account, or execute a tool. DCR may create server-owned client registrations; local cleanup does not assert their deletion.

The guarded debug host admits exactly `grain-harness-linear`, `https://mcp.linear.app/mcp/readonly` and SDK scope `read`. Its marker cannot mix controlled MCP, native account, store or DeepWiki fixture modes. The alias and consent slot are unavailable without that marker and absent from ordinary builds. Even an unexpected completed callback does not permit tool execution through this preflight adapter. The original production Linear catalog entry is unchanged. Vault inventory/deletion is metadata-only, bounded, and targets that alias plus this run's UUID service; ordinary grants are never imported or cleared.

Test the cancellation oracle with `npm run test:agent -- --scenario mcp.linear-cancel-disable --fault skip-linear-cancel`. It must **Fail** at the five-second observation that cancellation settled, then retire the owned flow in `finally` and report cleanup Pass. This short observation does not replace the production five-minute authentication deadline. The fault is refused in other suites or multi-case selections. Reports retain scope/resource/PKCE and cleanup booleans, never authorization URL, state, client ID or grant contents.

This is preparation for checks 9/12/17, not their acceptance. The next account unit must provide a private browser handoff and a bounded human-sign-in wait, verify the actual granted scope/lifetime, suppress private workspace data in diagnostics, and validate a fixed harmless read against actual authenticated tool metadata before permitting its dispatch. A suitable nested schema is required for check 17; normal reads alone do not certify it. Real MCP expiry/refresh uses the issued grant's actual clock, not API lifetime assumptions or modified credentials. No personal sign-in is needed for the preflight command.

### Human Linear consent (explicit opt-in)

`mcp-linear-guards` verifies the new ready/open/grant controls in the real host, first without an available consent URL, then with actual SDK readiness and host cancellation. It never opens a browser. `mcp-linear-sign-in --linear-sign-in` is a separate interactive case; the flag is refused in other suites, and a noninteractive launch is refused before any profile/network/browser work. Both suites are excluded from `all`.

The account owner performs only two browser steps:

1. Cancel or close the first Linear consent page **without approving**. Type only `cancelled` and Enter in the runner terminal. The report distinguishes this human acknowledgement from the actual denied/cancelled backend outcome, callback release and zero vault entries. Accidentally approving this first flow fails the procedure.
2. Sign in to Linear in the second page and approve the requested **read-only** access. The SDK validates the real callback, performs the actual code exchange and stores the grant in this run's Windows vault namespace. The runner then verifies scope/lifetime metadata, authenticated tool discovery, real restart and persistence, and disconnects/cleans up. It **does not execute any tool** or send workspace results to a model.

Only the fixed alias Connect receives a 310-second IPC observation, around the unchanged production five-minute authentication deadline; its discovery receives 95 seconds around the unchanged 90-second production budget. Other commands/providers keep ten seconds. Cancellation input is bounded, abortable, accepts only the fixed acknowledgement, and releases its readline/input listeners and timer on every exit. A timeout or terminal close fails and retires the owned login; it never counts as successful consent.

The harness validates the grant **before every vault publication**, including refresh: exact issuer, read-only SDK-granted scopes, a nonempty bearer token and bounded client/timestamp/lifetime metadata. If the token response omits scope, the locked SDK's RFC 6749 requested-scope resolution is recorded explicitly; it is not reported as an explicit token-response scope. Unknown expiry remains `expiryKnown: false`, never an assumed lifetime. Reports contain only scope source, issued/expiry seconds, refresh availability and supported-tool counts. They omit authorization URLs, token/client values, tool names/descriptions, host logs, model/event bodies, snapshots and screenshots in human mode. Provider/error bodies are replaced by fixed stage diagnostics. Local scope cleanup remains independent of remote grant revocation; remove the test authorization from Linear's account settings if you also want remote revocation.

This procedure prepares live acceptance; it does not by itself close 9/12/17. Actual browser/exchange/discovery behavior must be run with the account owner, reviewed and repeated. A credentials badge or a discovered tool count is insufficient to certify harmless Agent execution, nested compatibility or post-expiry refresh. Tool calls remain blocked until a separately reviewed fixed read allowance can be based on real authenticated schemas. Credentials/profile are deleted when the run completes, so no idle test host or grant is left for a later coding phase. The `oauth2` harness-only dependency only names the SDK's already-locked v5 token trait; it adds no new library version, client or OAuth implementation.

### Native foundation (B1a)

| ID                        | Actual application assertions                                                                                                                                                                                                                                                                                                                                             |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.typed-contract`   | Seven text/number/entity and optional/null/zero cases round-trip through the actual worker and are verified by the scripted model. Each requires approval and one dispatch. Changing a valid declaration without reload refuses the old approval with zero dispatch; a fresh approved call works. Native declarations retain the four-parameter limit.                    |
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

The prerequisite alone credits **no numbered checks**. The subsequent [binding/owner audit](../../docs/Extensions%202.0/NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md) accepts 41/45/53 using the two additional scenarios below. The later schedule and refresh audits below cover 42–44/50–51 separately. Controlled consent does not certify browser UX, live-provider compatibility or other OS vaults.

| Scenario              | Precise real-app coverage                                                                                                                                                                                                                                                                                                                                                                                         |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.auth-binding` | Expired unbound session and historical id-only vault grants refuse tool loading/read/refresh before and after restart. Explicit reconnect yields actual verified reads across restart. Four reviewed declaration changes (client/token endpoint/scopes/API hosts) refuse old approvals, require reconnect and use fresh correctly configured grants.                                                              |
| `native.auth-owners`  | Actual Grain CLI pack/doctor for developer A/B, installed A versus developer B accounts, same-directory registration and production WebSocket reload, new-directory disconnect, stale approval refusal, restart and installed restoration. Parked uninstall independently deletes exactly one credential while the developer account remains usable. Actual returned JSON verifies both account and source owner. |

```powershell
npm run test:agent -- --scenario native.auth-binding --fault unchanged-auth-declaration
npm run test:agent -- --scenario native.auth-owners --fault wrong-owner
```

These new faults must give Fail/exit 1/cleanup Pass. The first reimports an unchanged declaration and must fail the expected reconnect assertion. The second changes actual compiled developer output and must fail source-owner verification. Faults are limited to their named scenarios. The model accepts a missing action schema as a negative result only for the exact actual native account refusal; generic unavailable output is never accepted.

Historical credential setup is fixed to the installed fixture in the explicitly enabled, idle harness; it never accepts or returns tokens/keys/namespace/path. It uses the actual run vault and expires the deliberately unbound grant so a mistakenly accepted refresh would be detectable. Finite declaration variants add only a fixed second public client/token path/read scope or matching localhost API/network permission. No arbitrary endpoint/client/scope or normal trust exception is allowed. Vault inventory reports only a bounded count in the exact run namespace, not keys or blobs.

The accepted combined runs still needed final independent deletion of three discarded developer grants. Their old sessions are not available to the selected owner, but ordinary-app orphan reconciliation remains open in R3. End-of-run harness cleanup does not certify production crash/orphan recovery. The following B1c units retain separate audits and repeated acceptance.

### Native sign-in/switch/expiry schedules (B1c partial)

| ID                          | Actual application assertions                                                                                                                                                                                                            |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.auth-cancellation`  | Actual CLI developer project and WebSocket reload, disable and Disconnect at callback/held exchange boundaries; six production cancellations, exact vault counts, actual late callback refusal/reusable ports, fresh reads/restarts.     |
| `native.auth-switch`        | Disconnect A/connect B refuses the old Agent approval without dispatch; fresh actual B read/restart succeeds.                                                                                                                            |
| `native.auth-failed-switch` | Explicit pending-flow cancellation, provider denial and actual token HTTP failure preserve A/count across reads/restarts; subsequent successful B switch refuses stale A approval.                                                       |
| `native.auth-expiry`        | Actual partial consent rejected without replacing A; real two-second grant without refresh requires reconnect before/after restart; actual expiry with refresh performs one exchange/read, zero new logins and no extra restart refresh. |

The [focused audit](../../docs/Extensions%202.0/NATIVE-AUTH-SCHEDULE-AUDIT.md) accepts checks 42/43/50/51 only. The historical four-case audit does not credit check 44; its separate refresh/logout and two-provider procedure follows below. Live-provider/browser, orphan reconciliation and whole authentication certification remain open. No clock/production-timeout override or vault edit is used to accelerate these cases; observed barriers and actual short-lived provider grants retain the required assertions. `native.auth-cancellation` requires the stamped actual CLI, like the owner case.

```powershell
npm run test:agent -- --suite native-auth-schedules
npm run test:agent -- --scenario native.auth-cancellation --fault uncancelled-login
npm run test:agent -- --scenario native.auth-expiry --fault accepted-partial-consent
```

The subset uses the same real-app runner/cleanup and avoids repeating unrelated cases. Both faults must give Fail/exit 1 and cleanup Pass. The harness-only Cancel operation calls production cancellation for the fixed fixture and preserves the prior selected account; it exposes no arbitrary target or approval bypass. The unchanged-installed-reimport driver failure is retained; developer reload is exercised through the actual CLI/project/WebSocket path.

### Native refresh/logout and provider independence (B1c final numbered unit)

`native.auth-refresh-logout` covers check 44. It approves an actual expired-account tool call, holds the provider's real refresh response, disconnects and reconnects, and verifies no old read or replay occurs. Fresh B reads and restart identify the replacement account. A separate held production auth future is awaited after logout, proving late refresh completion refuses publication and leaves the actual run vault empty before reconnect.

It then registers and reviews **two fixed native identities in the same host**: `com.grain.harness.auth` and `com.grain.harness.auth-peer`. They have distinct clients, scopes, authorization/token/API routes and actual grants. While the primary refresh remains held, the Agent searches, selectively loads, approves and reads the peer's actual B result. Both accounts survive restart without reauthentication; removing the primary preserves the peer's actual read and credential count.

The Agent supports one active request. To create the concurrent primary refresh, a fixed debug-only `Refresh` control calls production `grain_auth::access_token` for the primary's current approved owner/generation and immediately drops its zeroizing result. IPC returns only null on success. It exposes no arbitrary ID/host/generation, token or tool execution; the peer read still uses normal Agent approval. This certifies auth-provider network independence, not simultaneous Agent requests. Fixed provider handoffs have separate finite slots. Event receipts identify each fixed extension. Peer TLS/API routing, exact package admission and metadata-only vault cleanup stay within the owned marker/profile.

```powershell
npm run test:agent -- --scenario native.auth-refresh-logout
npm run test:agent -- --scenario native.auth-refresh-logout --fault released-refresh
npm run test:agent -- --suite native-auth
```

The fault deliberately releases the primary response before the peer read; it must give Fail/exit 1 and cleanup Pass because independence was not observed under a held refresh. Assertions retain real expiry, barriers, result identity, dispatch counts and restarts; no production clock/timeout or credential bytes are modified. The [focused audit](../../docs/Extensions%202.0/NATIVE-AUTH-REFRESH-AUDIT.md) records reviewed verdicts, repeated evidence and exclusions. Live account/browser/other-OS acceptance, vault failure/crash reconciliation and retained input observations remain separate.

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

| ID                        | What is asserted                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `store.close-offline`     | Ten actual Store loading/route-close/reopen cycles cancel held refresh HTTP responses and drop the resident index. The real Install button installs a verified package disabled, real consent and Agent greeting work across restart, cached offline browsing shows its notice and disables Update, and a direct backend install is also refused. The installed greeting remains usable.                |
| `store.pending-mutations` | During held real artifact downloads, production disable and remove operations supersede the pending update. Disabled/removed choices survive actual restart. Both schedules repeat under a developer override; the parked installed record is preserved on disable and removed without breaking the active developer. Explicit subsequent installation/consent returns the new greeting across restart. |
| `store.integrity-close`   | A same-length corrupt artifact fails its hash check; closing cancels a held artifact request; an invalid catalogue signature becomes offline and admits no artifact fetch. Corrected signed bytes install and produce the actual approved greeting.                                                                                                                                                     |

The runner starts an owned ephemeral loopback HTTP fixture only when a store scenario is selected. Its fixed public test seed signs modern minisign `ED` messages using Blake2b-512 prehash and a signed trusted comment. The separate debug host uses the matching **test publishing anchor only inside its validated isolated profile**. Normal publishing roots, mirrors and release initialization are unchanged. Cached index loading still passes through the production signature verifier and rollback-floor logic. No arbitrary endpoint/key or ordinary account is admitted. Model/store fixture ports cannot target the ordinary event listener on 7124 or collide with owned event/model listeners.

This certifies signed fixture installation and ownership through the production verifier/installer, not a published production extension or production root rotation. The fixture returns no roots-chain/revocation document. Accounts, OS vault recovery, root rotation, live publishing, power loss and physical file-picker/mouse behavior remain separate. The [focused store audit](../../docs/Extensions%202.0/NATIVE-STORE-AUDIT.md) records acceptance and exact build/run evidence.

```powershell
npm run test:agent:logic -- --group store
npm run test:agent -- --scenario store.close-offline --fault unclosed-store
```

The deliberate fault withholds navigation away from the real loading Store page. Expect the cancelled-response assertion to fail, exit 1 and cleanup Pass, including release of the owned fixture listener and held socket. It is accepted only for this scenario and never counts as a passing product run. No ordinary-app microphone or external sign-in step is needed for these fixed permission-free store cases.

## Verify the harness detects a real failure

### Registry recovery scenarios

| ID                             | What is asserted                                                                                                                                                                                                                                                            |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `native.registry-refusal`      | Malformed, future-version, inconsistent-identity and oversized registry startup preserve the file, refuse extension operations, record a diagnostic and spawn no worker. Restored bytes recover actual greetings through real restarts.                                     |
| `native.registry-preservation` | Installed/disabled records and a synthetic opaque account pointer survive restart. A stopped-host corrupt artifact triggers actual startup quarantine; restored bytes do not silently clear it. Explicit validated import and real consent/greeting recover across restart. |
| `native.registry-save-failure` | Real Windows publication-sharing locks fail enable/disable saves. Enable rolls back; disable takes effect live but honestly remains unsaved. Actual restarts distinguish those outcomes. A successful explicit disable/re-enable recovers.                                  |

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
- Native/MCP credential service names receive a per-run harness suffix. No production credentials are copied. Ordinary fixtures create no OAuth credentials. The explicit `native-auth` fixtures write only their run-scoped native credentials and independently verifies vault cleanup; broader crash/recovery and live-account certification remain future work.
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

Remaining suites can cover the blocked official initialization prerequisite and broader applicable authentication conformance, a native+MCP task, forced model misbehavior, human-assisted real MCP OAuth consent, live-model selection, actual summon/global shortcuts and microphone/pill behavior. The named official tool subset below is separately accepted; current controlled MCP tests do not certify these remaining paths.

## Pinned official conformance through Grain

Build the stamped real harness application first; use Windows, Node and Python `cryptography`, with no concurrent native compilation. The separate private package locks `@modelcontextprotocol/conformance` to **0.2.0-alpha.12**. Installation uses locked `npm ci --ignore-scripts` in ignored `.build/conformance/`; it adds no application dependency.

```powershell
node tests/agent-harness/conformance.mjs --install
node tests/agent-harness/conformance.mjs --suite tools
node tests/agent-harness/conformance.mjs
node tests/agent-harness/conformance.mjs --scenario mcp.conformance-tools-modern --fault missing-official-check
```

The tools subset expects **exit 0**: separate legacy 2025-11-25 and modern 2026-07-28 tool cases Pass. The complete entry currently expects **exit 2**: those two Pass and standalone initialization Blocked because the pinned raw fixture sends an invalid empty discovery reply. The fault expects **exit 1**, missing required official evidence detected after a real successful call and cleanup Pass. Check the Node exit code and JSON, not just an outer shell's generic nonzero code. A Blocked/empty/skipped/warning result is never a Pass.

Each official case has its own actual server, isolated real application and owned-PID HTTPS relay. The relay preserves wire bodies/status/metadata and checks the upstream listener PID before every exchange; it accepts no user account, redirect or arbitrary endpoint. Grain performs production discovery, selected-tool loading and actual Agent UI approval. Exactly one `add_numbers` call with 5 and 3 must return the real result 8; both official numeric and wire-schema checks, application acceptance and independent cleanup must pass.

Stable IDs are `mcp.conformance-tools-legacy`, `mcp.conformance-tools-modern` and `mcp.conformance-initialize`. These require the separately owned official binding; plain `run.mjs` cannot silently substitute a synthetic peer. Ordinary `--suite all` selects all 54 self-contained cases. Reports retain raw official checks, application children, identities, case verdicts and cleanup under `.runs/conformance-*/`. Keep init Blocked until a compatible fixture is audited; do not force a different production negotiation mode.

The [focused conformance audit](../../docs/Extensions%202.0/MCP-CONFORMANCE-AUDIT.md) records the pin, evidence, blocked prerequisite and retirement conditions for the temporary relay. The [retention inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) preserves the earlier interrupted-root cleanup exception. This named subset is not full SDK-tier, OAuth, live-provider or whole B2 acceptance.

## Controlled real-app MCP suite

The opt-in public live suite is documented separately below. Its external availability does not affect the self-contained suite.

Requires the stamped real harness application and Python `cryptography` for owned TLS. No user account, model API key or production trust-store change is required. A fixed debug-only provider uses only its run's exact loopback endpoint and CA; it refuses account commands. The external peer supplies wire replies, not Agent/executor mocks.

```powershell
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.response-preview --fault short-mcp-preview
```

| ID                       | Separate assertions                                                                                                                                                                                                                                               |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcp.legacy-http-probe`  | Generic-ID HTTP 400 discovery rejection, one fresh legacy handshake per operation, JSON/SSE management and approved nested reads, exact result, no call replay, session deletion and zero retained sessions.                                                      |
| `mcp.transport-contract` | Modern/legacy JSON/SSE, two-page nested reads, one approved call, no idle negotiation, actual restart and stale disable refusal.                                                                                                                                  |
| `mcp.mixed-catalog`      | Supported/excluded schemas, excluded selection with zero dispatch, empty supported catalog, repeated cursor and changed-approval refusal.                                                                                                                         |
| `mcp.response-preview`   | Multi-byte text/large structured data, bounded model preview, distinct model/UI omission notices and fresh recovery in JSON/SSE.                                                                                                                                  |
| `mcp.transport-bounds`   | Twelve named lifecycle/overflow/drop variants: declared/chunked JSON, SSE data/small comments, error body and lost reply. Honest unknown result, one call, no replay and fresh read after each.                                                                   |
| `mcp.close-cancellation` | Four approved held calls, actual panel close/HTTP disconnect/session cleanup, attempted late reply, fresh result isolation; two pending-approval closes never dispatch.                                                                                           |
| `mcp.catalog-budgets`    | Twenty-four lifecycle/framing/limit combinations: management Test and actual Agent independently refuse tool counts, rejected-definition duplicates, cursor/page limits, aggregate metadata and received bytes; zero action exposure/dispatch and fresh recovery. |
| `mcp.http-deadline`      | Four real 45-second approved-call timeouts; unknown result, one call, measured duration, response/session release, discarded late reply and fresh recovery.                                                                                                       |
| `mcp.discovery-deadline` | Modern/SSE and legacy/JSON each finish two 35-second pages, then stop the third at the real 90-second absolute deadline; no tool dispatch, no retained delay timer/session and fresh recovery.                                                                    |

There are 80 maintained scenario IDs: 71 self-contained real-app cases (including nine MCP transport, eight MCP OAuth, six Agent workflow and eight Agent interruption cases), three explicitly external official cases (one Blocked), two opt-in public live MCP cases and four opt-in genuine-model cases. There are 53 runner self-tests. Ordinary `all` selects only the 71 self-contained cases; official/live entries supply their explicit prerequisites separately. Run cases serially after native compilation finishes. Full MCP transport and Agent expiry waits use unchanged real clocks. Each failure records its exact stage, stops the batch and leaves later cases Not run. A nonzero prerequisite/reporting failure is not an accepted oracle check.

```powershell
node tests/agent-harness/run.mjs --scenario mcp.catalog-budgets --fault accepted-mcp-catalog
```

The catalog fault supplies an ordinary complete catalog where rejection is required. Expect the first precise management refusal assertion to fail, zero tool calls and cleanup Pass. It is admitted only for that scenario.

Reports now checkpoint completed cases before/after each scenario, preserving `activeScenario` and cleanup Not run until final teardown. A hard process loss can leave owned scratch/credentials behind; no partial report certifies cleanup or the full run. Follow the [hard-interruption inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) before explicit recovery. Native-auth scratch deletion alone cannot reconcile vault grants. The initial interrupted MCP run and policy-blocked cleanup are retained in the [catalog/deadline audit](../../docs/Extensions%202.0/MCP-CATALOG-DEADLINE-AUDIT.md).

[Catalog audit](../../docs/Extensions%202.0/MCP-CATALOG-TRANSPORT-AUDIT.md), [bounds/cancellation audit](../../docs/Extensions%202.0/MCP-BOUNDS-CANCELLATION-AUDIT.md), and [retention/cleanup inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) give evidence identities and exclusions. Controlled partial coverage never certifies live MCP OAuth, account persistence or official conformance. Retain fixtures as regression tests; delete owned profiles, keys, credentials, processes and response handles each run. Feature-gated test seams have documented retirement conditions.

Implementation/design references: [Playwright WebView2 automation](https://playwright.dev/docs/webview2), [Microsoft WebView2 environment options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl), [Tauri's distinction between mock-runtime and real-application tests](https://v2.tauri.app/develop/tests/), [Windows SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) and [foreground window restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow). These support the adapter choice; Grain's isolation and evidence policies are local requirements.

## Opt-in public live MCP acceptance

Build first, leave the desktop idle and run serially after native compilation. This uses the real application and production MCP adapter with a scripted model, normal TLS trust, and no account. The debug provider is fixed to `https://mcp.deepwiki.com/mcp`; only `read_wiki_structure` and `read_wiki_contents` for public `modelcontextprotocol/rust-sdk` are admitted. Questions, private repositories, extra arguments, credentials and arbitrary endpoints are refused before dispatch. No DeepWiki provider is added to the ordinary product catalog.

```powershell
node tests/agent-harness/run.mjs --suite mcp-live
node tests/agent-harness/run.mjs --scenario mcp.live-read-disable --fault missing-live-evidence
node tests/agent-harness/run.mjs --scenario mcp.legacy-http-probe
```

The live suite expects exit 0; service or desktop failures retain their actual verdict. The fault expects exit 1 after a genuine approved read, because required result evidence is deliberately withheld from the verifier; cleanup must pass. No automatic retry masks a failure.

| ID                           | Separate assertions                                                                                                                                                                                                                                                                                      |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcp.live-read-disable`      | Live discovery; actual approved result; disabled stale approval and fresh request both refuse with zero backend attempts; re-enable and actual restart restore reading. The fresh refusal retains an unrelated harmless native directory entry; empty-directory text-only fallback is outside this case. |
| `mcp.live-response-recovery` | Actual ordinary read, large public documentation request, bounded preview or honest unusable/unknown result, then a fresh actual ordinary read. No blind replay.                                                                                                                                         |

Reports contain fixed endpoint/repository, per-stage result kind/byte count, model evidence and client-side attempt observations. Attempt counts are conservative backend observations, not independent remote receipts. No raw documentation is retained in the JSON evidence. These cases exclude OAuth, private accounts, nested live schemas, live-model judgment and universal provider certification. Controlled overflow fixtures supply exact byte-limit evidence; an unknown live response does not prove remote failure or exact received payload size. See the [focused live audit](../../docs/Extensions%202.0/MCP-LIVE-READ-AUDIT.md). Both cases are excluded from ordinary `all`.

## Controlled MCP provider independence

```powershell
npm run test:agent -- --suite mcp-independence
node tests/agent-harness/run.mjs --scenario mcp.auth-provider-independence --fault wrong-mcp-peer-account
node tests/agent-harness/run.mjs --scenario mcp.auth-fixed-port-conflict --fault skip-fixed-port-conflict
```

The two positive cases share the real host and existing peer implementation. A second server starts only for these selected cases, with a separate origin/listener and client/code/token maps. Both actual SDK dynamic login futures remain open together; completing one and cancelling the other must leave only the completed grant enabled. The cancelled issued callback is refused. Approved reads prove the actual A/B accounts independently, including restart, disabled stale approval and removal of only one provider. The second case expects an exact refusal while the first preregistered flow owns port 31938, then serial login/read/restart recovery on that released fixed port. A conflict is never counted as authentication.

Each read asserts one wire call to its selected provider, zero calls to the other, exact nested returned JSON/account and a model receipt naming that provider. Agent reads remain serial; login futures may overlap independent approved reads. Neither a real model nor a personal account is required for these mechanical checks. External browser consent/live-account compatibility remains separate. Both fault commands must Fail/exit 1 with cleanup Pass. See the [focused provider audit](../../docs/Extensions%202.0/MCP-PROVIDER-INDEPENDENCE-AUDIT.md). Metadata-only vault cleanup admits exactly the four fixed keys in the current run namespace; both servers, handoff generations and disposable profiles are retired.

## Controlled SDK MCP OAuth acceptance

`mcp-auth` uses the actual locked Rust SDK, production protected-resource/issuer discovery, public dynamic registration, S256 PKCE/resource-bound exchange, real callback listener and scoped Windows vault. The fixed debug-only Account and Client targets require `mcpAuth` and its run-owned HTTPS peer. It never accepts an arbitrary provider, token, callback destination or certificate. The private handoff consumes the SDK's actual URL and the fixture's actual redirect; it replaces external browser consent only. No personal account or OS trust-store modification is required. Python `cryptography` supplies the existing disposable TLS setup. The Client target uses the same owned resource/issuer with three fixed preregistered IDs and fake secrets. It exercises the production credential settings and secret vault; it does not certify provider independence.

| ID                              | Independently asserted coverage                                                                                                                                                                                                                                                      |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mcp.auth-fixture`              | Actual A/B grants, selected nested read, no call before approval, exact account/result, old A generation refusal with zero calls, actual A/B restarts and disconnect.                                                                                                                |
| `mcp.auth-denied-cancelled`     | Denied callback and real Cancel sign-in command finish with zero exchange/grant, immediately reusable exact callback ports, then fresh approved A/B reads.                                                                                                                           |
| `mcp.auth-late-callback`        | Cancel sign-in, Disable and Developer Mode off each refuse the issued old redirect, with no exchange/grant/resurrection after actual restart; new B login/read/restart succeeds.                                                                                                     |
| `mcp.auth-close-cancellation`   | Modern/legacy crossed with JSON/SSE: actual approved held read and pending approval close; bounded local cleanup, late result refusal, preserved scoped grant/enablement and exact fresh/restart account A reads without new authorization.                                          |
| `mcp.auth-shutdown`             | Provider Disable and Developer Mode off each cross modern/legacy JSON/SSE: actual held receipt, bounded unknown outcome/UI, no replay, session deletion, disabled discovery refusal, preserved grant, stale confirmation refusal and fresh/restart A reads without signing in again. |
| `mcp.auth-client-configuration` | Empty-secret public and actual confidential login; public ID change, same-ID secret rotation/removal invalidate old grants/approvals; unsupported missing secret refuses, fresh/restart A/B reads recover with exact scopes and no DCR.                                              |

Build first; run serially on an idle desktop:

```powershell
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --scenario mcp.auth-fixture --fault wrong-mcp-account
node tests/agent-harness/run.mjs --scenario mcp.auth-fixture --fault abandoned-mcp-credential
node tests/agent-harness/run.mjs --scenario mcp.auth-close-cancellation
node tests/agent-harness/run.mjs --scenario mcp.auth-close-cancellation --fault lost-mcp-account
node tests/agent-harness/run.mjs --scenario mcp.auth-shutdown --fault skip-mcp-disable
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault skip-mcp-client-change
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault abandoned-mcp-client-secret
```

All six faults must report Fail/exit 1 and cleanup Pass. Wrong-account changes the actual authenticated provider result; it must fail the account oracle. Abandoned-credential stops after one real grant; independent `mcpVaultCleanup` must report `deleted: 1`, `remaining: 0`. Lost-account invokes the actual production Disconnect command after an approved held read closes; the preservation assertion must fail rather than silently reconnect. Skip-disable leaves the actual held call enabled; the immediate provider-state assertion must fail before any queue timeout. Skip-client-change leaves the prior grant intact; the immediate grant-removal assertion must fail. Abandoned-client-secret stops after the real confidential secret is stored; independent cleanup must report `clientSecretsDeleted: 1` and `clientSecretsRemaining: 0`. Inventory/cleanup reuses `auth-cleanup.ps1 -Mcp`, matching only the exact run service and four fixed account keys and their scoped client-secret service; it never reads blobs. Normal success requires zero remaining scoped grants/client secrets/sessions/sockets and removed owned scratch material.

`mcp-foundation` selects the nine existing transport cases and these ten cases in the same real host, exercising their coexistence. It preserves genuine 45/90-second deadlines and independent scenario/stage assertions; it stops on first failure. Nineteen cases reuse the maintained peer implementation; independence alone starts a second owned issuer/listener. No production service is introduced. The OAuth evidence journal has a fixed 2,048-entry ceiling and reserves one constant terminal-error entry; its exhaustion self-test verifies failure without rethrow or private-request leakage.

Only actual bearer lookup and exact returned nested JSON establish account evidence. The model oracle distinguishes exact Cancelled/not-dispatched refusal, exact dispatched account uncertainty and exact successful account/result; unrelated failures cannot certify any of them. The close case requires four cancelled wire calls and twelve actual successful reads across four real restarts. Shutdown requires eight cancelled receipts, eight stale confirmations with zero calls, and 24 successful fresh/restart A reads, with no new authorization exchanges. Evidence contains bounded account/stage/counter flags, never authorization URLs, codes, PKCE verifiers or tokens. The client case requires ten actual successful reads, four stale refusals with zero dispatch, five restarts, exact secret inventory/rotation and one actual unsupported-secret rejection with zero issued token. External browser UX, live OAuth/refresh and failed-switch preservation remain separate. The independence cases are documented above; the controlled expiry and refresh cases are documented below. See the [SDK/late-login audit](../../docs/Extensions%202.0/MCP-AUTH-FIXTURE-AUDIT.md), [authenticated cancellation audit](../../docs/Extensions%202.0/MCP-AUTH-CANCELLATION-AUDIT.md) and [shutdown audit](../../docs/Extensions%202.0/MCP-AUTH-SHUTDOWN-AUDIT.md) and the [client-configuration audit](../../docs/Extensions%202.0/MCP-CLIENT-CONFIGURATION-AUDIT.md) for exact boundaries.

## Agent staged loading and read/write workflows

Interruption acceptance extends these fixtures without a second runtime. Run `node tests/agent-harness/run.mjs --suite agent-interruption` for eight controlled cases, and explicitly opt in with `node tests/agent-harness/run.mjs --suite agent-interruption-live --live-configured` for two genuine-model cases. Each live case covers denial/independent unchanged verification and model unavailability after an actual approved write. Both live suites are excluded from `all`.

The controlled batch independently covers native/MCP denial, pending/active-model Stop by the real close handler, actual receipt retention after HTTP 503, MCP ambiguous write response and one actual 120-second MCP approval expiry plus host restart. The expiry wait is not shortened and no persisted time is edited. A changed-argument retry is blocked before dispatch; permitted independent verification proceeds under a separate approval. MCP peer receipts prove actual object effects separately from model prose. The post-write outage is induced only at the owned model boundary; the user's proxy/settings stay available.

Isolated negative commands `--scenario agent.workflow-denial-native --fault skip-workflow-denial` (or its MCP counterpart) and `--scenario agent.workflow-expiry --fault skip-workflow-expiry` must report expected Fail/exit 1 with cleanup Pass. The former intentionally approves the disposable write; the latter omits the wait and must detect the still-pending token immediately. Never use faults as positive acceptance. A lost receipt, false completion or extra dispatch also fails the independent reply oracle. Genuine hosted uncertainty remains unavailable/Blocked; forced response-loss timing belongs to the deterministic peer. See the [interruption audit](../../docs/Extensions%202.0/AGENT-WORKFLOW-INTERRUPTION-AUDIT.md) for exact cases, evidence and retained limitations.

Run the maintained controlled batch after building the real host:

```powershell
node tests/agent-harness/run.mjs --suite agent-workflow
```

| ID                                             | Independent proof                                                                                                                                                                                                            |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent.staged-native` / `agent.staged-mcp`     | Two metadata pages, exact/no-match search, one selected read, a second tool added later, earlier schema preserved and original read reused.                                                                                  |
| `agent.schema-budget`                          | Eight supported large schemas exceed the 32 KiB task budget; publication is atomic and the prior read remains callable. Individual field and eight-ID limits remain unchanged.                                               |
| `agent.directory-coverage`                     | Actual startup with 101 owned installed extensions exposes 100 directory entries, states incomplete coverage and refuses the omitted owner without workers. Offline seed/restoration and owned profile cleanup are explicit. |
| `agent.workflow-native` / `agent.workflow-mcp` | Approved read → exact disposable write → independent verification in one task. Actual batch tails are withheld; three consumed-token replays and a disabled owner's write never dispatch.                                    |

Negative oracles must report Fail/exit 1 at the specific assertion, with cleanup Pass; they are not normal acceptance runs:

```powershell
node tests/agent-harness/run.mjs --scenario agent.staged-native --fault missing-staged-selection
node tests/agent-harness/run.mjs --scenario agent.staged-mcp --fault missing-staged-selection
node tests/agent-harness/run.mjs --scenario agent.workflow-native --fault wrong-workflow-receipt
node tests/agent-harness/run.mjs --scenario agent.workflow-mcp --fault wrong-workflow-receipt
```

The selection fault requests the first schema again instead of the second, so the next real offered schema set fails. The receipt fault corrupts the actual worker/peer write count; a generic success cannot pass. Both adapters keep their independent dispatch counters. A guarded debug capture retrieves only the current confirmation token into runner memory for production-command replay; it neither executes nor bypasses confirmation, and no token is written to status/model/evidence.

After the user chooses the ordinary configured model, run the two genuine-model cases explicitly:

```powershell
node tests/agent-harness/run.mjs --suite agent-live --live-configured
```

Use `--scenario agent.live-native-workflow --live-configured` or its MCP counterpart to isolate a failure. This opt-in reads only the selected model registration from the normal `%APPDATA%/com.grain.app` profile. Only the selected key is retained for outgoing model headers; no secret/history/account data is copied into the isolated application, test marker, model prompt or report. Smart rotation, absent configuration and unsupported endpoint admission are blocked prerequisites; this mode does not search other/portable profiles. HTTPS uses ordinary certificate validation; HTTP is admitted only for a configured loopback proxy. These tests consume the configured model's normal quota.

The real model receives a fixed disposable-object task through Grain's real Agent/model boundary and chooses the calls itself. The trace checks metadata before schemas, incremental 0/1/2/3 exposure, unchanged earlier definitions, exact returned object/write count and a single approved write. Reports preserve bounded action/schema/receipt metadata, no raw model prompts or credentials, and identify the configured model evidence class. Batch emission is recorded as observed; deterministic batch-withholding remains covered by the controlled cases.

The live adapter has 1 MiB request/response limits, a 60-second upstream request deadline, no redirect or idle-pool reuse and owned socket/timer/frontend cancellation. Harness waits permit 120 seconds for genuine-model stages; production budgets/deadlines are not changed. Closing it drops credential references, not a JavaScript zeroization guarantee. Both `agent-live` cases are excluded from `all`; the live flag is refused in ordinary suites. Genuine runs do not replace SDK OAuth/account testing or check 48's denial/expiry/Stop/failure observations. See the [focused audit](../../docs/Extensions%202.0/AGENT-WORKFLOW-AUDIT.md) and the retention inventory before removing debug-only observation/relay hooks.

## Controlled MCP expiry and refresh recovery

```powershell
npm run test:agent -- --suite mcp-refresh
npm run test:agent -- --scenario mcp.auth-refresh-recovery --fault wrong-mcp-refresh-account
```

The positive suite uses two separately reported cases in one real host. Recovery issues a real two-second grant, restarts, waits issuer expiry and performs an actually approved A read through SDK refresh. The rotated replacement lasts 40 seconds, above the SDK's 30-second proactive buffer; another real restart/full expiry proves generation 2 uses the persisted replacement. A long-lived replacement survives a third restart and read without another refresh or browser login. Refusal separately verifies no refresh token and actual `invalid_grant`: zero tool dispatch/new login, exact current host failure, then explicit reconnect to B and approved fresh/restart reads. No clock or stored credential is edited; only the external owned issuer defines its short test lifetimes.

The fault changes the actual refreshed bearer account and must exit 1 with cleanup Pass. The issuer and acceptance oracles have client/resource/rotation/account and privacy negative tests. The complete runner has 57 self-tests. See the [focused refresh audit](../../docs/Extensions%202.0/MCP-REFRESH-RECOVERY-AUDIT.md) for final report identities, retained candidate/oracle repair and the current generic reconnect-error finding.

These controlled cases do not accept live check 12. Explicit isolated Linear consent, actual issued lifetime and restart have since passed through the opt-in human suites; genuine expiry/refresh remains pending. Linear has no nested object inputs in the inspected supported catalog; the separate anonymous Hugging Face suite supplies that prerequisite. No ordinary credentials are copied and these local fixture commands must not target an ordinary account. See the [current audit](../../docs/Extensions%202.0/MCP-NESTED-LIVE-READ-AUDIT.md).
