# Authentication and lifecycle: reuse review, 3 October 2026

**Forward update, 3 October:** the separate [registration-issuer unit](MCP-REGISTRATION-ISSUER-AUDIT.md) reproduces and fixes preregistered old-secret forwarding across authorization servers. Additional criterion 54 passes its focused review and final real-app regressions. The SDK keeps generic discovery/grant behavior; Grain keeps bounded OS-vault registration ownership and explicit credential Save. Typed recovery/status and public client metadata registration remain separate next units. Baseline remains 52 Pass / 1 Deferred; physical removal remains on hold. The original comparison and dated evidence below retain their scope.

**Subsequent B5b checkpoint, 3 October:** native standard OAuth now uses the selected exact `oauth2` 5.0.0 library. The separate [native reuse audit](NATIVE-OAUTH-REUSE-AUDIT.md) records final-path verification, compatibility corrections, unchanged vault formats and the interrupted-run cleanup exception. This document retains the original B5a research/checkpoint; next units are configured-client issuer ownership/typed recovery and Grain-owned client metadata registration. Check 12 and physical retirement remain Deferred/on hold.

Scope: hosted MCP authentication, credential ownership, connection setup and disposal, plus native OAuth library ownership. Tool selection, calling policy, model budgets and agent frameworks are outside this comparison. Base: `2c4c506a`, branch `extensions/tool-only-retirement`. The unrelated `src/app/bindings.ts` working change is excluded.

## Decisions and execution order

1. Keep the official Rust MCP SDK and Grain's small host adapters. Pin `rmcp` to `=3.5.0`, retaining the existing client/auth/HTTP features and disabled defaults. Certify this dependency change separately before changing native OAuth or authentication UX.
2. Keep native extensions and MCP extensions under the existing common account/approval boundary. Native OAuth standard primitives will move to `oauth2` 5.0 in B5b; the native worker, vault and owner rules stay Grain-owned. No gateway, JavaScript sidecar or replacement agent framework is justified by this review.
3. Defer baseline check **12**, the genuine live Linear token-expiry/refresh procedure, at the user's explicit request. Ledger: **52 Pass / 1 Deferred / 0 active baseline Pending**. It is not passed or waived for live-provider release certification. Completed runs removed their scoped grants; a future procedure must explicitly prepare a new grant and verify real expiry, refresh, restart and cleanup. Controlled real-expiry/rotation/refusal evidence remains useful.
4. Put B5c obsolete-capability removal on hold. Do not remove old host branches or deduplicate cancellation/bounds wrappers in this SDK slice. Existing capability refusals remain enforced; retained code does not restore extension privileges. Later physical retirement needs platform confidence and a separate approved slice.
5. After SDK acceptance, implement B5b as a separate tested/audited change, then authentication status/error classification and client metadata registration support as distinct units. Tool-calling improvements remain outside this research and are not silently pulled forward.

## Will Linear need a daily login?

The expected behavior is silent access renewal using the stored refresh token, including after restarting Grain. Linear documents expiring access with refresh rotation. Our actual isolated MCP grant had an 86,100-second access lifetime and an available refresh token. Those observations do not establish the lifetime of the refresh token or certify the live refresh exchange. [Linear OAuth documentation](https://linear.app/developers/oauth-2-0-authentication), [Linear MCP documentation](https://linear.app/docs/mcp).

Grain already delegates MCP token renewal to the SDK through `AuthorizationManager::initialize_from_store` and `AuthClient`, and saves replacements through the guarded OS vault. Controlled providers have verified actual expiry, rotation, restart and refused-refresh recovery. A user should need fresh consent when their grant is revoked, unusable or missing, or their account/client/permissions change. A temporary network or vault failure should not erase an otherwise valid connection. Live Linear renewal remains **Deferred**, not proven by provider documentation or a credentials badge.

## Pinned primary-source comparison

Inspected repository snapshots dated 2 October; source behavior, not star count, drives the choice. Kilo's inspected MCP core shares OpenCode ancestry, so those are not three independent authentication implementations. File links pin the reviewed revisions.

| Project | Observed implementation | Apply to Grain |
| --- | --- | --- |
| Goose, `591edd47cf2cfea4957d720c607cf2a4def8673d` | Rust `rmcp` 3.4.1 in the workspace; its own HTTP/auth wrapper and configuration-backed credential store. Handles post-refresh authentication failure and scope challenges around the SDK. | Confirms the current library family and need for host-owned storage/policy. Adopt truthful recovery classification; do not copy broader extension capabilities or transparent action retry. |
| OpenCode, `c42ae0d56b6f86f8df39d451d6d2cfe6414b3928` | Official TypeScript SDK OAuth provider callbacks, configured or dynamic clients, refresh/expiry persistence, server-URL binding, restrictive credential-file mode and file update locks. Reports authentication/client-registration requirements. | Keep SDK-managed generic OAuth and explicit account binding. Preserve our OS vault and generation guards rather than adopting a separate plaintext credential file. |
| Kilo Code, `76bcfd40be616a72f4697b3041565f322245b462` | Shared OpenCode-style OAuth integration plus a hosted client metadata document, enabled only for matching default public-client callback configuration. | Clear registration compatibility improvement: support a Grain-owned metadata document when the issuer advertises it. Do not reuse Kilo's identity/URL or fabricate a deployed Grain URL. |

Goose sources: [workspace SDK version](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/Cargo.toml), [HTTP authentication adapter](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/agents/extension_manager/streamable_http.rs), [credential adapter](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/oauth/persist.rs).

OpenCode sources: [OAuth provider](https://github.com/anomalyco/opencode/blob/c42ae0d56b6f86f8df39d451d6d2cfe6414b3928/packages/opencode/src/mcp/oauth-provider.ts), [credential persistence](https://github.com/anomalyco/opencode/blob/c42ae0d56b6f86f8df39d451d6d2cfe6414b3928/packages/opencode/src/mcp/auth.ts), [connection lifecycle](https://github.com/anomalyco/opencode/blob/c42ae0d56b6f86f8df39d451d6d2cfe6414b3928/packages/opencode/src/mcp/index.ts).

Kilo sources: [OAuth provider and conditional metadata URL](https://github.com/Kilo-Org/kilocode/blob/76bcfd40be616a72f4697b3041565f322245b462/packages/opencode/src/mcp/oauth-provider.ts), [hosted metadata identity](https://github.com/Kilo-Org/kilocode/blob/76bcfd40be616a72f4697b3041565f322245b462/packages/opencode/src/kilocode/mcp/client-metadata.ts).

## Existing protections and concrete gaps

| Area | Current Grain state | Next action |
| --- | --- | --- |
| OAuth discovery and PKCE | SDK metadata discovery; hosted issuer/auth/token/registration HTTPS policy; explicit S256; loopback callback ownership and stale-callback refusal. | Keep. Review SDK discovery changes against our real-provider and controlled fixtures. |
| Credentials and account replacement | OS vault, bounded/zeroizing serialization, generation-guarded publication and per-provider operation leases. Logout/disable beats old work; distinct accounts/providers stay separate. | Keep. No extra refresh engine or duplicate lock. Production discarded-grant reconciliation remains separately open. |
| Refresh failure classification | SDK renews stored grants; host errors/status do not yet distinguish every revoked grant from transport/vault failure. | Add typed classification using SDK auth errors. Preserve grants on transient failures; show reconnect only for actual invalid/revoked/mismatched credentials. Never turn this into automatic action replay. |
| Preregistered client issuer binding | Configured client IDs/secrets are provider-keyed; stored grants have SDK issuer binding. Ordinary operation refuses changed issuers through SDK initialization. Fresh explicit sign-in builds configured-client requests from current metadata without a separate persisted registration-issuer record. | Before broader confidential-provider certification, reproduce issuer A → B with an owned peer and verify no A client secret is transmitted to B. Preserve a registration-issuer binding independently of cleared tokens where needed. Source storage granularity is established; credential disclosure is **not** claimed without a wire reproduction. |
| Client registration | Curated dynamic and preregistered clients; SDK already supported metadata-document registration in 3.1.4, but Grain does not configure a metadata URL. | Add SDK `AuthorizationRequest::with_client_metadata_url` integration plus registration fallback tests. Deployment of the stable public HTTPS document and exact redirect inventory is a prerequisite, not accomplished by this upgrade. |
| Requested scopes | Existing provider/catalog and native declaration boundaries; no autonomous scope expansion feature is accepted. | Any future insufficient-scope recovery requires explicit user consent and renewed account/approval ownership. Do not grant every scope merely because another client retries broadly. |
| Disposal and latency | Finite operations, immediate host cancellation, 90-second absolute operation deadline including queueing, two-second cleanup, no idle MCP processes/connections. | Keep. Exercise modern/legacy JSON/SSE, authenticated shutdown, late results and dead connections with real clocks. Idle/active RAM certification remains open. |

The current **2026-07-28** MCP authorization profile recommends client metadata documents and retains dynamic registration as deprecated backward compatibility. Grain's missing metadata URL is therefore a concrete forward-compatibility gap. Retain working DCR/preregistered providers while adding the preferred path; this does not guarantee every provider supports one-click setup. The older profile remains relevant to supported legacy peers. [Current MCP authorization specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization), [legacy profile](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).

The registration profile also requires client credentials to remain associated with the issuing authorization server. The focused rotation follow-up above concerns configured registration credentials, beyond the already tested token/account/config-generation guards. Trace: `mcp_set_client_credentials`/`client_secret` → `connect_oauth` current metadata → configured `AuthorizationRequest`; compare with `authorization_manager`'s stored-grant issuer check. Do not label this closed merely because the SDK protects grant issuer changes. [Current client registration and issuer binding](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration).

## SDK 3.1.4 to 3.5.0: adapter audit

Reviewed both published crate sources in the Cargo cache, including `transport/auth.rs`, `streamable_http_client.rs`, `worker.rs`, `service/client.rs` and manifests. [Official 3.5.0 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.5.0) was published 28 September; Apache-2.0, MSRV 1.88, below this machine's Rust toolchain. Keep enterprise/JWT/server/macros features disabled. Only the `rmcp` package version/checksum should change in the lockfile.

- New credential-store error and optional refresh guard separate persistence failures from reauthorization. The credential fields/configuration format remain compatible. Our provider lease already surrounds SDK refresh and guarded vault publication; acquiring that same lease again from a refresh guard would deadlock. The SDK's default no-extra-guard behavior is retained. Cross-process credential coordination is not claimed.
- Both published versions already declare native-application registration, refresh grant support and conditional advertised `offline_access`; those are not missing features to rebuild. Grain's callback compares issuer strings exactly before accepting either success or error, including when `iss` is present without advertisement.
- Refresh now validates stored client identity, reloads within optional coordination, preserves refresh credentials when a response omits replacement, and maintains requested-scope resolution. Recheck account/config replacement, rotated refresh restart, missing refresh and actual `invalid_grant` without changing clocks.
- HTTP workers now support cancellable concurrent/control POSTs and optional bounded session recovery. Grain continues to set `reinit_on_expired_session(false)`, so automatic expired-session replay is disabled. Existing cancellation, wire bounds and operation-scoped DELETE ownership stay intact.
- Upstream now contains a similar discovery-error correlation repair, but Grain's bounded wrapper performs that decision before the bare backend. It is intentionally retained pending equivalent final-path evidence and the user's removal hold.
- No stored grant is migrated, no browser is opened by the dependency change, no extra background runtime is added. A debug binary-size comparison is useful build evidence, not a RAM measurement.

Graph tools were consulted first. Semantic search returned no matching auth nodes; the impact summary mixed unrelated entities and did not resolve SDK ownership. Direct adapter/cache-source inspection and maintained evidence provide the dependency trace. A graph summary alone is not a security or runtime certificate.

## B5a verification and handoff

Before accepting the upgrade: normal MCP logic tests, harness self-tests and isolation guards, ordinary compilation, stamped real host build, two clean `mcp-foundation` runs, an explicit failing oracle, public live reads/nested read, Linear no-consent metadata/cancellation and control guards, official production-wrapper conformance, scoped source/diff audit and cleanup inspection. Agent/native smoke regressions check the shared host; they do not expand tool-calling research or modify its policy.

Commands and final evidence will be recorded below after execution. Check 12 stays Deferred regardless of controlled-suite success. Known standalone conformance initialization and all seven release gates remain open until separately resolved. No new human test batch is needed for this dependency-only slice.

The SDK renamed its deprecated `ClientInfo` alias to `ClientConfig`; Grain's import/service/helper types now use that name with identical underlying initialize parameters. No call/auth algorithm changes. The initial selector review incorrectly treated separate HTTP/session source filenames as sibling Rust modules; direct `#[path]` inspection confirms they are nested under `grain_mcp`, already included in the existing 49-test selector. The provisional selector edit was reverted; no harness feature or test ID was added.

Static/build results: 49 normal MCP tests, 51 harness-feature MCP tests, six isolation marker tests and all 69 Node self-tests pass. `cargo check --locked --bin handy` and the stamped real-host build pass. All-target Clippy exits zero; 176 emitted diagnostics (107 distinct warnings across targets) point outside the changed MCP module. Whole-tree `cargo fmt --check` fails on unchanged `grain_audio_journal.rs`, `handy/managers/transcription.rs` and `rolling.rs`; direct `rustfmt --edition 2021 --check src-tauri/src/grain_mcp.rs` and the scoped Git whitespace check pass. Those unrelated files and user bindings are preserved.

Dependency/build evidence: the lockfile changes only `rmcp` version/checksum; its existing feature tree still excludes server, macros, JWT and enterprise extensions. `StoredCredentials` and `OAuthClientConfig` declarations are identical between published versions. Debug harness binary: **137,203,200 → 137,463,808 bytes**, +260,608 (+0.190%). No memory savings, idle working-set baseline or complete R5 resource certification is inferred. Actual fixture/session/process disposal and case latency are measured below; full resource profiling remains an open release task.

### Final evidence and scoped acceptance

**B5a SDK compatibility slice accepted.** No new regression was observed in the reviewed boundary. This is not universal MCP compatibility, a complete security certification or closure of R0–R6. B5b native OAuth is next; issuer-registration ownership and typed recovery get their own focused auth unit before broader provider certification, followed by metadata-document integration. Physical retirement remains on hold and live check 12 remains Deferred.

| Evidence | Result |
| --- | --- |
| `run-JQBgW7`, full `mcp-foundation` | 19 Pass, 258 observations, 172 actual calls; 593.727 seconds of scenario time; cleanup Pass. |
| `run-ZZF654`, fresh-profile full repeat | Same 19 Pass / 258 observations / 172 calls; 589.893 seconds; cleanup Pass. Actual four 45-second call and two 90-second discovery deadlines retained in each run. |
| `run-A2MLax`, six controlled Agent workflow cases | All Pass, including native/MCP selection and read/write/verify, directory/schema limits and stale/duplicate approvals. Cleanup Pass. |
| `run-1QEsmG`, `run-tIpb8n`, `run-C1yYKB` | MCP performed-write/lost-response, Stop and later model failure each Pass; uncertainty prevents retry and completed receipts survive. Cleanup Pass. |
| `run-WuoB5i`, ordinary native smoke | Both cold/warm and denied-call cases Pass, cleanup Pass. Native OAuth code/dependencies were unchanged; its full provider suite was not rerun in this SDK slice. |
| `run-22wnue`, public DeepWiki | Both real read/disable/restart and bounded large-read/recovery cases Pass, cleanup Pass. |
| `run-fCnVzv`, public Hugging Face | Fixed real nested read and actual restart Pass, cleanup Pass. No account or live model involved. |
| `run-pjk3pR`, `run-7ZrkMu`, Linear without consent | Two actual SDK metadata/DCR/cancellation cases and the consent/control guard Pass; zero scoped grants/client secrets remain. No browser login or token-expiry claim. |
| `conformance-JwGVEk` | Legacy and modern tools each have two official SUCCESS checks plus actual Grain acceptance. Standalone initialization remains **Blocked**: pinned fixture returns HTTP 200 with an empty discovery result, no initialization check emitted. Aggregate command is nonzero, cleanup Pass; not called a full conformance pass. |
| `run-ezlLTx`, deliberate accepted-catalog corruption | Expected Fail/exit 1 at `stateless-json-tool_count: management Test accepted incomplete catalog`; zero tools dispatched; cleanup Pass. The harness detects the fault. |

The two full runs use the same final binary, source and runner identities. Test-library compilation overlapped early stages of the first batch without replacing the owned host; the final repeat ran after all compilation/static checks had completed. Evidence is from the preserved working tree, including the unrelated bindings change, not a claim that those user edits are part of this commit.

- Binary SHA-256: `4e532ee277fa9c3cd70402517aec3062fb3bc184de6614a59c2305b50204cdbb`.
- Application source fingerprint: `688e3199a0d642cb3ac6e9275cdab75924493cbe8a363a7452b8dcaf829dfe14`.
- Runner fingerprint: `fc898b88c711067a343b18f9c421fd082617e5735d10602404198ebd486c1d0f`.
- Independent post-run inspection: **15 actual application scopes, 88 recorded host PIDs and 27 owned listening ports** checked; no owned hosts/listeners or disposable data/package/TLS children remain. Eight inventory-only vault/client-secret checks across the controlled and Linear namespaces each return zero. `tests/agent-harness/.runs/sdk-3.5-cleanup.json` retains that metadata-only inspection. No ordinary namespace was read or modified.

Reproduction, serially after compilation and with the desktop available:

```powershell
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/*.test.mjs
# Run twice, each from a new owned profile:
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --suite agent-workflow
node tests/agent-harness/run.mjs --scenario agent.workflow-unknown-mcp
node tests/agent-harness/run.mjs --scenario agent.workflow-stop-mcp
node tests/agent-harness/run.mjs --scenario agent.workflow-failure-mcp
node tests/agent-harness/run.mjs --suite smoke
node tests/agent-harness/run.mjs --suite mcp-live
node tests/agent-harness/run.mjs --suite mcp-hf-live
node tests/agent-harness/run.mjs --suite mcp-linear-preflight
node tests/agent-harness/run.mjs --suite mcp-linear-guards
# Two Pass / known initialization Blocked; nonzero, inspect the report:
node tests/agent-harness/conformance.mjs
# Expected Fail/exit 1 at the named catalog assertion, cleanup Pass:
node tests/agent-harness/run.mjs --scenario mcp.catalog-budgets --fault accepted-mcp-catalog
```

All reports stay under the ignored maintained `.runs/` directory; profiles, credentials, TLS keys and active fixtures are disposed. No new scenario, idle service, temporary platform scaffold or user testing assignment is introduced. Historical policy-blocked cleanup of `run-FCSQDD`/`run-t1ux8t` is untouched; this inspection concerns only the named new scopes. `AGENT-BUDGET-01`, live refresh, registration-issuer wire verification, public metadata deployment, orphan reconciliation, other-OS/live-model recertification and measured resource release gates remain explicitly open.
