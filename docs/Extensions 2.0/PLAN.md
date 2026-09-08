# Grain Extensions 2.0 — Agent, Launcher, and Dynamic UI Plan

**Status:** Approved architecture and implementation plan

**Date:** 2026-09-08

**Scope:** Extension discovery, Agent actions, execution, and host-owned presentation

**Implementation state:** Active. Amendment D's two-level directory/loader runtime is implemented behind the existing Agent and action-execution contracts. Amendment E's visual-ownership cleanup is implemented on `ui/grain-2.0`; real-application visual approval and end-to-end reference-extension validation remain.

This plan replaces the product architecture in `docs/Extensions V1/PLAN.md`. V1 code and documentation remain useful as a migration baseline, evaluation harness, and record of implemented security controls. They must not be extended into a second permanent system.

The supplied design documents and prior research are inputs, not normative specifications. This document resolves their contradictions against the current Grain codebase and the latest product direction.

---

## Amendment E (2026-09-08) — Extensions expose capabilities, never visuals

**Decision.** Extension-authored visuals are removed from the current public contract. Extensions may expose actions, authentication, prompt/data contributions, shortcuts, and Grain-rendered declarative settings. They may return bounded data or text. They may not supply HTML, CSS, component trees, themes, skins, custom settings panels, workspace/overlay windows, or replacement layouts.

The two existing appearance choices remain Grain features:

- Pill `Wave` and `Matrix` are native `PillSkin` settings.
- Agent `Side card` and `Center panel` are native `agent_panel_position` settings.

The Center panel no longer depends on the retired `grain.agent-center-layout` pack or `agent.reply-surface` slot. The prewarmed Extension Mode window remains host-owned and may render only routing, choice, progress, and finite text results; it cannot interpret extension-provided views.

**Enforcement.** Pack, developer, and trusted-catalogue validation all reject retired visual capabilities, visual slots/variants, `overrides:*`, surface declarations, pill-theme payloads, and nested panel settings. Registry publishing validates before deriving signed metadata. Worker and frontend bridges for authored surfaces are removed, including the main-window relay that previously let a panel invoke host APIs as an extension identity.

Old visual fields remain deserializable only as compatibility tombstones so unsupported manifests fail with a specific error. They are not exported as the author SDK or rendered by Grain. Registry healing deletes stale visual slot occupancy and the retired center-layout record.

This amendment supersedes any later section that describes current extension-authored rich views, workspace/overlay web surfaces, custom panels, visual slots, or extension-defined pill/Agent appearance. Future host-rendered Dynamic UI, if built, consumes Grain-owned semantic interaction data; it does not restore author control of pixels. See `VISUAL-OWNERSHIP.md` for the implementation boundary.

---

## Amendment A (2026-08-28) — Dynamic UI is decoupled; the Agent chat is the interim surface

**Decision.** Dynamic UI is removed from the critical path. Building the Dynamic UI runtime (Phases 5 and 7) as a prerequisite would stall the whole feature — it is a months-scale effort on its own. Instead we build the **entire functional stack now** — Grain Space, retrieval, the Agent, the extension→Agent connection, the recommendation algorithm, **and real extension execution** — and use the **existing Agent chat panel as the interim interaction surface**. Anything an action needs — confirmation, a follow-up field, a choice, a review, a result, a receipt — is rendered as **host-emitted markdown (and simple affordances) inside the Agent chat**. This makes the full stack end-to-end testable for accuracy immediately. Real Dynamic UI moves to a **parallel, non-blocking R&D track**.

**The keystone that makes this clean — one contract, two renderers.** The semantic interaction protocol of §9.2 (`ConfirmAction`, `ChooseOne`, `ChooseMany`, `RequestText`, `RequestFields`, `TextResult`, `StructuredResult`, `Success`, `Error`, `ExecutionReceipt`, `Progress`) is the **host-owned contract**. It gets **two renderers**:

1. **Markdown-in-chat renderer** — build now. Interactions are rendered as structured markdown in the Agent reply, with host-owned affordances for the side-effecting ones.
2. **Dynamic UI rich renderer** — the parallel track (Phases 5/7). Swaps in later **behind the same contract**.

Producers (the Agent, a built-in provider, an extension) emit interaction **requests**; they never render. So Dynamic UI arriving changes the *renderer*, not the *producer contract* — a refinement, never an overhaul. This is exactly the ownership §9.1 already demands ("the producer can request state transitions; it cannot create windows").

**Invariants preserved (non-negotiable).** The interim surface being markdown does **not** relax any security requirement in §12, and confirmation remains a **Rust/host policy decision** (§2.5). Concretely: a risky prepared call is **not executed** when its tool call arrives; the host withholds it, emits a `ConfirmAction` interaction (rendered as a markdown confirmation with an explicit host affordance), and executes **only** after the host — not the model — receives the user's confirmation through that affordance. Extend the existing `AgentReply.confirm_delete` pattern rather than inventing a parallel path. A model writing "confirmed" in prose is never proof of confirmation.

**Re-sequencing.**

- **Phases 5 and 7 (Dynamic UI runtime + rich views): parallel track.** They no longer gate Phases 3, 4, or 6.
- **New near-term critical path:**
  1. **Interaction contract + markdown-in-chat renderer** — the interim Dynamic UI. The enabling layer for everything below.
  2. **Phase 3 — third-party execution** on top of it: prepared calls, host risk policy, host-gated confirmation via the markdown surface, worker execution, receipts, cancellation, unknown-outcome handling.
  3. **Phase 4 — multi-action composition**, using the same surface for per-step progress/results.
  4. **Registry unification** (the Phase 2 tail): Grain Space and extensions execute through one generic provider interface.
- **Phase 6 (Command shell)** UI also leans on the interim surface until Dynamic UI lands.

**Amends §2.4.** "Agent interactions and extension action interactions use the same Grain-owned Dynamic UI session runtime" now reads: they use the same **interaction contract**; the **runtime behind it is markdown-in-chat now, Dynamic UI later**.

All frontend work stays on `ui/grain-2.0` per the UI protocol; the interim markdown surface is Agent-panel chat rendering, not a new window or a browser harness.

---

## Amendment B (2026-08-28) — No third shortcut, no launcher, no extension-authored app UI. Only Dictation + Agent. (Host-rendered Agent Dynamic UI is NOT cancelled — see the clarification below.)

**Decision.** Grain has exactly **two** invocation concepts: **Dictation** (speech → text) and the **Agent** (natural language → everything else). There is **no third shortcut**, **no Command shell**, **no Launcher**, **no extension-search overlay**, and **no extension-authored rich UI / standalone "mini-app"**. Extensions are reached **only conversationally, through the Agent**. Every extension interaction — a result, a confirmation, a follow-up, a choice, a review — is rendered as **markdown in the Agent chat** (Amendment A).

**Why.** A third shortcut that pops an overlay to search and open extensions — each with its own dynamic UI — turns Grain into "many applications inside Grain," a launcher of mini-apps. That is Raycast. Grain is a speech-first tool; competing with Raycast in the launcher space is the wrong fight and dilutes the product. The Agent is the one door to extension capability, and it is conversational, not a menu.

**This cancels (supersedes the referenced sections):**

- **§1 "Command":** replaced by **"Agent."** The second concept is the Agent, not a Launcher+Agent shell.
- **§2.1 "launcher-visible":** removed. Every non-dictation capability is **Agent-enabled or nothing**; the launcher/Agent contribution split collapses to one. `Standalone` now means "contributes Agent actions" (not "opens a view independently"); `Extend` is unchanged.
- **§2.6 Shortcut contract:** no Command shortcut and no Extension-Mode→Command migration. Retire the Extension Mode binding; **add no third default binding.** Dictation and Agent are the only defaults (both already exist).
- **§4 Target architecture:** drop the `Command shortcut → Command shell → Launcher projection` path. The Agent session is reached from the Agent shortcut; the Capability Index feeds **only** the Agent retriever + `search_actions` (its launcher projection, §6.2 `LauncherEntryRecord`, is cancelled — and was never built).
- **§9.3 Rich-view layer, §9.5 launcher shell behavior, §10 Launcher (whole section):** cancelled.
- **Phase 6 (Command shell + launcher) and Phase 7 (rich public views):** cancelled.

**This does NOT cancel Dynamic UI (clarified 2026-08-28).** The line is *who authors the view*:

- **KEPT — host-rendered Agent Dynamic UI (the second renderer; parallel R&D track).** The Agent rendering structured results from one or many extensions as coherent, native-looking cards that compose, merge, and carry follow-ups and confirmations — while staying legible ("this is GitHub, this is Teams, this is the summary I asked for"). Extensions supply **structure and data**; the host renders (§9.1). Composing *unrelated* extensions' outputs into one seamless surface is far harder than showing a single GitHub issue list — which is exactly why it develops in parallel and does not block the deterministic stack. The interaction contract therefore keeps **two renderers**: markdown-in-chat (build now) and host-rendered native Dynamic UI (parallel, Phase 5).
- **CANCELLED — extension-authored standalone app UI.** A single extension shipping its *own whole-app view* (a Spotify app, a GitHub app with its own issue-creation screens) opened as a mini-app. That is the §9.3 rich-view / Phase 7 extension-view-SDK direction, and the Raycast mini-app model this amendment rejects.

The rule: the **host may render richly; an extension may not author its own app UI.** Everything an extension contributes to the UI flows through the host-owned interaction contract as structure + data — rendered as markdown now, native cards later.

**Unaffected.** The recommendation algorithm, Agent retrieval (hot set + `search_actions`), extension→Agent connection, Grain Space, and Phase 3/4 execution all stand — they were always the Agent path. Amendment B removes the launcher/overlay/rich-UI branch that Grain will not build.

---

## Amendment C (2026-08-29) — Initial vs Advanced version. Cloud, agent intelligence, and sub-agents.

**Versioning.** Everything specified above and built so far is the **Initial version**: one Agent, one retrieval pass, direct action execution. It ships and gets proven first. This amendment records the **Advanced version** — additive intelligence on top. Nothing here changes the Initial version's contracts; do not start it until the Initial version is validated in real use.

### C.1 The extension intelligence boundary (the governing rule)

**Extension developers implement domain access. Grain implements intelligence.**

An extension exposes only what its service *can provide* and *can do* — never workflow understanding, and never awareness that other extensions exist:

```text
Slack     Sources: search_messages, get_thread, find_user
          Actions: send_message, reply
GitHub    Sources: search_issues, get_issue, get_comments
          Actions: create_issue, update_issue
```

Grain's Agent layer owns: intent, screen/app/selection context, memory and working state, which sources are relevant, query generation, retrieval + rerank, cross-service connection, action planning, multi-extension composition, freshness checks, confirmation policy, and learning reusable workflows.

```text
Extensions = eyes + hands      Grain Agent = brain      Grain Memory = continuity
```

The payoff: independently authored extensions participate in workflows their developers never coordinated on. "Didn't Sarah mention this yesterday? Check if we already have an issue, and if not create one from what she said" becomes context → memory → Slack search → GitHub search → compare → propose → confirm → GitHub action, with neither developer aware of the other.

**Additive change this implies:** `Sources` becomes a first-class contribution type alongside `Actions` — a read that returns evidence, distinct from an act that changes the world. §6.2's `KnowledgeSourceRecord` and §8.6 are the existing hooks; the Advanced version promotes them to a real retrieval + execution path. The Initial version's `Action` + `SideEffect::Read` covers the simple case until then.

### C.2 Progressive agent execution

Not every request deserves a large workflow. Escalate only as ambiguity demands:

- **Level 1 — Direct.** The user names the source or action ("Check Slack for what Raj said") → invoke it directly. This must handle **most** requests, cheaply. *(This is what the Initial version does today.)*
- **Level 2 — Bounded investigation.** Several known sources ("Compare yesterday's meeting with the GitHub issue") → small parallel workers, each returning **compact evidence, not prose**, then one synthesis.
- **Level 3 — Open investigation.** Genuinely ambiguous ("Didn't we already discuss this?") → use context + memory + project state + historical source usage to **score likely sources** (Slack 0.91, GitHub 0.84, Meetings 0.72, Gmail 0.19), search only the likely ones, and expand progressively only if evidence is thin.

Level 3's source scoring is the Capability Index applied one level up — the same retrieval discipline (bounded candidate set, rank, expand on miss) already built and hardened for actions.

### C.3 Model strategy

Optimise **time to successful action**, not benchmark intelligence:

```text
Dictation cleanup  → cheap Flash, non-thinking
Normal Agent       → fast Flash
Source/sub-agents  → same Flash or cheaper
Open investigation → several Flash workers in parallel
Rare hard synthesis→ escalate to a stronger/Pro model
```

Most operations stay inexpensive because most requests are one source, a small retrieved context, a short tool call, and little generated output. Large multi-source investigations must remain **exceptional**, not the default shape.

### C.4 Cloud's role

The Agent architecture works with local **and** managed models. Grain Cloud is managed compute + continuity, **never a requirement for intelligence**:

```text
Grain Cloud    managed Agent inference · managed embeddings · optional stronger-model
               escalation · sync · cross-device memory · hosted Grain Space
Local/open     local LLM · local embeddings · local memory · BYOK APIs · local extensions
```

### C.5 Sequencing

Advanced work begins only after the Initial version is proven end to end. Likely order when it does: (1) `Sources` as a first-class contribution, (2) Level 2 bounded parallel workers with compact-evidence returns, (3) source scoring for Level 3, (4) model-tier routing, (5) Cloud managed inference/continuity, (6) learned reusable workflows.

---

## Amendment D (2026-08-30) — Two-level extension discovery; one sequential Agent

**Decision.** Replace action ranking, hot-set exposure, and `search_actions` with a two-level progressive-disclosure contract:

1. **Extension directory.** Every Agent request receives a compact, deterministic directory of enabled extensions that currently have approved Agent actions. Each entry contains only the stable extension id, display name, one concise capability description, and action count. It is inert metadata: producing it never starts a worker or resolves credentials.
2. **Extension load.** The Agent has one host meta-tool, `load_extension(extension_id)`. A successful call atomically exposes every approved action schema for that extension on the next model hop. Loaded extensions accumulate for the current request, so one Agent can work across GitHub, Slack, Calendar, and other providers without a second routing pass.

Core host tools, such as Grain Space, may remain directly exposed because they are part of Grain rather than an installed extension. They still obey the same schema validation and execution policy.

**Why this supersedes retrieval.** At the intended initial scale (roughly 25 enabled extensions), the capable Agent already has enough information to choose a provider from names and concise capability descriptions. Preselecting action schemas can hide a provider needed later in a multi-extension task, while a second semantic/BM25 router adds latency, model/storage cost, failure modes, and evaluation work. Grain should not predict a subset before the Agent has reasoned about the request.

**Single-Agent execution.** The Initial version uses one Agent and a bounded sequential tool loop. The Agent may load and call several extensions, but calls execute one at a time in provider order. Multi-agent delegation and parallel execution remain deferred. Later parallelism belongs behind the executor boundary and must not change discovery, permissions, confirmation, or receipts.

### D.1 Task-scoped loader contract

- `load_extension` accepts one exact id from the directory; names, aliases, invented ids, disabled extensions, and unapproved manifests do not resolve.
- Loading is idempotent. It rechecks current extension availability and authentication state each time, then publishes schemas atomically or publishes none.
- Provider-facing action names remain deterministic, collision-checked, and bound to an authoritative task-local name→canonical-action map. A hash/prefix is an address, never authorization.
- An action may execute only if its schema was offered at the start of that model round. A model cannot call `load_extension` and smuggle an undisclosed action into the same response.
- The directory, schema descriptions, action titles, and tool results are untrusted prompt data: sanitized, size-bounded, and explicitly framed as data rather than instructions.
- Loaded state exists only for one backend Agent request, persists across that request's model hops, and is destroyed at its terminal answer or confirmation hand-off. A confirmation resumes the exact prepared call without reconsulting the model, so it does not need a live tool registry.
- A process interruption does not replay tool calls. The user retries the request; side-effecting calls retain the existing idempotency/unknown-outcome rules.

### D.2 Hard bounds for the Initial version

- At most 100 extension directory entries are placed in model context. Reaching this scale triggers evaluation of an additive `search_extensions` level; it does not revive action ranking.
- At most 16 extensions and 128 extension action schemas may be loaded in one request.
- One extension may expose no more than the manifest contract's existing 24-action ceiling.
- The existing Agent wall-time and tool-hop budgets remain mandatory; the hop budget must accommodate load→call sequences across several extensions without becoming unbounded.

### D.3 Security and lifecycle

The existing Rust execution boundary remains authoritative. Loading a schema does not activate extension code and grants no permission. Before execution the host still validates arguments, action approval digest, enabled state, identity, realm/token, risk floor, confirmation, expiry, idempotency, output limits, and worker provenance. Credentials remain opaque host-owned state and there is exactly one account per extension.

Extension disable/update/uninstall invalidates the index and causes later loads or calls to fail closed. Task-local state owns no worker, listener, credential, or background service and therefore needs no explicit long-lived cleanup path beyond dropping the request state.

### D.4 Implementation sequence

1. Add a pure extension-directory projection and task-local loaded-extension registry in `grain-core`, with deterministic ordering, sanitization, collision rejection, strict `load_extension` argument parsing, and hard bounds.
2. Replace the live Agent hot-set/`search_actions` wiring with the directory plus loader while retaining the Capability Index only as an approved-action metadata store during this migration.
3. Rebuild the provider tool list after every load. Reject action calls absent from the model round's offered-tool snapshot.
4. Route loaded actions through the existing `prepare`/risk/confirmation/worker executor unchanged.
5. Add tests for multiple sequential extension loads, idempotent load, unknown/disabled ids, collisions, limits, prompt sanitization, same-round smuggling, schema validation, confirmation, and state drop.
6. After end-to-end validation, remove runtime retrieval code and retain the old benchmark only as historical evidence or a future >100-extension comparison harness.

**Superseded sections.** Amendment D replaces §2.2's action-retrieval decision, §4's hot-set/`search_actions` path, §6's ranking role, §7.1–§7.5, Phase 1 as a shipping prerequisite, Phase 2's hot-set/search work, and §17's instruction to start with a retrieval benchmark. Their security, manifest-normalization, stable-name, measurement, and code-free-discovery requirements remain in force where applicable.

---

## 1. Outcome

Grain will have two primary invocation concepts:

1. **Dictation** — capture speech and insert text.
2. **Command** — open one Grain-owned command shell for everything else.

The Command shell combines two access paths without merging their behavior:

- **Launcher:** deterministic, manual discovery and opening of extension commands or views.
- **Agent:** natural-language action selection, execution, composition, follow-up, and results.

There is no permanent third “Extension Mode” and no third default shortcut. The existing Extension Mode shortcut becomes the Command shortcut after a compatibility migration. Per-extension shortcuts remain optional.

Extensions contribute capabilities to one or both access paths. Grain owns discovery, policy, lifecycle, execution mediation, and all Agent interaction surfaces.

---

## 2. Decisions frozen by this plan

### 2.1 Product kinds

V1 uses `searchable`, `standalone`, and `extending` as mutually exclusive kinds. That model is retired.

The active product kinds are:

- **Extend:** augments an existing Grain workflow or surface.
- **Standalone:** supplies commands, actions, or a view that can be opened independently.

`UI` is reserved as a possible future top-level kind, but is not a V2 launch kind. UI is currently a contribution/surface, not an extension identity.

Agent discoverability and launcher visibility are orthogonal contributions:

| Extension | Kind | Agent actions | Launcher commands/views |
|---|---|---:|---:|
| GitHub | Standalone | Yes | Optional |
| Spotify | Standalone | Yes | Yes |
| Color Picker | Standalone | No | Yes |
| Text post-processor | Extend | Optional | Optional |

Avoid “searchable” and “non-searchable” in new contracts: launcher entries are searchable too. Use **Agent-enabled** and **launcher-visible**.

The existing “extension shapes” documentation describes Settings placement shapes. Those shapes are not product kinds and should be renamed when that documentation is revised.

### 2.2 Discovery model

The Agent never receives the full action catalog. Every request receives:

- the user request and conversation context;
- a small, bounded directory of enabled Agent-capable extensions;
- a locally retrieved hot set of likely action schemas;
- an always-available `search_actions` meta-tool; and
- a small set of truly core Grain tools.

The hot set is a latency optimization, not an allowlist. If it misses, the Agent authors a focused `search_actions` query and can narrow by extension or namespace. Only then is an extra model round trip required.

### 2.3 Execution model

The Agent selects exact actions. An extension no longer receives the entire transcript and independently guesses which command was intended.

Grain validates the selected action, arguments, permissions, authentication state, current context, and risk immediately before execution. Manifest metadata never activates extension code.

### 2.4 UI model

Agent interactions and extension action interactions use the same Grain-owned Dynamic UI session runtime. Dynamic UI has two levels:

1. **Semantic interactions** for confirmations, choices, follow-up fields, progress, results, errors, and receipts.
2. **Strict rich views** composed from an allowlisted Grain component tree.

Extensions control structure and data. Grain controls rendering, layout rules, styling, accessibility, focus, animation, and security boundaries. Arbitrary HTML/CSS is not part of the new public Agent/Dynamic UI contract.

### 2.5 Risk model

Confirmation is a Rust policy decision. It is not controlled by prompt wording, a model decision, or an extension’s UI definition.

The V1 “auto-send” concept does not carry into Agent actions. A safe action may execute immediately; a risky action requires a host confirmation. Per-action and global policy may become stricter, never weaker than the host-classified risk.

### 2.6 Shortcut contract

The Command default inherits the existing Extension Mode chord: **Alt+Shift+Enter on Windows/Linux** and **Option+Shift+Enter on macOS**. The binding remains editable in normal Settings. Migration renames the action and preserves each user’s customized chord; it must not silently reset bindings or register a duplicate global shortcut. The current Dictation binding is unchanged.

---

## 3. Non-goals

- A third default shortcut or separate permanent Extension Mode.
- A second Agent dedicated only to extensions.
- A prompt containing every extension instruction or action schema.
- Settling QuickJS versus V8 versus Wasm before Agent actions can ship.
- Arbitrary extension HTML/CSS inside Agent confirmation or result surfaces.
- Long-lived workers for discovery, ranking, or metadata inspection.
- A hidden classifier deciding whether typed text means launcher search or an Agent request.
- Production implementation before the contracts and benchmarks in Phases 0 and 1 pass.

---

## 4. Target architecture

```mermaid
flowchart LR
    U["Dictation shortcut"] --> D["Dictation pipeline"]
    C["Command shortcut"] --> S["Grain Command shell"]
    S --> L["Launcher projection"]
    S --> A["Agent session"]
    M["Installed manifests"] --> I["Static Capability Index"]
    I --> L
    I --> R["Local action retriever"]
    I --> Q["search_actions"]
    R --> H["Bounded hot set"]
    H --> A
    Q --> A
    A --> X["Rust action coordinator"]
    X --> P["Permission, auth, schema, risk gates"]
    P --> E["Lazy extension executor"]
    E --> X
    X --> V["Dynamic UI session runtime"]
    A --> V
    V --> S
```

The Capability Index is the shared metadata plane. Agent retrieval, launcher search, Settings, and diagnostics use different projections of it. They do not create separate catalogs.

The extension host is the execution plane. It is activated only after Grain has an exact action or an explicit view-open request.

---

## 5. Current implementation audit

### 5.1 Reuse

| Existing component | Decision |
|---|---|
| `extension_host.rs` worker supervisor, per-extension identity/token, lazy connection, idle reaper | Keep as the first execution adapter. Do not wake it during discovery. |
| `host_api.rs` capability and privileged-operation broker | Keep and extend. It remains the security boundary. |
| Static extension index in `extension_host.rs` | Evolve into Capability Index V2. |
| `grain_llm_client.rs` tool specifications, tool calls, and conversation entries | Generalize for deferred extension tools and explicit provider capability checks. |
| Grain Space note tools | Migrate behind the generic action registry as the first built-in provider. |
| `ExtensionView` schema, budgets, validators, and stable event IDs | Reuse as the strict rich-view layer. |
| `extension_view.rs` cleanup and renderer window | Generalize into a producer-neutral Dynamic UI session manager. |
| Grain-owned workspace lifecycle and sleeping-window policy | Reuse for explicit rich-view entrypoints where permitted. |
| V1 recommendation evaluator and fixtures | Convert into action-retrieval and end-to-end routing evaluation infrastructure. |

### 5.2 Migrate or retire

| Existing component | Problem | Target |
|---|---|---|
| `ExtensionKind::Searchable` | Conflates identity and access path | `Extend` / `Standalone` plus Agent and launcher contributions |
| `recommend`, `needs`, and full-transcript hand-off | Routes to one extension, then asks it to interpret intent | Exact Agent action selection and typed arguments |
| `grain_action_session.rs` | Finite single-extension state machine | Generic Agent action coordinator plus Dynamic UI sessions |
| `ExtensionModeAction` and separate shortcut | Creates a third mental mode | Compatibility alias that migrates to Command |
| Recommendation pill chooser | Makes extension selection the primary decision | Transitional debugging UI only; remove after Agent parity |
| V1 auto-send settings | Attached to the old hand-off model | Host risk policy and optional stricter user policy |
| Extension-specific confirmation/result window | Tied to Extension Mode and one producer | Producer-neutral Dynamic UI session |
| Permanent privileged Grain Space tool path | Prevents one coherent action model | Built-in capability provider in the same registry |

### 5.3 Keep separate until deliberately migrated

The currently supported workspace/overlay web surfaces are an existing restricted runtime feature. They must not be presented as the new Dynamic UI contract or expanded opportunistically. A later security and compatibility ADR must decide whether to retain, further isolate, or replace them. New Agent surfaces use semantic interactions or allowlisted rich views.

---

## 6. Capability Index V2

### 6.1 Principles

- Built entirely from installed, validated manifests and host-owned state.
- Does not import, parse, or execute extension source code.
- Rebuilt incrementally on install, update, enable/disable, permission, auth, and locale changes.
- Stores normalized strings once and shares them across projections.
- Excludes disabled or incompatible extensions before retrieval.
- Records why an action is ineligible for diagnostics without exposing secrets to the model.

### 6.2 Conceptual records

Names below describe the contract. Phase 0 must settle serialization and backward-compatible field names before SDK edits.

```text
ExtensionDirectoryEntry
  extension_id
  display_name
  one_line_summary
  namespaces/domains
  enabled + platform compatibility

AgentActionRecord
  canonical_id                 # github.create_issue
  provider_extension_id
  title
  description
  when_to_use
  when_not_to_use?             # high-value negative boundary only
  aliases[]
  example_requests[]           # representative, bounded, diverse
  tags/domains[]
  input_schema                 # strict, bounded JSON Schema subset
  output_summary
  side_effect_class
  host_risk_floor
  required_capabilities[]
  auth_requirements[]
  context_requirements[]
  execution_entrypoint
  scoped_selection_guidance?   # bounded, retrieved only with action

LauncherEntryRecord
  canonical_id
  extension_id
  title + subtitle
  keywords[]
  icon reference
  open behavior                # action, semantic flow, or rich view
  required context/capabilities

KnowledgeSourceRecord
  canonical_id
  extension_id
  title + bounded description
  source type and freshness
  required capabilities/auth
  retrieval entrypoint or static index reference
```

### 6.3 Prompt safety

Manifest strings are untrusted data even when signed or installed intentionally.

- Enforce per-field and aggregate byte/token limits.
- Reject or normalize control characters, bidi overrides, invalid Unicode, and deceptive identifiers.
- Serialize directory and search results as clearly delimited data.
- Never place extension text above Grain’s system policy.
- Treat scoped selection guidance as advisory and action-local. It cannot redefine tools, authorize capabilities, suppress confirmation, or instruct the Agent to ignore policy.
- Treat tool results as untrusted data, not instructions.
- Record the manifest digest used for every invocation.

### 6.4 Stable tool names

The canonical action ID remains human-readable (`github.create_issue`). Provider-facing tool names use a deterministic safe encoding such as `ext__github__create_issue` and are mapped by Rust. Execution accepts only names exposed in the active Agent session and still performs an exact registry lookup.

---

## 7. Agent discovery and retrieval

### 7.1 Eligibility before ranking

Hard filters run before lexical or semantic scoring:

- extension enabled and compatible with the current platform;
- action declared and manifest version supported;
- required account/auth state available when known;
- required host capabilities granted or requestable;
- action context valid for the active app/document/selection;
- enterprise or user policy permits discovery;
- extension/action not quarantined.

An ineligible action must not enter the hot set. `search_actions` may return a concise unavailable match only when that helps the user repair auth or permissions; it must be marked non-callable.

### 7.2 Initial hot set

The first production baseline is a schema-aware, field-weighted lexical retriever with:

- exact canonical ID, provider name, title, alias, and namespace boosts;
- separate weights for description, examples, parameter names/descriptions, and tags;
- ASR-aware normalization without destructive stemming of identifiers;
- negative-boundary penalties only where explicitly declared;
- diversity across extensions and action intents;
- a small bounded result count set by benchmark, not intuition.

Existing embeddings may be evaluated as a second-stage or hybrid signal. Do not assume dense retrieval is better. Compare lexical, dense, and reciprocal-rank-fusion variants on the same frozen corpus.

### 7.3 `search_actions`

`search_actions` is always present and has a compact schema:

```text
query: required focused natural-language or capability query
extension: optional exact extension/namespace filter
limit: optional small bounded integer
```

It returns a bounded set of action definitions plus stable IDs and eligibility state. It does not run an action and does not start a worker.

When the Agent calls it:

1. Rust searches the static index.
2. Results are appended as a tool result.
3. The next model request exposes the newly retrieved action schemas.
4. The cumulative exposed set remains bounded; least-relevant unused schemas can be evicted.
5. The Agent may call an exposed action or refine search within a strict hop budget.

The executor rejects invented or undisclosed action names with a structured “discovery required” result. Discovery is not authorization; every execution check still follows.

### 7.4 Extension directory

The directory gives the Agent coarse routing knowledge without schemas: enabled extension name, one-line summary, and a few namespaces/domains. It is bounded by tokens and entries. Large installations use directory search rather than silent truncation.

### 7.5 Evaluation

The primary offline metric is **eligible action Recall@K**, including false exclusion. Ranking position and downstream selection accuracy are secondary. The suite must include:

- exact names and aliases;
- conversational paraphrases;
- ASR errors and missing punctuation;
- near-neighbor actions within one extension;
- near-neighbor actions across extensions;
- explicit extension names with ambiguous verbs;
- long requests containing incidental terms;
- requests requiring two or more actions;
- requests where no action should be called;
- unavailable actions caused by auth, platform, permission, or context;
- adversarial manifest text and malicious tool results.

Required comparisons:

- full-catalog oracle versus hot set;
- lexical versus dense versus hybrid;
- hot-set-only versus hot-set-plus-`search_actions`;
- one extension, 5 extensions, 30 extensions, and at least 100 actions;
- provider/model combinations Grain actually supports.

Do not set production thresholds until the frozen corpus and baseline results are checked in.

---

## 8. Agent tool loop

### 8.1 Provider capability gate

Current providers may silently operate without native tool calls. That behavior is unacceptable when an action is required.

At session start Grain must know whether the selected provider supports the required tool protocol. If not, Grain must either use a separately validated strict structured-action adapter or explain that extension actions are unavailable and offer a supported provider. It must never treat prose such as “done” as proof an action executed.

### 8.2 Generic registry

One Rust registry exposes built-in and extension actions through the same interface:

```text
describe(action_id, context) -> eligible definition or reason
prepare(action_id, arguments, context) -> validated prepared call
execute(prepared_call, cancellation) -> structured outcome
```

Built-in providers may execute in process. Third-party providers execute through the extension host. Both use the same schema validation, risk classification, receipts, cancellation, and result contract.

### 8.3 Prepared calls

Before confirmation Grain creates a prepared call containing:

- canonical action and extension IDs;
- normalized, schema-valid arguments;
- manifest and permission-policy digests;
- risk and side-effect classification;
- an idempotency key when the action supports it;
- relevant context snapshot identifiers, not unbounded raw context;
- expiry and revalidation rules.

Confirmation approves this exact prepared call. Rust rechecks auth, permission, manifest digest, context, and expiry immediately before execution. Material changes require a new confirmation.

### 8.4 Multi-action composition

- Use a bounded tool-hop and wall-time budget.
- Start with sequential execution. Parallel execution requires explicit purity/independence metadata and a later ADR.
- Feed structured, bounded results back to the Agent.
- Preserve provenance when output from one action becomes input to another.
- Ask for consent before crossing extension trust boundaries when sensitive data is involved.
- Show completed, failed, skipped, and unknown steps separately.
- Never retry a side-effecting action after an ambiguous timeout unless an idempotency contract makes the outcome safe.
- Support cancellation between actions and propagate cancellation to the active executor.

### 8.5 Result contract

Every execution returns one of:

- `succeeded` with bounded structured data and receipt;
- `failed` with stable error class and safe user message;
- `cancelled`;
- `unknown_outcome` for ambiguous side-effecting timeouts;
- `needs_interaction` with a semantic Dynamic UI request.

Raw extension exceptions and secrets are never sent directly to the model or UI.

### 8.6 Knowledge-to-action

Knowledge sources and actions share extension identity and policy infrastructure, but remain separate contribution types. Static knowledge metadata may be searched without activating code; dynamic or authenticated reads are explicit read actions. Retrieved knowledge carries source, freshness, extension, and authorization provenance. If the Agent turns knowledge from one provider into an action in another, the cross-extension data-flow policy applies before transfer.

---

## 9. Dynamic UI

### 9.1 Ownership

A Dynamic UI session is Grain-owned. Its producer may be the Agent, a built-in action, or an extension action. The producer can request state transitions; it cannot create windows, choose arbitrary CSS, capture global input, or bypass focus and cleanup policy.

### 9.2 Semantic interaction layer

The initial version should be an enum, not arbitrary component composition:

- `Progress`
- `ConfirmAction`
- `ChooseOne`
- `ChooseMany`
- `RequestText`
- `RequestFields`
- `TextResult`
- `StructuredResult`
- `Success`
- `Error`
- `ExecutionReceipt`

Every request has stable action IDs, accessibility labels, optional expiration, and a bounded data payload. Sensitive values are typed and redacted from logs.

### 9.3 Rich-view layer

The existing strict `ExtensionView` tree is the starting point. V2 must add only capabilities justified by reference extensions and retain:

- an allowlisted component catalog;
- structural depth/node/string/option budgets;
- validated stable event IDs;
- host-owned theme, spacing, focus, animation, and accessibility;
- incremental patches or host-owned data updates rather than unbounded tree replacement;
- deterministic cleanup when closed, completed, timed out, or cancelled.

Rich views are appropriate for extension-authored manual experiences and complex Agent follow-up. Ordinary confirmations and results use the semantic layer.

### 9.4 Session states

```text
opening -> listening/input -> reasoning -> interaction?
        -> executing -> interaction/result -> completed
                         \-> failed/cancelled/expired
```

A session may loop through interaction, reasoning, and execution. There is no static global “one Extension Mode session” owner. The host tracks session IDs, producer identity, active executor, window attachment, cancellation, and terminal cleanup.

### 9.5 Agent shell behavior

The Command shell opens in a deterministic launcher state. Typed text filters launcher entries locally. It also shows an explicit “Ask Grain” row/action; selecting that row sends the text to the Agent. Voice input goes to the Agent. This avoids a hidden classifier and accidental model calls while preserving one shortcut and one shell.

After Agent submission the same shell becomes the Dynamic UI session: listening, reasoning, confirmation, follow-up, progress, and result are state transitions rather than separate windows that must visually imitate one another.

Detailed visual design belongs to the UI 2.0 branch and requires real-application user approval. The architecture does not require a browser harness.

---

## 10. Launcher

- Index launcher entries from static manifests; do not start workers to search.
- Search names, subtitles, keywords, action titles, and bounded host-owned recency/frequency signals.
- Keep Agent retrieval and launcher ranking separate even though they share normalized records.
- Selecting a simple action enters the same prepare/risk/execute path as the Agent.
- Selecting a rich view activates only that view’s entrypoint.
- Manual-only extensions can omit Agent metadata entirely.
- Launcher history is host-owned, locally stored, bounded, and clearable.
- Icons use the existing standardized host asset pipeline; one test icon may be reused in fixtures, but production packages must deliver validated icons.

---

## 11. Runtime and lifecycle

### 11.1 Immediate runtime decision

Use the existing shared supervisor and isolated per-extension worker model as the first execution adapter. It already supplies lazy activation, identity-bound tokens, bounded calls, and idle cleanup. Replacing the engine is not required for the architecture above.

### 11.2 Activation rules

Activation occurs only for:

- an exact action selected for preparation/execution when code is actually needed;
- an explicit launcher rich-view open;
- an enabled background contribution covered by a future separate contract.

Index construction, Agent directory creation, local retrieval, and `search_actions` must be code-free.

### 11.3 Engine benchmark ADR

QuickJS, V8/Node, and Wasm can be benchmarked in parallel. The ADR must measure cold start, steady-state RAM, isolation, package compatibility, cancellation, crash containment, and maintenance cost using representative extensions. No migration occurs only because another runtime is theoretically smaller.

---

## 12. Security requirements

These are release gates, not cleanup work:

1. Rust remains the sole privileged-operation broker.
2. Extension identity, token, realm, and action ID are bound and checked on every call.
3. Manifest metadata cannot activate code.
4. Input schemas are validated in Rust before extension execution; outputs are size/type checked afterward.
5. Host risk floors cannot be weakened by manifests, prompts, model output, or user settings.
6. Confirmation displays the exact extension, action, material arguments, data destinations, and side effects.
7. Prepared calls are revalidated after confirmation to prevent time-of-check/time-of-use races.
8. Secrets are represented by opaque handles; they are not inserted into Agent context or Dynamic UI payloads.
9. Tool results and extension-authored text are untrusted prompt data with strict budgets.
10. Cross-extension composition retains provenance and applies data-flow consent/policy before passing sensitive output onward.
11. Transcript or conversation context is not given wholesale to an extension. It receives only validated action arguments and explicitly authorized context.
12. Timeouts distinguish safe failure from unknown side-effect outcome.
13. Install/update invalidates cached action definitions and in-flight prepared calls when their manifest digest changes.
14. All listeners, sessions, windows, workers, and cancellation handles are released at terminal state.
15. Audit records contain IDs, timing, policy decisions, and redacted argument summaries; never raw secrets or unrestricted transcripts.

Threat-model tests must cover prompt injection in manifests/results, forged action names, schema confusion, stale confirmations, confused-deputy calls, unauthorized cross-extension data flow, worker impersonation, oversized UI trees, replay, and cancellation races.

---

## 13. Performance and observability

Capture monotonic timestamps for:

- transcript/input finalized;
- eligibility and hot-set retrieval complete;
- first model request sent;
- first model response/action/search call received;
- fallback search complete;
- action prepared;
- confirmation shown and answered;
- worker requested, connected, and ready;
- privileged call started and completed;
- result rendered;
- session and worker destroyed.

Track distributions, not averages:

- eligible Recall@K and false-exclusion rate;
- fallback-search rate and search hops;
- wrong-action, no-action, and invented-action rates;
- confirmation accept/decline and corrected-argument rates;
- execution success/failure/unknown outcome;
- cold/warm worker latency;
- peak and steady-state RAM by installed and active extension count;
- session leaks and workers alive after idle deadline.

Exact budgets are Phase 1 outputs. The invariant is that installed-but-idle extensions add metadata memory, not runtime-engine memory.

---

## 14. Manifest and compatibility migration

Phase 0 must publish a versioned manifest proposal and fixtures before parser edits. Conceptually:

```yaml
kind: standalone # or extend

agent:
  summary: Work with GitHub repositories and issues.
  actions:
    - id: create_issue
      title: Create issue
      description: Create a new issue in a GitHub repository.
      whenToUse: The user wants a new issue, bug report, or tracked task.
      examples: [...]
      inputSchema: {...}
      risk: confirm
      entrypoint: actions.create_issue

launcher:
  commands:
    - id: open_issues
      title: Open Issues
      keywords: [bugs, tickets]
      entrypoint: views.issues
```

Compatibility rules:

- Existing V1 manifests continue to parse during a time-bounded migration window.
- `extending` maps mechanically to `extend` where semantics are unchanged.
- `standalone` maps to `standalone`.
- `searchable` does not auto-convert to a trusted Agent action. A migration tool can generate a reviewable draft, because V1 routes transcripts while V2 declares exact actions and schemas.
- `recommend`, `needs`, and V1 auto-send fields become deprecated warnings, then errors only after reference extensions and migration tooling exist.
- Old bindings migrate atomically: keep the user’s configured chord, rename its action to Command, and avoid creating a duplicate binding.
- Diagnostics must distinguish legacy sessions from V2 Agent actions during rollout.

---

## 15. Implementation phases

### Phase 0 — Contract freeze and legacy feature freeze

**Goal:** Remove ambiguity before production edits.

Work:

- Approve terminology, two-shortcut UX, conceptual manifest, action/result/error contracts, and risk classes.
- Write ADRs for provider tool support, Dynamic UI ownership, current workspace HTML compatibility, and manifest trust.
- Mark V1 Extension Mode maintenance-only; accept security and correctness fixes but no new features.
- Define compatibility window and rollback flags.
- Create representative manifests for GitHub, Spotify, Linear, Calendar, and a manual-only Color Picker.

Exit gate:

- Contracts and fixtures are reviewed; no unresolved naming or ownership contradiction blocks code.

### Phase 1 — Capability Index V2 and retrieval benchmark

**Goal:** Prove actions can be discovered with high recall without waking extensions or flooding the prompt.

Work:

- Implement a pure, code-free index core with Agent directory, action, and launcher projections.
- Adapt the V1 evaluation harness to action Recall@K and downstream fixtures.
- Implement field-aware lexical baseline and exact name/ID/namespace boosts.
- Benchmark existing dense retrieval and hybrid RRF against the same corpus.
- Add `search_actions` tests, unavailable-action diagnostics, memory measurements, and adversarial metadata tests.

Likely areas:

- `crates/grain-sdk/src/manifest.rs`
- a new low-level index/retrieval crate or `grain_*` Rust module outside `handy/`
- `src-tauri/src/extension_host.rs` index integration
- existing recommendation evaluation modules and fixtures

Exit gate:

- A frozen benchmark report establishes K, directory budget, false-exclusion rate, latency, and memory envelope. Metadata search never starts a worker.

### Phase 2 — Generic Agent action registry

**Goal:** Make Grain’s Agent use one deferred-tool architecture before third-party execution is enabled.

Work:

- Introduce registry `describe/prepare/execute` contracts and structured outcomes.
- Move Grain Space tools behind a built-in provider adapter.
- Add hot-set tool exposure and `search_actions` to the existing model loop.
- Make cumulative tool exposure and search hops bounded.
- Add explicit provider tool-capability gating and honest unsupported UX.
- Instrument retrieval and model/tool-loop timings.

Likely areas:

- `src-tauri/src/agent.rs`
- `src-tauri/src/grain_llm_client.rs`
- `src-tauri/src/grain_space/agent_tools.rs`
- new `grain_capability_*` modules

Exit gate:

- Grain Space actions pass existing behavior tests through the generic registry; fallback discovery is covered end to end; unsupported providers cannot report false success.

### Phase 3 — Third-party action execution and host risk policy

**Goal:** Execute one exact extension action safely through the current runtime.

Work:

- Map an Agent action record to a lazy execution entrypoint.
- Validate arguments and action exposure in Rust.
- Implement prepared calls, risk floor, confirmation, revalidation, receipts, idempotency metadata, cancellation, and unknown outcomes.
- Use semantic confirmation/result interactions rendered by the current strict host surface as transitional UI.
- Build one read action and one side-effecting action in reference extensions.

Exit gate:

- No transcript hand-off; no code activation during discovery; safe read action and confirmed write action pass security, cancellation, stale-confirmation, and cleanup tests.

### Phase 4 — Multi-action composition

**Goal:** Let the Agent complete bounded workflows across built-in and third-party providers.

Work:

- Add provenance-bearing structured results.
- Implement bounded sequential tool loops, partial failure, per-step receipts, and cancellation.
- Add cross-extension data-flow policy and consent UI.
- Test GitHub/Linear/Calendar-style two- and three-step workflows, including unavailable middle steps and ambiguous timeouts.

Exit gate:

- Multi-action workflows cannot bypass risk/permission boundaries and present truthful partial outcomes.

### Phase 5 — Producer-neutral Dynamic UI runtime

**Goal:** Replace Extension-Mode-specific view ownership with one session runtime.

Work:

- Define the semantic interaction protocol and session lifecycle.
- Generalize `extension_view.rs` and `ExtensionView` validation behind Agent/core/extension producers.
- Add focus, expiration, reconnect, cancellation, and terminal cleanup tests.
- Move Agent confirmations, follow-up, progress, and results into the runtime.
- Add patch/data-update support only after measuring full-tree update costs.

Exit gate:

- Agent and reference extensions use the same host runtime; session closure leaves no listeners, windows, workers, or state alive.

All frontend implementation in this and later UI phases must remain on `ui/grain-2.0` until the user explicitly approves a merge.

### Phase 6 — Unified Command shell and launcher

**Goal:** Deliver the two-shortcut product model.

Work:

- Build static launcher projection/search and manual-only extension support.
- Convert the Agent panel into Command shell states backed by Dynamic UI.
- Use explicit “Ask Grain” submission for typed text and Agent submission for voice.
- Migrate the Extension Mode binding to Command while preserving the user’s chord.
- Remove the default separate Agent/Extension Mode conflict without removing optional per-extension shortcuts.
- Perform real-Tauri visual review with the user; no browser harness.

Exit gate:

- Dictation and Command are the only default invocation concepts; launcher discovery is code-free; Agent transitions remain in one fluid window.

### Phase 7 — Rich public views

**Goal:** Support useful extension-authored UI without losing Grain ownership.

Work:

- Publish the allowlisted component and event SDK.
- Add only primitives demanded by reference extensions.
- Add efficient data binding/patches, accessibility conformance, localization, payload budgets, and lifecycle rules.
- Resolve the compatibility ADR for existing workspace/overlay HTML surfaces.

Exit gate:

- Complex reference views work within measured RAM and update budgets, and malformed/adversarial views cannot escape host constraints.

### Phase 8 — Runtime engine decision

**Goal:** Replatform only if measured benefit justifies compatibility and maintenance cost.

Work:

- Run the engine benchmark ADR against reference extensions.
- If another runtime wins materially, introduce it as an execution adapter and migrate incrementally.
- Otherwise record the decision and keep the existing host.

Exit gate:

- Evidence-backed ADR; no speculative rewrite.

### Phase 9 — Legacy removal

**Goal:** Delete the old single-extension route after V2 parity.

Work:

- Remove V1 recommendation chooser, transcript hand-off, auto-send, old action session, obsolete manifest fields, and compatibility aliases.
- Retain useful evaluation fixtures after renaming them for action discovery.
- Migrate diagnostics and documentation to V2 terminology.

Exit gate:

- No shipped flow depends on Extension Mode, `searchable`, recommendation pill selection, or V1 auto-send; rollback closes deliberately.

---

## 16. Rollout and rollback

- Put V2 index, Agent registry, third-party execution, Dynamic UI sessions, and Command shell behind separate host-owned feature flags.
- Dual-index manifests during migration, but never execute both V1 and V2 paths for one request.
- Persist schema versions with cached index records and discard mismatches.
- Keep legacy Extension Mode callable only for controlled parity testing until Phase 6 is approved.
- A phase can roll back to the previous coordinator without downgrading installed manifests; manifests remain declarative and versioned.
- Security fixes apply to both paths during coexistence.

---

## 17. Work that should start next

Start **Phase 0 followed by Phase 1**, not Dynamic UI polish and not a runtime rewrite.

The first implementation deliverable should be a pure Capability Index V2 plus a checked-in action-retrieval benchmark. It settles the schema every later layer consumes, proves the hot-set/fallback premise, reuses existing evaluation work, and adds no extension-code execution risk.

After that, move Grain Space tools through the generic Agent action registry. This validates deferred discovery and provider behavior with a trusted built-in provider before third-party execution expands the security boundary.

---

## 18. Primary prior art informing the plan

- [Ratel retrieval and search](https://docs.ratel.sh/retrieval-and-search): host prefilter plus Agent capability search, schema-aware BM25, and optional hybrid retrieval.
- [OpenAI Codex deferred tool search](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/handlers/tool_search_spec.rs): bounded deferred metadata search and source/namespace descriptions.
- [Raycast command lifecycle](https://developers.raycast.com/information/lifecycle) and [security model](https://developers.raycast.com/information/security): lazy command lifecycle and a narrow host API boundary.
- [Shopify Remote DOM](https://github.com/Shopify/remote-dom): extension-controlled composition over a host-controlled component allowlist.

These are architectural references, not contracts to copy wholesale. Grain’s low-RAM target, Rust capability boundary, local execution model, and two-shortcut UX remain decisive.

---

## Amendment E — Remote MCP development providers (2026-08-31)

### Decision

Before building more native extensions, Grain will use a small curated set of
hosted MCP servers to prove the Agent platform against real tools and accounts.
This is a **development integration layer**, not a decision to expose MCP as a
public production extension format.

The first catalog is GitHub, Linear, Notion, Atlassian, Slack, and Google
Calendar. A provider represents one service and one connected account. Native
extensions remain the production runtime and are not migrated or removed by
this work.

### Protocol and lifecycle boundary

- Support remote HTTPS Streamable HTTP only.
- Prefer MCP `2026-07-28` stateless discovery. Current hosted providers upgrade
  independently, so negotiate `2025-11-25`/`2025-06-18` when necessary and use
  their required protocol initialization handshake.
- A protocol handshake is not server lifecycle ownership. Every operation is
  still self-contained: never spawn a provider process, open a standalone GET
  stream, deliberately retain `Mcp-Session-Id`, or keep an idle MCP
  connection/task alive. Cancel and drop the client service after each list or
  call.
- Reject stdio, SSE-only, arbitrary-URL, and unsupported protocol servers with
  an actionable compatibility error.
- Do not implement MCP Apps, arbitrary HTML, resources, prompts, subscriptions,
  sampling, tasks, or Dynamic UI in this phase.

### Authentication

- Connecting and reauthorizing are explicit Settings operations; the Agent can
  never start an OAuth flow during a task.
- Use OAuth 2.1 Authorization Code + PKCE, protected-resource/authorization-
  server metadata discovery, issuer validation, and the provider-supported
  client-registration mechanism.
- Persist tokens and dynamic client credentials only in the OS credential
  vault. There is no plaintext fallback.
- Linear, Notion, and Atlassian use their advertised dynamic registration and
  form the zero-custom-app validation tranche. GitHub, Slack, and Google
  Calendar require a pre-registered client ID/secret supplied through developer
  settings.
- One account is stored per MCP provider. Disconnect deletes its credential.

### Agent integration

- Add connected, enabled MCP providers to the existing bounded Level-1
  directory. Loading one performs a stateless `tools/list` and publishes the
  resulting schemas on the next model round through the existing task-local
  exposure map.
- Treat server names, descriptions, schemas, annotations, and results as
  untrusted data. Sanitize and bound them before model/UI exposure.
- MCP tool annotations are hints, never authority. During this validation
  phase every MCP call uses Grain's host-owned confirmation gate. The exact
  provider, tool, arguments, endpoint identity, and tool-set digest are
  revalidated after approval.
- Preserve the Agent's current sequential tool loop and global hop/call/tool
  ceilings. No parallel or multi-agent orchestrator is introduced here.
- Return bounded text/structured results with provider provenance. Reject image,
  audio, embedded-resource, input-required, task, and oversized results rather
  than expanding the UI/runtime boundary.

### Network and resource policy

- Endpoints come from the compiled catalog; arbitrary URLs are not accepted.
- Require HTTPS, disable redirects for MCP and sensitive OAuth exchanges, use
  finite connect/request timeouts, cap response/event sizes, and never forward
  credentials across origins.
- Construct no idle engine. The only reusable runtime resource is one lazy HTTP
  client/connection pool; each discovery or call service is cancelled and
  dropped before returning.

### Delivery order and exit gate

1. Catalog, status, credential configuration, connect/disconnect, and a
   Settings-only discovery check.
2. Provider-neutral task-local schemas and MCP `tools/list` loading.
3. Prepared-call/confirmation execution adapter and bounded result mapping.
4. Manual account testing against Linear, Notion, and Atlassian: connect,
   discover, load from a fresh Agent request, confirm a call, and receive its
   result. Then expand provider coverage without blocking core-platform work.

Initial test gate: three real providers can be connected from Settings without
creating OAuth apps, discovered by a fresh Agent request, loaded only when
relevant, called sequentially through the confirmation boundary, and fully
cleaned up after each operation. No MCP process, session, listener, or tool
schema survives beyond its documented scope.
