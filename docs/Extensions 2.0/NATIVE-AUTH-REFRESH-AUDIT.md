# Native refresh/logout and provider independence: focused audit

**Date:** 2 October 2026 (India; run timestamps UTC). **Branch:** `extensions/tool-only-retirement`. **Base:** `196c8aae88c0f3007182fc192980e827c211dfab`, plus the recorded working tree. **Verdict:** check **44** has reviewed real-app automated Pass evidence. The numbered native foundation requirements B1a/B1b/B1c are now accepted: **38 Pass / 15 Pending** overall, comprising 15 human and 23 reviewed automated checks. The retained intermittent Escape observation, production orphan reconciliation, live providers and all seven R0–R6 release gates remain open. This is not whole authentication or whole Block 2D certification.

## Scope and procedure

One maintained case, `native.auth-refresh-logout`, brings the inventory to **40 distinct scenarios** and `native-auth` to **eight**. The existing `native-auth-schedules` subset still selects four. There is no SDK/dependency upgrade, OAuth replacement, new platform capability or UI design change.

1. Import and explicitly review the fixed installed native fixture. Receive a real two-second OAuth grant, wait for its actual expiry, and approve the Agent's account read. Hold the actual external refresh response. Disconnect through production auth, verify the old worker is gone, and reconnect B before releasing the old response. The interrupted call yields one genuine unknown outcome, zero old API reads and exactly one dispatch/refresh; it is not replayed or reported as a verified account result. Fresh B reads identify the replacement account before and after actual host restart.
2. Start another actual production refresh with its response held. Disconnect and verify the actual run vault returns to its baseline. Release and **await the auth future**: it must fail rather than republish the old grant. Both disconnected status and vault count remain unchanged after completion. Fresh reconnect/read works.
3. Import and review a second fixed identity, `com.grain.harness.auth-peer`, with its own client, scope, authorization/token/API routes and actual vault grant. While the primary's refresh remains held, the Agent searches, selectively loads, approves and verifies the peer's actual B/peer result. The host session stays identical, the primary has no API read, and the peer performs no refresh. Only afterward release and await the primary's refresh. Both accounts read correctly after restart without additional authentication. Removing the primary deletes its credential while the peer still reads successfully. Final removal returns the actual vault to baseline.

The app has one active Agent request. The concurrent primary refresh uses a narrowly scoped **debug-only auth probe**, not a second Agent request: `Refresh` invokes production `grain_auth::access_token` for the fixed primary's current approved registry generation and immediately drops its zeroizing result. IPC returns only null on success. The peer still follows normal discovery, schema loading, approval, broker, worker and authenticated network dispatch. This proves auth-provider network independence; it does not claim concurrent Agent conversations.

Real short-lived provider grants and observable network barriers accelerate the test. Production clocks, timeouts, restart requirements and vault contents are not edited. New case scenario time in the final repeat is **13,690 ms**; the full affected eight-case suite totals **95,339 ms**, excluding launch/cleanup/build.

## Audit findings and disposition

- Graph-first discovery/change/review queries were used. The graph did not resolve the new runner flow or complete caller relationships; zero reported impacted flows is not evidence of zero impact. Scoped source review covered fixed controls, production auth preparation/generation checks, per-session refresh locks, guarded publication, authenticated native fetch, event recording, model oracle, runner cleanup and existing account controls.
- The optional target is a finite Primary/Peer enum, never an extension ID supplied by the runner. Peer controls refuse developer/legacy setup and Refresh. Exact peer pack admission requires the fixed identity/provider/client/scope/hosts/endpoints. Ordinary fixture authority remains unchanged. Connect, Disconnect and removal still call production commands; actual UI review remains mandatory.
- Each fixed provider has a separate bounded consent slot. Old flow cleanup can clear only its own UUID. Scoped trust uses the existing owned certificate and exact marker HTTPS port. The peer API client admits only its own `/peer/me` route; the primary retains `/me`. No OS trust-store or production HTTPS change is introduced.
- The auth probe cannot supply a host/generation, export credential data, select a grant or execute a tool. Production preparation still checks the reviewed enabled owner, current generation, host membership and selected session; stale refresh publication is refused after logout. No new background engine or persistent service is added.
- Provider code binds codes/refresh/access grants to their provider, client and token route. Reads refuse another provider's token. Journals remain bounded and contain only counters/stages/account labels. Existing sockets, maps, held responses and TLS scratch are released.
- The first attempt exposed a harness omission: receipts excluded the second fixed identity. Added that exact identity to the feature-only bounded recorder and included the extension ID in receipts. Peer dispatch assertions now independently check its identity; filters for ordinary fixtures remain compatible.
- The next attempt completed application assertions but failed the final model-error check: the account oracle understood failures/success but not production's honest unknown-outcome summary. Added an exact negative-result branch with `accountUnknown`, no `accountVerified` and no tool replay. An intervening shell encoding corruption was caught by the actual-app run and repaired with an explicit Unicode escape. Unit and real-app checks now agree on the production summary. No product outcome classification was changed.
- Fixed evidence retention when a final scenario assertion fails after observations have already been collected: preserve those observations when recording the failure. Earlier failed reports remain unchanged and are not relabeled as Pass.
- Metadata-only Windows cleanup admits exactly the two fixture key families under the exact run service/UUID; its 256-entry bound, credential-type checks and refusal of unexpected owned keys remain intact. It never reads credential blobs. The isolated new-case runs need zero final credential deletions. The full native-auth regression still needs deletion of **four previously discarded developer grants** from the older ownership/cancellation procedures. Harness cleanup is not production orphan reconciliation.

## Recorded evidence

Local reports are under ignored `tests/agent-harness/.runs/<run>/evidence/report.md` and `report.json`; immutable summaries and identities are retained here.

| Run | Result | Meaning |
|---|---|---|
| `run-f2Fe8Y` | Fail; cleanup Pass | Peer returned the actual result, but fixed recorder lacked peer dispatch receipts. No acceptance credit. |
| `run-klCNBc` | Fail; cleanup Pass | Application assertions completed; final scripted-model rejection detected missing unknown-outcome handling. No acceptance credit. |
| `run-9HA7tF` | Fail; cleanup Pass | Actual interrupted call caught the oracle's corrupted Unicode delimiter. No acceptance credit. |
| `run-8D1wOG` | Pass; cleanup Pass | First complete corrected case, 15,235 ms. |
| `run-29K04A` | Expected Fail/exit 1; cleanup Pass | `released-refresh` deliberately releases the primary early; assertion rejects peer independence without the held barrier. |
| `run-brwb89` | All eight Pass; cleanup Pass | Full affected native-auth regression; all four discarded developer credentials independently deleted, remaining zero. |
| `run-vo9rXc` | Pass; cleanup Pass | Final fresh-profile repeat, 13,690 ms; zero final vault deletions, remaining zero. |
| `run-cDW5MA` | Both Pass; cleanup Pass | Ordinary cold/warm tool and declined-approval regression, with updated receipt recording. |

Final host SHA-256: `2851390a825305017db6b86d7eca976ba4872b9d67143bc527fa082ab262e70b`. Source fingerprint: `8bba17f71e02bc4776114f3010f4db15b276cbed1d022dc13a158c96d4a8d169`, built at 01:34:47 UTC from the base above. CLI SHA-256: `10babd722118db5ff80d79aa549355c381d2da750585de49eeb72f8ed068dd4a`, stamped against that source. Final corrected/fault/full/repeat/smoke runner fingerprint: `1b013fa1cfdbe7f7962b88af3122af6b5bdb5f746520cace785a2d1cefc12f3c`. Earlier reports retain their original fingerprints. The unrelated pre-existing `src/app/bindings.ts` edit remains uncommitted and participates in source identity.

**23 normal-build auth tests** passed in `logic-Bj6BzF`. **Four final feature guards**, **20 final runner self-tests**, normal Rust check, feature Clippy, actual frontend type/build and harness/CLI builds passed. Clippy reports existing warnings, with no diagnostics in the modified Rust files. The final recorder repair was rebuilt and feature guards/Clippy were rerun before runtime. Normal auth logic itself was unchanged. Scoped format/whitespace, ledger consistency, report privacy and owned scratch/process cleanup are verified before commit.

## Next handoff

No new manual batch for this controlled native unit. The **15** remaining baseline checks are B2 MCP tools/transport **5**, B3 MCP authentication **7** and B4 Agent workflows **3**. Next build controlled B2 coverage through the current locked SDK and production wrappers. Complete baseline tests/audits before library replacement, physical cleanup or new features. Human-assisted live sign-in, documented provider expiry, browser UX, vault failures/crash/orphan recovery, retained input investigation and all whole release gates remain separate; none is waived by this Pass.

Maintained execution/commands: [forward plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md), [ledger](MCP-EXTENSION-PROGRESS.md), [runner instructions](../../tests/agent-harness/README.md).
