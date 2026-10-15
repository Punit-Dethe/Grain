# Fast tool execution: lexical-first execution plan

Status: lexical source checkpoint and bounded Gemini/Harbor read comparisons
recorded; actual-catalogue quality and broader adoption gates remain open.
Updated 15 October 2026.

This is the next bounded work order within
[platform stabilization](PLATFORM-STABILIZATION.md). It turns the user's supplied
"Grain Fast Tool Execution: Deep Research and Recommended Architecture" into an
implementation and comparison plan, supplementing [our research](TOOL-DISCOVERY-RESEARCH.md).
The user's latest decision is explicit: **test without embeddings first**.
No embedding model, model download, inference runtime or semantic benchmark is
part of this work order. Reconsider those only after reviewing lexical results.

## Current checkpoint

Harbor follow-up: the user's saved `deepseek-v4.1-flash:free` model completed
four genuine Agent journeys with fallback disabled and observed model identity
verified. Search answered in 7.36 s with deferred hints versus 33.10 s without;
one action versus eight (one identical duplicate in control). Full-page hints used
search, one recovery and fetch, answering in 12.15 s. The control answered in
27.04 s after five searches/two recoveries but never loaded fetch: **partial task
failure despite no provider error**, not an equivalent speed baseline. Both
pairs shared initial IDs/schema bytes. Settings/borrowed credential restoration
passed; no normal-profile changes, installer, browser or product source changes.
The earlier Gemini full-page hint sample took 17.39 s; different sessions/provider
conditions prevent attributing that difference solely to the model. These four
reads are a bounded compatibility/performance check, not the broader write/mixed
workflow gate. Hints stay debug opt-in. Investigate repeated-search/missing-fetch
recovery and complete the existing quality gate before adoption; no embedding
dependency follows automatically. Detailed verdicts/evidence are in progress.

Latest live result: the user restored 9router and three genuine read-only Agent
journeys completed. The comparable warm control and hint trial used the same
initial six action IDs and schema-byte count: hints combined discovery with an already-offered search,
reducing four model requests to three in this sample. Full-page result 13.38 s
with hints versus 21.71 s in the warm control; answers 17.39/25.44 s. An earlier
control missed catalogue preparation and is excluded from a matched claim.
One sample, unequal provider/preparation timing and one provider are insufficient
for adoption. Combined recovery is live-verified for this request; the offline
quality gate and broader comparison remain open. Keep the debug switch opt-in.
See the progress record for all three journeys, evidence and limitations.

Latest bounded experiment: scoring purpose/prose separately did not improve the
72-case K=6 regression coverage (57/72 baseline, 54/72 separate fields, 56/72 with
purpose enrichment); all scoring/projection changes were reverted. A debug-only
`GRAIN_AGENT_DEFERRED_HINTS=1` trial now gives the model at most 8 KiB of inert
deferred IDs/purpose hints, without their schemas. It can request exact missing
tools through existing combined recovery. Default/release behavior is unchanged.
Inspired by [Anthropic's deferred discovery](https://www.anthropic.com/engineering/advanced-tool-use)
and [OpenAI's namespace descriptions](https://developers.openai.com/api/docs/guides/tools-tool-search);
Grain's bounded per-tool hints are our experiment, not an implementation claim
about either vendor. These sources do not establish a Grain performance gain.
The initial recovery check was blocked by local 9router connection refusal before
any model/tool action; a normal launcher attempt was policy-rejected. The user
subsequently restored it, allowing the first same-binary comparison recorded above.
Retire the debug switch at that decision. The quality gate remains open.

15 October: reserve 8 KiB for recovery by capping a non-eager initial publication
at 24 KiB; recovery/eager publication retain the 32 KiB total. Report remaining
capacity and preserve already-offered tools. A second genuine Notion read used
search then fetch without discovery or duplicates, full-page result at 13.80 s
and answer at 18.38 s. This did not exercise live recovery or prove a speedup.
On 24 fresh requests, K=6 complete-set coverage remains 17/24 under either budget;
the adoption gate stays open. Purpose-only projection/clause/ranking trials were
rejected and reverted rather than retaining unproven complexity. Sources for the
trial: [Elastic field weighting](https://www.elastic.co/docs/reference/query-languages/query-dsl/full-text-filter-tutorial)
and [Meilisearch ranking rules](https://specs.meilisearch.dev/specifications/text/0123-ranking-rules-setting-api.html).
These inform retrieval principles, not measured Grain gains or a dependency choice.
Notion's exported fetch tool documents its Markdown resource fallback; test that
before claiming a resource reader is required. Use prerequisite-complete write
labels in future comparisons. Full results and limitations are in the progress record.

Latest evidence: the terminal-only debug probe exported 55 supported Notion tools
and ran one configured-model read with six action schemas plus recovery, zero
discovery calls, one execution and no duplicates. Confirmed outcome at 9.28 s,
answer at 17.10 s from backend preparation start; no matched baseline speedup claim.
The 48-case offline experiment uses production ranking and schema packing.
After vocabulary/fuzzy-scoring/exact-name corrections, K=6 complete-set coverage
is 22/24 tuning and 19/24 former holdout; K=8 is 23/24 and 20/24. **Below 95%.**
Inspected cases are now regression evidence; use fresh cases for adoption.
Multi-step and mixed-language misses remain. The live read preceded these later
offline corrections. No embeddings, live writes, browser or installer. See the
[progress record](MCP-EXTENSION-PROGRESS.md) and terminal commands in the harness README.

First-round lexical publication and combined recovery now exist in source.
The existing ranker is reused without dense scores. Catalogue definitions are
task-owned, preparation runs up to four independent providers concurrently with
a shared three-second I/O deadline (SDK cleanup may take up to its separate
two-second bound), and recovery can retry unavailable metadata through normal
discovery. Successful snapshots are reused for ranking/schema loading only;
execution still re-discovers and checks current identity/digest. No idle cache,
new background worker, embedding dependency or transport-lifetime rewrite.

Small known eligible catalogues (up to eight actions, all definitions fitting)
are offered eagerly; otherwise six lexical candidates are selected and budget
packing skips definitions that do not fit. Recovery accepts up to four queries
and optional exact extension scope, merging intent candidates and publishing
up to eight additional schemas. Already-offered tools remain stable. The old
loader is no longer offered or dispatched in the live Agent; remaining pure core
loader fixtures are not compatibility support and can retire when obsolete
harness procedures are adapted.

Backend diagnostics now correlate preparation, offered IDs/count/bytes,
model durations and provider outcomes with a monotonic task clock. This is
**backend tool-turn timing**, not yet speech-end/submission-to-visible-receipt
measurement; frontend/STT clocks in Block 1 and receipts in Block 4 remain open.
Required-set evaluation is added separately from existing acceptable-alternative
metrics. The existing frozen benchmark is supporting evidence, not the planned
48-request real catalogue holdout or a latency adoption gate.

Implementation review covered canonical/name-map consistency, schema-budget
packing, deadline cleanup, account/catalogue freshness, query bounds and
non-replay of unknown writes. A replaced loader changed the expected fixture
tool count from four to three; continuation checks were updated accordingly.
No production adoption claim, new installer, model switch or live writes.

Verification at this source boundary: 92 focused/core/protocol/old-benchmark tests
Pass, plus normal backend Clippy correctness checks (existing warnings). The
user reiterated no browser control. The attempted dev browser-IPC probe ended
before an Agent/provider request, and its owned app was closed. Block 3 now uses
a thin debug one-shot probe in the existing headless setup: MCP catalogue
export or configured Agent request, bounded request file/output, alpha identity
check, no browser, audio stack, native worker listener or testing service. This
checks MCP execution; native real-app acceptance remains separate. Keep the probe
inside the existing testing documentation and retire disposable driver files.

## Goal and boundary

Reduce the interval from a submitted request to the requested external action
being completed and acknowledged, without losing correct tool, argument or target
selection. Finding tools quickly is a supporting metric, not the product outcome.

Normal path:

`request → host prepares usable tools → first LLM request → validated action → provider outcome → receipt`

Recovery path:

`missing capability → search_tools searches and publishes schemas → next LLM request → action`

Small eligible catalogues can expose their complete schemas directly within the
same budget. Larger catalogues use lexical preselection. A shortlist is a set of
choices for the LLM, never permission to execute its highest-ranked tool directly.
The model still determines operations, arguments, prerequisites and clarification.

Retain native and MCP tools under the shared capability boundary. Custom MCP UI
stays hidden in release/alpha; its existing identities and execution paths must
continue to work. No OAuth redesign, new provider-specific routing branches,
release build, marketplace changes, call-limit increase, vector database, routing
LLM, code sandbox, background service or new test engine.

## Source audit and adjustments to the supplied research

Source baseline: `1e3779a1`, branch `extensions/main-integration-20261010`.
The graph overview succeeded but the relevant capability search returned no nodes;
the following findings were therefore checked directly against source.

| Area | Existing implementation | Plan consequence |
| --- | --- | --- |
| First tools | `src-tauri/src/capability.rs::open` supplies `search_tools` and `load_extension`, no actions | Prepare and publish selected actions before the first request, through the same exposure map. |
| Retrieval | `crates/grain-core/src/capability_index.rs` already implements field-weighted BM25, fuzzy token handling and rank fusion | Reuse its lexical path. Audit provider-name promotion and candidate coverage before trusting it; do not build another ranker. |
| Evaluation | `capability_eval.rs::Case.expected` means acceptable alternatives; any one matching action can pass | Add a distinct complete-required-tool-set check. Do not reinterpret old alternative labels as prerequisites. |
| MCP projection | `capability.rs::mcp_action_input` leaves parameter metadata/examples/use guidance empty | Derive bounded parameter names/descriptions from actual schemas; reuse native author metadata. |
| Exposure | `publish_selected` enforces a 32 KiB task schema budget, canonical mappings and digests | Budget actual serialized callable definitions, including host tools. Do not truncate schemas or treat top-K as a guarantee. |
| MCP catalogue | `grain_mcp.rs::list_tools` opens/discovers/closes; search/load call it again | Reuse a request's snapshot for ranking/loading, with existing execution-time freshness checks. |
| MCP execution | `execute_on_service` discovers tools and compares the current digest before dispatch | Keep this check initially. Catalogue reuse for retrieval does not establish execution freshness. |
| SDK hints | Pinned `rmcp = 3.5.0` has cache-hint fields; Grain's discovery function returns only tools | SDK support alone does not make Grain cache-aware. Cross-request freshness needs a separate measured decision. |
| Connection lifetime | Shared HTTP pool exists; per-provider `Control` serializes operations | Do not rewrite transport or assume same-provider calls can overlap. |
| Tool loop | `agent.rs` offers a snapshot each round and dispatches sequentially | Preserve offered-name checks, cancellation, receipts and unknown-outcome retry protection. |
| Visible receipt | AgentPanel awaits `agentRun`; backend accumulates receipts | Earlier progress delivery is a separate later change, not evidence that routing improved execution. |

Observed starting evidence: three Notion tasks, 22 calls, 15 discovery calls,
7 successful provider executions, no exact within-task duplicate calls. Last
provider outcomes were about 39/53/46 seconds after the first model request;
answers followed at 43/58/50 seconds. These exclude STT and earlier preparation,
are coarse timestamps, and are not a controlled baseline or independently
verified persisted-object results. Query counts alone do not prove why searches
repeated. Existing details are in [progress](MCP-EXTENSION-PROGRESS.md).

Two further qualifications:

- A fresh MCP protocol can supply TTL/scope hints, but old servers or zero/absent
  hints do not justify fresh cross-request caching. Honour private account scope,
  minimum freshness across paginated results and invalidation. Our short-lived
  clients cannot hear notifications after closing; never assume they do.
- Notion can return an asynchronous task even for some synchronous updates.
  A successful MCP transport/result envelope is not proof the requested page
  change is finished. Record queued/running separately and wait for terminal
  evidence when required; do not claim faster completion from acceptance alone.

The oversized `notion-query-data-sources` admission failure remains a separate
compatibility issue. Keep rejected tools visible in the evaluation's unsupported
coverage report, not in callable candidates; do not silently raise limits here.

## Block 1 — establish clocks, catalogue projection and lexical evidence

1. Extend existing task-correlated diagnostics, using monotonic elapsed time:
   submission; transcript ready where available; catalogue ready; shortlist ready;
   each model request/response; dispatch; provider outcome; displayed receipt.
   Carry one request ID through backend/frontend; do not subtract unrelated
   process clocks. For voice distinguish end-of-speech from final transcript;
   report an unavailable earlier timestamp honestly rather than estimating it.
2. Record offered public tool IDs/count, schema bytes, selected IDs/ranking
   provenance, discovery versus action count, recovery count, catalogue fetch
   count/time, and terminal status. No credentials, raw user text, arguments or
   result bodies in ordinary diagnostics. Explicit disposable fixtures may keep
   sanitized request labels and owned object IDs for verification.
3. Obtain enabled GitHub/Notion catalogue snapshots through existing commands or
   acceptance IPC once, plus a small native fixture. Store timestamps, owner
   scope, catalogue digest and supported/rejected counts. Do not copy credentials
   into catalogue fixtures; review descriptions before committing public data.
4. Build bounded search documents from provider/tool names, title, description,
   parameter names/descriptions, enums where meaningful, and declared native
   aliases/examples/use guidance. Handle nested properties with depth/byte bounds;
   no arbitrary reference fetches, author text as instructions or generated docs.
5. Feed the existing `CapabilityIndex` lexical path without dense scores. Check
   how its exact-provider tier behaves when GitHub names 100 tools: naming a
   provider should bias provider choice, not erase distinctions among its tools.
   No active-screen dependency or new context collection. Use only the request
   and bounded context already legitimately available to the Agent.

Checkpoint: focused projection/ranking tests, supported catalogue coverage and
one small labelled retrieval comparison. No provider writes or live LLM needed.
Development batches share this checkpoint rather than testing every minor edit.

## Block 2 — first-round callable tools and one recovery operation

1. Change capability preparation to accept the submitted request and bounded
   recent conversation hints before `run_with_tools` makes its first LLM request.
   When catalogue I/O is required, preserve cancellation and deadlines and
   bounded preparation; an unavailable provider must not starve every other one.
   Fetch separate providers concurrently only through their existing gates.
2. Keep one task-owned catalogue/index snapshot. Reuse it for searches and schema
   selection; drop it at task end/cancellation. Do not hold MCP services or
   embedding resources. Keep discovery freshness separate from dispatch freshness.
   Cross-request TTL caching is not required for this first experiment.
3. If the whole eligible surface is small (initial heuristic: at most 8 actions
   and all definitions fit), publish it eagerly. Otherwise select approximately
   6 ranked actions, evaluating K=4/6/8 under the existing 32 KiB budget. Budget
   Non-eager initial publication is now capped at 24 KiB, leaving at least 8 KiB
   for recovery without eviction; recovery/eager publication can use the full
   32 KiB task budget. Packing must continue past a candidate that does not fit, not silently stop
   at its rank. Report missing required tools caused by byte limits explicitly.
4. Reuse `publish_selected`, canonical identity mapping and source/account/digest
   validation. Cross-provider publication must leave each provider's map/specs
   consistent. Already-published actions remain stable during a task; recovery
   adds within the total budget and reports saturation without hidden eviction.
5. Make `search_tools` accept a bounded query batch and optional extension scope,
   search the task catalogue and publish matching schemas together. Return loaded
   IDs, no-match/budget/availability feedback and no execution. New tools are
   usable on the following request only; a same-frame invented call still fails.
   Keep `load_extension` internal/test-only if needed, not in normal model tools.
   No legacy extension compatibility requirement.
6. Update discovery instructions: use already-offered tools directly; search when
   capabilities are missing. Never force search first. Follow-up provider hints
   are soft hints, not proof of object identity or permission. Multi-step requests
   need all necessary operations; do not apply a hard one-search-per-task limit.
7. Preserve auto-approval, schema/source/account checks, exact target resolution,
   bounded outcomes and unknown-write non-replay. A local rank never authorizes
   execution or fabricates missing parameters.

Checkpoint: lexical retrieval, exposure/alias mapping, first-frame availability,
recovery, stale account/catalogue, cancellation and unknown-write tests. Audit
the combined source diff once at this boundary, fix findings, then run Block 3.

## Block 3 — compare before adopting

### Offline, fast and repeatable

Start with 48 manually checked requests: 24 tuning and 24 untouched holdout.
Include explicit services, paraphrases, ASR-like typos, Hindi/mixed language,
follow-ups, read/write neighbours, multi-step and cross-provider tasks, unavailable
tools and ambiguous destinations. Label prerequisite sets and acceptable
alternatives distinctly (for example, all requirement groups must have a match).
Avoid labels that force a habitual read where the real provider does not need it.

Compare current metadata search, rich lexical K=4/6/8 and full-schema coverage
where that catalogue actually fits. A full catalogue that exceeds limits is
"not applicable", not silently a truncated all-tools control. Compute:

- Per-required-tool recall and complete-required-set hit rate after schema packing.
- Results by language/task class, irrelevant candidates, unsupported capabilities
  and reasons for misses; no-match/ambiguity correctness needs LLM validation too.
- Index construction, query latency and memory, including cold setup separately.
  Repeat cheap queries at least 100 times to report p50/p95; fresh-process runs
  measure application-cold, not an asserted OS disk-cache cold state.

Use ordinary `grain-core` tests/fixtures and a small terminal adapter if needed.
Production ranking is the tested implementation; no second Python BM25 copy.
Reuse existing evaluator where possible, adding complete-set semantics explicitly.
Retain useful regression fixtures and delete disposable exporters/adapters after
their evidence is recorded. Do not extend the harness into a benchmarking product.

### Small real-Agent comparison

Use the configured Grain model/provider (currently Gemini via 9router), same
settings, same tool catalogue and equivalent disposable starting objects.
Compare the current discovery path to the new lexical path; the current path is
a temporary test control, not a permanent compatibility branch. Add an eager
control only for a genuinely small eligible surface.

Start with six tasks, two runs per path (24 journeys): GitHub README read,
Notion find/read, private-page creation, page update with a known destination,
GitHub issue creation, and independent GitHub+Notion reads. Cover recovery and
native-tool checks with deterministic tests; add a live miss only if results
leave that behavior uncertain. Alternate paths/order to reduce provider-load
bias. Use fresh uniquely named objects for writes; never replay old user writes.
Check persisted titles/content/targets through an independent owned-fixture read.
For asynchronous results, require terminal status and requested effects.

Measure request→dispatch, request→confirmed action, request→visible receipt,
model/discovery rounds, duplicates and correct final effect. Report each task,
paired medians and range; 24 journeys cannot establish dependable live p95 or
production error rates. Historical user logs are context, not the control arm.

### Adoption gate

Proposed experiment gates, not prior results or production guarantees:

- Zero wrong targets/unauthorized calls, lost receipts or replayed unknown writes.
- No live task-success regression; every critical required-set fixture passes.
  Holdout complete-set hit rate at least 95% (23/24), with misses inspected by
  class rather than concealed in an overall average. Critical failures block
  adoption even if the average passes; widen the sample if it is inconclusive.
- Routine explicit requests have zero model discovery calls; deliberately missed
  tools recover through one combined operation when budget permits.
- Target at least 25% lower paired median request→confirmed-action time on the
  discovery-heavy tasks, fewer avoidable rounds and no material cold regression.
  Report small-sample uncertainty and provider jitter. If slower, diagnose rather
  than proclaim a winner; lexical query p95 over 50 ms triggers profiling before
  adopting a heavier dependency, not a universal hardware requirement.

If lexical-only passes, stop routing experimentation and adopt it. If it fails,
first inspect projection, provider-name flooding, context, K/bytes, model prompt
and gateway schema handling. Embeddings remain deferred for a separate decision
after these results; no automatic Model2Vec/EmbeddingGemma escalation.

## Block 4 — shorten acknowledgement and optimize measured remainder

After the routing comparison, make only justified follow-ups:

1. Deliver task/call-correlated receipts through existing Tauri events as outcomes
   arrive. Keep one ordered receipt per call, ignore stale task events, preserve
   them through final-answer errors and avoid duplicates in final rendering. This
   is a real AgentPanel change requiring user visual acceptance.
2. Say completed only when supported by terminal provider evidence. Queued/running
   work gets a pending message, not success. Initially use generic neutral receipts
   when no validated output contract supports a specific title/link. Do not parse
   arbitrary prose as a guaranteed "page created" result. Verify actual Notion
   async shapes/polling support before introducing any shared pending-outcome
   contract; keep interpretation in provider/package data or existing boundaries,
   not GitHub/Notion branches in the routing engine.
3. Revisit unnecessary preflight reads only where actual schemas/docs and exact
   user targets allow it. Notion private-page creation needs no parent search;
   adding under a named page does. Current prompt's blanket live target resolution
   needs a narrow review for new-object creation and explicit stable IDs, not a
   wholesale removal of ambiguity protection.
4. Profile remaining catalogue refresh/handshake costs. Consider account-scoped
   TTL-aware catalogue reuse only if meaningful, covering pagination, absent/zero
   hints, invalidation and bounded idle cleanup. Reuse HTTP pooling already present.
5. Consider concurrency first across distinct providers for independent reads if
   timing justifies it. Same-provider serialization and uncertain writes stay
   unchanged until separately proven. No dependency DAG or speculative writes.

Checkpoint: focused event/outcome tests, a short real dev-app receipt check and
visual acceptance. Measure execution and acknowledgement improvement separately.

## Delivery and evidence

Sequence: Blocks 1–2 as one coherent development batch, Block 3 as its comparison
checkpoint, then only justified Block 4 changes. Commit/push scoped checkpoints,
update [progress](MCP-EXTENSION-PROGRESS.md), and record passed/failed/deferred tests
and removed experiment code. Use dev builds/terminal IPC, no Computer Use or new
installer release for this experiment. A dev compile can still be required by
Rust changes; avoiding an installer does not mean no compilation.

Result report must contain catalogue digest/count/bytes, fixture revision,
model/provider settings, cold/warm definitions, paired outcome table, misses,
rounds/fetches, elapsed segments, memory and exact adoption decision. Keep real
credentials and account-bound cache contents out of committed evidence.

No new user testing required at planning time. Existing GitHub/Notion consent can
support owned disposable checks; ask only if account state changes or new consent
is needed. Provider token-expiry acceptance and broader extension-platform gates
remain separately tracked; this experiment does not close them.

## Primary references checked

- [MCP client best practices](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/docs/2026-07-28/develop/clients/client-best-practices.mdx): focused tool discovery, host memoization and cache considerations.
- [MCP cache-hint semantics](https://py.sdk.modelcontextprotocol.io/client/caching/): scope, freshness and conservative handling across protocol versions.
- [Anthropic tool search](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool): searches names, descriptions and argument metadata, supports BM25 and deferred schema loading. This is a reference, not a provider dependency.
- [LiveKit tool-loop design](https://docs.livekit.io/agents/logic/tools/design/): focused tools, parameter clarity and concise results.
- [Notion supported tools](https://developers.notion.com/guides/mcp/mcp-supported-tools): private-page defaults, synchronous/asynchronous outcomes and terminal polling requirements.
- [Notion client guidance](https://developers.notion.com/guides/mcp/build-mcp-client): access checks are optional before ordinary calls; tools enforce access themselves.

The pasted document's opaque citation IDs and illustrative accuracy percentages
are not independent evidence. Source-qualified recommendations above are adopted;
vendor model/performance numbers and suggested outcomes remain hypotheses.
