# MCP expiry and refresh recovery audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Scope:** controlled prerequisites for check 12 and preparation for live Linear checks 9/12/17. **Status:** controlled unit accepted after focused audit, final combined run, clean repeat, detected fault and independent cleanup. The numbered ledger remains **50 Pass / 3 Pending**; no live account acceptance follows from these fixtures.

## Plan and research

1. Inspect the maintained SDK/store path and actual Linear metadata before adding test coverage.
2. Extend the existing owned issuer, not the product or OAuth engine. Issue real expiring grants and validate client/resource binding and refresh rotation; keep default auth scenarios unchanged.
3. Exercise actual host restart, real clocks, approved nested reads, unavailable refresh and explicit recovery. Run affected auth regressions, clean repeats, a deliberate account fault and independent cleanup before accepting this controlled unit.
4. Prepare user-assisted Linear consent separately; retain all three live requirements until actual provider evidence exists.

[MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [RFC 8707](https://datatracker.ietf.org/doc/html/rfc8707) and [RFC 6749 section 6](https://datatracker.ietf.org/doc/html/rfc6749#section-6) inform resource-bound refresh and rotation. The pinned [Rust SDK 3.0.0 source](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.0/crates/rmcp/src/transport/auth.rs) refreshes with a 30-second buffer, saves the replacement grant and recognizes `invalid_grant`. Tests wait past issuer expiry rather than counting a proactive refresh as post-expiry recovery. No system time, stored timestamp, token or production deadline is edited.

Graph-first exploration and focused change/review context identify shared fixture/runner consumers, high impact and incomplete test edges. Direct tracing supplements the graph: `authorization_manager` initializes from the scoped production store; `AuthClient` owns refresh; production session tickets serialize operations and bound vault publication. No Rust, Handy, frontend, model settings, dependencies or ordinary profile changes are needed. The existing stamped native build can be verified and reused because only independently fingerprinted runner code changes.

## Maintained schedules and assertions

| Case | Procedure and required evidence |
|---|---|
| `mcp.auth-refresh-recovery` | Real two-second initial grant with refresh; restart and wait actual issuer expiry; approved A read requires exactly one refresh and no new login. Replacement has a 40-second lifetime, above the SDK buffer; restart and wait its complete expiry. A second approved A read must use persisted rotation generation 2. A long replacement survives another restart and approved A read without further refresh/login. |
| `mcp.auth-refresh-refusal` | Separate expired grants without a refresh token and with an issuer rejecting refresh as `invalid_grant`. Discovery must refuse with the exact current host message, zero tool dispatch, no browser handoff/new login and no replacement credential. Explicit disconnect clears the grant; fresh B login, approved B read and restart B read recover each schedule. |

The issuer verifies refresh client identity, configured secret and exact resource before consuming its one-use refresh grant. The replacement stays with the grant's account; generation proves rotation persisted across actual host restarts. Tokens expire on the external issuer's real clock, and expired bearer access is challenged. Evidence contains bounded account/generation/lifetime flags and request counts; it excludes token strings, codes, authorization URLs and private result contents.

`verifyMcpRefresh` rejects missing exchanges, wrong accounts/generations, premature refresh and missing resource/rotation evidence. `wrong-mcp-refresh-account`, admitted only for its isolated recovery scenario, changes the actual refreshed bearer account. This must fail independently of model prose. Unit checks also verify expired access, wrong client/resource refusal, old refresh replay rejection, generation 2 and `invalid_grant`; these are protocol tests, not real-app acceptance.

## Focused findings and retention

The first candidate `run-hYV2sb` passed recovery, then refusal failed because its anchored regex omitted Playwright's `page.evaluate:` transport prefix. Only that known prefix is now normalized; the entire host message must still match exactly. A negative self-test refuses timeout and unrelated error messages. The candidate is retained without final acceptance credit. The corrected isolated refusal run is supporting evidence; final combined/repeated evidence is recorded below.

Combined candidate `run-3jZc4G` passed the eight prior auth cases and refresh recovery, then hit the issuer's exact 1,024-entry evidence ceiling during refusal's final restart read. Its terminal `oauth-error`, model search failure, full journal and cleanup Pass are retained; the failed batch receives no final acceptance credit. The finite ceiling is now 2,048 to hold ten cases; no entry is silently dropped, and the exhaustion self-test still verifies bounded truthful failure with a reserved terminal error slot.

Production currently turns an expired/unrefreshable account's negotiation failure into `MCP protocol negotiation failed. Check the account and provider availability.` The issuer's precise refused-refresh receipt and zero dispatch establish this test's cause. This generic reconnect guidance is a retained product-quality finding; a credential badge does not establish usable authorization. No broad error-classification refactor or live check 12 acceptance is claimed by this testing unit.

Keep the two cases, fixed issuer controls and account/generation/privacy assertions as maintained infrastructure. Issuer maps stay bounded and are cleared when its owned listener closes; no refresh worker, timer, background login, credential import or new product engine exists. Remove these controls only when equivalent final-path acceptance covers real expiry, rotation, rejection and recovery. No temporary unused implementation scaffold is introduced.

## Verification evidence

All five final reports below share runner fingerprint `cec7292ec3ef29870be91ceea39d0a20569b20a025f1066e59b05054c862a71a`. The native executable is the verified existing `C:\gt\debug\grain-agent-harness.exe`: SHA-256 `7369879b2e745802301d901e8bd8b51a20ec866b8b4946a2ff85718e2dc5f56f`; source fingerprint `e9cb7049c8037ba6e746136044283cb33266556ee8bf65d340e0a4c696011dc4`; built `2026-10-02T16:42:33.596Z` at precommit base `0b38037023716d830ff8de81513d44a4708aa8c5`. Binary/native/frontend input recomputation passed. Runner work starts at `535f0535`; no native inputs changed, so rebuilding the identical application would add time without strengthening this evidence.

| Final evidence | Result |
|---|---|
| Combined auth `run-wJn4Mo` | All ten cases Pass; new recovery/refusal 44.448/11.502 seconds. Full issuer journal retains 1,031 entries without exhaustion; no OAuth error. Eight earlier auth regressions Pass in the same host. |
| Clean repeat `run-7S8DIz` | Both cases Pass in a fresh profile, 44.589/11.748 seconds. Two genuine expiry waits, two rotated refreshes and three approved A reads; both refusal schedules each recover through fresh/restart B reads. |
| Wrong-account fault `run-LwWDgJ` | Expected Fail/exit 1 in 3.363 seconds: actual provider account `B !== A`. Cleanup Pass; no positive acceptance credit. |
| Smoke `run-TKb6QN` | Native cold/warm and Agent decline Pass. |
| Transport `run-sdC7nN` | Real modern/legacy JSON/SSE transport-contract regression Pass. |
| Static/oracle checks | 57 runner self-tests Pass, zero skipped; changed-file Prettier and scoped whitespace checks Pass. The issuer overflow test still fails closed at its new finite ceiling. No Rust/frontend implementation changed, so no new Rust compilation or release certification is claimed. |

All eight inspected run roots, including both rejected candidates and the earlier supporting refusal run `run-gT3rhA`, report cleanup Pass and retain only their marker and evidence. JSON/Markdown/log evidence contains no private issuer credential marker. Scoped MCP inventories end at zero grants/client secrets; provider sessions close. Independent Windows inspection finds zero owned processes and zero listeners on all **19 recorded service/callback ports**. The runner separately checks its owned CDP listener and WebView/profile cleanup. Existing production discarded-grant reconciliation and the older policy-blocked roots are not silently certified by this inspection.

Current inventory: **82 IDs = 73 self-contained + 3 official + 2 public-live + 4 configured-model**, **57 runner self-tests**; `mcp-auth` ten, `mcp-foundation` nineteen and `mcp-refresh` two. Runtime suites remain sequential on the single owned event port. This unit does not certify held-refresh/logout races, transient outage recovery or live account compatibility. All seven release gates, official initialization, retained input findings, production discarded-grant reconciliation and the two older policy-blocked scratch cleanups remain separately open.

## Linear preparation and human role

The user selected Linear. [Linear's MCP guide](https://linear.app/docs/mcp) documents `/mcp/readonly` and a `read` scope on `/mcp`. Direct anonymous requests from this machine on 2 October returned HTTP 200 for `/.well-known/oauth-protected-resource/mcp`, the root resource metadata and `/.well-known/oauth-protected-resource/mcp/readonly`. Their resources match the respective exact URLs, issuer is `https://mcp.linear.app`, and the read-only resource advertises only `read`. [Issuer metadata](https://mcp.linear.app/.well-known/oauth-authorization-server) advertises DCR, S256 and refresh. This resolves the previous research-tool access limitation; it is not a successful SDK login or tool call. Anonymous initialization POSTs to both resources return 401 with the respective correct resource-metadata URL. Both challenges advertise `scope="read write"`, including the read-only route. Treat route-only scope assumptions as unverified: the finite live path must explicitly request `read` through the SDK and inspect the actual consent/granted scope before enabling its fixed harmless reads.

[Linear's API OAuth guide](https://linear.app/developers/oauth-2-0-authentication) describes 24-hour API tokens. That does not establish the MCP grant's issued lifetime. Do not shorten a live grant, edit its stored timestamp, infer refresh from API documentation or require an idle test app all day. A live expiry checkpoint should resume the same isolated grant only after its actual issued lifetime, with minimal metadata evidence and explicit owned-process cleanup between stages.

The current harness admits fixed controlled accounts and public no-account DeepWiki; it does not yet admit a personal Linear grant. Next add a finite opt-in Linear path to the existing runner, with isolated vault keys, a fixed read-only operation allowlist, actual host/SDK/browser consent and owned cleanup. No arbitrary live endpoint/token injection, ordinary-profile import or account writes. Determine the actual returned tool schema before choosing a nested read; if none is supported, check 17 stays Pending and needs another eligible provider.

When that path is ready, the human batch is: (1) refuse/cancel one actual consent flow and leave the remainder to automated callback/grant/retry assertions; (2) approve the fresh isolated Linear sign-in and select the intended workspace; (3) let the harness approve only the documented harmless reads and verify restart/recovery. Account authentication stays with the user; credentials are never pasted into chat. These are preparation instructions, not a manual batch assigned against today's ordinary profile. Controlled expiry coverage does not replace live consent, schema or recovery evidence.
