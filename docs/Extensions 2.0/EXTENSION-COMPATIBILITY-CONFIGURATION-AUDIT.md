# E1c: compatibility, configuration and checkpoint testing

3 October 2026. Branch: `extensions/tool-only-retirement`. This completes the provisional E1 contract when the final evidence below is accepted; it does not freeze the public SDK or certify production release. E2–E8 remain separate delivery stages. Physical retirement remains on hold.

## 1. What changed and why

Developer loading and CLI doctor already checked native `grainApi`, but installed pack validation treated it as informational. An explicitly future-profile pack could therefore enter a path that a developer project could not. One bounded, pure SDK helper now owns the numeric exact/caret grammar for author checks, developer loading and every installed validation boundary. There is no general semver evaluator, background service, additional dependency or second runtime.

Native pack validation refuses explicit unsupported requirements before execution or registry publication. Catalogue installation additionally checks its literal minimum API before artifact download/staging; signed metadata cannot substitute for downloaded manifest validation. The audit separated catalogue visibility from installation eligibility: valid future-version tool cards remain visible, while installing them is refused. This preserves the existing browsing behavior rather than changing store design.

The only removed functions are duplicate **active** version parsers in developer loading and author checks. Retired OS/context/prompt/visual capability branches, refusal tombstones, legacy wrappers and the external registry are retained. No Handy-derived file is edited.

## 2. Provisional compatibility profile

| Layer | Current rule | Ownership / boundary |
|---|---|---|
| Grain native API | `1.0`; numeric `major.minor[.patch]` exact or leading caret; same nonzero major for caret | Shared SDK helper, developer loader, author doctor, installed manifest/runtime admission |
| Missing native API | Existing installed metadata may omit it; author projects require an explicit supported requirement | Legacy compatibility only; does not authorize retired capabilities |
| Unsupported grammar | Wildcards, general ranges, prerelease/build suffixes, leading zeroes, overflow, oversized values and future requirements refused | Bounded static compatibility error; no attacker-provided value echoed |
| Catalogue minimum API | Literal numeric version no newer than the host; legacy omitted minimum accepted | Installation/staging check, separate from tool-only browse eligibility |
| MCP descriptor | Schema `1`, exact canonical `grainApi: "^1.0"`, bounded nonsecret descriptor | E1b strict parser; native grammar does not loosen this JSON profile |
| Extension package version | Publisher artifact identity, distinct from API requirement | Existing package validation, digest and approval/owner checks |
| MCP protocol version | Negotiated by the official SDK during initialization | Independent of descriptor schema and Grain API; no package-version negotiation shortcut |

Current native execution means an embedded JavaScript tool extension (`scripted` internally), not the retired companion-executable tier. It exposes declared tools and the narrow invocation/storage/network/auth API described in the [native author audit](NATIVE-AUTHOR-CONTRACT-AUDIT.md). The Agent supplies arguments; an extension cannot acquire screen/OCR/selection/host prompts through this contract.

The MCP profile covers remote HTTPS Streamable HTTP with `oauth` or `none` authentication. Existing SDK interoperability includes the tested older/modern negotiated protocol cases; this does not add an author-selectable legacy SSE transport, STDIO, subprocess launch, arbitrary custom runtime or custom-MCP UI. E1b metadata admission creates no network client and does not prove arbitrary DNS/network destinations safe.

## 3. Configuration and account ownership

| Information | Rule | Implementation status |
|---|---|---|
| Native tool arguments | Declared, validated tool inputs | Existing runtime / author contract |
| Native own-service nonsecret preferences | Scoped extension storage and tool inputs can be used today | Existing API; no generic contributed settings page reinstated |
| Native service authentication | Public OAuth client metadata/declared API hosts are author metadata; Grain owns consent, PKCE, vault and current account binding | Existing single-account-per-extension implementation; native `oauth2` reuse accepted |
| Raw credentials | Stay in the host vault; native auth reports status and authenticated network calls ask Grain to attach credentials | Existing host boundary; no raw token getter |
| MCP publisher metadata | Endpoint and supported authentication mode only | Strict E1b descriptor; no enablement, credentials, trusted status or host account IDs |
| Direct custom MCP setup | Bounded nonsecret JSON without a package; host assigns configured source/connection/account identity | **E2 implementation remains**; current catalog developer panel is not this feature |
| Store MCP setup | Verified descriptor provenance plus the same host connection/auth/lifecycle boundary | **E2/E4 implementation remains**; not yet installable from the store |
| Optional setup forms | Grain renders only declared own-service nonsecret setup when needed; secrets use host-managed vault references/account setup | **E2/E5 design and implementation remain**; no extension-owned React/UI, prompt priority or core-setting hooks |

Host source identity comes from the actual acquisition path, not a JSON field claiming trust. Catalog vault keys are preserved. E1b defines distinct configured/store namespaces; those namespaces are not yet persisted or exercised as live connections. E2 must bind consent and execution to connection/account/source/configuration revisions, reject stale approvals after a change, and require explicit migration rather than silently borrowing an old account. The existing native ownership/reload/restart tests cover native behavior, not this future MCP route.

## 4. Error ownership

- SDK validation gives a bounded compatibility error; CLI doctor reports `E_API_VERSION`; developer/install boundaries return the same refusal before mutation/dispatch.
- Native handlers use the declared result/error contract. Host validation handles unusable, oversized, malformed and forbidden follow-up results. A provider action without a confirmed usable reply remains an unknown outcome.
- MCP metadata validation uses typed contract errors. Official SDK transport/auth negotiation and existing Grain recovery classification remain responsible for connection failures, temporary retry advice, reauthentication and account ownership.
- Descriptor/schema errors cannot become proof that an already-dispatched action had no effect. Completed actions and uncertain writes are never automatically repeated. E2 must reuse this distinction rather than translate every error into retry.

No new authentication mechanism, account migration, automatic approval or tool-discovery policy is introduced in E1c.

## 5. Faster development without weaker acceptance

**Scheduling amendment, accepted by the user on 4 October and applied 5 October:** the table below records the earlier checkpoint policy; it no longer requires full application suites for every nearby edit. Use affected production component/integration tests during development. Run a small real-app acceptance set and focused audit at a coherent feature checkpoint. Run extended suites for changes to their actual boundary and at integration/release gates. Preserve every existing requirement and historical result; moving evidence to a lower layer requires equivalent assertions, not silent deletion.

Freeze general harness expansion. Reuse existing isolated-profile/credential/cleanup support; introduce a new harness seam only when a required behavior cannot be proved at a smaller real boundary. Keep real clocks for actual transport/OS timing acceptance, live sign-in for provider behavior and genuine-model checks for Agent workflow changes. SDK protocol requirements may use pinned compatible official conformance. No wholesale runner migration, production-clock shortcuts or retroactive Pass credit. Record one block audit with its evidence and remaining acceptance, rather than requiring fresh negative modes and broad repeats for each small edit. The next coherent store-MCP checkpoint must cover acquisition, account/update/revocation ownership and actual application management before activation is accepted.

Develop a coherent bounded block before rebuilding the real application. During edits run affected fast logic/type checks; do not postpone permission/account/ownership guards until the end. At the checkpoint use one stamped build for serial real-app scenarios. Each scenario retains its own setup, assertions, stages, dispatch counts and verdict; a failure stops the batch and preserves evidence. Sharing a process/build is not sharing a verdict.

The maintained `extension-contract` subset selects twelve existing/new cases: public results, API compatibility, typed inputs, legacy migration, CLI package ownership, owner restoration, native account fixture, signed store integrity/close, MCP transport contract, MCP provider/account independence, native cold/warm and Agent denial. It uses the real Grain host and isolated profile, not a browser replica. It adds no shipped runtime. All original suites remain available.

| Changed boundary | Required checkpoint coverage |
|---|---|
| Pure extension contract/validation/author types | Affected workspace and author checks, `extension-contract`, focused audit, fresh affected repeat and negative control |
| Native permissions, ownership or execution | Relevant full native foundation/installation/account/failure/Agent suites; expand beyond the contract subset |
| OAuth, issuer, grants, vault or consent | Relevant full native/MCP auth suites and independently inventoried scoped credentials; live consent if the changed behavior requires it |
| MCP transport, protocol, deadlines, cancellation, paging or resource lifetime | Full affected MCP foundation cases, including actual 45-second HTTP and 90-second discovery deadlines |
| Shared fixture/executor/cleanup | Broader affected suites and cleanup negative oracles; no acceptance based on a narrow passing case |
| Documentation only after acceptance | Links/consistency/whitespace checks; no unnecessary application rebuild |
| Release certification | E8/R0–R6 complete coverage/CI and required live/visual review; this subset is insufficient |

The slow deadline cases are not removed or shortened; they already passed on E1b and remain mandatory for changes affecting their boundary. No increased production budgets, lowered assertion count, automatic retry, parallel use of the fixed IPC/callback ports or live-model substitute is permitted. After audit repair repeat the affected case and its negative control; do not automatically repeat unrelated passing suites without a new risk. Results remain Pending until executed and reviewed.

## 6. Research and source audit

The [official MCP lifecycle specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle) separates initialization/version negotiation and capability agreement from package authoring. It also requires bounded request handling. This supports retaining SDK protocol ownership and real deadline regressions.

[OpenCode MCP configuration](https://opencode.ai/docs/mcp-servers/) keeps server connection/authentication configuration with the host. The pinned [Goose extension source](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/agents/extension.rs) distinguishes extension connection forms. Grain takes the ownership/separation pattern; it does not adopt every process-launch or platform capability. This is an architectural inference, not a claim that those projects implement Grain's contract or tests.

Graph context/search/impact preceded file inspection. Semantic lookup lacked the target method; minimal impact reported no flows, while review context later reported 70 impacted nodes in 24 files and a high risk score. Neither proves correctness or absence of tests. The required refactor suggestion ignored its file filter and returned unrelated repository-wide removals; none were applied. Manual call-path/diff review traced manifest validation, doctor, developer loading, runtime admission, store pre-download and core pre-filesystem staging. The catalogue visibility correction and a pre-hash/pre-filesystem refusal regression were made before final runtime acceptance.

Existing Clippy and normal/harness host warnings are retained; this is not a warning-free build claim. The unrelated generated `src/app/bindings.ts` working-tree change is preserved and present in the recorded build, but excluded from the commit. E3 still owns the previously recorded CLI/desktop IDNA dependency parity issue. No legacy or external repository cleanup is performed.

## 7. Evidence and acceptance

Forward **59 Pass** after final-path execution, source audit, fresh affected/negative runs and independent cleanup. E1's provisional compatibility/configuration/error ownership rules are settled; the public freeze remains E8.

| Evidence | Final result |
|---|---|
| Locked workspace Rust tests | **485 Pass**: 272 core + 4 integration + 9 CLI + 23 author checks + 54 pill + 79 SDK + 22 registry tools + 22 router; zero ignored/failed |
| Normal backend developer logic | `logic-zrzREl`: **4 Pass** |
| Normal backend store logic after compatibility correction | `logic-w6tgrt`: **8 Pass** |
| Maintained Node self-tests | **74 Pass**, including checkpoint selection and wrong-scenario fault refusal |
| Generated author project | `author-gF6tXu`: four stages Pass; supported types, fifteen negative cases, actual compiler negative and generated bundle |
| Real-app checkpoint | `run-cHrC16`: **12/12 Pass**, 88 stage observations, 17 recorded host lifetimes; scenario time **59,082 ms**, full run about 62 seconds excluding build |
| Fresh compatibility repeat | `run-QcwCt7`: Pass, seven observations, **1,744 ms** |
| Deliberate unsupported-as-current fault | `run-fpudqD`: expected Fail/exit 1 at **Missing expected rejection**, not an unrelated transport/setup failure; cleanup Pass |
| Independent read-only cleanup | `e1c-contract-cleanup.json`: three completed scopes, 19 PID references, zero reused PIDs/owned processes/listeners/scratch/native grants/MCP grants/client secrets/registrations |
| Build and static checks | Real Tauri/CLI stamped build, TypeScript/Vite production build and workspace Clippy exit 0; existing warnings retained |

Final product source fingerprint: `3dec808ee1e2e9b0d33959c21119b605624838aa98136066b066577fc7383131`. Host SHA-256: `8fe1c424729917fe9db02689d755b2883dac48eb20af78f6d6c2a72088a9c028`. CLI SHA-256: `39d4359a4e23a8e88feaa2dfdbe97ed15974f18d720ad364ba95776c3942e569`. Runner fingerprint: `fa4fd165f24642e364d3eda111adb10214fe5d3041f130bd0b3545bc2757c5c3`. Precommit base: `0fb45c1a`; reports retain dirty-file inventory including the unrelated bindings change.

The API scenario refuses three unsupported installed requirements with unchanged registry bytes/owner and zero added dispatch, refuses the future developer requirement without displacing the working installed owner, then executes a legacy omitted-profile package. The negative supplies a supported profile at that exact refusal boundary and is caught. The staging regression gives a future-profile catalogue entry invalid artifact bytes: compatibility refusal must win before hash or filesystem work. Existing E1b descriptor/identity matrices remain in the passing workspace checks. No production transport deadline/cancellation/auth algorithm changed, so the already accepted full E1b MCP deadline batch is retained rather than repeated for this contract-only edit.

## 8. Retention and next delivery

Keep the shared SDK helper/tests, compatibility scenario, exact negative fault, checkpoint selector/test and documentation as maintained regression infrastructure. Ignored build logs, `.runs/author-*` generated projects, completed run reports and read-only inspection scripts are disposable evidence; they are not production features. Completed runtime scopes must retain only their evidence/marker, with zero owned processes, listeners, scratch profiles/TLS or scoped native/MCP credentials.

The three earlier interrupted roots (`run-FCSQDD`, `run-t1ux8t`, `run-uZZ7J8`) remain separate policy-blocked cleanup exceptions and receive no new acceptance. Live Linear expiry/refresh check **12 Deferred** and permanent public client metadata/provider activation **56 Pending** remain unchanged.

After E1c acceptance, **E1 provisional contract is complete**, with E2–E8 (seven delivery stages) remaining. Next is one bounded E2 custom remote MCP acquisition/persistence unit under the descriptor and ownership rules above. Custom networking/auth/store routing, SDK/CLI descriptor authoring, publishing, management UI, remaining runtime policies, measured mixed sources and public freeze are not claimed implemented by this document. No new manual test batch is needed for E1c.
