# MCP conformance: focused adapter audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Source base:** `d41e9a4005df31fb25ffe7612ff40a9e4e2659bd` plus the recorded scoped working changes. Testing infrastructure only; no MCP SDK, production OAuth, Handy or UI replacement.

## Verdict and scope

| Maintained case | Official scenario / version | Result | What actually happened |
| --- | --- | --- | --- |
| `mcp.conformance-tools-legacy` | `tools_call`, 2025-11-25 | Pass | Grain negotiated, discovered and selectively loaded `add_numbers`; actual Agent approval dispatched exactly one call with 5 and 3; the typed real result was 8. Official tool and wire-schema checks succeeded. |
| `mcp.conformance-tools-modern` | `tools_call`, 2026-07-28 | Pass | Same real discovery/approval/result path, with modern discovery and zero initialize messages. Official tool and wire-schema checks succeeded. |
| `mcp.conformance-initialize` | `initialize`, 2025-11-25 | Blocked | The official raw fixture answered Grain's actual Auto discovery probe with HTTP 200 and `result:{}`. Grain refused protocol negotiation; the fixture produced zero official checks. |

Only the two named tool cases are accepted. The complete entry remains nonzero: two Pass, one Blocked, cleanup Pass. Empty/skipped/warning checks, missing required checks, a failed application run or unverified cleanup cannot become a conformance Pass. These cases support checks 8/17 but do not supply their live/provider/account portions. The ledger stays **39 Pass / 14 Pending**; B2, B3/B4 and all seven R0–R6 gates remain open.

The initialization blocker is an observed fixture compatibility issue, not a certificate for initialization or a reason to weaken Grain. The independently guarded classifier requires the exact negotiation failure, one HTTP-200 empty-result discovery observation, zero official checks and application cleanup Pass. Other failures remain Fail. Do not force legacy initialization, synthesize a response or change the production negotiation mode to make this fixture green. The raw fixture's generic response and the version-aware tool fixture differ in the [pinned initialize source](https://github.com/modelcontextprotocol/conformance/blob/f44482ba17df816d3176962a11cdf36aec9bda00/src/scenarios/client/initialize.ts), [tool scenario](https://github.com/modelcontextprotocol/conformance/blob/f44482ba17df816d3176962a11cdf36aec9bda00/src/scenarios/client/tools_call.ts) and [mock server](https://github.com/modelcontextprotocol/conformance/blob/f44482ba17df816d3176962a11cdf36aec9bda00/src/mock-server/index.ts). That diagnosis combines the source with recorded real wire behavior. Grain's locked [rmcp 3.1.4 Auto client](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.1.4/crates/rmcp/src/service/client.rs) remains unchanged.

## Dependency and integration

`tests/agent-harness/conformance/package.json` and its lock pin `@modelcontextprotocol/conformance` **0.2.0-alpha.12**, published source **f44482ba17df816d3176962a11cdf36aec9bda00**. npm integrity: `sha512-eXtZFgmtZiU5SsvXty+RGXGnyIqEVm3GLU/HZmg1wMjtKOocPZZgD9HQKd30oJhio5UB0wFJmsc6r8u48Q+EYA==`. It is a private test dependency installed with locked `npm ci --ignore-scripts` into ignored `.build/conformance/`, never an application dependency. Stable 0.1.16 source was also compared: its raw initialization fixture does not supply the modern Auto probe needed here. No unreviewed latest-version install or production SDK upgrade occurs.

The official CLI starts its actual scenario server and invokes a fixed owned client entry. That entry launches the real isolated Grain harness application. The narrow HTTPS relay keeps the production provider's exact HTTPS endpoint/CA policy while forwarding unchanged bodies, statuses and protocol headers to the official owned loopback HTTP server. It refuses credentials, redirects, arbitrary paths/endpoints and unexpected tool arguments; verifies the official listening PID at startup and before every exchange; caps incoming requests at 64 KiB and observations at 128. A bounded 4-KiB passive discovery observation diagnoses the blocker without changing the wire. Sessions, sockets and upstream handles close explicitly.

The scripted model verifies actual search → selected load → UI approval → exact untrusted-result envelope → final result. It neither supplies the tool result nor approves through a direct executor. The only Rust addition is a finite debug instruction behind existing application/window/profile guards. The [official client runner](https://github.com/modelcontextprotocol/conformance/blob/f44482ba17df816d3176962a11cdf36aec9bda00/src/runner/client.ts) supplies the command/URL handoff; Grain's wrapper adds ownership and evidence checks. The [2026-07-28 versioning contract](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/versioning.mdx) is the protocol reference. A bare SDK example with forced negotiation is not evidence for Grain's Auto adapter.

## Reproduction and expected exit codes

Build the stamped real application with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1`. Use Windows, Node, Python `cryptography`, an idle desktop and no simultaneous native compilation. Then:

```powershell
node tests/agent-harness/conformance.mjs --install
node tests/agent-harness/conformance.mjs --suite tools
node tests/agent-harness/conformance.mjs
node tests/agent-harness/conformance.mjs --scenario mcp.conformance-tools-modern --fault missing-official-check
```

The tools subset exits **0**. The full entry currently exits **2** for the retained initialization Blocked result. The deliberate missing-check fault exits **1** after a genuinely successful approved tool call and cleanup Pass. Inspect the Node exit code and JSON report, not a surrounding shell's generic nonzero mapping. Cases run serially, never replay an action and retain separate verdicts. Official observations are bounded at 60 seconds; the parent has a 90-second observation bound and may stop only its owned child tree, preserving failure/cleanup status.

There are **51 maintained scenario IDs**: 48 self-contained real-app cases and three externally supplied official cases, one currently Blocked. Ordinary `run.mjs --suite all` still selects all 48 self-contained cases. Official IDs require the owned CLI binding and cannot silently run without it. This is an explicit prerequisite distinction, not a waived test.

## Final evidence

Reports live under `tests/agent-harness/.runs/<run>/evidence/report.json`; raw official checks and each actual application child report are retained beside them.

| Final run | Result | Cleanup |
| --- | --- | --- |
| `conformance-A5VZOH` | Two tool Pass / initialize Blocked; complete entry nonzero | Pass |
| `conformance-FVzpxr` | Fresh-profile tools subset: two Pass, exit 0 | Pass |
| `conformance-3ZS7TL` | Complete clean repeat: two Pass / one Blocked, recorded Node exit 2 | Pass |
| `conformance-KM0Sf7` | Required-check fault detected after real modern tool success, exit 1 | Pass |
| `logic-pOtLWL` | All 46 normal production MCP tests Pass | Pass |
| `run-TAS4pn` | All eight existing real-app MCP cases Pass, including unchanged 45/90-second deadlines | Pass |
| `run-yp7aB8` | All eight native-auth regression cases Pass; zero remaining run-scoped credentials after independent cleanup | Pass |
| `run-Ufo4fZ` | Both ordinary smoke cases Pass | Pass |

Legacy official wire validation reports 17 messages; modern reports 14; both report zero violations and exactly one successful numeric tool check plus one successful schema check. Final conformance children retain zero sessions/upstreams/delays and remove their owned data/fixture/TLS roots; official server ports are closed. The intentional fault removes only the required check from the verifier's input; raw official success evidence remains retained. It is not product acceptance.

The MCP regression retains 116 separate observations and 80 actual tool calls, with zero retained sessions/held replies/delays. Native-auth teardown independently deletes four discarded run-scoped credentials and verifies zero remaining; this does not close production orphan reconciliation. Native/smoke reports share the exact final build and runner identities below. Owned data/fixture/TLS directories are absent after all three regression runs.

| Identity | SHA-256 |
| --- | --- |
| Host `C:/gt/debug/grain-agent-harness.exe` | `ebd9c3895a2ae6b84dcd08bc6d2b86a5a924c36f19306f0cc94c301e76c2f40d` |
| Build source | `504051b63059c83a92805a014fd2e90cfb4a6d97af9fc3989def30c8d0793325` |
| Final runner | `116dc22d2009392cb1257998f2091b8911d35ee83f8afb8a26ce6e6c199ae080` |
| Official CLI | `b56fb575e61edaac83c2008af40a6a78b47815c4038a02a9b04cd336d38832a0` |
| Dependency lock | `be5af404161495eeee32c65bac3dde71fb204e281a94b673159881190ed4f1ae` |

Four feature-gated Rust harness guards, 28 runner self-tests, the real application/CLI build, frontend type checks, scoped JavaScript formatting, Rust formatting and whitespace checks Pass. Self-tests include actual listener-PID refusal without killing the server, URL restrictions, missing/empty/skipped/warning official evidence, wire validation, exact numeric result and replay refusal. Existing unrelated backend warnings remain. The pre-existing generated `src/app/bindings.ts` diff is excluded.

## Focused audit and retained findings

Five graph calls covered task context, module search, change detection, review and impact. Detection reported eight changed files/entities and eight gaps; review reported high transitive impact (500 entities, 37 files, 64 gaps); impact returned 69 direct and a truncated 500 of 567 transitive entities. These broad/incomplete graph results were supplemented by tracing actual fixed debug guards, scenario selection, CLI handoff, per-exchange ownership, result/approval and cleanup paths. They are not a complete-flow certificate.

Early `conformance-SnFISJ` failed raw initialization before the exact Blocked classifier existed. Individual legacy `conformance-Efl0if` and modern `conformance-Pd0ngy` passed before the final evidence guards; final clean runs supersede their identities. An initial runner test expected every ID in ordinary `all` and failed 25/26; it was corrected to assert the three explicit external prerequisites and final 28 tests pass. These attempts remain historical and are not erased or counted as final acceptance.

The upstream CLI emits a Node shell-argument deprecation warning. The maintained command is fixed and inputs are structured/validated; no arbitrary user command is introduced and upstream is not patched. Future pin changes need the same audit. Previous hard-interrupted `run-FCSQDD` still has Pending scratch cleanup after automatic approval review rejected its deletion with “blocked by policy”; this turn did not retry it through another tool. Its exact inventory remains in [harness retention](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory). New runs verified their own cleanup independently.

## Retention and next work

Keep the lock, finite model instruction, official verifier and real-app scenarios as regression infrastructure. Cache installations and read-only `.build/conformance-reference/` / `.build/conformance-modern-reference/` research clones are ignored, unused by runtime and removable when idle. The relay is intentional temporary integration support; retire it when the pinned official runner offers suitable owned HTTPS with equivalent production-path coverage. Retire the narrow Blocked classifier only after a compatible initialization fixture is audited; never turn Blocked into Pass by deletion. No new production background service is retained.

Next remains within B2: resolve the official initialization prerequisite and complete eligible live/provider/account portions of checks 8/17/20/31, then audit that module before B3/B4. No new manual batch is assigned by this controlled slice. General OAuth conformance, full official SDK tier, live-model quality, resources/prompts/elicitation/tasks/subscriptions and release measurements are not certified here. Tool-only product scope does not waive applicable transport/authentication obligations. Baseline testing still precedes dependency replacements, physical retirement and new platform implementation.
