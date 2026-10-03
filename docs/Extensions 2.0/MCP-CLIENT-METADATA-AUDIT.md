# MCP client metadata registration: integration and deployment gate

Date: 3 October 2026. Branch: `extensions/tool-only-retirement`.

## Scope and plan

Implement the remaining B5 client metadata seam using exact official `rmcp` 3.5.0. Native OAuth remains exact `oauth2` 5.0.0. Keep host ownership, bounded HTTPS, issuer-bound grants, PKCE, explicit consent, callback validation, approval and uncertain-action no-replay. No new auth engine, arbitrary MCP configuration, discovery-policy change, registry publication, UI redesign or obsolete-code deletion.

The user confirmed during this unit that Grain has **no official website** and existing sites are experimental. Production `CLIENT_METADATA_URL` is therefore `None`; no experimental domain, another application's client identity or undeployed URL is used. This is an explicit release prerequisite, not an accepted production feature.

Execution order: graph/source and primary-source review → minimal SDK request/callback integration → bounded owned HTTPS document/issuer cases in the maintained real-app harness → affected regressions and deliberately failing oracle → focused source audit/fixes → fresh acceptance/independent cleanup → commit/push. Complete the controlled integration now; public deployment and provider acceptance remain separately gated.

## Research and chosen behavior

- [MCP draft client registration](https://modelcontextprotocol.io/specification/draft/basic/authorization/client-registration), retrieved 3 October: prefer available preregistered credentials, then metadata-document identity when explicitly advertised, then DCR for compatibility. The client hosts the public document; the authorization server resolves its URL and validates identity/redirects. The previously cited dated/latest client-registration pages were inaccessible this turn; this comparison uses the accessible draft, not an invented published version.
- [IETF CIMD draft revision 02](https://datatracker.ietf.org/doc/draft-ietf-oauth-client-id-metadata-document/02/), published 6 July 2026: public HTTPS URL identity, exact `client_id` match and JSON delivery. This remains an Internet-Draft, not a final RFC.
- [Official Rust SDK request/session implementation](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.0/crates/rmcp/src/transport/auth.rs): verified directly against the locked local crate source. `AuthorizationRequest::with_client_metadata_url` and `AuthorizationSession::new` already implement selection, configuration and authorization. Grain supplies only its owned identity and matching callback. No SDK fork or parallel registration implementation.
- [Kilo pinned OAuth provider](https://github.com/Kilo-Org/kilocode/blob/76bcfd40be616a72f4697b3041565f322245b462/packages/opencode/src/mcp/oauth-provider.ts) and [metadata URL declaration](https://github.com/Kilo-Org/kilocode/blob/76bcfd40be616a72f4697b3041565f322245b462/packages/opencode/src/kilocode/mcp/client-metadata.ts): the public document describes its default callback/public client, so Kilo offers it only for that matching configuration. Grain likewise uses an exact inventory; Kilo's URL is never Grain's identity.

The public URL is portable client identity, not a token, shared secret or extension-author API key. Grants and configured registration credentials retain their existing issuer/resource/account ownership. Provider scopes remain negotiated by the SDK; the public document contains no provider-specific scope or account data. Keep refresh-token grant metadata, while never assuming every provider issues a refresh token.

## Implementation and lifetime

`grain_mcp.rs` passes the owned optional URL to the existing SDK request. Dynamic providers use the exact fixed IPv4 callback only when both a metadata URL exists and discovered metadata advertises boolean `true`. Otherwise DCR keeps its existing ephemeral loopback port. Configured clients retain priority and their fixed port.

Configured-client callback ownership is reserved **before issuer discovery**, preserving immediate conflict refusal with zero foreign metadata/credential work. Dynamic registration discovers first so Grain can choose the correct callback. The same local listener drops on discovery/registration failure, cancellation, callback completion or generation replacement. Existing 300-second login and HTTP/operation/cleanup bounds stay. No document fetch, client, worker or refresh service is retained while idle.

The metadata URL is a host-owned constant, not extension-controlled input or a production environment-variable override. The existing opt-in `agent-harness` build derives only its exact owned account fixture's `/oauth/client.json` URL from already guarded profile configuration. Ordinary/production catalog tests prove those aliases and experimental hosting cannot activate it.

`scripts/check-mcp-client-metadata.mjs` is a maintainer deployment checker, not shipped app logic. It validates a narrow public native-client profile, exact identity/redirects, refresh/code metadata and absence of unreviewed fields/secrets. Live verification requires HTTPS, direct 200 JSON, strict UTF-8, at most 8 KiB and a ten-second deadline; it follows no redirect and changes no host setting, vault or website. A local file Pass does **not** certify public hosting. Existing callback-source parity tests keep its inventory aligned with Grain.

## Maintained controlled procedure

New scenario `mcp.auth-client-metadata`, supporting forward criterion **56**, reuses actual Tauri/SDK/settings/vault/Agent approvals and short-lived owned TLS servers. No ordinary account, real provider or live model is used.

1. Metadata support plus DCR: actual server-side HTTPS document fetch; metadata identity wins and zero DCR requests.
2. Metadata support without DCR: actual consent/code/PKCE/resource exchange and approved account read still work.
3. No advertised metadata support: DCR occurs once, with no document fetch.
4. Configured confidential client plus metadata support: existing registration wins, correct synthetic secret reaches only its issuer, no document fetch/DCR.
5. All four registrations: modern fresh approved reads, actual host restart, legacy approved reads identify the same account with no new consent/code exchange.
6. Neither registration mechanism: explicit registration refusal, zero consent/exchange/grant.
7. Separately wrong document ID, wrong redirect and HTTP 503: issuer verifies the exact injected response over real HTTPS and refuses before consent/exchange. The harness then uses production Disable to cancel the waiting login; listener/zero grant and no automatic downgrade are verified, then fresh B login/read recovers. Grain cannot observe an error rendered solely inside the external browser; this does not certify immediate app-side diagnosis of that browser error.
8. CIMD fixed callback conflict: zero document/consent/token work after discovery, no grant; releasing the owned conflict recovers.
9. Cancel an issued CIMD callback: production Disconnect releases the listener; actual late delivery is connection-refused and issues no grant; fresh B login/read succeeds.

Run serially against one stable stamped build:

```powershell
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-client-metadata
# Expected Fail/exit 1 at wrong mechanism, cleanup Pass:
node tests/agent-harness/run.mjs --scenario mcp.auth-client-metadata --fault missing-mcp-cimd
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/*.test.mjs
```

The fault disables advertised metadata support for the first priority schedule. The SDK legitimately chooses DCR; the strict metadata-client receipt assertion must fail. This is oracle evidence, never a passing acceptance run. Separate scenarios/substages retain failures; no clock reduction or automatic scenario retry.

## Focused audit and candidate history

Graph-first context/search, impact and detect-changes → review-context were used. The graph failed to locate the registration seam, gave truncated/unrelated impact entities and lacked accurate runtime-flow coverage. Source tracing and actual wire/account/cleanup evidence remain necessary; graph risk/zero-flow output is not acceptance.

- `run-bAuWfr`: four priority/restart modes passed, then test cleanup incorrectly attempted an empty client ID. The production command correctly rejects it. Cleanup now uses explicit supported public credentials to remove the synthetic secret; no app API relaxation.
- `run-K4nSlM`: intentional document refusals were mislabeled as unexpected fixture errors. They now have explicit receipts; all unexpected failures remain fatal.
- `run-5QD2iy`: complete supporting case Pass before final audit tightening. The audit found that an unrelated fetch failure could satisfy an invalid-document schedule. Each schedule now asserts the actual expected status/body/ID/redirect before recording its refusal; this earlier Pass is not final-source acceptance.
- `run-g0rZde`: seven existing auth cases Pass, fixed-port check fails `54 !== 51`; remaining cases Not run. The implementation had moved configured-client port acquisition after three metadata requests. Restore early reservation for preregistered clients rather than weakening that zero-request oracle. The final build and affected full batch must recertify the fix.
- All four candidate runs report cleanup Pass. None is promoted to final acceptance. The initial source formatter hit a transient Windows mapped-file error; a subsequent format succeeds. Existing unrelated binding generation/whitespace and nested-repository changes stay outside this commit.

## Final evidence and scoped verdict

Accepted host SHA-256: `2f669732771ee0fb2907493955d29b29b381087714380f5e95485750e40b6678`; application source fingerprint: `d2ef548621901cb04bc7af4ab2effe57633f7a52c7affe115e4ceca150a28696`; runner fingerprint: `0793b57139b5785a97b171632a0e14fca31ff8749c54fede30eb48fb83e11b11`. Build base is `004acaff` with the scoped working diff and pre-existing generated `src/app/bindings.ts` churn (211 additions/206 deletions); the unrelated binding edit is not committed. The final source differs from the initial candidate build because the preregistered port-refusal regression was repaired.

| Evidence | Result | Scope |
|---|---|---|
| `run-Jr4Cni` | All 13 auth cases Pass; cleanup Pass | 198 observations, 48 actual host lifetimes/restarts, 233,485 ms total scenario time; zero grants/secrets/registration records after teardown |
| `run-OyLinO` | Fresh metadata case Pass; cleanup Pass | Same three fingerprints, 30 observations, five host lifetimes, 22,167 ms; all three vault inventories zero |
| `run-xS7bv2` | Expected Fail/exit 1; cleanup Pass | 1,597 ms; wrong SDK registration mechanism assertion detects disabled advertisement; no acceptance credit |
| `run-aQRO6Z` | Six Agent workflow cases Pass; cleanup Pass | Shared native/MCP selective loading, schema budget, coverage and approved workflows |
| `run-4laZ5x` | Two smoke cases Pass; cleanup Pass | Native cold/warm call and declined Agent action |
| `logic-PNrRUb` | 57 normal-build MCP tests Pass | Final source; initial `logic-B5VU0i` belongs to the earlier candidate only |
| `cimd-rust-feature.log` | 59 feature-build MCP tests Pass | Includes fixed production-hosting guard and existing OAuth/transport logic |
| `cimd-feature-guards.log` | Seven host-marker and eight MCP guard tests Pass | Feature-enabled admission/owned endpoint/callback restrictions; normal-build zero-selector attempts have no credit |
| `cimd-selftests.log` | 73 Node self-tests Pass | Metadata document/profile/parity, fault admission and retained runner oracles |

The stamped build includes frontend type/Vite and locked host/CLI checks. Locked normal `handy` check and feature-lib Clippy pass; Clippy retains 74 pre-existing diagnostics outside the changed Rust file. Scoped Rust formatting, Prettier and whitespace checks pass; unrelated generated binding whitespace is excluded. No source change was justified after the final auth/fresh/negative passes. The first attempted normal-build harness guard selectors ran zero tests because those modules are feature-gated; they earn no test credit. Feature-enabled guard results are recorded separately.

Independent **read-only** inspection (`.runs/client-metadata-cleanup.json`, 3 October 09:13:02 UTC) covers all nine candidate/final/runtime scopes, **101 recorded host PID references and 21 scoped vault inventories**. No owned host/root command line, marker/event/callback listener, scratch fixture/profile/TLS directory or grant/secret/registration survives; no unrelated PID reuse was observed. It verifies identity rather than terminating arbitrary processes. Existing three policy-blocked old scratch scopes are outside this inspection and remain untouched/Pending. The inspector is ignored disposable evidence support, not new product machinery.

Verdict: **controlled client metadata integration accepted within these boundaries; production activation/full criterion 56 Pending**. No human auth batch or existing baseline requirement is removed, and no live provider acceptance is inferred from the owned fixture.

## Public activation gate and remaining work

Forward **56 remains Pending for public deployment/provider acceptance**, even when its controlled integration is accepted. Baseline stays **52 Pass / 1 Deferred (live Linear expiry 12)**; forward 54/55 retain their separate Passes. No new human account batch is assigned.

When permanent Grain-owned hosting exists, publish this reviewed minimal document, replacing only the placeholder with its exact permanent HTTPS document URL:

```json
{
  "client_id": "https://PERMANENT-GRAIN-HOST/oauth/client.json",
  "client_name": "Grain",
  "application_type": "native",
  "redirect_uris": ["http://127.0.0.1:31938/mcp/oauth/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "token_endpoint_auth_method": "none"
}
```

Then validate the local file and actual unauthenticated direct public response:

```powershell
node scripts/check-mcp-client-metadata.mjs HTTPS_DOCUMENT_URL --file CLIENT_DOCUMENT.json
node scripts/check-mcp-client-metadata.mjs HTTPS_DOCUMENT_URL --live
```

Verify ownership, continuous hosting/JSON content type, exact callback and supported provider metadata; change the host constant and its explicit activation guards in one reviewed unit. Repeat controlled priority/refusal/cancel/restart regressions and one eligible real provider's consent/harmless read/restart. Do not try the template/experimental site in production, broaden redirects, embed secrets or silently fall back after a selected metadata flow is rejected. Hosting choice/deployment is not performed here; no publishing approval is inferred from the experimental sites.

One auth activation prerequisite remains, followed by **eight E1–E8 delivery lanes** of differing size. R0–R6 are seven release gates inside eventual certification, not seven extra completed development phases. Public API/connection/SDK/CLI/catalog/store/runtime/scale/release work remains open. Physical obsolete-code removal is separately on hold.

Retain the owned document and strict fault receipts as maintained regression coverage; no temporary alternate UI, SDK fork or second runtime was added. Ignored run artifacts/inspection scripts are disposable evidence support. Three older policy-blocked interrupted scratch roots remain untouched/Pending. Controlled same-host fixture success does not certify public availability, every provider/OS, actual-clock Linear refresh, whole-app resource bounds or production orphan reconciliation. The live deployment checker has no actual public Grain target yet; its network branch requires verification at the activation gate. The existing browser handoff can require explicit Cancel or its bounded timeout when a server refuses without returning a callback. Custom configured MCP/credentials and general registration fallback UX remain E2/E5/E6 work; curated preregistration prerequisites are preserved.
