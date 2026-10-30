# Native OAuth library reuse: B5b

3 October 2026. Scope: standard native-extension authentication only. MCP continues to use `rmcp`'s authorization manager. This slice does not change tool-calling policy, remove retired extension capabilities, delete MCP wrappers, certify real Linear refresh, or close the seven release gates.

## Plan and ownership

1. Trace native connection, callback, exchange, refresh and account publication before editing. Graph search finds `grain_auth.rs`, but impact/review reports contain zero affected flows and stale entities such as the removed parser; use source traces and actual tests rather than interpreting those reports as complete coverage.
2. Promote the already locked `oauth2` exact 5.0.0 dependency to normal builds with defaults disabled. Replace manual authorization parameters, UUID-based state/verifier generation, PKCE hashing and duplicate grant-request construction. No additional dependency version, HTTP stack, credential format, timer or resident service is introduced.
3. Keep Grain's transport, raw-grant validation, callback listener and account/vault ownership rules. Run normal logic, ordinary compilation, maintained real-app acceptance, fault detection and cleanup inspection. Audit the accepted slice before commit/push.

The previous [reuse plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md) and [multi-project review](MCP-AUTH-LIFECYCLE-REUSE-AUDIT.md) select the library. Current primary references are [oauth2 5.0 documentation](https://docs.rs/oauth2/5.0.0/oauth2/) and its [official repository](https://github.com/ramosbugs/oauth2-rs). The documentation explicitly supports a custom asynchronous HTTP adapter and requires disabling redirects. Published crate source under the normal Cargo cache was inspected for endpoint request construction, response parsing, PKCE and token accessors. This is library reuse within Grain, not adoption of an agent framework or a second MCP OAuth implementation.

## Resulting boundary

| Concern | Owner after this change |
| --- | --- |
| Authorization URL, encoding, scopes, extra parameters, S256 PKCE | `oauth2` |
| Random state and verifier | Library cryptographic randomness; 32-byte state and 32-byte verifier input |
| Code/refresh grant form construction and standard response parsing | `oauth2` |
| HTTP client, TLS fixture scoping, destination check, redirects, deadlines and response budgets | Grain adapter using the existing reqwest 0.12 dependency |
| Raw scope syntax, Bearer/header safety, required scopes, refresh credential validation, expiry bounds | Existing Grain validation, before SDK normalization and before publication |
| Account session/revision, declaration fingerprint, generation checks, refresh lock, failed-switch rollback | Existing Grain ownership rules |
| OS vault and serialized `TokenSet` | Existing Grain format and namespace; no migration or fallback auth implementation |
| Callback path/host/state/code checks, listener cancellation and disposal | Existing Grain callback/flow rules |

The adapter performs only a POST to the declaration's parsed token URL. It preserves the existing 20-second HTTP timeout, no redirects, 64 KiB declared/streamed body limit, scoped harness TLS and no persistent client pool. URL comparison uses parsed URLs so harmless URL canonicalization does not reject an otherwise approved endpoint. It forwards SDK-generated request headers, not provider response headers. After strict JSON/grant validation it supplies JSON to the SDK irrespective of provider Content-Type, preserving the old parser's compatibility. The old host accepted validated 2xx responses whereas the SDK requires 200: normalize only after validating the actual successful status and grant. The real HTTP logic test returns 201 without Content-Type for code exchange and 200 for refresh to exercise that compatibility deliberately.

There are two bounded parses of a successful reply: Grain validates the raw scope syntax and the SDK parses the standard token response. This deliberately preserves rejection of malformed scope spacing rather than trusting normalized scopes. It is not a second grant flow or credential store. The only new boxed future is per HTTP request to satisfy the SDK async-client trait and the host's `Send` requirement; it does not create a background task or retained state.

SDK errors and reqwest errors are never displayed with Debug/Display. Transport/parse failures use static safe messages; the existing bounded provider-error code filter remains. An SDK parse error's owned raw body is explicitly zeroized. Valid token type spelling is normalized to `Bearer`, with unchanged semantic checks and persisted field names. Missing refresh credentials/scopes retain the prior accepted values as before; no automatic retry or action replay is added.

Host-owned raw buffers, temporary host token fields and persisted credentials retain zeroizing wrappers/drop behavior. SDK and reqwest secret-bearing allocations are not guaranteed to zeroize; do not claim complete in-memory erasure. SDK request/response buffers and objects are short lived and not logged or persisted. Memory profiling remains a separate release gate.

## Verification and audit

**Disposition: B5b library reuse is accepted within the existing controlled native-auth boundary.** Real-provider interoperability, the deferred Linear test and whole release certification remain open. No numbered acceptance result is inflated. A corrected intermediate build was rejected by the maintained stamp check because its source changed during compilation; it was not used for acceptance. Intermediate compiler/test failures remain in ignored `.runs/` reports rather than being presented as passes.

| Evidence under `tests/agent-harness/.runs/` | Result |
| --- | --- |
| `logic-3fUaAJ` | 24 normal native-auth logic tests Pass; actual encoded code/refresh wire fields, validated 201/200 replies, binding, scope/credential/expiry refusal, redirect/declared/chunked limits, safe diagnostics and ownership |
| `native-oauth-feature-tests.log` | Same 24 tests Pass with `agent-harness` enabled |
| `native-oauth-check-final.log`, `native-oauth-build-accepted.log` | Ordinary binary check and maintained full real-app build Pass, including TypeScript/Vite/CLI |
| `native-oauth-selftest-accepted.log` | 69 harness self-tests Pass |
| `native-oauth-clippy.jsonl` / `.stderr` | All-target Clippy exits 0; 176 warnings across unchanged targets. Two distinct native-auth warnings repeat across targets and match the previous SDK audit's warnings in untouched code; not a warning-free claim |
| `run-vX7JB5` | Eight native-auth cases Pass, 187 observations, 168 peer requests, cleanup Pass; before the final validated-2xx compatibility correction |
| `run-mTmKDe` | Final build: all eight native-auth cases Pass, 187 observations, 168 peer requests, cleanup Pass |
| `run-NlYbqZ` | Final build: all ten MCP-auth regression cases Pass, 138 observations, cleanup Pass; no actual live-provider grant |
| `run-jMkhMb` | Final build: all six existing shared Agent workflow cases Pass, cleanup Pass; no tool policy or scenario changes |
| `run-nqcif2` | Expected Fail/exit 1: `native.auth-expiry --fault accepted-partial-consent` detects “Rejected login selected a new account”; cleanup Pass |
| `native-oauth-cleanup.json` | Independent completed-scope inspection: five scopes, 96 recorded host PIDs, twelve owned marker/event ports, zero live owned hosts/listeners or scratch children, five metadata-only grant/secret inventories all zero |

Final host SHA-256: `7295cda79e195568b27027a375992a0a49e028d253ed3abba7dd0c70c32bdd7a`. Source fingerprint: `d1c59d186cbb01873d93b63d494ebf58d42813b216694b99f49f53d98bda3dd9`. Runner fingerprint: `a3c91dea8a2f726d4185e2a65fcb3ebb69b70e6654317bba4da081d76060241b`. Build base is `b687fa95` plus the tested working tree, including the preserved unrelated `src/app/bindings.ts` change, which is not committed in this unit. Formatting/whitespace checks pass for changed source; pre-existing repository-wide formatting debt is not fixed. Cargo.lock is unchanged.

Source review confirms unchanged persisted fields, declaration validation requiring nonempty scopes and rejecting reserved overrides, callback ownership, refresh publication guards and permission boundaries. The SDK's response parser requires 200 and can retain raw bytes in errors; the focused compatibility/redaction correction handles these before final acceptance. Graph review alone is insufficient: it reported stale removed entities, zero flows and incomplete gaps. No changed auth transport, publication or cancellation path is accepted solely from compilation or graph risk scores.

Reproduction, from the repository root:

```powershell
node tests/agent-harness/production-tests.mjs --group auth
node --test tests/agent-harness/*.test.mjs
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite native-auth
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --suite agent-workflow
# Expected Fail/exit 1 with cleanup Pass:
node tests/agent-harness/run.mjs --scenario native.auth-expiry --fault accepted-partial-consent
```

Run ordinary `cargo check --locked --bin handy` and `cargo clippy --locked --all-targets --features agent-harness` from `src-tauri/`. Feature-test reproduction uses the maintained Windows DLL runner with `cargo --config 'target.x86_64-pc-windows-msvc.runner=["powershell.exe","-NoProfile","-File","C:/Projects/Grain/grain/scripts/run-rust-test.ps1"]' test --locked --lib --features agent-harness grain_auth::`. No deadline or stored credential is edited to accelerate these tests. Do not run multiple real-app suites concurrently.

The first MCP regression `run-uZZ7J8` lost its execution session after nine case observations. It has no completed teardown and supplies no final-suite acceptance. Recovery found no owned host/WebView or marker/event listeners; maintained scoped cleanup deleted one grant and left zero grants/client secrets. Automatic approval review rejected verified-root deletion of `data/fixture/mcp-tls` with “blocked by policy.” These folders remain Pending; no alternate deletion route was used. The last CDP port lacks a checkpoint. A fresh full run is required. Its ignored `evidence/interruption.md` records the exact marker and recovery limits. The earlier pending roots remain untouched; passing new-run cleanup does not close these exceptions or production orphan recovery.

The native fixture now treats state as an opaque sufficiently long URL-safe nonce rather than enforcing the obsolete UUID-hex representation. Exact callback state equality, independent challenge/verifier hashing, account ownership, transport receipts and all existing cleanup assertions remain in force. No scenario ID, acceptance count or production timeout is shortened.

## Remaining limits

- Genuine real-provider native OAuth and refresh recertification require an appropriate provider/client and account-owner consent; disposable local TLS peers do not certify that interoperability.
- Baseline check 12 stays **Deferred**, not passed. Actual Linear MCP refresh remains a later owned-grant procedure.
- Configured-client issuer ownership, typed reconnect/transient error classification and Grain-owned client metadata registration remain subsequent focused units.
- Physical obsolete-code retirement and wrapper removal stay on hold. No work enters the Handy-derived tree.
- No new temporary harness engine or production migration scaffold needs retirement. Existing harness-retention exceptions and official initialization Blocked status are unchanged.
