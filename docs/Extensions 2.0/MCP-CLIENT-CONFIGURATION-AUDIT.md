# MCP client configuration and stale approval audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Precommit base:** `1d2424983bcc0451cb6d8d3398c350e17405f3dd`. **Status:** focused audit, final combined acceptance, clean repeat, negative oracle and affected regressions complete. Checks **11 and 13 accepted**: **46 Pass / 7 Pending** (15 human, 31 reviewed automated). No new manual batch. Whole B2/B3 and all seven release gates remain open.

## One setup, two distinct requirements

`mcp.auth-client-configuration` covers checks 11 and the missing client-configuration portion of 13 in the real Grain application. Existing actual account-switch and disable/re-enable evidence is retained for the other portions of 13. The maintained runner uses actual settings commands, locked `rmcp 3.1.4`, its actual metadata/PKCE/resource/issuer/callback flow, scoped Windows grant/secret stores and the real Agent's discovery, selected loading and approval. The fixed Client target is debug-only, shares the existing owned resource/issuer and requires the isolated authenticated marker. Three fixed preregistered IDs and fake secrets model explicit server-supported public/confidential registrations. They never configure ordinary providers or personal credentials.

The independently recorded schedules are: missing client/invalid input refusal; empty-secret public login/read/restart; public ID change; transition to confidential registration; same-ID secret rotation; secret removal with real `invalid_client` refusal; and supported public recovery. Each of the four changes has its own stage, actual grant removal and disabled state, exact scoped secret count, an old pending confirmation refused without a wire call, genuine A/B recovery and restart. Five approved account logins supply ten actual reads; four stale confirmations supply zero calls. Every restart preserves configuration and actual account without a new exchange. Preregistration must never issue DCR.

The confidential issuer actually checks the SDK-supplied secret against the configured fake version; counts alone cannot certify the matching secret. Removing the secret deletes its separate OS entry and invalidates the old grant. An attempted confidential login then acknowledges the valid callback but the awaited SDK token exchange fails: the issuer issues zero tokens and account/enablement stay absent. Recovery uses only the explicit server-supported public client. No arbitrary credential is invented to bypass rejection.

## Ownership and focused audit

No ordinary production account, authorization or transport implementation changes. Rust edits only add a feature-gated fixed preregistered catalog item, existing TLS/consent target selection, two finite Agent instructions, and retention of only that run's client setting across real restarts. The ordinary client-ID clearing harness bootstrap previously prevented this persistence test; no ordinary user setting is modified. No new engine, listener, process, shared-account gateway, mock Tauri UI or dependency upgrade.

External issuer maps remain bounded (32 client/token entries; 16 codes/callbacks; 1,024 journal entries), dropping at teardown. Fixed preregistration entries account for the modest map change. Secret/code/token/authorization values never enter model receipts or journals; only constant refusal and bounded account/version flags do. Shared issuer/resource is intentional for client-configuration tests and supplies no acceptance for independent-provider check 14.

`auth-cleanup.ps1` continues to require the exact run UUID/marker, isolated service suffix and fixed credential key pattern. It now inventories grant and secret namespaces separately and independently removes both at final cleanup. Native service/key matching is unchanged. It reads credential metadata, never credential blobs. A non-MCP secret request is refused. No cache of inventory results or softened cleanup assertion is introduced.

Graph overview/file summary/search/impact preceded source reads. Changed-file/review context reported missing flows and coarse test gaps instead of exact assertions; direct tracing therefore covered settings invalidation, client-secret storage/deletion, SDK preregistration and auth-method selection, actual callback sequencing, restart filtering, selected-provider model routing and ownership cleanup. Graph gaps are not evidence of correctness.

Source review verifies that real `mcp_set_client_credentials` invalidates its ticket and old grant, disables the provider and saves the new ID/optional secret. The issuer checks actual PKCE/resource/client and token authentication. The model checks exact provider metadata and returned account/nested JSON, refusing another provider, unknown failure or replay. Final full-case totals catch extra calls/results between stages. The deliberate skip-client-change fault must fail immediately at old-grant removal, before reauthentication or a wait can obscure the reason.

## Verification and retained failures

All 49 normal-build MCP tests pass (`logic-YzLDwH`), all eight feature guards pass through the maintained Windows manifest adapter (`.runs/client-config-guards.log`), and all 38 runner self-tests pass with zero skipped. New self-tests refuse cross-provider selected metadata and actual issuer mismatched-secret token issuance while requiring privacy-safe evidence. Frontend type/build, maintained real-host build, scoped formatting/whitespace and feature-backend Clippy pass. Clippy retains 74 existing warnings; none references the changed harness/MCP Rust seams. The pre-existing `src/app/bindings.ts` changes remain untouched/excluded.

Initial direct feature-test invocation omitted the maintained Windows manifest runner and failed before any tests with `0xC0000139`. The same tests through `scripts/run-rust-test.ps1` pass eight checks without an application change; the failed invocation receives no acceptance credit.

`run-GHUYuL` correctly failed a harness assumption during secret removal: HTTP 200 acknowledges a syntactically valid callback before token exchange, so it cannot be required to be HTTP 400 for a later exchange rejection. Actual issuer refusal, no token/grant, awaitable SDK failure and cleanup Pass were already present. The repaired assertion requires HTTP 200 acknowledgment plus the actual awaited exchange failure, zero issuance/grant and reusable listener. No provider response, production behavior or required login acceptance was weakened. Its early stages remain partial history, not acceptance.

`run-mEQzMs` passed the five preceding OAuth cases and two client stages, then exhausted the fixture's 16-entry callback owner set. The code incorrectly retained ownership after delivery. Each private callback owner is now consumed once before its actual network delivery, including late refused deliveries; its issuer code stays valid so Grain still has to reject a cancelled login. No bound, production callback behavior or token protection is relaxed. The complete final six-case repeat below proves the repair. This failed combined run has cleanup Pass and no acceptance credit.

| Run | Verdict / boundary |
| --- | --- |
| `run-qWImsj` | Full client case Pass: 20 observations, ten actual approved reads, four stale refusals, five real restarts, 124 peer and 147 issuer entries. Cleanup Pass; zero scoped grant/client secret/session/held response. Initial supporting result; final repeats/regressions required. |
| `run-U3PBGK` | All six OAuth cases Pass together; 99 observations, 1,165 peer and 847 issuer entries, 70 actual tool calls. Client case requires exactly ten successes/four stale refusals/no unknown, no DCR and zero exchanges after each restart. Cleanup Pass, zero grants/client secrets/sessions/held replies. |
| `run-cwMGgp` | Full client case Pass again in a fresh profile after the final callback-owner repair; all 20 observations and exact receipts pass, cleanup Pass, no scoped grant/client secret/session/held response remains. |
| `run-bUf7TW` | Deliberately skipped client change fails immediately with `Client change did not remove the old grant`/exit 1 at the public-ID change; cleanup Pass, zero scoped grant/client secret. No acceptance credit. |
| `run-6erjxp` | Deliberately abandoned actual scoped confidential secret causes Fail/exit 1. Independent end-run cleaner deletes exactly one real secret; `clientSecretsDeleted: 1`, remaining secret/grant zero, cleanup Pass. No acceptance credit. |
| `run-bGMpA9` | All eight native-auth cases Pass; shared vault cleaner/model behavior remains correct. Cleanup Pass; independently deletes four discarded scoped grants, none remaining. Production orphan reconciliation remains open. |
| `run-f8V80F` | Both ordinary Agent smoke cases Pass; cleanup Pass. |
| `run-yW6zqA` / `run-Z6ConR` | Real MCP transport contract and mixed catalog cases each Pass in a separate profile; new disabled fixed Client provider does not pollute selected tools/approvals. Cleanup Pass. |

Final runtime identities: host `2b9afd277ac7869a2483f6779a1011119e71a2cf2304cdad364fd19ac4308d1c`; source `f9b02d8547f2b9e64ab28b79f096bae29a5463b0a10efaddef520ed55f944893`; runner `731fdc8b8ef17e3a05beacb06a6c0f4b5e68c2fedf88a0cbb0359de2d7db8155`. Reports are retained under `tests/agent-harness/.runs/<run>/evidence/`. Code/fixtures are frozen throughout final runtime verification. The earlier standalone result precedes the callback-owner repair and is supporting history only.

Independent inspection of all eight final roots verifies matching identities, cleanup Pass, no private MCP marker, zero scoped grants/client secrets and only marker/evidence retained. No matching owned application/WebView/provider remains and listener 17124 is closed. An initial broad process-text check matched its own inspection shell; the final exact application/provider-name plus run-root check excludes that shell and confirms disposal. Neither failed earlier run is relabelled Pass.

Check 13 combines actual A/B account-switch receipts from `mcp.auth-fixture`, authenticated disable/re-enable refusal from `mcp.auth-shutdown`, and the four final client changes. The same six-case final run verifies all portions with genuine fresh reads, while check 11 uses only actual issuer-supported public/confidential preregistrations and explicit unsupported-secret refusal. This closes two complete objectives without merging their meaning or deleting a requirement. Official conformance/long-clock/public-live evidence from the prior shutdown audit remains historical and is not newly certified here.

## Efficient verification without less coverage

Compile once for a stable slice. One six-case OAuth run covers the prior account-switch, denial, late-callback, Agent-close and shutdown paths plus client changes; retain distinct case/stage verdicts and stop on failure. Repeat the new complete case in a fresh profile, detect its deliberate fault, and run shared native-auth cleanup/model regressions, ordinary Agent smoke and actual transport/catalog cases. No transport deadline/correlation/budget or ordinary authorization algorithm changed, so repeating the unrelated four 45-second and two 90-second waits adds no new evidence here. Their final unchanged-path evidence remains in the shutdown audit. Run native checks before/after runtime, never during it.

```powershell
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault skip-mcp-client-change
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault abandoned-mcp-client-secret
node tests/agent-harness/run.mjs --suite native-auth
node tests/agent-harness/run.mjs --suite smoke
node tests/agent-harness/run.mjs --scenario mcp.transport-contract
node tests/agent-harness/run.mjs --scenario mcp.mixed-catalog
```

Retain the Client target, fixed issuer modes, secret inventory and exact case/oracles until equivalent final-path real-app coverage exists. Remove the replaced seam and references together; keep regression objectives. No unused refresh/extra-provider scaffold is built. Run profiles, TLS files, fake credentials, maps, sockets and owned processes expire at teardown; privacy-safe evidence persists. Earlier policy-blocked scratch roots remain separately Pending, with no new deletion route attempted.

## Primary-source references and limits

The [current official client-registration source](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/authorization/client-registration.mdx) distinguishes pre-registration, CIMD and compatibility DCR and requires issuer association. The [locked Rust SDK OAuth documentation](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.1.4/docs/OAUTH_SUPPORT.md) describes preregistration/client-secret inputs and injectable SDK HTTP. [Goose's OAuth host integration](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/oauth/mod.rs) also supplies explicit client configuration around the SDK. Grain retains its own current-ticket, exact-approval, metadata/issuer, profile and vault boundary; these references do not replace acceptance. Locally locked `auth.rs` was checked for preregistration and `client_secret_post` selection instead of assuming today's upstream matches 3.1.4.

No live browser consent/denial, real-account documented refresh, independent MCP provider, CIMD certification, new ordinary catalog provider or live model result is implied. Live requirements 9/12/17 and independent/Agent requirements 14/46–48 remain separate. Baseline acceptance still precedes dependency replacement or new platform features; all whole release gates remain open.
