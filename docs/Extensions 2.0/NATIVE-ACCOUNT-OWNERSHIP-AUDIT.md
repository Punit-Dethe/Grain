# Native account binding and ownership — B1b audit

Date: 2 October 2026 (India; evidence timestamps below are UTC). Branch: `extensions/tool-only-retirement`, base `c131e3b6`. Companions: [execution order](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md), [progress ledger](MCP-EXTENSION-PROGRESS.md), [fixture prerequisite audit](NATIVE-AUTH-FIXTURE-AUDIT.md), [runner instructions](../../tests/agent-harness/README.md).

**Historical B1b snapshot:** its 33/20 ledger and five-check handoff below are retained. [Later B1c schedule acceptance](NATIVE-AUTH-SCHEDULE-AUDIT.md) closes 42/43/50/51; current progress is 37 Pass / 16 Pending, with 44 next.

## Verdict and scope

**B1b's numbered native binding/owner requirements 41, 45 and 53 are accepted as reviewed real-application automation against a controlled public-client provider.** This is host-policy acceptance, not live-provider/browser compatibility, crash reconciliation or whole authentication certification. The ledger is **33 Pass / 20 Pending**: 15 user-reported and 18 reviewed automated. Five B1c checks remain (42–44/50–51); all seven R0–R6 release gates remain open. No SDK/OAuth replacement, shipping feature, UI design or ordinary-profile/vault modification is included.

Two new maintained scenarios expand the inventory from 33 to **35 distinct cases**. Their provider is external to Grain; the actual app still performs declaration validation, real permission review, OAuth/PKCE, callback/code exchange, OS-vault reads/writes, registry publication, tool loading/approval, worker execution and restart. Developer fixtures are built with the actual stamped Grain CLI and local esbuild, not precompiled mock workers. The scripted model verifies actual returned JSON, including both account and source-owner labels.

## Accepted procedures

| Check | Real application evidence |
|---|---|
| 41 | `native.auth-binding` starts with an actual account A grant. Fixed debug setup constructs an expired unbound session credential, then separately an expired historical id-only credential in the run vault. Each reports `needs_reauthorization`, refuses selected tool loading before dispatch, and produces zero API reads/refresh exchanges, both before and after owned restart. Explicit reconnect yields a real A read and survives another restart. Four separately reviewed changes—client ID, token endpoint, scopes, API hosts—each refuse an old approval with zero dispatch, require reconnect and accept a fresh read/restart. Restoring the base declaration also requires reconnect. Actual consent text includes the added scope/host; the provider verifies the requested client/scope and correct token path. |
| 45 | `native.auth-owners` connects the installed owner as A. Developer A starts disconnected, connects as B, and retains B through same-directory registration and the real developer WebSocket reload without another exchange. Another directory starts disconnected and connects separately. Unloading restores the installed A grant. Removing the parked installation while developer A is connected deletes exactly one credential according to independent vault metadata counts; B still performs real reads before and after restart. Old id-only reconnect requirements are supplied by check 41. Native helper identity/service are fixed; no MCP grant is read or rewritten. Live MCP grant compatibility remains in B3. |
| 53 | The actual CLI packs and doctors distinct developer A/B handlers. Published owner transitions installed → A → B → installed refuse all three old pending approvals without dispatch. Real results identify the expected source and account; restart after B and installed restoration keeps those values without silent reauthentication. The final parked-installation removal leaves only the developer owner, and unload removes that owner. Earlier permission-free 28/39 evidence remains retained. |

Each final binding run verifies **17 real account/owner replies and eight explicit account-load refusals**. Each owner run verifies **11 real replies and two disconnected-owner refusals**, four successful CLI commands and three obsolete-approval refusals. A credentials badge, generic model-written answer or absence of an error cannot pass these assertions. Missing action schemas are accepted as a negative result only when the actual loader gives the exact supported native account refusal; other missing schemas remain errors.

## Focused source audit

Graph tools were not available in this session; RTK was also missing. The repository's fallback was used with direct, scoped reads/diffs. Audit traced `capability::load_extension`, `grain_auth` binding/status/access and publication, `grain_commands` import/load/unload/uninstall, registry developer parking/session ownership, worker retirement, CLI child cleanup, developer sockets, model assertions, provider receipts and vault metadata cleanup. It does not infer isolation from absent graph edges.

- New application code is harness-feature-only. The existing ordinary app/profile and release refusal remains intact. The original permission-free fixture admission is unchanged. Auth operations require the isolated main window/profile plus explicit marker port and fixed fixture ID/path family. Developer registration uses production paths and cannot authorize a tool directly.
- Declaration variations are finite: the two fixed public-client names, two exact token paths, base/read-extra scopes and base/base-plus-localhost API hosts. The additional localhost API declaration must carry exactly its matching additional localhost network permission. TLS remains restricted to the marker's HTTPS localhost endpoints, normal certificate/hostname verification and owned certificate. Arbitrary hosts, clients, ports, scopes and permissions remain refused. No shipping trust exception is introduced.
- Historical credential setup requires an idle Agent and installed owner. It uses the production token serializer/vault service internally, expires the selected fixed fixture token and strips binding, optionally creates its id-only form and clears its registry pointer. No key/token/path/namespace is accepted from IPC or exported to JavaScript. It retires only the reviewed fixture worker. Unique session keys and registry revision checks prevent this setup from clearing a newer account pointer; any failed setup remains a failed test with independent cleanup.
- The provider independently checks the expected client and scope, PKCE, redirect and token endpoint; account reads are derived from the actual bearer credential. The model checks actual result JSON against the requested account and source owner. Journals retain flags/stages/counts and a token-path label, not tokens/codes/verifiers/authorization URLs or private result payloads.
- The new inventory switch uses the same bounded metadata-only credential enumerator as cleanup. It is read-only, scoped to the exact run service and fixed key family. Parked uninstall must reduce its count by exactly one while a real developer read remains usable. Cleanup still reenumerates and verifies zero credentials remain.
- Developer reload adds only the second fixed harness ID to the existing test client; the production DevControl role remains reload-only. CLI helpers keep owned-process tree cleanup, deadlines, bounded output and offline dependency use. New fixture folders/packages are explicit owned scratch children and are removed even on failure.
- The negative account-load driver waits for an actual verified loader refusal and Agent completion. It does not treat an initially idle flag as completion. Normal successful reads still require approval and exactly one dispatch; stale approvals require zero. The oracle self-test rejects generic unavailable results and forged owner labels.

No normal-build behavior was changed in this unit. Existing auth Clippy warnings remain; no new harness/auth warnings were found. References rechecked: [Goose extension ownership and cache invalidation](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/extension_manager/mod.rs), [LibreChat real-session disposal assertions](https://github.com/LibreChat-AI/LibreChat/blob/main/packages/api/src/mcp/__tests__/MCPConnectionDisposeRace.test.ts), and the pinned keyring implementation/Grain production sources. These inform ownership and observable tests; they do not certify Grain or replace its native policy.

## Failures retained and corrections

| Earlier evidence | Finding and disposition |
|---|---|
| `run-dD7cSH` | Harness incorrectly waited for action approval after an unusable credential. The actual native loader correctly refused schema hydration. The driver/model now verify that exact early refusal, zero dispatch/read/refresh and no accepted result. |
| `run-waqXKt` | Harness expected an unchanged tool title on an auth-only permission review. Actual UI correctly showed only changed authentication. The assertion now checks provider/scope/host review; later added scope/host text is verified explicitly. |
| `run-FUSQAG` | Immediately after submit/restart, the initial idle flag was read before the model's negative result arrived. The driver now requires the independent refusal flag as well as inactive state. |
| `run-hhQ4pu` | The added localhost API host lacked its matching network permission. Production validation correctly rejected it. The finite fixture variant now pairs that host with its exact permission, and the fixed admission checks require the pair. |
| `run-BIyDxJ` | All three cases passed before final endpoint/client/scope and independent vault-count assertions. Earlier evidence, not relabeled as final. |

All four early failed runs have cleanup Pass and vault remaining zero. No scenario/call is automatically retried to convert a failed result into success. Corrections above changed fixture setup/assertions; no production bug repair is claimed by those failures.

## Final evidence and reproduction

Windows x64, Node `v24.11.1`, WebView2 `154.0.4258.48`, controlled HTTPS provider and scripted model. Two final clean profiles share runner fingerprint `41a9981e4eb89932e46890ca575391f8d3bb49376bec367ea55ba052eb03c435`.

| Directory below `tests/agent-harness/.runs/` | Final result |
|---|---|
| `run-Sm6vQR` | Three account cases Pass; fixture/binding/owner durations 3,323 / 21,257 / 12,695 ms; 2 October 00:24:14 UTC |
| `run-SUUw9I` | Independent combined repeat Pass; 3,252 / 21,658 / 12,610 ms; 00:25:54 UTC |
| `run-SIpUxE` | Wrong source-owner fault Fail/exit 1, 3,418 ms; detects actual forged developer result; cleanup Pass |
| `run-hLUZEv` | Unchanged-declaration fault Fail/exit 1, 8,284 ms; actual connection stays `connected` instead of falsely requiring reconnect; cleanup Pass |
| `run-Flf10g` | Existing ordinary native hot-reload scenario Pass, 19,552 ms; auth control without its marker remains refused; cleanup Pass |
| `run-03ezD3` | Ordinary cold/warm tool and declined-approval smoke cases Pass, 1,090 / 907 ms; cleanup Pass |
| `run-n0JUwa` | Final formatting-only runner repeat: all three account cases Pass, 3,383 / 22,309 / 12,630 ms; 00:38:56 UTC; cleanup Pass |

Formatting of the new scenario module changes only the runner identity: `run-n0JUwa` records `d48584834c54a4ef99ca07e4907d78adf9732bfe690eadb56913ea75d4ef350e`. Earlier repeat/fault evidence retains its original fingerprint.

All final account/fault runs share binary SHA-256 `0cd3d1f5721a9a35ad02e0e5e3c7347be3c2ec1683ad7652aa4762be3e321039`, source fingerprint `846c08a53c1544dd5de17071751285e4b0ecab444bdb713e520d5ad12b1e0a61`, built 2 October 00:18:45 UTC from base `c131e3b6a0d49efc153b6e603ebd13ec6a94803e` plus recorded dirty source. The actual CLI SHA-256 is `10babd722118db5ff80d79aa549355c381d2da750585de49eeb72f8ed068dd4a`, stamped against that same source. The unrelated existing bindings edit participates in the build fingerprint and is not changed/committed by this unit.

Normal backend check, **23 auth tests** (`logic-GrnLKF`), three final feature guards, **20 runner self-tests**, real frontend type/build, harness build and final Clippy pass. Scoped formatting, PowerShell parsing, evidence privacy/scratch cleanup, document links, 53-check ledger and scoped whitespace are checked before commit. Reports remain ignored; no private token marker or authorization URL appears in final evidence text.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
npm run test:agent -- --suite native-auth
npm run test:agent -- --scenario native.auth-owners --fault wrong-owner
npm run test:agent -- --scenario native.auth-binding --fault unchanged-auth-declaration
```

Use the documented Windows/WebView2/Node/Python-cryptography prerequisites and an idle desktop; run runtime suites serially. Both fault commands must give Fail/exit 1 and cleanup Pass. The original wrong-account/abandoned-auth prerequisite faults remain separately documented.

## Explicit limits and next block

The final combined runs required independent deletion of **three discarded developer grants** after their registry owners were removed; remaining zero. Selected owners cannot use these abandoned sessions, but production developer-grant orphan reconciliation is still open. The cleanup helper is a harness guarantee, not a claim that ordinary unloading reconciles every vault orphan. Preserve this finding in R3's existing reconciliation work and release audit; B1b acceptance covers the numbered read/binding/owner requirements only.

No live provider or browser UX, issuer-specific scope equivalence, cross-platform vault, power-loss recovery, microphone/pill/shortcut behavior, genuine-model judgment, measured RAM/handles, MCP authentication or whole R3 certification follows. The earlier Escape investigation remains open. No extra manual batch is needed for this controlled native unit. Next: **B1c 42–44/50–51**—held callbacks, cancellation/switch/refresh/logout, failed-switch persistence, partial consent and expiry—then its focused audit and repeated tests before moving to MCP B2.
