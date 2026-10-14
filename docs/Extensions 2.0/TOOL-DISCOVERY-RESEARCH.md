# Tool discovery: research and proposed correction

Research date: 14 October 2026. Recommendation only; no runtime change, provider
action, model switch, new harness or application build performed in this review.

**Execution decision:** the user supplied a deeper architecture review and then
explicitly chose to test without embeddings first. See the
[lexical-first execution plan](FAST-TOOL-EXECUTION-PLAN.md) for code reuse,
first-round tools, combined recovery, measurement and adoption gates. Semantic
candidates below are research context, not dependencies in this work order.

## Second pass: request-to-action latency (latest direction)

The user clarified that Grain's main product is executing requests accurately
and quickly, not matching coding-agent architectures. Optimize the full path
from submit/end of speech to the requested external change being confirmed.
Search duration, discovery-call count and time to final prose are supporting
metrics. This section supersedes the earlier recommendation to make a model
search call the default first step: the stronger candidate is **host-side tool
selection before the first model request**, with combined search/load as recovery.
No implementation or model download was performed in this research pass.

### What the current traces establish

| Workflow | First model round → last successful provider outcome | First model round → final answer |
| --- | --- | --- |
| List pages | ~39 s | ~43 s |
| Fetch/create pages | ~53 s | ~58 s |
| Search/fetch/update page | ~46 s | ~50 s |

These clocks start at the first logged model round, not at the actual spoken or
typed submission. They exclude transcription and earlier preparation; do not
report them as full end-to-end measurements. Roughly 4–5 s of each example occurs
after the last provider success. Most avoidable model discovery precedes the
action, so making search faster without removing those rounds misses the larger
opportunity. Unknown/wrongly targeted actions cannot count as fast successes.

### Closer reference systems and evidence quality

- [LiveKit's voice tool-loop guide](https://docs.livekit.io/agents/logic/tools/design/)
  focuses on small relevant toolsets, capability-oriented descriptions, precise
  parameters and concise useful results. It also covers unnecessary handoffs and
  testing the calls actually made. Its tool-count guidance is a heuristic, not a
  universal cutoff. These patterns address action latency, not coding work.
- [Composio search](https://docs.composio.dev/toolkits/meta-tools/search_tools)
  batches independent intent queries, returns several tools per query plus
  prerequisite/connection information, and supports continued session context.
  [Multi-execution](https://next.docs.composio.dev/toolkits/meta-tools/multi_execute_tool)
  targets independent operations in parallel. Adopt the batching/preparation
  principles, not a wholesale migration of Grain accounts and execution to a
  hosted service. Dependent reads/writes still require ordering.
- [Semantic Router](https://github.com/aurelio-labs/semantic-router) implements
  non-generative embedding-based route selection, including local encoder
  options. The relevant idea is shortlist selection without a generative model
  turn; generating arguments and completing multi-step tasks remains distinct.
  It is Python infrastructure, not a natural drop-in dependency for Grain's Rust
  executor. Source checked: `88fb4dcdc1044f060da82f0b52e829e085bc1407`.
- [Portkey's MCP filter](https://github.com/Portkey-AI/mcp-tool-filter) embeds tool
  metadata once and selects a shortlist from the request before LLM inference.
  Source separates embedding, similarity and cache timings. Its benchmark file
  labels tables as expected/preliminary results, not proof of reproducible
  end-to-end latency. A sub-10-ms headline does not guarantee a new query/model
  cold start on this PC. The small, older TypeScript project is architectural
  evidence, not a chosen production dependency. Checked source:
  `f53ebc13793268ba4ca1d549239f3fb59b403a63`.

The important shift: the host does retrieval preparation; the main LLM receives
usable tool schemas immediately instead of spending inference rounds searching.

### Embedding and decision-model candidates

| Candidate | Why test it | Cost/limit that must be measured |
| --- | --- | --- |
| BM25 over rich tool metadata | Cheap CPU baseline; preserves exact service/tool/parameter vocabulary | Weaker paraphrase/multilingual matching; quality depends on metadata |
| Model2Vec/Potion static embeddings + BM25 | Very small/fast semantic candidate; official Rust library | Less contextual understanding; English and multilingual models differ |
| Small transformer embedder + BM25 | Established local semantic retrieval through maintained Rust inference libraries | Query inference, model load, idle RAM and language coverage |
| EmbeddingGemma 2 text-only + BM25 | Current Google on-device, multilingual semantic candidate | Real Windows runtime/export support and CPU/cold-start cost are unverified |
| FunctionGemma or trained decision model | Could make recurring fixed commands local | Training/evaluation and changing community tool sets; not an embedding replacement |

[Model2Vec](https://github.com/MinishLab/model2vec) uses static embeddings distilled
from sentence transformers. Small English models and larger multilingual models
exist. [Its Rust implementation](https://github.com/MinishLab/model2vec-rs) can
load local weights without a model-serving sidecar. Vendor speed/size claims
must be tested on tool retrieval, especially read/write distinctions, negation
and context-dependent follow-ups. Checked Rust source:
`1567eaa5797f02d040ae28bf810d4065b2ecede6`; Python reference:
`611a1191a3f6f0d427b0518160e6da381dac5dca`.

[FastEmbed-rs](https://github.com/Anush008/fastembed-rs) is an Apache-2.0 Rust
inference-library candidate. Checked source
`473fff39e0b0dfcb89603040a3b1b0e7d4f0934e` lists MiniLM, BGE, multilingual E5 and
original EmbeddingGemma 300M/quantized variants. That is not confirmation of
EmbeddingGemma **2** support. Validate ORT dependency/runtime compatibility before
adding anything to Grain; do not ship a Python server just for embeddings.

[Google's 6 October announcement](https://blog.google/innovation-and-ai/technology/developers-tools/embeddinggemma-2/)
confirms EmbeddingGemma 2 exists: 740M parameters fully multimodal, modular 270M
text path, 768-dimensional output reducible to smaller vectors. Reported ~191 MB
active text-weight RAM uses quantization on Pixel 11 Pro; it is not total Windows
process memory or a query-latency result for Grain. Text tool discovery needs no
vision/audio encoders. Newness/multimodal benchmark improvements alone do not
establish better tool retrieval than a smaller text-only model.

[FunctionGemma guidance](https://ai.google.dev/gemma/docs/functiongemma) positions
it as a 270M function-calling base for customization on a defined action surface.
It is worth a later fixed-command experiment, but is not evidence that arbitrary
new MCP tools will work accurately without training. The user's unidentified
non-LLM decision model was illustrative, not a required technology choice.

Retrieval research adds an important accuracy check: [ToolRet](https://arxiv.org/abs/2503.01763)
tests retrieval models specifically on tool inventories and finds that general
retrieval ability does not automatically translate to reliable tool selection.
[Tool-REX/document expansion](https://arxiv.org/abs/2510.22670) studies enriched
tool descriptions. This supports indexing purpose, constraints and parameter
semantics rather than relying on names or buying the latest embedder. It does
not justify generating descriptions with an extra LLM on every user request.

### Proposed fast path and fallback

`submitted request + bounded conversation context → host shortlist → LLM chooses
validated call/arguments → existing executor → provider outcome → user receipt`

Prepare native/MCP catalogue metadata when the extension becomes available,
outside repeated model discovery rounds. Rebuild index entries when definitions
or ownership change. Precompute document vectors on catalogue revision; embed
only the current bounded query at request time. Preserve exact tool identities.

Use lexical/exact matching plus optional dense/static retrieval. Start by
comparing fusion of their candidate sets; do not assume cosine scores are
calibrated confidence or use similarity as authorization. Include all operations
needed for the request, not just the nearest single verb. Keep a recoverable
combined search/load tool so missing candidates remain discoverable.
Update the current instruction that always tells the model to search first:
already-offered tools should be callable immediately, with discovery used for
missing capabilities. Keep the existing canonical-ID/exposure map and schema
validation rather than adding an alternate dispatch path.

For hundreds/a few thousand tools, a normalized-vector scan may be sufficient:
1,000 × 256 float32 vectors is about 1 MB of vector data, excluding metadata and
container overhead. Query embedding/model weights often dominate cost, not this
scan. No vector database/server is justified without measured scale needs.
Keep optional model residency scoped to active Agent use; measure cold-start
after release as well as warm requests. Do not reload weights on every tool call.

### Other parts of action latency Grain controls

1. **Catalogue reuse:** avoid repeated tools/list discovery for model searches.
   Current MCP HTTP client already shares a pool; do not claim every operation
   makes a new TCP connection. Short-lived SDK services and execution-time digest
   revalidation remain separate correctness boundaries. Measure their cost before
   proposing scoped active-task service reuse; no permanent idle sessions.
2. **Model input:** selected schemas, clear parameter formats and useful bounded
   results reduce selection errors/retries and prefill work. Preserve schema
   meaning; arbitrary stripping/renaming parameters previously caused gateway bugs.
   Keep provider/model selection unchanged for comparisons.
3. **Independent operations:** current Agent batch dispatch iterates sequentially.
   Overlap genuinely independent reads/preparation only when provider serialization
   and cancellation allow it. `search → fetch target → update` is dependent; fewer
   calls achieved by guessing the target is not a valid optimization.
4. **Immediate outcome:** report a confirmed provider result as soon as received,
   rather than waiting for the final LLM prose. This reduces confirmation delay,
   not the time the remote action takes. Do not announce the whole task complete
   when later requested steps remain.
5. **Speech preparation:** [LiveKit preemptive generation](https://docs.livekit.io/agents/logic/turns/tuning/)
   overlaps work with turn completion, with speculative-cost trade-offs. Grain can
   investigate harmless catalogue/retrieval preparation while speech completes,
   but has no demonstrated streaming-partial-transcript path here. Never execute
   speculative writes before the actual request is submitted. This is a later
   overlap opportunity, not the first discovery fix.

### Bounded experiment before choosing a library/model

Use actual enabled GitHub/Notion/native schemas and a small labeled request set.
Compare (A) host BM25 shortlist, (B) BM25 + small/static embeddings, (C) BM25 +
EmbeddingGemma 2 text-only if a supported local backend is available. Include
direct schema exposure as a small-catalogue control; retain combined discovery
as fallback. Do not stack all candidate models/rerankers into the product.

Retrieval-only terminal runs need no provider writes or full application build.
Use straightforward existing test boundaries/a disposable script; record and
retire temporary experiment support. Cover paraphrases, mixed language, explicit
app names, follow-ups, multi-operation requests, near-identical read/write tools,
missing tools and catalogue changes. Hold out requests not used to tune metadata.

Measure required-tool-set Recall@K under a schema-byte budget, actual argument and
target correctness, extra recovery calls, p50/p95 warm/cold retrieval, peak/idle
RAM and model loading. Then use a small real dev-app read/create/update checkpoint
on disposable objects. One sample cannot establish production p95 or zero error.

Add consistent request timestamps for submit, transcription ready, catalogue ready,
model request/response, provider dispatch/outcome and first displayed receipt.
Record completion latency and failure/unknown outcomes separately; faster wrong
actions must not improve the success metric. Use the same configured LLM and
comparable tasks. Choose the least costly candidate that retains measured quality
and reduces submit-to-confirmed-action latency. There is no measured model winner
or honest seconds-to-completion promise from this research alone.

Second-pass source snapshots are ignored evidence under
`tests/agent-harness/.runs/fast-tool-routing-research-20261014/`. No model weights,
new runtime dependency, hosted service signup or experimental engine was added.

## Finding

Grain's current discovery path adds avoidable model round trips. Three completed
user Notion workflows made 22 calls: 15 discovery calls and 7 successful provider
executions. Each performed 3–5 metadata searches, then one explicit schema load.
There were no identical tool/argument repeats. Approximate model request/response
intervals account for 141 of 151 elapsed seconds; dispatch durations total 11.2 s.
These are a small observed sample, not a performance benchmark.

I did not find a published average or universal maximum for tool-discovery calls
per task in the reviewed sources. Architecture permits zero discovery calls with
eager schemas, or one successful search with automatic schema loading. Neither is
a guarantee that every real task needs zero/one search. Complex requests, failed
retrieval and new providers can need more. Percentage of calls alone is misleading:
one discovery plus one execution is 50%, but only one additional inference round.

## What the reference projects do

Checked released versions as well as pinned current source, because OpenCode's
development architecture differs from its release. Release references resolved to:

| Project | Released version | Commit |
| --- | --- | --- |
| Codex | `rust-v0.162.1` | `092d3acd6bec3e3a14bdc7e7a2810ab628ab759d` |
| Goose | `v1.54.0` | `cd643d2126fffab4c6a6186905211ba868d6a657` |
| OpenCode | `v1.19.0` | `733b2c627e5aec39cb33cc22373bdce33053ef2c` |

### Codex: search also loads

The [released search handler](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/core/src/tools/handlers/tool_search.rs)
uses BM25 ranking and returns loadable definitions. Its search entry point exposes
matching tools on the next model call; the model does not perform a separate
metadata-search-then-load handshake. The source also caches its search handler
while inventory/search information is unchanged. Different direct/Code Mode and
model capability paths exist; this is not a claim that every Codex configuration
has identical exposure.

In [current metadata indexing](https://github.com/openai/codex/blob/76ed3b484d62ccceb9fa512ff4dd139f421814c3/codex-rs/tools/src/tool_search.rs),
searchable text includes names, names with underscores separated, descriptions
and schema property names/descriptions. [Current default limit](https://github.com/openai/codex/blob/76ed3b484d62ccceb9fa512ff4dd139f421814c3/codex-rs/tools/src/tool_discovery.rs)
is eight results. That is eight returned candidates, not eight discovery calls.
BM25 is ordinary lexical relevance scoring, not another LLM or hosted embedding
request. Current Codex source inspected: `76ed3b484d62ccceb9fa512ff4dd139f421814c3`.

### Goose: direct exposure remains a valid choice

[Released Goose guidance](https://github.com/aaif-goose/goose/blob/cd643d2126fffab4c6a6186905211ba868d6a657/documentation/docs/guides/managing-tools/code-mode.md)
distinguishes traditional eager exposure from Code Mode. Direct tools have no
model-side discovery prerequisite. Code Mode instead offers function listing,
signature inspection and TypeScript execution, allowing multiple provider calls
and intermediate data processing in one script. Its guidance associates the
direct path with simpler/smaller workflows and Code Mode with larger inventories
and multi-step work; extension count is guidance, not a schema/token guarantee.

The [Code Mode extension is opt-in](https://github.com/aaif-goose/goose/blob/cd643d2126fffab4c6a6186905211ba868d6a657/documentation/docs/mcp/code-mode-mcp.md).
Current [extension manager](https://github.com/aaif-goose/goose/blob/9686d674000a5fbb8139214863289e4820714771/crates/goose/src/agents/extension_manager/mod.rs)
caches public tool metadata with scope/version checks. This is evidence for
inventory reuse, not permission to retain stale authorization. Current source:
`9686d674000a5fbb8139214863289e4820714771`.

### OpenCode: distinguish release from development

[Released tool resolution](https://github.com/anomalyco/opencode/blob/733b2c627e5aec39cb33cc22373bdce33053ef2c/packages/opencode/src/session/tools.ts)
adds connected MCP tools directly when experimental Code Mode is off. Thus that
path needs zero model discovery calls, while paying input-context cost.
[Runtime flags](https://github.com/anomalyco/opencode/blob/733b2c627e5aec39cb33cc22373bdce33053ef2c/packages/opencode/src/effect/runtime-flags.ts)
default experimental features off unless enabled. [Code Mode execution](https://github.com/anomalyco/opencode/blob/733b2c627e5aec39cb33cc22373bdce33053ef2c/packages/opencode/src/tool/code-mode.ts)
can instead orchestrate multiple MCP calls through a confined interpreter.

The pinned development branch now has a [compact catalogue](https://github.com/anomalyco/opencode/blob/b474b842595c958229f358255f9f75c2448d94e0/packages/core/src/codemode/catalog.ts)
with a 2,000 estimated-token inline budget, namespace coverage and short tool
signatures. Its [instructions](https://github.com/anomalyco/opencode/blob/b474b842595c958229f358255f9f75c2448d94e0/packages/core/src/codemode/instructions.ts)
allow synchronous search inside an execution script. That can avoid a separate
model inference for each lookup. [Registry snapshots](https://github.com/anomalyco/opencode/blob/b474b842595c958229f358255f9f75c2448d94e0/packages/core/src/tool.ts)
reuse rendered inventory by revision and visible tool set. Do not describe these
development details as the defaults of every installed release. Current source:
`b474b842595c958229f358255f9f75c2448d94e0`.

### Provider documentation supports the same trade-off

[OpenAI tool search](https://developers.openai.com/api/docs/guides/tools-tool-search)
combines discovery/loading, recommends clear namespace descriptions and keeps
loaded definitions available in subsequent turns. It documents eager loading
for small/frequent tool sets and deferred loading for larger ones. Its Responses
API tool-search feature requires supported OpenAI models; it is not a drop-in
feature for Grain's configured Gemini model through 9router. We can adopt the
host-side pattern using ordinary function calls without changing that route.

[Anthropic tool-search documentation](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool)
describes searches whose returned references automatically expand into definitions,
usually loading a small relevant set. It recommends leaving frequent tools eager
and indexing parameter names/descriptions as well as tool names. Its
[engineering guidance](https://www.anthropic.com/engineering/advanced-tool-use)
explicitly acknowledges added discovery latency and reduced benefit for small,
compact inventories. The reported token savings are not a percentage reduction
in tool calls or an average search count.

[Cloudflare Code Mode](https://blog.cloudflare.com/code-mode/) illustrates the
broader approach of generating a programming interface and executing composed
tool operations. This is useful context, not a reason to add another interpreter
to Grain immediately.

## Specific differences in Grain

1. `search_tools` returns metadata only and instructs the model to call
   `load_extension`. Even ideal retrieval costs two separate decisions before
   execution. A single successful search could instead publish those schemas.
2. `search_metadata` scores the count of matching substrings in ID/title/description.
   Common words can rank many unrelated tools equally; schema parameter metadata
   is absent from this search representation. BM25 gives common/rare terms and
   differing descriptions more useful weights. This is a source-level weakness,
   not proof of the exact reason for every observed search.
3. Every search/load re-fetches the MCP catalogue through a new short-lived SDK
   service. Model inference dominates latency, but repeated metadata/network work
   is still unnecessary when an unchanged task-local snapshot is sufficient.
4. Every fresh `agent_run` creates an empty capability session, including follow-up
   messages. Tool schemas already selected in a conversation are rediscovered.
   Reuse requires explicit conversation lifetime and revision/account checks;
   a process-global cache is inappropriate.
5. The initial directory describes extensions, not their individual operations.
   A bounded tool-name/signature summary or eager compact tools can remove searches
   without publishing every full schema in a large catalogue.

These differences are host discovery responsibilities. Correcting them should
not require every community MCP/native extension to change its package or SDK.
Existing traces omit queries, offsets and hit counts: necessary pagination and
semantically repeated searches cannot yet be distinguished. Zero identical-call
repeats does not establish that all discovery was useful.

## Recommended bounded implementation

First checkpoint: revise the current path, keeping provider-neutral execution.

1. Make one discovery call rank and atomically publish a small useful set of
   matching schemas. Reuse existing alias/exposure/schema validation. Remove the
   mandatory separate load from model guidance/surface; keep pure loading helpers
   where useful. Metadata discovery never executes a provider action.
2. Reuse catalogue metadata within that task and use a maintained lexical ranking
   implementation, with normalized names and bounded schema-derived metadata.
   Keep the current execution-time account/source/digest checks and SDK lifecycle.
3. Add only focused diagnostic fields: catalogue revision, returned tool IDs,
   match count, offset, selected-schema bytes and catalogue reuse. Avoid raw user
   requests/arguments/credentials. Do not create a tracing dashboard or new harness.

Second checkpoint, only if the first leaves a measurable gap: compare bounded
eager exposure for genuinely small inventories and retaining selected definitions
for the active conversation. Select by schema bytes/token cost and correctness,
not the number of extensions: one GitHub extension can expose 100 tools. Invalidate
on catalogue/package/account/source changes and destroy state on conversation close.

Code Mode is a possible later optimization for complex workflows. It adds interpreter,
cancellation, permission and result-handling work that is not needed to remove
Grain's separate search/load round. Do not adopt it or a vector database just to
correct this small discovery path. No further ceiling increase is proposed.

## Comparison and exit criteria

Use normal retrieval/publication tests and a small real-dev-app checkpoint on the
configured `ag/gemini-3.8-flash`. No installer needed. Reuse existing disposable
fixtures/workspaces; do not replay the user's completed creations/updates.

- Check ranking of representative read/create/update vocabulary; all requested
  operations must remain reachable. Cover native tools and both MCP providers.
- Check exact alias identity, schema-byte bounds and stale/disabled/account-change
  refusal after catalogue reuse. Preserve uncertain-effect retry protection.
- Compare the same small set of requests before/after. Measure successful completion,
  discovery calls/model rounds, selected tools, schema/input tokens, latency and
  unnecessary write repetitions. Log failures separately; do not hide retries in averages.
- Initial engineering target: one cold discovery call for the basic single-provider
  workflows, with a second allowed for recovery/new needs. Warm selected-tool reuse
  can aim for zero. These are targets, not industry averages or hard execution caps.
- If one search serves each of the three observed workflows, discovery could fall
  from 15 calls to 3 while retaining the same 7 provider calls. That is a structural
  illustration, not a promised result or latency estimate. Validate selection quality
  and context cost before claiming the improvement.

The oversized `notion-query-data-sources` schema is a separate compatibility item.
It is not repaired by a better search index, and its admission cap should not be
silently expanded as part of this change.

Source snapshots are ignored research evidence under
`tests/agent-harness/.runs/tool-discovery-research-20261014/`; they are not product
dependencies or maintained test code. Earlier user traces are in
`tests/agent-harness/.runs/notion-review-20261014/analysis.json`.
