# Shared extension platform: bounded stabilization

Updated 14 October 2026. This is the active work order, supplementing the
[execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md). It supersedes the earlier
GitHub breadth → Notion sequence. Source implementation and outstanding live
acceptance are distinguished below.

**Current gate:** the changed package/account contract and its focused audit
are accepted for the supported source profile. The matching GitHub signed update,
retained account, 100-tool discovery and read/restart already pass; repaired
9router now supports verified issue create/update/close and branch/file creation.
The stale issue-list check and obsolete base64 fixture expectation are attributed
and corrected without replaying writes. Update/branch traces directly show tiered
2 → 4 / 2 → 3 exposure; the separate file trace is still missing, not claimed Pass.
See [exact evidence and limits](MCP-EXTENSION-PROGRESS.md).
Notion 0.1.0 is signed and published at catalogue index 5, with independent public
delivery Pass. It uses the existing SDK/DCR path. The user has connected it and
exercised page retrieval/create/update; three completed traces show 7 successful
Notion executions, no identical repeated calls and no limit exhaustion. Full
provider/cross-provider acceptance remains pending, including the oversized
database-query schema. Discovery latency is the next identified improvement:
15 of 22 calls are search/load rather than actual Notion execution. See the
latest trace review in the progress document. Publication alone requires no new
alpha build; the subsequent Agent runtime change below does require an updated
runtime, currently verified in the development app.
Earlier dated descriptions below preserve the development history and do not
override this current gate. Custom MCP management remains hidden in alpha/release.

**Discovery research checkpoint:** [the comparison and proposed correction](TOOL-DISCOVERY-RESEARCH.md)
now cover released/pinned Codex, Goose and OpenCode plus provider documentation.
The user's later clarification prioritizes total request-to-confirmed-action time.
The latest recommendation is host-side shortlist preparation before initial LLM
inference, comparing lexical and optional local semantic retrieval; combined
search/schema publication becomes recovery. Reuse catalogue metadata and measure
actual action/receipt timestamps against the recorded Notion flows. No runtime
change or embedding-model selection has been made from these recommendations yet.

**Next work order, 14 October:** [fast tool execution](FAST-TOOL-EXECUTION-PLAN.md)
turns the supplied deeper research into a bounded lexical-only experiment. The
user explicitly chose to test without embeddings first. Reuse the existing
`CapabilityIndex` BM25 path, enrich schema metadata, prepare callable tools before
initial inference and combine search/load recovery. Develop these together, then
compare complete-required-tool-set coverage and actual confirmed effects against
the current path in the dev app. Receipt/transport changes follow only where
measured. Planning only: no runtime edits, benchmarks, model downloads or builds
have occurred for this work order.

**14 October Agent policy correction (latest user decision):** native and MCP
tool calls execute automatically after ordinary extension enablement/account
consent. Per-call approval dialogs are removed from capability dispatch, while
schema, source/account ownership, digest, availability and cancellation checks
remain. Conservative write/risk classification is retained; automatic approval
does not reclassify writes as safe reads. Unconfirmed outcomes still prevent
automatic replay of that tool in the same request.

The observation ceiling is now **50 tool rounds / 200 requested calls**, replacing
8 / 24. Duplicate requests execute normally after confirmed success and are
counted, not suppressed. Existing context/result/schema limits and timeouts are
independent. Per-task diagnostic events record round, offered count, requested
tool, repeat occurrence, automatic approval, outcome and elapsed time. They do
not contain raw arguments, results, credentials or argument fingerprints.
Finalization explicitly sends `tool_choice: none` and instructs the model to
summarize received results. A model ignoring finalization cannot execute more
tools; actual receipts survive model/continuation errors.

Focused normal-build verification: 25 tests pass across tool continuation,
execution, capability and LLM transport. Real dev-app IPC on the configured
`ag/gemini-3.8-flash` also passes one installed Notion read: empty search becomes
a normal final answer without confirmation (5 tool rounds, 5 calls, 0 duplicates,
1 automatic approval, ~32 seconds). No installer/release was built. This is one
read checkpoint, not full Notion write or cross-provider acceptance. Evidence:
`tests/agent-harness/.runs/notion-auto-20261014/`. The disposable IPC probe and
dev launcher belong to that ignored evidence folder, not the maintained harness.

Follow-up cleanup: dormant confirmation/continuation APIs and existing harness
procedures that exercise the former approval policy remain for now; they are not
the current Agent policy. Update/retire those procedures at the next checkpoint
before treating their old approval assertions as current acceptance criteria.
Separate observed provider limitation: `notion-query-data-sources` exceeds the
existing 64 KiB schema admission limit and is excluded. Do not claim full Notion
tool coverage or silently expand that contract in this approval/budget fix.

**14 October source checkpoint:** bounded public request headers now pass from
descriptor/direct JSON through validation, CLI packing, store artifact ownership,
registry persistence and SDK transport. The runtime GitHub toolset branch is
removed. The GitHub repository has a validated 0.1.1 candidate declaring
`transport.headers: {"x-mcp-toolsets":"all"}`; no catalogue publication or new
installer has occurred. Existing custom form/JSON edits preserve these options.
Publisher-public OAuth onboarding is now implemented in source too: optional
`oauthClient` supplies a public ID and exact issuer, with required advertised
`none` client authentication and existing S256 PKCE/SDK/vault ownership. Explicit
connection setup retains precedence; no packaged secrets or new OAuth service.
Client/issuer changes rotate account bindings. Normal sign-in/refresh and
admission/ownership checks pass. Item 1's code is complete for this supported
profile; live third-party and matching installed/package acceptance remain open.

**Later 14 October publication checkpoint:** human approval of registry PR 3
replaces the exact first-party tester exception with the 0.1.1 submission;
other packages still require independent review. Genuine preparation/attestation,
signing, publication and independent public HTTP delivery pass. Catalogue index 4
at registry commit `36704a63fd14bcabb28a4bc5a4f349fed2010ddc` carries tag `v0.1.1`,
source `394176037b46ecbfde34d7af0996a505b3352400`. The matching runtime artifact
build `38547281600` is in progress; installed update/account/Agent acceptance is
pending. Earlier references to an unpublished candidate describe the source
checkpoint before this publication. No production application release or Notion.

## Outcome and scope

**Current live frontier:** matching alpha/package signed update and retained
account pass (`github-alpha-kgOwRL`, followed by user interruption before a write).
The matching package exposes 100 supported tools (`github-alpha-dQ3MHw`), and
read/restart/read passes with cleanup/restoration (`github-alpha-vbh65Q`). Fresh
issue creation still omits `title` (`github-alpha-3JahkO`), with no approved write
or matching issue. Offline execution of the actual local 9router 0.5.65 converter
proves it deletes the `title` property; downloaded published 0.5.99 does the same,
including required-list removal. Do not attribute this omission to a Grain alias
bug or solve it by renaming MCP parameters. The failed generic instruction
candidate is reverted in source. Further writes need user approval of another
model route or a separately approved gateway fix; the shared gateway is untouched.
The cause of the earlier update/close failure is not retrospectively proven.

An author should add a supported native or remote MCP extension by publishing
its own package, without adding a provider branch to Grain. Users browse,
install, configure an account and use tools through the same extension journey.
GitHub is the existing live reference; do not start Notion or more provider
scenario batches until the shared gaps below are closed.

The supported profile remains restricted JavaScript native tools and remote
HTTPS Streamable HTTP MCPs. This cannot promise that every future provider,
authentication scheme or MCP transport will work without platform updates.
Custom MCP management stays hidden in release/alpha. Its registry, validation,
account and runtime paths must remain usable; do not fork a store-only runtime.
No new marketplace, authentication service, testing engine or legacy migration.

## Source audit: what already exists

| Boundary | Current source/evidence | Conclusion |
| --- | --- | --- |
| Author contract | `crates/grain-sdk/src/mcp.rs`; native authoring profile and CLI | MCP packages declare URL/authentication; native packages declare tools and handlers. Neither needs Agent screen/prompt access. |
| Package admission | `crates/grain-core/src/mcp.rs`, `mcp_connections.rs` | Signed artifact identity, bounds and endpoint admission already exist. Stored definitions are checked against original artifact bytes on reload. |
| Shared execution | `src-tauri/src/grain_mcp.rs`, `grain_mcp_connections.rs`, `capability.rs`, `agent.rs` | Store and custom MCPs already resolve into the same SDK runtime. Native/MCP tools already enter the Agent capability path. Keep these paths. |
| Account ownership | Connection leases, runtime owners, OS vault and official SDK authorization manager | Account/source changes and stale approvals already have shared checks. A new provider does not justify another account/session engine. |
| Publication | Existing CLI doctor/pack/submit, signed GitHub catalogue and installer | Keep the publication architecture. Ordinary descriptor/listing/artwork changes belong in extension repositories. |
| Tool selection | Directory/search/load and real model-boundary diagnostics | Existing tiered selection remains. Live failures below need attribution before changing retrieval or budgets. |

Two concrete contract gaps remain:

1. **Resolved in source:** `McpTransport::StreamableHttp` carries URL and optional
   validated public `headers`. GitHub toolset selection is package-owned rather
   than hardcoded in `transport_config`. Live candidate publication/acceptance
   remains separate; the current published package is unchanged.
2. **Resolved for public clients in source:** publisher `oauthClient` metadata
   reaches the shared SDK sign-in/refresh path with exact issuer and account
   ownership checks. The compiled GitHub client remains restricted to its verified
   extension/resource. Arbitrary confidential-client secrets and provider-specific
   live acceptance are outside the new public-client contract, not claimed solved.

Native authoring and publication have already been implemented and exercised in
earlier checkpoints. They need an affected-contract review, not a second SDK or
another full acceptance inventory. UI design is accepted and is outside this block.

## Work order and completion gates

### 1. Package configuration and account contract

Resolve provider options and publisher OAuth in the existing descriptor/host
boundary. Check the maintained SDK hooks before introducing custom machinery.

- Choose the smallest package-owned, public transport configuration that meets
  the observed need. Keep credentials, Authorization/Cookie, routing, MCP session
  and protocol control host-owned. Arbitrary header support is not automatically
  safe or necessary: a field named “public” cannot prove its values are non-secret.
- GitHub also offers a documented `/mcp/x/all` URL. Evaluate that alternative
  against resource/issuer/client binding before adding a general header API.
  Do not weaken exact GitHub credential ownership merely to avoid a contract edit.
- If options are added, preserve them through CLI packing, artifact admission,
  registry reload and runtime construction. Bind them to acquired revisions and
  stale-tool invalidation. Never accept unsigned overrides of store definitions.
- Define the supported third-party public-client registration route. Separate
  public client metadata from vault secrets; discover/validate the actual issuer.
  Retain explicit connection setup for providers that require it. Do not promise
  embedded confidential secrets or build an OAuth proxy for this launch.
- Preserve existing first-party GitHub sign-in and custom MCP isolation. No
  new permanent website identity is needed just to begin this block.

**Done when:** a representative signed package carries its supported provider
configuration through the real shared path without a provider-specific runtime
branch, author guidance accurately states account prerequisites, and focused
parser/CLI/registry/auth/transport tests pass. The existing GitHub exception is
removed only when its package and credential binding have a working replacement.

### 2. Resolve the observed Agent routing failures

Use the existing metadata trace to connect offered schemas → selected names →
loaded canonical IDs → dispatched action → verified effect. Inspect the failed
reports first. Do not call an unexecuted update a permission failure, or a model
claim a completed action. Fresh isolated tasks and conversational follow-ups
are different test inputs; check whether the fixture preserves necessary context.

Reproduce identified defects with ordinary Agent/capability tests where possible.
Change shared routing, validation or feedback only for an attributable defect.
Do not hardcode GitHub prompts, relax exact write approvals, replay uncertain
writes or increase the 8-hop/24-call budget merely to obtain a pass.

**Done when:** the observed failures have an attributable explanation/fix, focused
regressions pass, and one bounded GitHub real-Agent checkpoint reports actual
exposure/selection/execution. Existing native acceptance remains valid unless
the affected contract changes. No exhaustive exercise of every GitHub API.

### 3. Author-to-install checkpoint and focused audit

Use the existing CLI/catalogue route to verify the revised package can be
validated, published with explicit maintainer approval, installed, configured,
updated/disabled and removed. Prefer existing normal tests for artifact rejection,
stale leases and credential isolation. One actual install/account/tool journey
closes the block; no per-case installer builds or second publication backend.

Audit the changed contract, credential ownership and execution boundaries after
the checkpoint. Only then add Notion as a second-provider compatibility check.
Notion is useful for provider independence; it is not a prerequisite for each
shared source edit. Later custom-MCP exposure gets its own deliberate acceptance.

**Done when:** the author guide and implementation agree, the publication journey
works with the revised contract, affected checks pass and the focused audit has
no unresolved launch-blocking findings. This is not a production certificate.

## Latest live evidence: preserve both success and failure

**14 October trace-capable follow-up:** build `38446169856` from `7d7b96f2`
is installed. `github-alpha-Oq3oFk` discovers 100 supported tools;
`github-alpha-QX5iKK` passes private read/restart/read with two discovery schemas,
then three including the correctly resolved getter. New write runs `xtT5DP`
and `lOGzZe` correctly resolve `issue_write`, but the model omits the requested
title; exact scope guards refuse, no write is approved, no matching issue exists.
The earlier update/close failure is still unattributed. A generic requested-field
instruction is implemented as a candidate improvement; matching live acceptance
is pending. The file prompt now states its independently verified branch exists
because each stage is a fresh conversation. No uncertain write was replayed.

This build predates package-owned headers and public-client declarations. The
100-tool/read result certifies its old host-owned toolset behavior, not the
unpublished 0.1.1 package or latest source. The historical runs below retain
their original limitations. No Notion package or provider-specific runtime fork.

The historical runs below used the older installed binary
`425610fd66921c14c9c26ebddc21b757b8d7c15419a8029aec567f7c0e72d3c2`,
not the new full-toolset/trace source. Reports are under ignored
`tests/agent-harness/.runs/<run>/evidence/`.

| Run | Observed result | Remaining limitation |
| --- | --- | --- |
| `github-alpha-iuyNiW` | Private README read and read after restart; 4/4 stages Pass, cleanup Pass | Read/restart acceptance, not all-toolset coverage |
| `github-alpha-yTL7At` | 49 supported default tools discovered; preflight/catalogue and cleanup Pass | Not the raw all-toolset catalogue or proof that all tools execute |
| `github-alpha-F01z0v` | Issue #1 created with independently verified title/body; cleanup and log restoration Pass | Update/close Fail; independently observed still open with original body; cause not established |
| `github-alpha-OwevZN` | Branch `grain-check-d7acad35` created and independently verified; cleanup and log restoration Pass | File creation Fail: model proposed Create branch again; scope guard refused it, no file write approved |

The test issue and branch remain for inspection. No failed write was replayed.
Absence of an observed mutation is not proof of every internal dispatch outcome.
Actual all-toolset exposure remains unverified until a matching binary is used.
Tester build [38446169856](https://github.com/Punit-Dethe/Grain/actions/runs/38446169856)
was dispatched from `7d7b96f2`; no additional build/release is required for this audit.
That build predates the package-header contract and cannot accept the 0.1.1
candidate. Batch a matching tester build after the shared account contract is
ready; do not publish the candidate to the current catalogue prematurely.

## Testing effort and parallel work

Use fast affected normal checks during development, one real-app checkpoint per
completed shared block, then an affected-boundary audit. Do not reopen historical
test counts or create a new matrix for every provider. Harness changes must be
limited to missing evidence or execution safety. The temporary `--files` subset
used for the independent branch/file run is removed from maintained source; its
report remains valid historical evidence. Metadata-only failure diagnostics stay.

A second account is not needed for contract ownership or implementation now.
An independent, read-only review of the settled contract could run in parallel
later; it must not alter the same files, use live credentials or launch another
test batch. Build time should overlap useful source work. Do not poll unchanged
build progress or start extra builds just to stay busy.

## Reference checks for the next implementation

- [GitHub remote server](https://github.com/github/github-mcp-server/blob/main/docs/remote-server.md): explicit toolset URLs and request-header configuration.
- [OpenCode MCP configuration](https://opencode.ai/docs/mcp-servers/): remote configuration separates headers and OAuth/client setup. This is a reference, not a requirement to copy its entire surface.
- Grain's locked `rmcp` transport already exposes `custom_headers`; the current
  GitHub implementation uses that maintained hook. Inspect the installed SDK
  source for ownership/cleanup behavior when changing the contract.

Item 1's supported source contract is implemented; the active code/checkpoint
work is item 2, then matching package/install acceptance and affected audit.
This document does not claim live third-party OAuth or GitHub writes are solved.

The bounded Agent argument instruction follows normal prompting guidance, not
provider-specific schema rewriting or relaxed execution checks:
[OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling)
and [Anthropic tool-use guidance](https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works).
Its behavioral acceptance still requires the real configured model.
