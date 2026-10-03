# MCP authentication recovery and status

Date: 3 October 2026. Branch: `extensions/tool-only-retirement`.

## Scope and implementation

Resume the planned B5 recovery/status unit after configured-client issuer ownership. The selected libraries stay exact `rmcp` 3.5.0 and native `oauth2` 5.0.0. No dependency replacement, general MCP configuration, public contract freeze, tool-discovery policy or obsolete-code removal occurs here. Live Linear expiry check 12 remains Deferred by the user; all R0–R6 release gates remain open.

The previous stored-account setup path classified every error as an unavailable account needing reconnect. The SDK transport's typed authentication failure was subsequently erased into a generic protocol-negotiation failure. The developer badge preferred the presence of credentials over any recovery state, and failed management operations did not refresh its status.

Implementation steps:

1. Inspect graph context and the actual SDK/storage/HTTP call paths; compare protocol and maintained hosts before editing.
2. Introduce a small nonsecret recovery enum. Definitive SDK authorization/refresh rejection requires sign-in; issuer/client registration mismatches require explicit registration setup; supported typed configuration/scope refusals require setup review; vault backend failures remain separate; unclassified SDK/metadata/request failures preserve credentials and give retry-later guidance.
3. Observe the existing authenticated HTTP adapter's `StreamableHttpError::Auth` before the SDK worker erases its type. Keep the SDK error itself unchanged. Use the existing finite provider control and generation guard to store only an optional enum. No tokens, HTTP clients, listeners, idle tasks or new refresh engine are retained. Invalidation/restart clears observations; `stored` remains an inventory statement, never an unperformed health check.
4. Return the specific safe recovery message for failed account setup/negotiation. Do not retry a failed operation, start a browser, erase a transient grant or fall back to another protocol lifecycle following a typed auth failure. Existing post-dispatch uncertainty/no-replay behavior is retained.
5. Refresh existing developer status after both successful and failed management actions, render its reported state and offer explicit Sign in again for a retained grant requiring reauthorization. This is behavior in the existing panel, not an accepted store redesign or UI 2.0 phase.
6. Extend the maintained real-app MCP fixture and scenario inventory; repeat affected acceptance, deliberately corrupt the failure fixture, review the source/diff, inspect cleanup and record precise limits before accepting the unit.

## Primary-source basis

The [official Rust SDK 3.5.0 authentication source](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.0/crates/rmcp/src/transport/auth.rs) distinguishes `TokenRefreshRejected` from `TokenRefreshFailed` and credential-store errors. Its `get_access_token` maps rejection/no refresh to `AuthorizationRequired`, preserving infrastructure failures. Grain observes those types rather than interpreting arbitrary exception strings or implementing a second refresh path. [RFC 6749 section 5.2](https://www.rfc-editor.org/rfc/rfc6749.html#section-5.2) defines `invalid_grant` as a definitive unusable grant/refresh response; a network failure alone cannot establish that. The [current MCP authorization profile](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) supplies the discovery/authorization boundary.

[Pinned Goose HTTP authentication integration](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/agents/extension_manager/streamable_http.rs) confirms the need for host recovery handling around the official SDK. [Pinned OpenCode connection/status integration](https://github.com/anomalyco/opencode/blob/c42ae0d56b6f86f8df39d451d6d2cfe6414b3928/packages/opencode/src/mcp/index.ts) distinguishes auth, client registration and other failures. Its string-based classification and retained pending transports are not copied into Grain. These are source references and design comparisons, not evidence that Grain's runtime passes.

## Maintained tests and retention

Forward criterion **55** is separate from the original 53 baseline requirements. `mcp.auth-temporary-recovery` uses actual issuer expiry, real SDK refresh, real application commands and approved Agent reads. It separately identifies HTTP 503, malformed success JSON and dropped token responses under modern and legacy lifecycles. Each checks one failed refresh attempt, zero tool dispatch/new consent on failure, retained scoped vault grant/enablement, truthful status, real host restart and successful recovery of the same account with one rotating refresh and no additional login. A metadata failure/recovery checks the valid grant separately.

The existing `mcp.auth-refresh-refusal` checks actual expiry without refresh and actual `invalid_grant`, specific reconnect advice, retained grant, no automatic consent/action, explicit replacement and restart recovery. The issuer case retains wire secret-boundary assertions and now expects the precise registration failure. Rust logic checks distinguish SDK types without string matching or leaking private payloads, and stale account-generation publication cannot replace current recovery status.

The fixture adds only an enumerated refresh-failure switch to the existing bounded issuer. Its maps/sockets/TLS/profile/vault entries retire through standard owned teardown; evidence retains mode/timing/counts, never grant bodies. Keep this scenario and its `missing-mcp-refresh-failure` negative control while auth recovery depends on this adapter. Remove only with equivalent final-path regression proof. No unused product scaffold is introduced.

## Limits and review

Observations are deliberately session-local: restart returns credentials-stored inventory until an explicit operation establishes fresh recovery status. No health polling is added. A retained rejected grant is not claimed usable. Disconnect remains the explicit removal path; temporary failures do not delete it.

`rmcp` still collapses some token-server errors (including `invalid_client`) into a string-bearing generic refresh failure. Grain does not parse that string or assert a precise registration cause it cannot establish. Such responses receive conservative unavailable guidance; richer upstream typed classification remains future work. Actual OS vault lock/failure injection is not certified by controlled network cases; its mapping/generation safety receive logic/source evidence. Cross-process coordination, discarded-grant reconciliation, other operating systems/providers, full RAM measurements and public UI/store certification remain separate gates. Fresh explicit-login error handling is not universally recertified or claimed fully sanitized by this stored-account unit.

Graph minimal context/semantic search were used first; auth search returned no matching nodes and impact/review summaries mixed unrelated entities. Source inspection, exact SDK types and real-path tests supply the actual boundary trace. Source review includes enum-only storage, consistent generation-before-observation lock order, cancellation, registration-secret ownership, provider independence, operation disposal, existing execution phase preservation and frontend failure refresh. Graph coverage summaries alone are not acceptance.

## Verification evidence

**Scoped unit accepted; additional forward 55 reviewed automated Pass.** Baseline remains **52 Pass / 1 Deferred (12)**, with separate **54/55 Pass**. Inventory is **90 IDs / 75 self-contained / 70 harness self-tests**. No new manual account batch. Physical removal remains on hold; all seven release gates and the broader ecosystem delivery lanes remain open.

Candidate static checks exposed a missing optional ticket argument in the existing protocol-test helper and ES2020's lack of `String.replaceAll`; those were repaired using explicit no-observer test construction and a global underscore replacement. Scenario inventory assertions were updated to include the additional independent ID without removing any case. Direct Windows Cargo invocation lacked Tauri's activation manifest and exited `STATUS_ENTRYPOINT_NOT_FOUND`; the maintained production-test runner embeds the existing test manifest and passes. No application crash is inferred from that test-executable launch failure.

Retained runtime candidates:

- `run-Jj9BZ9`: first temporary-recovery procedure passes through commands, before the final challenge mapping/UI procedure. Supporting evidence only.
- `run-HhBpUd`: nine earlier auth cases pass; strict no-refresh refusal fails. SDK `AuthClient::call_reacting_to_challenges` converts `AuthorizationRequired` into an unauthenticated request and ultimately `StreamableHttpError::AuthRequired`, so observing only `Auth` was insufficient. The final observer also handles typed challenges and insufficient scope; no provider-string parsing or auth replay was added.
- `run-qQiqZ6`: actual UI setup is blocked by the existing update dialog; failed Test/close attempts receive no acceptance. `run-qXK8cw` and `run-UkKEaM` expose an incorrectly nested relative row locator. The driver dismisses only the actual update details control and uses a relative exact provider-name locator. It never forces clicks through overlays. Bounded click retries must still satisfy the independent one-refresh-attempt/zero-dispatch oracles. `run-W2HLdM` then passes both refusal schedules and the actual reconnect UI.
- `run-DbUNvz`: all eleven existing cases pass, and the first three temporary schedules have receipts, but the combined procedure hits the external fixture's unchanged 32-token bound. Earlier completed scenarios retained issuer tokens despite host logout. The final new case requires zero host vault grants/sessions before retiring old fixture bearer/refresh maps, and repeats retirement after each owned logout. Receipt history and the original size bound remain intact. This fixture cleanup cannot certify production orphan reconciliation or erase earlier failed evidence.

All listed candidates complete with runner cleanup Pass. Final acceptance uses the final build/runner definitions below, not aggregate credit from these earlier attempts.

`run-muk8oe` is separately **Blocked/Not run**, cleanup Pass: a premature second invocation was refused because the existing suite owned the harness events port. No second host/marker/grant was created. It receives no negative-oracle credit; the actual negative control below ran serially after the full suite.

| Final evidence | Result |
| --- | --- |
| `logic-iV0d9K`, normal backend logic | 55 MCP tests Pass; maintained Windows activation-manifest runner; cleanup Pass. |
| Feature logic/admission | 57 MCP tests, seven profile/isolation tests and eight fixed MCP admission/contract tests Pass. No ordinary-build fixture admission added. |
| Runner self-tests | All 70 Pass; existing cases retained and inventory expectations updated. |
| `run-TvzqdD`, full auth regression | All 12 Pass; 168 observations, 44 host lifetimes/restarts, 206.787 seconds of scenario time; cleanup Pass, zero grants/secrets/registration records. |
| `run-TYc8jO`, fresh recovery repeat | Pass; 22 observations, seven host lifetimes, 28.589 seconds; all six fault schedules, metadata recovery and real temporary-status UI; cleanup Pass. |
| `run-msCNCq`, missing refresh-failure oracle | Expected Fail/exit 1 at `Missing expected rejection`; 3.243 seconds; cleanup Pass. Zero positive acceptance credit. |
| `run-RL8VSV`, shared Agent regressions | All six controlled native/MCP workflow cases Pass; cleanup Pass. |
| `run-QkuyTs`, ordinary smoke | Both cold/warm and denial cases Pass; cleanup Pass. |
| Build/static/source review | Stamped real host/frontend build and TypeScript check, normal `cargo check --locked --bin handy`, scoped Rustfmt/Prettier/whitespace checks and feature-library Clippy Pass. Clippy emits 74 diagnostics outside changed Rust files, zero in the changed MCP files. Source/diff review completed after the SDK challenge fix and fixture disposal correction. |

Final binary SHA-256 `a9be58b9ea5cfc5f11131c0749a00e9623219f9a8567f5afb0946747e344c7f0`; application source fingerprint `1e6ffd6da09d3719488ff6347c2093254cbae402340e2e79e03253c8b820c84e`; runner fingerprint `e12f89601d6729dbeff4843306f3b0e63d3198b855d0e6004e116cb94df31744`. Full suite, repeat, fault and shared regressions use this final definition. Reports record source HEAD `65e49c0a` plus dirty files; the unrelated pre-existing bindings changes were present in builds and remain excluded from this commit. The nested extension-registry checkout and its existing user edits are unchanged.

Independent read-only cleanup inspection covers all thirteen new completed/candidate/blocked scopes above. It checks recorded host PIDs by process identity, command lines owning a run root, owned marker/model/MCP/peer ports plus event/callback ports, disposable child directories, and thirty inventory-only grant/secret/registration namespaces across the ten authenticated scopes. Original hosts/root-owned processes, listeners, scratch and credentials are absent. PID reuse by unrelated processes is recorded explicitly rather than treated as either a leaked host or an absent numeric PID. The blocked preflight has no marker/host, so vault inventory is inapplicable; the two unauthenticated regression scopes also receive no vault claim. CDP closure comes from each runner's cleanup, rather than an invented independent debug-port checkpoint. Ignored `auth-recovery-cleanup.json` retains the inspection, and `inspect-auth-recovery.ps1` is disposable read-only evidence support. Three earlier policy-blocked interrupted roots remain excluded and Pending, with no alternate deletion attempt.

Reproduction after compilation, serially:

```powershell
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/*.test.mjs
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-temporary-recovery
# Expected Fail/exit 1, cleanup Pass; not positive acceptance:
node tests/agent-harness/run.mjs --scenario mcp.auth-temporary-recovery --fault missing-mcp-refresh-failure
node tests/agent-harness/run.mjs --suite agent-workflow
node tests/agent-harness/run.mjs --suite smoke
```

After scoped acceptance, the next bounded auth task is client metadata registration, requiring a real Grain-owned public HTTPS document and exact redirect inventory before production enablement. E1–E8 ecosystem delivery and held physical retirement are not silently started by this unit.
