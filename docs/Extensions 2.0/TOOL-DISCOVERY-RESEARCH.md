# Tool discovery: research and proposed correction

Research date: 14 October 2026. Recommendation only; no runtime change, provider
action, model switch, new harness or application build performed in this review.

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
