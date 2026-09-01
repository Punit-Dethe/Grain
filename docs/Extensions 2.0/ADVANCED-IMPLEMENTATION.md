# Extensions 2.0 — implementation audit and Advanced architecture

Status: **Initial hardening in progress; Advanced implementation is gated.**

This document applies Amendments A–C at the top of `PLAN.md`. Where the older
body discusses a launcher, Command shell, or extension-authored app UI, the
amendments win: Grain has Dictation and Agent only; interactions are host-owned,
text/markdown now and Dynamic UI later.

## 1. Current implementation against the effective plan

| Area | State | What exists | Remaining production gate |
|---|---|---|---|
| Capability Index V2 | Implemented | Static action projection, bounded hot set, `search_actions`, stable exposure map, benchmark corpus | Measure real installed-set RAM/latency and auth-aware eligibility |
| Generic Agent tools | Implemented | Retrieved actions become provider-safe tool definitions; undisclosed tools cannot resolve | Provider capability metadata must replace the current known-provider gate |
| Action preparation | Hardened | Rust validates exact schemas, owns risk, mints an opaque token, and binds approval digest + expiry | Add a versioned, independent `sideEffect` declaration and policy migration |
| Confirmation | Hardened | Exact prepared call is withheld and resumed without reconsulting the model | Real-app UX approval; test cancellation, close, expiry, and rapid resummon |
| Third-party execution | Implemented | Lazy worker invocation receives only action id, validated arguments, and idempotency key | Durable host dedupe ledger before any automatic write retry; resumable follow-up contract |
| Result handling | Hardened | Strict envelope, bounded/sanitised text, host-owned provenance and receipts | Add structured evidence/result size telemetry |
| Authentication | Partially implemented | One optional service declaration and one host-vaulted account per extension; public-client OAuth authorization-code + PKCE, loopback callback, refresh, exact-host token attachment | Provider adapters and action/source auth requirements |
| Composition | Not implemented | One Agent may make bounded sequential tool calls | Initial real-app validation first; Advanced Levels 2–3 below |
| Sources | Contract seed only | Read actions and knowledge-source concepts | First-class `Sources` schema and evidence execution path |
| Agent memory | Not implemented as an Agent subsystem | Conversation plus Grain Space notes/recall exist | Separate bounded working memory and routing/workflow memory |
| Dynamic UI | Parallel workstream | Semantic interaction structs and markdown renderer | Host Dynamic UI renderer; no extension-authored standalone UI |

The code audit fixed these concrete security/correctness defects:

1. The V2 index admitted action declarations without matching their stored
   approval fingerprint.
2. Execution used a placeholder manifest digest, so time-of-use revalidation
   did not protect third-party calls.
3. Model-authored arguments were not enforced against the manifest schema.
4. Worker results could spoof their source, suppress write receipts, return
   ambiguous shapes, and send unbounded/raw exception text back to the model.
5. Several tool calls in one model turn could execute after the first withheld
   confirmation; a global “latest token” could approve the wrong session's call.
6. Canonical ids/tool names could collide across publishers or after sanitising.
7. A known text-only provider could silently answer while Agent tools were
   advertised but unavailable.
8. Write calls appeared retry-safe merely because a key existed, despite the
   host having no durable deduplication ledger.

## 2. Authentication answer and required architecture

The current subsystem is a secure foundation, not universal application auth.
It works unchanged only when a service supports a native/public client using
authorization-code PKCE with a compatible loopback redirect and bearer tokens.
Secrets remain host-owned and are never delivered to extension JavaScript.

Provider implications:

- **GitHub:** not end-to-end with the generic flow today. GitHub's web
  authorization-code token exchange requires a client secret; Grain correctly
  refuses extension-supplied client secrets. Add a GitHub device-flow adapter or
  a Grain Cloud/GitHub App broker.
- **Microsoft Graph / Teams:** compatible in principle with a public desktop
  client and loopback redirect, but the registered redirect URI must match the
  provider rules. Add a tested Microsoft adapter instead of assuming every
  random loopback path is accepted.
- **Skype:** authentication alone is insufficient. The extension also needs an
  API that supports the intended Skype operation/account type. Where the
  operation is exposed through Microsoft Graph, it can use the Microsoft
  adapter; unsupported consumer Skype operations cannot be created by OAuth.

The production auth contract should be a host-owned adapter enum, never custom
extension OAuth code:

```text
PublicPkceLoopback | DeviceCode | ManagedBroker | ApiToken
```

Each extension represents one service and has at most one account. Cross-service
composition is exclusively an Agent concern: a GitHub extension cannot bundle a
Teams or Calendar connection, and extension code never selects an account id.

Add these fields before Sources ship:

- a boolean `requiresAuth` on each Action and Source;
- provider adapter + allowed issuer/token hosts in a reviewed host registry;
- scopes and account identity in the approval fingerprint;
- auth eligibility evaluated before ranking and again before execution;
- explicit reconnect/unavailable diagnostics, never a callable unauthenticated tool.

Confidential client secrets and GitHub App private keys belong in managed Grain
infrastructure, not extension packages or the desktop binary.

## 3. Advanced governing architecture

Extensions remain **eyes and hands**. They expose Sources and Actions. Grain's
Agent remains the brain: intent, planning, source selection, cross-extension
data flow, memory, confirmation, and synthesis.

### 3.1 First-class Sources

Sources are not “safe actions.” They have a retrieval-specific contract:

```text
SourceDecl
  id, title, description
  querySchema
  requiresAuth, capabilities[]
  freshnessPolicy, costClass, privacyClass
  maxItems, timeoutMs

EvidenceEnvelope
  sourceId, extensionId
  retrievedAt, freshness
  items[] { id, title, excerpt, canonicalRef, occurredAt? }
  citations[], confidence?
  sensitivity, provenance
```

The host validates and bounds the envelope. Source output is untrusted evidence,
not instructions. Extensions receive only the generated query and explicitly
approved context; they never receive the full conversation by default and never
learn which other extensions participate.

### 3.2 Progressive execution controller

- **Level 1 — Direct:** retrieve/invoke one named or high-confidence Source or
  Action. This remains the low-cost default and should cover most requests.
- **Level 2 — Bounded investigation:** at most three known Sources run in
  parallel under one deadline and cancellation scope. Each returns a compact
  EvidenceEnvelope; one Agent synthesis step consumes the merged evidence.
- **Level 3 — Open investigation:** a host retriever scores eligible Sources from
  context, memory, project state, exact naming, and bounded historical signals.
  Search the top set, run an evidence-sufficiency check, and expand once only if
  evidence is thin.

Hard limits are part of the contract: maximum sources, items, bytes, model hops,
wall time, and spend. Cancellation drops futures, worker requests, temporary
evidence, and any session-scoped auth handles.

Workers use the cheapest configured **tool-capable** model. A stronger model is
an escalation for difficult final synthesis, never the default routing layer.
Model selection is capability-based (tools, context, modality, local/cloud,
latency and cost), not a hard-coded vendor name.

### 3.3 Memory is five logical tiers over four physical stores

1. **Session scratch:** the current turn, unresolved goals, exact tool results,
   and prepared-action state. Hard byte/turn bounds; destroyed at terminal state.
2. **Recent episodic:** a bounded 48-hour record of source use, outcomes, and
   project/entity context. This is a decaying routing feature, not factual truth.
3. **Core profile projection:** stable preferences plus current themes projected
   from user-approved durable knowledge. It is rebuildable, inspectable, and not
   a second source of truth.
4. **Durable user knowledge:** explicit Grain Space notes and user-approved
   saved material. Existing note recall remains this layer.
5. **Routing/workflow aggregates:** compact host-owned success, decline,
   misroute, source-use, and reusable-plan statistics. Store ids and aggregates,
   not raw transcripts by default.

The physical stores are session scratch, bounded episodic, durable Grain Space,
and routing aggregates; the profile is a projection. Do not turn Grain Space
into an indiscriminate Agent transcript/vector store. User knowledge and system
optimisation have different consent, retention, export, deletion, and poisoning
risks.

### 3.4 Agent-first truth and target resolution

Agent-first means retrieval helps the Agent reach verified truth or a safe action
quickly. It does **not** mean every query must begin with an LLM, or that the
highest retrieval score becomes authoritative. Cheap host retrieval may prefetch
candidates; the Agent decides whether evidence is sufficient, searches again,
or asks the user.

Authority is separate from relevance:

| Evidence | What it may decide |
|---|---|
| Live provider read with canonical id and observation time | Current state of a mutable external object |
| Exact user-authored Grain Space note | What the user recorded; possibly stale externally |
| Extracted fact/profile projection | Personalisation and hypotheses; never an external write target by itself |
| Recent/routing memory | Search order and whether to investigate sequentially or in bounded parallel |

Similarity, graph distance, recency, and routing affinity affect candidate order
only. Exact naming, permissions, account scope, live provider identity, and an
explicit user choice outrank every learned score. A note's `savedAt` is capture
time, not event time or proof that its claim is current.

For “add this to the GitHub issue we have”:

1. Memory may propose repositories, issue numbers, or previous workflows.
2. The Agent loads GitHub and performs a live read/search in the eligible account.
3. The Source returns canonical refs such as provider/account/repository/issue,
   plus `observedAt` and a provider revision or `updatedAt` when available.
4. Exactly one target may be prepared. Zero matches or several plausible matches
   produce one concise clarification; newest-memory wins is forbidden.
5. Confirmation names the canonical target and binds the prepared arguments. The
   host revalidates provider revision when the provider supports conditional
   writes; a changed target requires a fresh read and confirmation.

Conflicting memories remain a visible evidence set. Grain may collapse them only
when one explicitly supersedes the other or live evidence resolves the conflict.
Unresolved disagreement is reported, never hidden by rank fusion.

### 3.5 Retrieval weighting audit

Grain's current Recall stack is a sound candidate pipeline, not yet a calibrated
truth pipeline: independent lexical, optional local-BGE, and entity-graph legs;
RRF to 20 candidates; then deterministic semantic/overlap/recency reranking to
six. BGE should be the recommended quality setting, but it remains optional:
lexical + graph retrieval must stay correct and useful without a 130 MB model.

Do not change the current `0.35 RRF / 0.25 semantic / 0.25 overlap / 0.15 recency`
blend by intuition. The graph's direct-vs-neighbour strength, capture time versus
event time, conflict/version state, and source authority are currently missing
signals; min-max semantic normalisation is also pool-relative. First add an eval
corpus and log per-leg ranks/features, then tune against Recall@K, MRR, conflict
accuracy, unsupported-fact rate, and ambiguous-write rate. Authority and evidence
sufficiency remain post-retrieval gates, never additional relevance weights.

### 3.6 Competitor audit: the Agent boundary

Audit baseline: Supermemory `143024fa34f3648b93d5667ab78b79de6b22c62b`
and Mem0 `71fba8d46436f88569d600f81a55208c38ad30b5` (2026-09-01).

- **Supermemory is not uniformly agent-first.** Its public integrations offer
  both automatic profile/query injection (and automatic conversation capture)
  and explicit search/add tools. Its useful advantage is the memory ontology:
  atomic facts, static/dynamic profiles, and `updates`/`extends`/`derives`
  evolution, plus continuously synced document connectors. The public server
  repository does not expose enough of the hosted retrieval engine to verify its
  exact scoring or conflict algorithm.
- **Mem0 is also integration-dependent.** It supports prompt injection, MCP, and
  explicit Agent tools. Current OSS v3 is ADD-only with dense candidates,
  keyword/entity boosts, optional reranking, and scoped memory ids; its hosted
  temporal/graph behavior is not an open write-target resolver.
- **Neither memory product solves Grain's GitHub example by memory retrieval
  alone.** A synced/indexed GitHub record may improve candidate freshness, but a
  safe update still requires a live provider identity read, ambiguity handling,
  and a write bound to one canonical target.

Grain should adopt fact evolution, profiles, event time, and raw-evidence links;
it should not copy unconditional prompt injection or background capture. Grain's
differentiator is one Agent that separates memory, live Sources, and Actions and
can prove which one authorized each conclusion or effect.

### 3.7 Cross-extension policy

Moving evidence from Source A into Action B is a data transfer. Before preparing
the action, the host records the source/destination, checks sensitivity and
destination policy, minimises fields, and includes the transfer in confirmation
when material. Neither extension may authorize the other.

### 3.8 Cloud boundary

Local remains capable: local Agent/model where available, local embeddings,
local memory, BYOK APIs, and local workers. Grain Cloud may add managed model
tiers, confidential auth brokers, cross-device continuity, hosted Grain Space,
and long-running orchestration. Cloud use is explicit, observable, revocable,
and subject to the same evidence/action contracts; it is not a second extension
runtime with looser policy.

## 4. Implementation sequence and gates

Advanced work must not start by adding sub-agents to the current loop. Complete
the Initial gates first:

### A0 — Initial release gate

- reference extensions exercise auth, direct read, confirmed write, timeout,
  cancellation, uninstall/update race, and strict result handling;
- real Tauri app visual approval of confirmation/result text surfaces;
- provider tool capability matrix and auth-aware eligibility;
- versioned host-owned `sideEffect` metadata so live reads and writes are
  distinguishable before execution;
- Agent truth policy: memory cannot authorize mutable external facts or targets;
  ambiguous live targets never execute;
- security tests for forged tools, stale/cross-session confirmation, prompt
  injection, oversized arguments/results, worker impersonation, and replay;
- RAM/latency measurements with a realistic installed catalogue.

### A1 — Sources contract and one-source path

Add versioned SDK structs, manifest validation/fingerprints, registry/index
projection, auth/capability eligibility, a strict EvidenceEnvelope, and one
reference Source. `EvidenceEnvelope` items must include authority class,
`observedAt`, canonical ref, and optional provider revision/event time. Level 1
must work end to end before concurrency.

### A2 — Working memory and provenance

Add a bounded session-owned evidence registry referenced by ids in model
context. Implement destruction, redaction, source citations, and cross-extension
transfer policy. Preserve conflict sets instead of silently picking by recency.
Do not add durable learning yet.

### A3 — Level 2 bounded workers

Add structured concurrency around the existing Agent turn: maximum three source
tasks, shared deadline/cancellation, compact evidence only, then one synthesis.
No free-running background agents and no persistent worker engine.

### A4 — Level 3 source retrieval

Build/evaluate the Source retriever separately from Action retrieval. Add source
Recall@K, evidence sufficiency, one progressive expansion, and hard cost limits.
Choose sequential investigation when the top source has high calibrated
confidence and margin; use bounded parallel retrieval when confidence/margin is
low. Memory features may change this order, never source eligibility or truth.

### A5 — Routing and workflow memory

Consume opt-in success/decline/misroute signals as bounded features. Add
poisoning resistance, decay, inspect/reset controls, and replayable workflow
plans whose every action is still revalidated and confirmed.

### A6 — Model tiers and Grain Cloud

Introduce capability-based model routing, strong-model escalation, managed auth
brokers, and optional continuity only after local Levels 1–3 pass the same
evaluation suite.

## 5. Required evaluation

Release gates are measured on realistic ambiguous workflows, not exact command
phrases:

- direct Action and Source Recall@K / false exclusion;
- evidence sufficiency and citation/provenance completeness;
- successful-task rate and unsupported-success/unsupported-fact rate;
- confirmation precision, decline/misroute rate, and cross-session replay rate;
- p50/p95 latency, model calls, tokens/cost, peak RAM, idle RAM, workers left alive;
- auth repair success and permission/auth bypass attempts;
- cancellation and ambiguous-write outcomes.
- contradiction resolution, stale-memory/live-source disagreements, and exact
  canonical-target accuracy;

The Advanced architecture is ready to implement only when A0 is green. The next
code change after A0 should be `SourceDecl` + `EvidenceEnvelope`, not a general
sub-agent engine or a rewrite of Grain Space memory.
