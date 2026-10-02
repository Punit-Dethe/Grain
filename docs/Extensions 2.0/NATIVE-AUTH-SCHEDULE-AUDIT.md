# Native sign-in, switching and expiry — B1c partial audit

Date: 2 October 2026 (India; evidence timestamps UTC). Branch: `extensions/tool-only-retirement`; base `cb333618`. Companions: [execution order](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md), [numbered ledger](MCP-EXTENSION-PROGRESS.md), [binding/owner handoff](NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md), [runner instructions](../../tests/agent-harness/README.md).

## Verdict

**Checks 42, 43, 50 and 51 are accepted as reviewed real-application automation against the controlled native provider.** The ledger is **37 Pass / 16 Pending** (15 human, 22 reviewed automated). B1c and whole native account certification remain open: check **44** still requires actual refresh/logout and two independent native provider identities in one application. All seven R0–R6 release gates remain open. Live browser/provider compatibility and vault reconciliation are separate requirements.

Four maintained scenarios expand the inventory from 35 to **39 distinct cases**. `native-auth` runs all seven account cases; `native-auth-schedules` selects only the four new cases. This is a maintained subset of the same real-app runner, not another harness or an alternate render path. No shipping behavior, dependency or UI design changes were made. The only application addition is a harness-feature-only Cancel operation for the existing fixed fixture, under the existing isolated main-window/profile/marker-port guard.

## Complete accepted procedures

| Check | Evidence required and observed |
|---|---|
| 42 | Actual CLI packs/doctors developer A, followed by real registration/enablement and account A reads. Production developer WebSocket reload, disable and native Disconnect each cancel B sign-in at both the waiting-callback and held-code-exchange boundary: six schedules. Each requires a native cancellation/changed-owner error, zero candidate publication according to independent vault metadata counts, actual late connection refusal and successful rebinding of the old callback port. Fresh A login/read/restart succeeds after each teardown. |
| 43 | With a real A read and pending Agent confirmation, actual Disconnect removes A's connection and worker. Connecting B then refusing the old A approval produces zero tool dispatch/read. A newly approved tool returns actual B JSON through the broker/worker/model; restart still reads B without another login. |
| 50 | Explicit production pending-flow cancellation, actual provider denial and actual token-endpoint HTTP failure each leave the prior selected A grant/count intact. Harmless A reads and owned restart/read identify the retained account after every failure. A subsequent successful B switch refuses A's pending approval without dispatch, and fresh B read/restart succeeds. Existing 23 normal-build auth tests retain candidate publication, failed-save rollback, supersession and cleanup proof; runtime tests do not damage the real registry to force those schedules. |
| 51 | An actual token response granting a different scope rejects B and stores/selects no candidate; A read/restart still succeeds. A genuine two-second OAuth grant without a refresh token reaches its stored production expiry, requires reconnect, refuses loading before dispatch/read/refresh, and retains that refusal after restart. Reconnect with declared scopes performs a real B read. A second short-lived grant with refresh reaches actual expiry, performs exactly one real refresh exchange and one approved A read, without another authorization-code exchange. Restart/read reuses the recovered grant without another refresh. |

Each new batch verifies **32 actual account/owner replies and two explicit unusable-account refusals**: cancellation 14 replies, explicit switch 3, failed-switch 9, expiry 6. Cancellation verifies six restart reads, failed-switch four, explicit switch one, expiry two; the unrefreshable refusal has an additional restart. Provider receipts verify exactly one expiry refresh. A badge or model-written claim alone cannot satisfy these assertions. Pending commands have rejection handlers attached immediately; driver IPC deadlines and generic errors cannot pass a native cancellation assertion.

The controlled provider is outside Grain. Production declaration review, OAuth/PKCE, callback handling, exchange, binding, Windows vault, registry publication, tool discovery/loading, one-use approval, broker authentication and worker execution remain in the application. Real CLI and WebSocket paths remain unchanged in authority.

## How execution became faster

- Reuse the existing guarded account fixture and verified account-read/owner controls. Build the actual application once for the new feature-only Cancel operation; reuse its stamped binary for the runtime repeats and faults. Definitions retain a separate runner fingerprint.
- Run related cases in one owned app/profile with explicit per-case fixture setup/cleanup. Keep required restart transitions and failures. Runtime runs remain serial because Windows app/event/CDP resources are shared.
- At race boundaries wait for an observed consent handoff or held provider exchange, rather than guessing a sleep duration. Require cancellation settlement, exact metadata/read/exchange effects and port release before continuing.
- For expiry use real short-lived grants issued by the external controlled provider and wait for the actual production expiry timestamp. The provider also refuses expired access credentials. No wall-clock override, production timeout reduction, vault edit or token export is used by these cases.
- Overlap independent Node/static verification with the build. Complete native compilation before app runtime to avoid Windows resource locks. Repeat the new subset after the focused review/faults; run the complete affected native-auth suite once for regression coverage. Unchanged microphone/idle suites are not repeated for this auth-only change.

The final four-case repeat takes **44,956 ms of scenario time**, excluding launch/cleanup and the build. This is a measured result, not an estimate for the remaining MCP/live-account procedures. Requirements, checks and audit gates were not removed.

## Focused audit and retained observation

Graph overview, affected flows, change detection and review context were queried first. The graph returned older B1b nodes and broad/unrelated bindings impact, with no indexed flows answering the new schedules. The new JS module was unindexed. Scoped direct source/diff reads therefore traced the new controls and production `connect_attempt`, callback, `publish_native_account`, `disconnect_reviewed`, `access_token`, command disable/import and developer reload teardown. No absence-of-edge or graph risk score is treated as test/security certification. RTK was not installed; raw command fallback was used.

- Cancel calls only production `cancel_extension` for the fixed fixture; it cannot approve declarations/tools, select an account, expose secrets or target an arbitrary ID. Normal builds do not compile this operation. Isolation/URL/old-handoff feature guards passed.
- Shared controls reuse actual permission review and verified results. Successful reads require one approval/dispatch and one provider read; refused reads require the exact loader account refusal and no dispatch/read/refresh. Failed-login drivers require observed production errors, not generic deadline success.
- Provider controls add only external failure/refresh/expiry behavior. PKCE, redirect/client/scope/token-path assertions, TLS and response bounds remain. Partial scopes are still a specific fixture policy, not universal issuer compatibility. Short-lived access tokens carry real server expiry; refresh supplies a fresh longer-lived response. Secret markers are private to the provider/production broker and absent from final evidence/model context.
- Held responses are owned and released on response close or cleanup. Callback rebind listeners always close. CLI process/socket/host cleanup is unchanged. Failed cases stop their batch; other cases remain Not run, and no call/scenario is automatically replayed to convert failure into success.
- Metadata-only vault enumeration retains its exact run namespace/key family and never reads blobs. Cancellation/failed/partial candidates must not increase the count or replace the prior selected grant. Independent end-of-run cleanup still validates zero remaining credentials.

**Earlier `run-yEL41k` is retained as Fail.** The driver assumed that reimporting an unchanged installed package was developer reload. Its waiting Connect reached the driver's 10-second IPC deadline; cleanup Pass, no candidate exchange and vault remaining zero. Source review showed that this import path does not proactively cancel that pending flow. The driver was corrected to use an actual CLI-built developer project and the production WebSocket reload specified by check 42. No production bug repair or unchanged-installed-reimport cancellation certification is claimed. Keep that import distinction available for the later lifecycle/account review.

Two primary implementation references were rechecked for observable disposal/ownership assertions: [LibreChat session disposal races](https://github.com/LibreChat-AI/LibreChat/blob/main/packages/api/src/mcp/__tests__/MCPConnectionDisposeRace.test.ts) and [Goose extension manager](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/extension_manager/mod.rs). The external expiry/refresh fixture follows the token-response and refresh concepts in [RFC 6749 sections 5–6](https://www.rfc-editor.org/rfc/rfc6749). These references inform test design; they do not certify Grain or replace its host policy.

## Evidence

Windows x64, Node `v24.11.1`, WebView2 `154.0.4258.48`, controlled localhost HTTPS provider/scripted model. Reports are ignored under `tests/agent-harness/.runs/`.

| Run | Result |
|---|---|
| `run-XutlHf` | Initial corrected four-case batch Pass: 20,535 / 3,262 / 11,190 / 10,797 ms; 2 October 00:56:39 UTC; cleanup Pass |
| `run-HIpvcC` | Complete seven-case native-auth regression batch Pass; new cases 21,136 / 3,339 / 11,402 / 10,931 ms; 01:00:32 UTC; cleanup Pass |
| `run-Mvz6ps` | Deliberately skipped cancellation Fail/exit 1, 12,931 ms; rejects the driver's IPC deadline instead of accepting a cancelled result; cleanup Pass |
| `run-loOwuO` | Deliberately supplied full consent where partial refusal was required: Fail/exit 1, 1,755 ms; actual new account selection detected; cleanup Pass |
| `run-Bf7f3b` | Final fresh-profile repeat of all four cases Pass: 20,167 / 3,108 / 11,019 / 10,662 ms; 01:03:59 UTC; cleanup Pass |
| `run-EzhQU6` | Final ordinary cold/warm tool and declined-approval smoke Pass: 1,324 / 950 ms; markerless auth control refused; cleanup Pass |

All final runs share actual host SHA-256 `ef08cc2575b5b1ed5cd1e99747cbb93204d1314994668ac84a3b3b08fa502ead` and source fingerprint `026d58e20a60932d27e96ac646621fcc0c0839ef21da3c8aace4331a8fca39b6`, built at 00:54:33 UTC from base `cb33361831c590a106c27aa0b084182b7190f0e3` plus recorded dirty source. Actual CLI SHA-256 is `10babd722118db5ff80d79aa549355c381d2da750585de49eeb72f8ed068dd4a`, stamped against the same source. The unrelated pre-existing bindings edit remains uncommitted and participates in the build fingerprint.

The corrected first/full runs use runner fingerprint `80245fc65e0bc0615f7850922ac5803be95718dbf45e19ab9b473982a2cb83b7`. Advertising the new subset in CLI help then changes it to `e35adb344b8f621aa2758a63d87cb33bdca266558b1b48761451ee74e9fc4799`, used by both faults and the final repeat. Earlier evidence is not relabeled. After those runs, Prettier restores required line endings in `run.mjs`; an exact comparison proves the source text is identical after normalizing CRLF/LF. This changes only the definition-byte fingerprint to `c69d228155a6a6731b68f32a0f2f7ea2c3b12977524c59c62f2f1c55f92f224f`. The recorded account/fault identities stay unchanged. **23 normal-build auth tests** (`logic-uhY4od`), **three feature guards**, **20 runner self-tests**, normal Rust check and feature Clippy (no diagnostics in the modified Rust file), actual frontend type/build and harness/CLI builds pass. Existing normal/harness build warnings are retained. Scoped formatting, whitespace, evidence privacy/scratch cleanup and final document/ledger checks are completed before commit.

All final run credentials are independently cleaned to remaining zero. The new developer cancellation unit needs deletion of **one discarded developer grant**; the combined suite needs four (three earlier owner-unit grants plus this one). This is the previously documented developer-vault orphan gap, not a newly fixed production reconciliation guarantee.

```powershell
npm run test:agent -- --suite native-auth-schedules
npm run test:agent -- --suite native-auth
npm run test:agent -- --scenario native.auth-cancellation --fault uncancelled-login
npm run test:agent -- --scenario native.auth-expiry --fault accepted-partial-consent
```

Use the maintained Windows/WebView2/Node/Python-cryptography prerequisites and an idle desktop. Build once with `tests/agent-harness/build.ps1` after app-source changes; runner-only changes require no app rebuild. Both fault commands must return Fail/exit 1 and cleanup Pass.

## Remaining order and limits

No new manual batch is needed for these four controlled native procedures. Next finish **44** using held refresh/logout and two distinct native provider identities in the same real app, then review/repeat that unit before MCP B2. Remaining baseline: **B1 1 + B2 5 + B3 7 + B4 3 = 16**. None of the original acceptance objectives was dropped.

Do not promote one-provider expiry success into provider independence, refresh/logout or vault-write crash certification. Live account/browser consent, cross-platform vaults, genuine-model judgment, microphone/pill/shortcuts, measured resources, orphan reconciliation and the retained Escape investigation remain separate. Human-assisted actual MCP login/restart/expiry check 12 still requires a real provider/account; the controlled provider cannot close it. Finish baseline tests before SDK/OAuth replacement or new platform implementation.
