# Guarded native account fixture — B1b prerequisite audit

Date: 2 October 2026 (India; run timestamps use 1 October UTC). Branch: `extensions/tool-only-retirement`. Companion to the [forward plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md), [progress ledger](MCP-EXTENSION-PROGRESS.md), [B1a audit](NATIVE-FOUNDATION-AUDIT.md) and [runner instructions](../../tests/agent-harness/README.md).

This is the historical prerequisite snapshot. The subsequent [native binding/owner audit](NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md) now accepts 41/45/53 with two additional scenarios; its current ledger is 33 Pass / 20 Pending. Requirements/evidence below retain the prerequisite's original scope and counts.

## Scope and verdict

The **B1b fixture prerequisite is implemented and audited**. It supplies controlled native-account coverage through the real application and fixes a reproduced disconnect cleanup failure. It does not accept the complete authentication block or numbered checks 41/45/53. The ledger remains **30 Pass / 23 Pending**, and all seven phase gates remain open. No SDK/OAuth library replacement, feature expansion, ordinary credential change or UI design change is included.

The maintained `native-auth` suite contains `native.auth-fixture`, deliberately with no numbered-check credit. It uses the actual isolated Tauri/WebView2 application, package importer, permission dialog, OAuth URL/PKCE creation, loopback callback, code exchange, Windows credential vault, account publication, Agent selection/approval and host-authenticated `grain.net.fetch`. Only the external provider and browser-consent handoff are controlled. It never stores tokens in a fake vault or simulates Grain's OAuth implementation.

## Test procedure and observations

1. Refuse an otherwise valid fixture with another client ID, then import the correct fixed fixture. Actual permission review leaves it disabled and starts no worker; the real Allow and enable action enables its tool.
2. Connect account A. The provider verifies client, scope, response type, redirect, state shape and S256 challenge/verifier before issuing a token. A real approved read returns A. No API read occurs before approval; one dispatch, server read and model-verified result follow it.
3. Hold another A approval, connect B and approve the old call. It must dispatch zero times. A fresh approved read returns B.
4. Restart the owned host. B remains connected, no worker is running initially, and a new approved read returns B without another token exchange.
5. Disconnect, verify disconnected status and zero workers, then uninstall. Worker/supervisor/token counts return to baseline. End-of-run cleanup checks the exact credential namespace and releases owned listeners/processes/scratch files.

Wrong-account output is detected using actual returned tool JSON and independent provider receipts, not the model's requested identity. A deliberately abandoned account leaves one real vaulted credential when its host exits; independent namespace cleanup deletes that entry and verifies none remain. Fault runs are Fail/exit 1, never product Pass. No scenario retry converts failure to success.

## Isolation and audit findings

- `grain_agent_harness_auth.rs` and command registration are feature-gated into the debug-only harness. The existing harness rejects release builds. Controls require the isolated main window/application/profile plus an explicit `authPort` marker. Reserved, zero and overlapping ports are refused. Ordinary permission-free fixture admission retains its authentication/nonempty-permission refusal.
- Admission fixes the separate ID `com.grain.harness.auth`, owned package path, one account-read tool, exact auth/network permissions, public client, read scope, API host and HTTPS endpoints. Production package validation and real user permission review still run. No arbitrary ID/path/endpoint, vault-read command or approval bypass is added.
- Each run generates a short-lived localhost certificate/key using Python `cryptography`. Files are confined to its disposable root. No OS trust store is modified. Certificate and hostname validation remain enabled; test trust is supplied only to an explicitly configured loopback token endpoint and the fixture's exact API origin/path. Requests keep normal response limits, deadlines and redirect checks. The fixture API client is dropped after use; no new shipping service or retained client cache is added.
- Controlled authorization URLs are consumed privately through the fixed harness command, never included in status/journals. Handoff storage has flow ownership; `PendingGuard::drop` clears only that flow's unconsumed URL, including cancellation/future drop. An old cleanup cannot erase a replacement. It never holds the handoff mutex while acquiring pending-flow state.
- Actual credentials use the existing run-specific service `com.grain.extension.oauth.agent-harness.<run UUID>`. The Windows cleanup helper requires the exact enabled marker/run UUID and matches only that fixture's legacy/session keys in that service. It enumerates metadata, does not inspect/print credential blobs, deletes matching keys and verifies zero remain. Its real abandoned-credential run confirms the pinned keyring target layout. Cleanup failures stay visible; ordinary namespaces cannot be selected through this helper.
- Journals contain bounded stages, account A/B, grant kinds, verification flags and dispatch/read counts. Tokens, codes, verifiers, authorization URLs and actual private results are excluded. Fixture tokens have a private marker; the scripted model refuses any frame containing it. Evidence is checked for that marker after final runs.
- Source audit fixed an asynchronous fixture assertion that could throw outside its Promise: an unexpected consent HTTP status now rejects normally so `finally` cleanup runs. Runner identity now includes Python helpers as well as JavaScript/PowerShell/JSON definitions, covering the new TLS generator. Provider code/token/journal collections are bounded and sockets/held responses are released on close. Held-token, denial, expiry/partial-scope and refresh controls are preparation for later scenarios; those full schedules are not certified by this prerequisite.

Graph exploration preceded raw reads. Change detection highlighted exchange/refresh/confirmation; review reported high structural risk and missing edges. Direct inspection therefore covered marker admission, feature gates, authorization lifetime/lock order, TLS routing, token-to-registry publication, generation retirement, real permission review, result/privacy oracles, callback bounds and independent vault cleanup. Zero graph flows is not evidence of no coupling.

## Reproduced production bug

`run-fNbqMZ` completed A/B reads, stale approval refusal and restart, but failed the real cleanup baseline: disconnect advanced the registry generation while its old native worker remained alive. Subsequent uninstall tried to stop the new record generation and missed that old worker, leaving one worker/token until later cleanup.

`grain_auth::disconnect_reviewed` now retires **only the reviewed old worker generation** immediately after account publication/cancellation, before credential deletion. `stop_extension_generation` checks the worker generation and then its exact token, preserving a replacement created meanwhile. A stale publication that never commits does not retire another owner. Existing disconnect semantics intentionally invalidate the live account even if saving that clear fails; the error is still reported. No broad stop-by-ID, automatic retry, new engine or stronger cross-store durability claim is introduced. The final real-app scenario explicitly asserts worker retirement at disconnect and the full post-uninstall baseline.

`run-iyS9Py` was a separate fixture mistake: the handler initially used the wrong `grain.net.fetch` signature. It was corrected to the existing `(url, {auth:true})` API. Both early failures had cleanup Pass and remain recorded. `run-oHreKs` passed after the production repair, before final audit assertions and flow-owned handoff cleanup; it is earlier evidence, not the final runner identity.

## Research and verification

Primary references were rechecked alongside the existing multi-project library decision: [reqwest certificate trust](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) supports scoped root certificates without disabling validation; [Node HTTPS](https://nodejs.org/api/https.html) provides the controlled external TLS endpoint; [Goose extension ownership](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/extension_manager/mod.rs) and [LibreChat disposal-race tests](https://github.com/LibreChat-AI/LibreChat/blob/main/packages/api/src/mcp/__tests__/MCPConnectionDisposeRace.test.ts) inform explicit teardown and late-owner testing. These do not certify Grain or supply its native account policy. The fixture exercises the retained implementation; later `oauth2`/SDK reuse remains behind the foundation gate.

Reproduction from the repository root, one runtime suite at a time:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
npm run test:agent -- --suite native-auth
npm run test:agent -- --scenario native.auth-fixture --fault wrong-account
npm run test:agent -- --scenario native.auth-fixture --fault abandoned-auth
```

Windows/WebView2 and Python with `cryptography` are required; no live login/API key or global certificate installation. Missing TLS-generation prerequisites report Blocked. Both fault commands must produce Fail/exit 1/cleanup Pass; abandoned-auth additionally records `nativeVaultCleanup.deleted: 1` and `remaining: 0`. Artifacts are ignored; scratch TLS/private keys are deleted after evidence collection.

Validation: normal-build auth **23 tests** (`logic-NFxi6u`), native host **49 tests** (`logic-qvNlt3`), three harness Rust guards and **20 runner self-tests** Pass. Real frontend type/build and stamped harness build Pass; ordinary backend check and normal/harness Clippy complete. Existing unrelated warnings remain, including two unchanged auth lints. Source formatting, PowerShell parsing, scoped whitespace and final runtime evidence are recorded below.

### Final recorded evidence

Windows x64, Node `v24.11.1`, WebView2 `154.0.4258.48`; real application with a scripted model and controlled HTTPS provider. Both final ordinary runs perform two exchanges, three authenticated reads (A/B/B), zero dispatches for the obsolete approval, and restart without another exchange. No scenario retries. All listed runtime runs have cleanup Pass; auth runs have vault `remaining: 0`.

| Evidence directory under `tests/agent-harness/.runs/` | Result and interpretation |
|---|---|
| `run-xQpc9T` | Final clean account fixture Pass, 3,267 ms, 1 October 18:51:21 UTC |
| `run-GHebO2` | Independent final clean repeat Pass, 3,283 ms, 18:51:45 UTC |
| `run-UUvnOS` | Wrong-account fault Fail/exit 1, 1,110 ms; actual returned JSON fails identity verification |
| `run-JrjB55` | Abandoned-auth fault Fail/exit 1, 515 ms; independent cleanup deletes one real vault entry, remaining zero |
| `run-zYPYHj` | Ordinary cold/warm and decline smoke cases Pass; auth control without the explicit marker refuses access |
| `run-5LXQdv` | Typed-contract and legacy-migration retests Pass, including six owned restart transitions; auth control remains disabled |

Final account runs and both final faults share runner fingerprint `44d9f83f54edce97c921d3cf9e11cfe6a1227fa1013fe2470dafb2ad6feace74`. The smoke runner fingerprint is `92b0ca10f03326f3cf4cd531cf8017ad2a53315b811ba6c33bc2095346cf7e39`; the foundation fingerprint is `56bd0ae834530f342084d0273254eac3625b02f8d43f7f323b7543a27b227405`. Those earlier runner identities precede only the consent-error Promise repair, suite-help correction and/or Python-helper identity inclusion. They are separately recorded, not relabeled as the final identity. The earlier clean repeats `run-WlCSKd`/`run-KlCpaD` and faults `run-aICTkw`/`run-BkTzz3` also have their expected verdicts and cleanup Pass; they predate the fingerprint correction.

All six use binary SHA-256 `d8d5bd406286ce03258707f88884ee69401ca736c03f292fca685d63a3bf8516`, source fingerprint `4e0fa26f938fe875e19be9bd9c1400ab7eb7d7cce4f7aca561e17df919cb8ebc`, built 1 October 18:30:40 UTC from base `a0d2e7e2139b51f04c20bc4d6bda81b460fba0c6` plus recorded dirty source. The pre-existing unrelated `src/app/bindings.ts` edit is included in the runtime source identity but is neither changed nor committed by this unit. Evidence JSON/log/Markdown scans find neither the private token marker nor authorization URLs; owned TLS scratch directories are gone. Definitions, audit and documentation are committed; machine-local reports remain ignored.

Final runner self-tests, scoped Rust/JavaScript formatting, Python syntax, PowerShell parsing, document-link/ledger consistency and scoped whitespace checks Pass. These checks do not increase numbered acceptance credit.

## Limits and next unit

Checks 41/45/53 need full declaration/unbound-grant, installed/developer A/B account isolation/restoration and applicable live-account portions. Checks 42–44/50–51 still need held callback/refresh/logout, failed-switch/persistence, partial consent and expiry schedules. No genuine-model, browser UX, live provider, other-platform, microphone/pill, RAM/handles, power-loss or whole R3 certificate follows. The retained Escape investigation remains open. No new manual batch is needed for this infrastructure unit; the next work is B1b's complete ownership/declaration scenarios using this fixture.
