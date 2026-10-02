# Authenticated MCP Agent cancellation audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Precommit base:** `84fa022d6f60f340ced83826f71d4f96cd4a6bd5`. **Status:** focused audit, final combined acceptance, clean repeat, negative oracle and affected regressions complete. Check **31 accepted**: **43 Pass / 10 Pending** (15 human, 28 reviewed automated). Whole B2/B3 and all seven release gates remain open. No new manual batch.

## Scope and ownership

`mcp.auth-close-cancellation` extends the maintained real-application harness. Actual Grain Agent search, selected loading, approval, execution, the locked Rust MCP SDK, its production callback flow and run-scoped Windows vault operate against the existing owned HTTPS issuer/resource. The configured safe read waits at the provider's actual HTTP response. It changes no remote state and requires no personal account.

Modern/stateless and legacy/session lifecycle are each crossed with JSON and SSE. Each of the four schedules independently requires:

- One actual account A wire receipt after approval, then closing the real Agent window while that HTTP read is held.
- HTTP closure and zero retained protocol sessions/held replies within ten seconds, no automatic repeat and no subsequent model continuation from the closed request.
- An attempted late provider reply discarded on the destroyed response, followed by one freshly requested, separately approved read returning exact account A and nested JSON.
- Closing another confirmation before approval, with zero dispatch and no model continuation, then another genuine A read.
- The actual enabled resource/account state, one scoped vault grant and unchanged authorization-exchange count after closure; actual restart and another approved A read without signing in again.

The full case requires exactly **four cancelled wire calls and twelve successful fresh/restart reads**. Total wire/model assertions catch extra results or replay between stages. A stored badge alone cannot pass: bearer lookup at the provider and the actual returned account/nested data supply identity evidence. The provider never supplies the Agent decision or approval.

Browser consent is still the private owned-fixture handoff from the SDK's actual URL to its actual issued callback. This certifies the configured safe-read cancellation/account procedure; it does not certify external browser UX, live OAuth expiry, remote undo, provider shutdown/disable, two-provider independence or eligible live nested schemas. No production write is used to provoke cancellation.

## Focused review and repair

Graph exploration preceded source reads. Initial broad impact was truncated and contained unrelated dependency hubs. Changed-file detection prioritised `mcpAuthHandlers` and reported no affected flows; minimal review listed 47 impacted nodes in 19 files without the exact assertions. Direct diff/source tracing therefore covered scenario selection, guarded fault admission, actual account controls, existing `McpService` Drop/HTTP cancellation and account ticket separation, receipt/continuation assertions, restarts and independent cleanup. The OAuth journal follow-up graph correctly identified its fixture/handler consumers. Missing graph flows are not assurance.

Agent close uses the existing real React handler/window destruction and waits for host run/approval release. Service Drop interrupts in-flight HTTP; account invalidation/disconnect remains a separate production operation. The harness reuses the existing held-response peer, actual account read oracle, restart ownership and metadata-only vault cleaner. No Rust, product UI, dependency, ordinary account configuration, Handy code, alternate render path or production engine changes.

**Audit strengthening:** require full-schedule totals in addition to per-stage assertions, detecting an automatic call or escaped success between stages. The deliberate `lost-mcp-account` fault invokes production Disconnect after the first held close; the preservation assertion must fail without reconnecting. It cannot fabricate success or edit a report.

**Reproduced harness failure:** `run-zjZ6PZ` passed the earlier three OAuth cases and the first cancellation schedule, then reached the existing issuer log's 256-entry ceiling during the second schedule. The actual peer journal ended in a constant error; model discovery received a failure instead of metadata, so the case correctly failed and cleanup passed. The grant remained intact; this is an evidence-capacity failure, not account loss. No acceptance follows from that partial run.

The issuer evidence log now has a fixed **1,024-entry ceiling**, reserving one constant `oauth-error` entry. Its catch path cannot throw again into a full log or copy an authorization URL/code/token. A focused self-test fills that actual handler's journal, repeats a private-query metadata request, requires explicit HTTP 400 refusal, unchanged capacity, exactly the constant terminal evidence and no private marker. Per-case peer/issuer errors still fail acceptance. No production resource limit or 45/90-second clock is relaxed.

## Final verification

| Run | Actual final verdict | Cleanup / acceptance boundary |
| --- | --- | --- |
| `run-82Kvn5` | All thirteen combined MCP cases Pass; 166 separate observations, 2,450 peer entries, 110 wire calls and 358 OAuth entries. Includes four real 45s call deadlines and two real 90s discovery deadlines. | Cleanup Pass; zero sessions/held replies/delay timers/scoped MCP grants. Full audited combined run. |
| `run-9jLHy8` | All four OAuth cases Pass again in a fresh isolated profile, including the complete cancellation matrix and four actual restarts. | Cleanup Pass; actual account A reused without new login; zero remaining grants. |
| `run-grOsdm` | Actual production Disconnect after held closure triggers `stateless-json: account disconnected`, Fail/exit 1. | Required negative oracle; cleanup Pass, zero remaining MCP grants. No acceptance credit. |
| `run-w5xHeH` | All eight native-auth cases Pass. | Cleanup Pass; independently deletes four discarded scoped grants, remaining zero. Does not close production orphan reconciliation. |
| `run-ckOqxX` | Both ordinary Agent smoke cases Pass. | Cleanup Pass. |
| Runner checks | All 35 self-tests Pass, zero skipped; scoped Prettier and changed-file whitespace checks Pass. | Includes actual-handler evidence-exhaustion refusal/private-data exclusion. |

In each final cancellation case, exactly four held reads are cancelled and twelve genuine A reads succeed after closure/restart. All eight measured held-close cleanups across the combined/repeat runs complete in **115–127 ms**, under the independently asserted ten-second test ceiling. This is functional cleanup evidence on this machine, not a universal latency benchmark.

| Final identity | SHA-256 |
| --- | --- |
| Real host `C:/gt/debug/grain-agent-harness.exe` | `9d1cbc468908f476b53358e8be1a769cdea09d2b5bb04915dca1ebd73a635ef2` |
| Build source | `c96c755296f5994dbd5b193ec11b5a46cc8838c5ecad97e6d585994223248964` |
| Final runner | `685e05aff68ebc146cca34c0a4dd6600fbdbc68e979f0103f575c3d2ae2e3a6b` |

Final reports are retained under `tests/agent-harness/.runs/<run>/evidence/`. Independent inspection of all five final runtime roots finds only marker/evidence, matching binary/source/runner identities, no private MCP token marker and zero scoped credentials. No matching owned host/WebView remains and event listener 17124 is closed. The runner/source were frozen throughout these runs. The unchanged stamped application build is verified; no unnecessary native rebuild occurs during runtime. The pre-existing `src/app/bindings.ts` diff (211 additions/206 deletions) is preserved/excluded; its existing whitespace warnings do not fail the scoped changed-file check.

The earlier `run-Xw1HBS` standalone pass and `run-ayOunR` detected disconnect fault predate the audit strengthening/journal repair and are supporting history only. Failed `run-zjZ6PZ` retains its precise stage and cleanup Pass. None substitutes for the final repeated unit. Previous normal-build, public-live and official-conformance evidence remains historical; those unchanged external paths are not newly certified by this controlled batch. The pinned official standalone-init blockage is not waived.

## Reproduction and retention

Build with `tests/agent-harness/build.ps1` if application sources changed; do not compile while runtime cases hold native files. Run serially on an idle Windows desktop:

```powershell
node --test tests/agent-harness/runner.test.mjs
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --scenario mcp.auth-close-cancellation --fault lost-mcp-account
```

The deliberate fault expects exit 1, the account-preservation assertion and cleanup Pass. Ordinary cases expect exit 0 with every independently required observation and cleanup Pass; Blocked/Not run is never accepted.

Keep the stable case, exact oracle, bounded evidence log and exhaustion self-test as regression support. Existing debug endpoint/CA/private-consent seams retain their [documented retirement conditions](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory). No unused fixture/process scaffold is added. Every run retires owned profiles, fixture/TLS keys, OAuth maps, scoped grants, hosts and listeners; marker/evidence remain. The two earlier policy-blocked interrupted scratch roots remain separately Pending and supply no credit.

This completed unit accepts only **31**. The actual numbered ledger has **43 Pass / 10 Pending**, with B2 nested live check **17** (one), B3 **9 and 11–15** (six), and B4 **46–48** (three) still requiring their own procedures and audits. The plans, current handoffs and ledger introduction now agree; historical snapshots keep their original counts. Official standalone initialization, all seven release gates, retained input findings, production orphan reconciliation and the two policy-blocked interrupted roots remain separate. Tests precede broad platform improvement.
