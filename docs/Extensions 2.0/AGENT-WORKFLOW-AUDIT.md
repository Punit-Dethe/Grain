# Agent selected-tools and approval-continuation audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Scope:** numbered checks 46 and 47. **Status:** focused audit complete; final controlled and genuine-model runs, clean repeats, four detected faults and affected regressions accepted.

## Acceptance contract

Check 46 needs metadata-only discovery, incremental native/MCP schema selection, preservation of earlier selections, pagination/exact/no-match coverage, directory and schema limits, and a genuine tool-capable model trace. Check 47 needs actual disposable read → approved write → independent verification in the same task, exact receipts, duplicate/stale refusal, and genuine-model runs through both adapters. The controlled model deliberately emits multiple calls; the genuine model is allowed to choose sequential calls. Record actual batch emission rather than assuming it happened.

Check 48 remains separate: multistep denial, real confirmation expiry, Stop/close, later model/provider failure and uncertainty plus its live observations. Single-operation historical tests and the live relay's own cancellation unit test do not fulfill that numbered requirement. Whole B4 and all seven release gates remain open until their stated criteria are met.

## Sources and review approach

The design cross-check used [Anthropic's on-demand tool search documentation](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool), the [MCP tool/confirmation/pagination specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), and [Goose's Agent source](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/agent.rs). These support narrow on-demand exposure and host-owned approval; they are references, not evidence that Grain implements a particular provider's search API or an entire agent framework.

Graph minimal context, file summary, impact radius, change detection and review context identified Agent/catalog/fixture consumers. Their coarse flow/test mapping and newly added JavaScript coverage gaps required direct diff/source review. Graph output with zero affected flows was not treated as a safety guarantee. The focused review examined `capability.rs`, `capability_agent.rs`, `agent.rs`, real React confirmation handling, harness guards, model serialization, peer configuration, scoped I/O, result identities, and teardown.

## What changed

- Six independently reported `agent-workflow` cases share one real host, scripted model and existing owned HTTPS peer. A native worker and MCP peer each operate one disposable object. Native values live only in the worker; MCP values live only in the owned peer. No personal provider writes or new extension privileges are introduced.
- The 24-tool supported catalog supplies two metadata pages, exact/no-match searches, two staged reads and reuse of the first tool. Neither an unused write nor bulk schema becomes callable. Schema-limit refusal loads eight valid definitions whose combined size exceeds 32 KiB; no partial publication occurs and the prior read still executes.
- Directory coverage seeds 101 valid permission-free installed artifacts only between owned host lifetimes. Actual production startup/discovery reports 100 of 101, refuses the omitted owner and starts no workers. The registry is restored on a second actual restart, including in `finally`; artifacts remain inside the disposable profile until owned-root teardown removes them. This is a startup fixture, not 101 individual consent approvals or a release-scale certification.
- A real model frame requests write/verify/write together. Grain holds the first write confirmation and records both tails as unexecuted. After approval, the model receives the write receipt correlated to its original call ID, then asks separately for verification. Three consumed-token replays are refused without dispatch or destroying the next approval. Disabling the native/MCP owner while another write is pending prevents that write and both batch tails.
- A guarded debug-only capture operation retrieves the current confirmation token into runner memory for replay tests. The token is absent from ordinary builds, status snapshots, model requests and reports. Replay uses the production `agent_confirm_action` command; the helper cannot approve or execute anything.
- Two opt-in `agent-live` cases use the user's selected ordinary Grain model through the existing local test-model HTTP boundary. The runner reads the fixed ordinary profile, retains only the selected credential for outgoing model headers, sends only owned fixture prompts/catalog/results, and records schema/action/receipt metadata. No credentials are copied into the harness profile, browser, marker, model context or evidence. Ordinary suites reject the live flag; `all` excludes both cases. Smart rotation and unsupported/missing configurations are explicit prerequisites.
- The genuine model chooses actual calls and final prose. The live adapter independently validates metadata before schemas, read before write-schema loading, receipt before verify-schema loading, unchanged earlier schemas and ordered actual results. It has 1 MiB request/response limits, a 60-second upstream request deadline, TLS validation, no redirects, no idle connection pool, socket/timer ownership and frontend-close cancellation. Live harness waits permit 120 seconds per approval stage without changing any production deadline. Closing the adapter drops its credential reference; JavaScript does not provide a zeroization guarantee.
- The scripted model now accepts OpenAI-compatible final rounds that omit `tools` after Grain exhausts its task budget. No fake schemas are added. Production Agent execution, SDK/auth logic, host tool budgets, ordinary UI and dependencies are unchanged; Rust additions are feature-gated test observation/instruction hooks only.

## Independent oracles and retained failures

Native completion events and exact actual worker results are checked separately. MCP dispatch is counted from the external peer's received `tools/call` requests and object write counter. Both models must observe actual results; a generic success sentence cannot substitute for the receipt. Original call identities bind withheld and resumed results; transcript order is not mistaken for execution order.

Candidate failures are retained, with no final acceptance credit:

| Run | Observation and correction |
|---|---|
| `run-DoMXTh` | Final task-budget round omitted `tools`; the scripted fixture incorrectly rejected that valid wire shape. Normalize only omission to an empty offered list and add a positive/negative final-round self-test. |
| `run-AaDIf4`, `run-TGEilk` | Ten selected IDs hit the existing eight-ID admission limit before the intended byte limit. Use eight supported schemas with multiple individually bounded description fields; preserve both real limits. The second run repeated the unchanged failing fixture, not an accepted repeat. |
| `run-0LSGHj` | The fixture assumed the pending write result preceded withheld tail results. Correlate all three by original model call IDs instead. |
| `run-oowgYK` | A repeated MCP setup tried unloading an already-absent native fixture. Permit only the exact fixed-owner absence diagnostic; keep every other error fatal. |
| `run-qopXnl` | Genuine model completed all three native actions with correct staged schemas and receipt, but the harness incorrectly required scripted final wording. Validate real receipts and displayed value rather than canned prose. |
| `run-mZjlx4` | A live approval wait still used 20 seconds. Repair option placement, preserve 20 seconds for controlled tests, allow 120 seconds for genuine stages and rerun controlled duplicate checks. |

The live-adapter review additionally made upstream ownership follow frontend cancellation and handle an already-closed frontend before contacting the configured model. A held real local HTTP request self-test verifies socket, timer and listener release, and refusal of extra arbitrary user prompts. That unit supplies no check-48 acceptance.

## Evidence

Final source passes with these stable identities:

- Native binary SHA-256: `fd0eeb2858cf8d92ea6c45c61ad9eea809da07545154d5ab571751f6057938ab`.
- Native source fingerprint: `56f83a631d409e44e7be2b62b215df2388ce5487ec2dc1d812e108852eebe232`; build commit `d9e3aa2892408b234fe8572f5a53f45d4311e229`, built `2026-10-02T12:14:11.648Z`.
- Final runner fingerprint: `2a415ce7cbbac3604ad115981ff40243b3526f6651e2e5039b45b6f82d5028bd`.
- Affected regression runner fingerprint: `f2d44591b7a33c7911d52768bb9d7558dbe5949b8a65c96de5acf524d29c571e`. Only CLI help wording changed between this regression pass and the final focused runs; no functional/native change. The final eight cases, four faults and 44 self-tests use the final runner.

| Evidence | Runs | Result |
|---|---|---|
| Genuine configured-model native and MCP workflows | `run-cNBq4M`, clean repeat `run-tZw8Nr` | Two cases each Pass, cleanup Pass. Each has eight real model requests, selected counts 0/1/2/3, three actual approved dispatches, one write, unchanged earlier schemas and same-task continuation. Actual batch emission is false. |
| Six controlled workflow cases | `run-n2UM7v`, clean repeat `run-f8xIyY` | All six Pass, cleanup Pass. Native/MCP staged reads, byte refusal, 101-owner directory and both write/verify/duplicate/stale procedures have separate observations. |
| Missing staged-selection faults | `run-t8LZMp` native, `run-vvcyky` MCP | Expected Fail/exit 1 at missing/preserved-schema assertion; cleanup Pass. |
| Wrong actual write-receipt faults | `run-Fy1JMk` native, `run-DnegLm` MCP | Expected Fail/exit 1 at exact receipt assertion; cleanup Pass. |
| Complete native auth regression | `run-7Mh18u` | Eight cases Pass, cleanup Pass; actual expiry/refresh/logout schedules retained. |
| Complete MCP auth regression | `run-ATMgeD` | Six cases Pass, cleanup Pass; real SDK/OS vault, account/close/shutdown/client-change paths retained. |
| Native typed/migration, ordinary smoke, MCP contract/catalog | `run-zK1SEp`, `run-sRAB9w`, `run-sj4X0t`, `run-lkYzmj` | Two + two + one + one cases Pass, cleanup Pass. |

The earlier controlled runs `run-uHDIuY`/`run-HloYHl` and genuine candidate `run-X1cP9e` are supporting evidence; the final repeated runs above own acceptance. Full per-case reports remain in ignored local `tests/agent-harness/.runs/<run>/evidence/`; these tracked identities and outcomes survive separately.

Independent final inspection confirmed exact fingerprints, actual observations/peer receipts and cleanup Pass for all eight final positive/fault runs. Every owned root contains only its nonsecret marker and evidence; settings, objects, generated artifacts, TLS material and WebView data are removed. No completed-run harness/WebView/helper process remains and all 16 recorded model/MCP listener ports are closed; callback port 17124 is free. The configured model key is absent from retained final files. The user's proxy on port 20128 is outside harness ownership and remains available. This does not certify arbitrary remote-provider disposal or production memory usage.

The host build passes frontend type checking, the actual embedded Vite build, CLI/native compilation and stamping. Focused production logic: 18 Agent, 15 pure capability, five host capability and eight harness-guard tests Pass. Feature Clippy passes with 74 retained pre-existing warnings; neither the ordinary Agent path nor new test seams introduce a diagnostic. All 44 runner self-tests pass with zero skips; changed JavaScript formatting and scoped diff whitespace checks pass. The pre-existing unrelated generated `src/app/bindings.ts` edit is preserved and excluded from this commit.

## Retention and next work

Keep the fixed workflow peer/worker/catalog, identity/receipt assertions, two deliberate faults, guarded token observation and opt-in configured-model adapter as maintained acceptance tools. Remove debug observation/relay hooks only when an equivalent owned real-app path supplies those proofs. Generated directory artifacts, settings, objects, TLS material, WebViews, sockets and helper processes are per-run disposable; preserve only nonsecret marker/evidence. No unused production engine or future-feature scaffold was added.

Checks **46/47 are reviewed automated Pass**; the ledger is **48 Pass / 5 Pending** (15 human, 33 automated). Whole B4 remains open for **48**. Next complete its controlled denial/real expiry/Stop/later-failure procedures and genuine-model observation, and independent MCP-provider check **14**. Combine compatible live-account portions of **9/12/17** only after actual provider/account/schema/consent prerequisites are available. The user selected the model already configured in Grain; no personal MCP provider was selected or certified in this unit. SDK/OAuth replacement, physical cleanup and new platform features remain behind the full baseline gate. Official initialization, retained input findings, production vault/orphan reconciliation and two policy-blocked historical scratch roots remain separate open items.
