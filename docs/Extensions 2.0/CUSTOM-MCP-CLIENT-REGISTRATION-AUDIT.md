# E2f: configured MCP registered clients — 4 October 2026

Forward **65 Pass**, after focused source audit, the full 36-case real-application checkpoint, detected negative control, final-runner five-case repeat and independent cleanup inspection. E2 remains in progress; this is not release certification.

## Implementation

Two main-window/Developer-Mode Tauri commands set and explicitly reset client registration for a configured OAuth connection. Both require the exact canonical revision. Definition/import JSON remains credential-free. Client IDs are nonsecret settings keyed by the host-created configured account; secrets, issuer bindings and grants use that exact account in the OS vault. Two records pointing at the same endpoint do not share these keys. No new authentication engine, library or persistent service is added.

Catalogue and configured setup share the existing discovery, registration publication and SDK exchange path. Saving discovers and validates issuer metadata/destinations before changing existing credentials. The cancellation generation and current record lease guard vault reads/writes and final settings publication; detached blocking vault tasks retain the existing operation lease. Registration is checked against both client ID and discovered issuer before loading a client secret. Fixed registered-client callbacks use the existing bounded loopback listener. A configured record without a saved client ID continues through SDK DCR; saving a client ID selects preregistration, including public clients with no secret.

Saving cancels old work, disables that account and removes the old grant/binding before changing its secret. An old grant cannot ride under a new registration, including same-ID secret rotation. Switching to a public client removes the previous secret. Disconnect removes the grant and enablement while retaining registration for the next explicit login. Explicit reset removes grant, secret, registration and configured client-ID mapping while preserving definition identity/revision. Endpoint/authentication changes and removal retire those keys before metadata publication; label edits retain registration but invalidate old approvals. Catalogue keys are preserved.

Vault, settings and registry publication are not a distributed transaction. A partial failure reports an error and may leave the preserved connection signed out, requiring explicit retry; it cannot authorize a new secret under an old binding or silently enable replacement metadata. Status reports stored configuration/connection state, not a live provider-health guarantee. The host does not infer preregistration support from arbitrary provider marketing, nor guess an official client identity.

## Research and audit

Primary references rechecked: [current MCP authorization specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) and [OpenCode MCP authentication/configuration](https://opencode.ai/docs/mcp-servers/). The specification supports preregistration and public client metadata, with DCR retained for compatibility. Our inference is to reuse the accepted official SDK and one host-owned registration path. Public client metadata remains inactive until Grain has a verified official hosting identity; no experimental website is substituted. A failed Goose raw-source fetch is not counted as fresh research.

Graph-first exploration, impact analysis, change detection and review context were followed by manual tracing because indexed cross-file flows/test associations are incomplete. Review covers generation/metadata ownership, blocking-task lifetime, mutation lock order, exact account keys, pre-secret issuer checks, cancellation, retirement/publication ordering and unchanged catalogue behavior. No Handy-owned feature code, UI design or physical obsolete-capability deletion is changed.

## Maintained acceptance coverage

The existing actual Grain harness, owned HTTPS issuer, SDK exchange and Windows vault are reused. Tests do not inject grants or mock Tauri. Two new independently reported cases retain named stage evidence:

| Scenario | Boundary |
| --- | --- |
| `mcp.configured-auth-client-rotation` | Public/confidential clients, same-ID secret rotation, stale approval refusal, actual account reads/restart, logout retention, explicit reset and fresh SDK DCR |
| `mcp.configured-auth-client-ownership` | Same-endpoint isolation, metadata/destination failure preservation, pending consent save/reset, modern JSON/legacy SSE active cancellation, revision guards and endpoint/auth/removal retirement |

The management case now refuses all ten configured commands with Developer Mode off and from the actual Agent window. Catalogue issuer-rotation, registered-client failure/publication, callback and transport/deadline coverage remain in the combined checkpoint; they are shared-path regression evidence, not certification of every custom provider. The new fault skips only a harness-owned client rotation and must fail the unchanged-grant assertion. It never bypasses production guards and is admitted only for its exact scenario.

```powershell
node tests/agent-harness/production-tests.mjs --group mcp
cargo test --locked -p grain-core mcp_connections::
node --test tests/agent-harness/runner.test.mjs tests/agent-harness/client-metadata.test.mjs
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp-connection-checkpoint
# Expected exit 1 at old-grant retention assertion, cleanup Pass:
node tests/agent-harness/run.mjs --scenario mcp.configured-auth-client-rotation --fault skip-configured-client-rotation
node tests/agent-harness/run.mjs --suite mcp-configured-auth
```

Build/backend checks precede application tests; run actual hosts serially and freeze source/runner inputs until teardown. Original production clocks and evidence bounds remain. Keep the scenarios, exact fault admission and account-scoped cleanup as regression infrastructure. Generated profiles/TLS/logs/inspectors under ignored `.runs/` are disposable evidence. Previously policy-blocked interrupted scratch roots remain untouched and have no acceptance credit.

## Verification and next work

Fast checks: **73** normal MCP tests (`logic-Qzk3mM`), **13** registry tests and **80** final-runner Node self-tests pass. Normal Clippy retains **68 existing warnings** after removing two new needless borrows. Locked CLI/feature build, embedded frontend/type check, scoped Rust/Node formatting and whitespace checks pass. Pre-existing dirty generated bindings are preserved and excluded; their existing whitespace is not attributed to this unit.

Stable broad checkpoint `run-e3Ewfy`: **36/36 Pass**, 739,423 ms of scenario time, cleanup Pass, **4,488 wire / 2,182 issuer events**. Actual HTTP/discovery deadlines measure 183,626/181,842 ms. Source fingerprint **27a1f16f49c92af373e0d2d3739911f4cea82f056e4d3383c015b3a3ea695482**; broad runner **c51f72068549d6412fd978755e079957fe999226147a9d9801c69403fcfdfb69**. The later harness-only fault-option forwarding changes no production source or ordinary scenario behavior. The broad report retains its original runner identity rather than being relabelled as final-runner evidence.

Final negative `run-Gty0cN` exits **1** at the skipped rotation's unchanged-grant assertion (**1 !== 0**), after the two legitimate initial account reads; no mutated approval is dispatched. Cleanup Pass. Final affected repeat `run-CtYa7D`: **5/5 Pass**, 71,558 ms, cleanup Pass, same source fingerprint, final runner **d3dcfc86da922f5836d249b6e798206e289137cbfbfe63a5b03f211094312b99**. Its five cases cover DCR/refresh, current ownership, preregistered clients and pending/active cancellation under the final context wiring. Node self-tests also pass on that final runner.

Independent inspection (`.runs/e2f-client-cleanup.json`) covers **nine completed scopes / 121 host PID references**, including failed/incomplete controls: zero owned processes/listeners/scratch and zero catalogue/configured grants, secrets and registrations. Previously policy-blocked scratch roots from older work are out of scope and remain untouched. All new completed scopes have cleanup Pass; a failed or deliberately stopped scope does not acquire acceptance credit from successful cleanup.

Initial `run-zydpmf` passed the three existing configured OAuth cases and failed the new public-ID rotation receipt assertion. Actual Grain correctly refused the disabled owner's old approval with “no longer approved or available”; the harness incorrectly requested the account-generation refusal. Two explicit configured-disabled instructions now select that one exact disabled-owner expectation. Node assertions reject exchanging the two messages; the ordinary configured account oracle is unchanged. No production guard or broad error acceptance was relaxed. The failed run is retained with cleanup Pass and no completion credit.

`run-ImeD9T` also retains three existing Pass results but fails because the newly named instruction had not been added to the host's finite instruction enum. That enum now includes only the two exact instructions. Source tracing also found that isolated restart setup filtered configured client IDs out of settings; it now retains only host-created accounts whose exact reserved endpoint/authentication matches this owned fixture, using the same validated record set as enablement. Restart assertions explicitly require the saved client flag and secret inventory, in addition to real SDK reads without reauthorization. These are maintained test-host corrections, not changes to ordinary startup or credential injection. Cleanup Pass; no acceptance credit for the failed unit.

Focused `run-hmFwfi` rotation and `run-XD3DFq` ownership pass, taking 22,947/18,395 ms with cleanup Pass. Their 187/206 issuer events plus the previous 1,789-event checkpoint require about 2,182 events, exceeding the previous 2,048-entry test journal. `run-Uvfydp` passes all eleven configured cases, then is deliberately stopped by terminating only its verified report-owned host (exact executable and runner parent checked). The runner records a subsequent host-unavailable failure and performs its normal finally cleanup; this incomplete checkpoint has no acceptance credit. The finite issuer evidence bound is now 2,560, with reserved terminal failure and capacity tests retained; the wire bound remains 4,608 and production limits/deadlines are unchanged. No failed evidence is truncated or removed.

`run-Nzo7aT` passes the normal rotation procedure despite requesting the negative fault: configured-auth context wiring omitted the option, so the mutation was not skipped. This gives **no negative-control acceptance**. The exact fault option is now forwarded to that existing handler context. Production/source code is unchanged; a fresh negative control and complete five-case affected repeat are required on the final runner, while the broad checkpoint retains its own recorded runner identity. No raw report verdict is relabelled to invent evidence.

Next: verified store descriptor acquisition through the same admission, ownership and runtime boundary. Public metadata hosting, later product UI/store/SDK work, tool-selection measurement and E8 release gates remain separate. Baseline **52 Pass / 1 Deferred (12 live Linear expiry)** and full **56 Pending** remain unchanged; forward **54/55/57/58/59/60/61/62/63/64/65 Pass**. Inventory **105 IDs / 90 self-contained / 80 Node self-tests**. Seven E2–E8 delivery stages retain work; no new manual test batch is assigned by this automated unit.
