# E2b — shared MCP destination boundary

3 October 2026. Accepted forward **61** for the shared networking prerequisite within E2, following the [direct connection registry](CUSTOM-MCP-CONNECTION-REGISTRY-AUDIT.md). It does not expose configured import commands or complete E2.

## Scope and implementation

Built-in providers now use a HTTPS-only HTTP client with automatic redirects and environment proxies disabled. Its resolver validates the complete bounded address answer before returning those exact addresses to reqwest's connector. Empty, mixed public/private and over-64-address answers refuse; lookup has a ten-second deadline. Reject private, loopback, link-local/cloud metadata, shared, multicast, documentation, benchmarking and reserved addresses. IPv6 is limited to public global-unicast space with conservative exclusions for special/translation/tunnel ranges. This is an Internet-service policy, not support for LAN MCP servers or a general claim about all network routing.

The existing bounded MCP adapter checks URLs before POST/stream/session cleanup, including the authenticated original-owner DELETE. Its application constructor requires a policy; the unchecked constructor exists only in Rust tests. URL checks refuse HTTP, IP literals (including normalized numeric IPv4), embedded credentials, fragments, zero ports, single-label and local/internal names. OAuth endpoint query parameters remain usable. Initial descriptor rules remain stricter about queries, as previously documented.

All three OAuth manager construction paths use the official SDK's HTTP hook: explicit client registration, sign-in and authenticated discovery/calls. Each discovery, registration, token and refresh request checks its destination before forwarding. SDK-owned metadata redirects pass through the same hook again; token HTTP does not automatically follow redirects. Responses retain status/headers and a one-MiB body bound. The SDK still owns discovery, PKCE, DCR/CIMD, exchange, expiry/refresh and credential-store coordination. The new adapter does not implement another OAuth state machine. Metadata authorization/token/registration/issuer URLs are checked before consent or client credential publication.

The maintained real-app fixture exception admits only the one or two exact HTTPS origins from that isolated profile's validated marker and scoped TLS client. Two origins are required for existing real issuer-rotation tests. Public live acceptance uses the production resolver/client. These exceptions and the plain-HTTP unit-fixture constructor are absent from ordinary/release builds. Catalogue provider IDs, vault keys, client configuration and generation ownership remain exact.

No additional always-alive task, DNS cache, database, process manager, connection registry, transport or dependency is added. The existing application-wide catalogue client remains; the policy adds no idle lookup. The direct registry is still unwired. Physical old-capability removal, the external extension repository and UI work remain outside this unit.

## Research and reuse decisions

[MCP security guidance](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices) identifies discovery/authorization metadata as SSRF boundaries. Syntax validation alone therefore cannot authorize a configured service. [Reqwest's resolver interface](https://docs.rs/reqwest/0.13.2/reqwest/dns/trait.Resolve.html) supplies the addresses used by its connector; use that interface rather than independently resolving, checking and then allowing another resolution. URL guards separately cover literal addresses, which need not invoke DNS.

The pinned [official Rust SDK auth source](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.0/crates/rmcp/src/transport/auth.rs) exposes `OAuthHttpClient` and `new_with_oauth_http_client` for every OAuth HTTP operation. Verified the installed 3.5.0 source, including no-network manager construction and manual same-origin metadata redirects. Keep protocol/auth logic in that maintained SDK and provide only host networking/response policy at its documented seam. The local dependency source is authoritative for this build; the release-tag source link is a reference, not a claim that an unverified newer version is in use.

[IANA IPv4](https://www.iana.org/assignments/iana-ipv4-special-registry/) and [IPv6](https://www.iana.org/assignments/iana-ipv6-special-registry/) registries inform conservative special-range exclusions. Some publicly reachable special anycast exceptions are deliberately refused; this is not a replica of every IANA reachability exception. Future range/policy changes require focused review and denial/positive tests.

[OpenCode's MCP configuration](https://opencode.ai/docs/mcp-servers/) provides a useful product reference: named remote connections and host-managed authentication without requiring a full extension package. Grain continues toward that model with its already accepted host-owned identity/persistence boundary. No larger agent framework or second transport is adopted here.

## Source audit and repairs

Graph minimal context, semantic/file lookup and impact preceded file exploration. The graph had no match for the relevant HTTP/session search; its impact result lacked downstream edges and later review included stale/unrelated entities and omitted the new module. A refactor-planning call did not support the attempted focused arguments. These outputs did not certify the change. Manual review traced every application HTTP constructor, OAuth manager construction, endpoint/metadata admission, credentials, cleanup, fixture exception and production/test compilation boundary.

Development verification caught a borrowed resolver iterator and error-type conversion; repaired before acceptance. Stronger networking correctly refused existing plain-HTTP loopback unit fixtures; their 28 failures are retained as an intermediate result, then repaired with an explicit test-only client rather than weakening the application policy. Reviewed fixture ownership allows the two declared origins needed by issuer rotation. Two new harness self-test failures caught a changed suite count and missing test import; both are retained and repaired.

The real-app refusal case initially needed corrections during source review before its first run: client IDs cannot be blank, and Disconnect intentionally preserves registration metadata. Its final oracle compares scoped registration/secret inventories before and after failed metadata validation, then verifies real successful sign-in and fresh/restart approved reads. It never erases another scenario's saved registration to force a zero count. Cleanup independently removes all remaining run-scoped bindings.

Final review made the public client factory enforce no automatic redirects itself; its application callers already enforced that identical policy, so runtime behavior did not change. Added the exact 64-address positive boundary and a real ten-second stalled-DNS deadline/drop test. The latter proves bounded waiting and disposal of the owned resolver future, not cancellation of an OS lookup already running inside Tokio's blocking resolver. A Clippy attempt during application testing hit Windows's shared runtime-DLL lock; the clean serial repeat passed after host cleanup. Keep compiler/build operations serial with application runs.

The unrelated `src/app/bindings.ts` change remains **211 insertions / 206 deletions**, included in build fingerprints and excluded from this commit. No existing wrapper or external extension source is removed. No new warnings point at the destination adapter; existing backend/SDK/CLI warnings remain.

## Evidence

| Evidence | Result |
|---|---|
| Final normal-build MCP logic | `logic-z8D5Yh`: **68 Pass**, including nine new destination cases; genuine ten-second DNS wait and dropped-future assertion |
| Maintained harness self-tests | **75 Pass**; new fixed-mode fixture/reset/privacy test; separate source/runner identities retained |
| Broad real-app MCP checkpoint before factory self-enforcement | `run-uTNj1X`: **23/23 Pass**, 653,618 ms scenario time, 655.836 seconds total, 50 host PID references; original 45/90-second deadlines intact |
| Initial scoped real-app destination case | `run-pvby4V`: Pass, 9,194 ms, before factory refinement |
| Final-build destination refusal/recovery | `run-DQK8c9`: Pass, 9,137 ms; three out-of-scope endpoint variants, actual oversized response, unchanged registration/secret inventory, no consent/exchange, fresh/restart A reads |
| Final-build issuer ownership | `run-abN0Dj`: Pass, 6,387 ms; actual two-owned-origin rotation and failed metadata recovery |
| Final-build public MCP compatibility | `run-bBiREa`: **2/2 Pass**, 23,460 / 19,778 ms; actual anonymous public DeepWiki reads through production DNS/TLS policy, disable and response recovery |
| Final-build Agent/native smoke | `run-tJRVi9`: **2/2 Pass** |
| Final-build negative control | `run-zCbv88`: expected **Fail/exit 1**, 1,496 ms, **Missing expected rejection**; valid metadata deliberately supplied where denial was expected, cleanup independently deletes the one resulting scoped registration |
| Independent cleanup | `e2b-destination-cleanup.json`: seven scopes, **61 PID references** inspected; zero owned processes/listeners/scratch/MCP grants/secrets/registrations. One PID reused by an unrelated process was left untouched |
| Static/build | Locked real host/CLI build, TypeScript/Vite production build, normal backend Clippy exit 0, scoped Rust/Node formatting and whitespace checks |

Final source fingerprint **`85f6659394485d62f6b949837d5461c6d89f3df0bb47b33374456dee94df69f5`**; host SHA-256 **`429779d6c5d628cae9da0e119c15de667afb7879df3ec62483e98fe8b13ef09c`**; unchanged CLI SHA-256 `7e8ec04b71e8fef19d6eff5d27f59d357046645baae241984758ba1e29d3cf50`; runner **`878b5e2c244ed4a541b78ed03c322da75d3fd96ceaf2e5e4a7e909929e2ee144`**. Precommit base `f98a48a5`. Broad pre-refinement source `804fac9f2ab98df51b14a208eb3b1314bfd2464d0e694cdbf2e612c8aba4c872`, host `b88c64a621633af143ff69ad5448864190749c2932c3dbd91c62ed11ba109d19`; it is not represented as a final-build run.

The full affected transport/auth suite preceded the final redundant factory enforcement. Final verification repeats all affected production logic, destination/issuer application cases, actual public transport and Agent smoke; the already accepted timeout/protocol algorithms were not changed or shortened. Retain the full suite for future transport/auth changes. Intermediate compile/unit/self-test/Clippy failures remain under `.runs/`; none receives Pass. Source audit and independent cleanup complete this prerequisite's acceptance. No generic configured-server runtime is inferred from these catalogue/adapter results.

## Remaining work and limits

Next expose host-owned direct import/list/edit/remove/enable/connect/status operations and bind the registry's current revision/account to existing SDK sessions, approvals and cancellation. Then test arbitrary configured destinations through that path with owned fixtures. Store MCP acquisition must share the same boundary and carry verified artifact/version provenance. A full E2 configured-source acceptance remains separate from the catalogue and pure resolver evidence here.

This policy guards Grain's HTTP traffic. The external browser and OS network routing are not controlled by reqwest; a browser address preflight cannot pin its later DNS result. Before configured authentication ships, separately settle authorization-page destination/consent and credential origin binding. Private/LAN endpoints, IP-literal MCP metadata, IPv6 translation/tunnel compatibility and corporate proxy support are not certified. Environment proxy use is intentionally disabled because a proxy can resolve destinations beyond this local guard; a later proxy feature needs an explicit equivalent boundary. SDK manual same-origin discovery redirect behavior is retained, while automatic HTTP redirects are disabled.

Keep the production adapter and tests. Retain the single fixed-mode extension to the existing OAuth peer and its actual-app scenario for subsequent configured-runtime regression. No new server/visual harness exists. Ignored logs, finished run directories and independent read-only inspection scripts are disposable evidence. Genuine Linear expiry **12 Deferred**, public client identity hosting **56 Pending**, all E2–E8 delivery work, release gates and the physical-removal hold retain their previous status. No new human batch is assigned.
