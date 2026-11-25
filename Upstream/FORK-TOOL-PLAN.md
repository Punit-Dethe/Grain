# Fork tool — design plan

Status: **M0 started** (2026-11-25). Name: **`tether`** (user-facing name may
change later). License: **MIT**. Repository: `Punit-Dethe/tether` for now; it
moves to an organisation account later. No MCP server is planned (§8).

A free, open-source tool for keeping a long-lived fork in step with its
upstream, built for AI agents to drive. It starts life as Grain's own
`Upstream/` system, generalised, rewritten in Rust and moved to its own
repository. Grain becomes its first user; nothing Grain-specific lives in it.

---

## 1. Why build it

Research (2026-11-25, sources at the end) found:

- **Sync and conflict fixing are solved.** GitHub "Sync fork", the Pull app,
  fork-kit, and — since 2026 — GitHub Copilot's "Fix with Copilot" and Agent
  Merge cover plain syncing and conflict resolution. Mergiraf and Weave cover
  code-aware merging. We must not rebuild any of that.
- **Carrying a small patch set onto each upstream release is covered, but only
  by a closed, hosted, paid service** (Graftfork, Nov 2026, $12/month, agent
  work billed per sync). The open alternatives (jj-fork, autofork, Cohere's
  vllm-skills, Electron's `patches/`) are scripts or skills, not tools.
- **Nobody handles forks that rewrote parts of upstream** — "product forks"
  like Grain, Positron, the VS Code-based editors, GitLab's web IDE. Graftfork
  moves fewer than half of conflicting syncs and lists UIs as roadmap. The
  practitioner pain ("review every commit regardless of purpose… cognitively
  heavy", "no clear marker for customisations", "automation fails once changes
  are substantial") is exactly this segment.
- **The closest open-source MCP tool** (fork-parity) only triages commits
  (1 star, no applying, porting or test gates).

So the tool is worth building **only** as: everything the good tools do
(no downside to switching) + the product-fork layer nobody has + an interface
designed for agents. Free, local, bring-your-own-agent.

## 2. Principles

1. **Agent-first.** ~90% of runs will be an AI agent. Every command has a
   `--json` form, every state says what to do next, every step can stop and
   resume.
2. **The tool decides "done", not the agent.** Agents claim success early. The
   tool runs the checks, validates evidence and refuses unsafe outcomes.
3. **Bring your own agent.** No built-in LLM, no API key, no server. The user's
   agent (Claude Code, Codex, …) does the judgement; the tool does state,
   context and proof. CI can launch a configured agent command headless.
4. **Calls the real `git`.** Rename detection, rerere, merge drivers and
   `replace --graft` only behave fully in git itself; a library would differ
   silently.
5. **No feature that is just git under a new name.** If git does it in one
   command, document the command instead.
6. **Fail closed.** Missing data, diverging sync points, unreviewed changes:
   stop with a reason, never guess.
7. **Small and quiet.** One binary, no daemon, no background work. State lives
   in the repo (decisions) and in `.git/` (in-progress sessions).
8. **Readable code.** Small modules, an `ARCHITECTURE.md`, tests next to the
   code — the project should be studyable and rebuildable by hand.

## 3. What we take from each tool (no downside to switching)

| From | Feature | Where in tether |
| --- | --- | --- |
| Graftfork | Detect each new upstream **release**; attempt in an isolated worktree; never touch the user's branch; deliver a branch/PR | `watch`, `sync` (worktree sessions) |
| Graftfork | Per change: **keep / drop (upstream has it) / rewrite**, with a note per change | carry mode, `decide` |
| Graftfork | Tests on **upstream-as-shipped and on the result**; move only if no new failure | `verify` |
| Graftfork | Safety rules: refuse a drop when upstream deleted a touched file; refuse a keep that empties a file; rewrite only with tests | carry-mode gates |
| Graftfork | "Stay" with a stated reason when it cannot move | session outcome |
| Cohere vllm-skills | Verify the fork is green **before** syncing (baseline); fix→test loop until green | `verify --baseline`, agent loop |
| Sync fork / Pull app | One-command sync for forks with no changes; scheduled CI that opens a PR | `sync` fast path, CI template |
| fork-kit / Grain | Merge mode for forks that cannot rebase; one sync PR | merge mode |
| Electron / jj-fork / Brave | Patch-carry mode: fork = ordered change set on upstream | carry mode |
| Copybara / Grain | Clean history: one sync commit + `Upstream-Synced:` marker | merge mode (default) |
| rerere (Grain) | Shared, versioned conflict memory | `.tether/rr-cache/` |
| Weave | Code-aware (function/class level) merging | **built in** via the `weave-core` crate (§8.1) |
| Mergiraf | Code-aware merging | supported as an external driver only (GPL-3.0) |
| fork-parity | Upstream triage by priority | inbox categories |
| stream-track / safeguard | Find upstream follow-up and security fixes | inbox `security` category |

## 4. Gaps in Grain's current system — and where each is fixed

| Gap found in the review | Fix in tether |
| --- | --- |
| No test check against upstream | `verify` runs the configured build/test on (a) the fork before sync, (b) upstream as shipped, (c) the result; only failures new in (c) block |
| No AI help where it matters (porting into rewritten code) | **Work packets** (§6.3) give the agent exactly the upstream diff, its reason, the mapped Grain-side destination and the acceptance checks; the port loop retries until build+tests pass (compiler-error-guided, as in the MIP paper) |
| Divergence map written by hand, drifts | **Intent markers** in code (`// tether: <reason>`) + a **generated** divergence map; any change to upstream-derived code without a marker or a recorded reason fails `check` — the "setting changed for no reason" case |
| Setup is custom (3,600 lines, Handy constants hard-coded) | One `.tether/config.toml`; `tether init` detects the upstream remote, the sync point, moved folders and test commands |
| Security fixes not ranked | Inbox `security` category first (keywords, GHSA/CVE references, upstream labels) |
| Only follows upstream `main` | Track `releases` (tags, default) or `branch` |

Grain's existing strengths carry over unchanged: relocation-aware port audit,
review queue with required evidence, divergence budgets, owned-area handling
(frozen frontend + silent-adoption detection), honest-history marker.

## 5. Concepts

- **Sync point** — the upstream commit the fork last synced to. Merge mode
  records it as `Upstream-Synced: <sha>` in a single-parent sync commit (proven
  in Grain's `Upstream/sync_base.py`). Carry mode records it in the session.
- **Inbox** — every upstream commit past the sync point, sorted into:
  `auto` (applies cleanly, no owned/relocated paths), `port` (touches a file
  the fork relocated, replaced or runs in parallel), `review` (touches an owned
  area), `security`, `conflict`. Each item has a state:
  `pending → decided (adapted | already-covered | not-applicable | deferred) → closed`.
- **Decision** — a recorded outcome with required evidence (problem, does the
  fork share it, destination paths, verification). Stored in
  `.tether/decisions/` — the memory that stops agents re-deciding.
- **Owned area** — paths the fork owns outright (Grain: `src/`). Upstream
  changes there never apply; they become `review` items.
- **Relocation map** — upstream path → fork path(s), kind
  `relocated | inert | parallel` (Grain's `relocations.json`).
- **Intent marker** — a short reason at each fork change inside
  upstream-derived code; the generated divergence map is built from markers +
  diff against the sync point.
- **Budget** — per-file divergence limit for upstream-derived code (Grain's
  ratchet).
- **Autonomy policy** — what an agent may finish alone (`auto` items, ports
  that pass verify) vs what needs a human (owned areas, security, budget
  growth).

## 6. Architecture

### 6.1 Modules (one Rust workspace)

| Module | Responsibility |
| --- | --- |
| `git` | The only place that runs `git`. Typed results, no parsing elsewhere |
| `config` | `.tether/config.toml` schema, defaults, `init` detection |
| `syncpoint` | Read/write markers; refuse diverging markers |
| `inbox` | Collect upstream commits, classify, track item states |
| `engine::merge` | Squash sync via temporary `replace --graft` + `merge --squash` (Grain's proven method) |
| `engine::carry` | Replay change set onto a release; keep/drop/rewrite rules |
| `owned` | Owned-area auto-resolution and silent-adoption detection |
| `relocate` | Relocation map + port audit |
| `map` | Intent markers, generated divergence map, unmarked-change check |
| `budget` | Divergence budgets (ratchet) |
| `verify` | Run configured build/test on baseline, upstream, result; diff the failures |
| `packet` | Build agent work packets |
| `decide` | Validate and store decisions + evidence |
| `session` | Resumable state in `.git/tether/` (what step, what is pending) |
| `report` | Text and JSON output; `status` next-action contract |
| `semantic` | `tether merge-driver`: Weave's entity-level merge as a git merge driver (§8.1) |

### 6.2 Files

```
.tether/config.toml        upstream, mode, owned areas, relocations, tests, autonomy
.tether/decisions/*.toml   one file per decided upstream item (merge-friendly)
.tether/budgets.toml       divergence budgets
.tether/rr-cache/          shared conflict memory
.git/tether/session.json   in-progress sync (never committed)
```

### 6.3 The agent contract

- `tether status --json` always returns: current phase, blocking items, and
  `next: { command, why }`. An agent can run a whole sync by following `next`.
- `tether packet <item> --json` returns a self-contained task: upstream diff
  (trimmed to relevant hunks), commit message + PR/issue text, mapped fork
  destinations with current code, intent markers in those areas, past
  decisions on the same paths, and the acceptance checks. Sized to a token
  budget set in config.
- `tether decide <item> <outcome> --evidence …` rejects incomplete evidence
  and outcomes the autonomy policy forbids.
- Every command is idempotent and resumable; a crashed agent loses nothing.
- Ships `AGENT.md` (a skill / instructions file) so "use tether to sync
  upstream" works with no further prompting.
- CI: `tether ci` runs the trial, and when `agent.command` is configured,
  hands packets to that command headless, then gates the result.

## 7. Commands (first version)

| Command | Does |
| --- | --- |
| `init` | Detect upstream, sync point, moved folders, tests; write config |
| `status` | Where things stand + the next action |
| `inbox [--category …]` | List upstream items with state |
| `packet <item>` | Work packet for one item |
| `sync [--target <rev>]` / `--continue` / `--abort` | Run a sync session |
| `decide <item> <outcome>` | Record a decision with evidence |
| `verify [--baseline\|--upstream\|--result]` | Build/test gates |
| `map` / `check` | Generated divergence map; CI gate (budgets, unmarked changes, owned areas, port audit) |
| `closeout <rev>` | Move the sync point over fully-decided items, tree unchanged |
| `doctor` | Explain misconfiguration in plain words |

All take `--json`. Nothing else in v1.

## 8. Works alongside (not instead of)

### 8.1 Weave, built in

[Weave](https://github.com/Ataraxy-Labs/weave) (Rust, ~1.4k stars, active —
v0.7.1 on 2026-11-02) merges at the function/class level and refuses when
unsure. Its merge engine is a separate library crate, `weave-core`
(`MIT OR Apache-2.0`, on crates.io, entry point `entity_merge`), so it fits
inside tether without forking:

- **How it plugs in:** git calls a merge driver per conflicting file. tether
  registers **itself** as that driver for one sync only
  (`tether merge-driver %O %A %B %P`, which calls `weave-core`), so users install
  one binary, and git's rename detection, rerere and the owned-area rules keep
  working around it. The driver is written to `.git/info/attributes` for the
  session and removed after it, and only for paths the repository has not
  given its own `merge=` rule (Grain's `src/** merge=ours` must keep winning).
- **Depend, don't fork.** Forking Weave would create the very problem tether
  solves. tether pins a `weave-core` version. If we need a change, we send it
  upstream; only if it is refused do we carry a patch (`[patch.crates-io]`),
  and that patch is then maintained with tether itself.
- **Cost to watch:** `weave-core` turns on `sem-core`'s `git` (libgit2, C) and
  `grammar-all` (~40 tree-sitter grammars, C) features, which means a slower
  first build and a bigger binary. Mitigation: tether builds it behind a
  `semantic-merge` cargo feature (on in release binaries, off for a minimal
  build), and we propose upstream a weave-core feature to choose grammars and
  drop libgit2.
- **Licences:** Weave is MIT-or-Apache, compatible with tether's MIT. Mergiraf
  is GPL-3.0-only, so it can be used as an external driver but never linked in.

### 8.2 Other tools

- **Mergiraf** — external merge driver, if the user prefers it.
- **GitHub Copilot / Agent Merge, `gh`, GitHub MCP** — PR creation and
  plain-conflict fixing stay theirs; tether produces the branch and the PR body.
- **Git MCP servers** — generic git access for the agent; tether does not
  duplicate them.
- **Graftfork** — a hosted alternative; tether is the local, open one.
- **fork-parity** — overlapping triage; tether's inbox is the superset.

### 8.3 No MCP server

Agents drive tether through its command line, which they already use well; an
MCP layer would add a process and a second interface for no new capability.
Revisit only if a real agent cannot use the CLI.

## 9. Dashboard (last, optional)

Only after the CLI is proven on Grain. A local, read-only page served by
`tether ui`, showing only what helps a decision:

- the inbox by category with counts and the oldest waiting item;
- the current sync: phase, what is blocking, what the agent did per item;
- divergence health: budgets near their limit, unmarked changes, stale markers.

No log walls, no activity feeds, no charts without a decision behind them.
Designed with user approval before building (same rule as Grain's UI).

## 10. Milestones

| # | Milestone | Done when |
| --- | --- | --- |
| M0 | Repo, workspace, CI, `ARCHITECTURE.md`, test harness (throwaway two-repo fixtures, as in Grain's `test_squash_sync.py`) | CI green on an empty command set |
| M1 | Merge mode at **parity with Grain's Python**: sync point, sync, closeout, inbox/ledger, decisions, budgets, owned areas, port audit | Golden tests: identical results to `Upstream/*.py` on Grain's real history (conflicts, index, ledger, budgets) |
| M2 | Agent layer: `status`/`next`, packets, evidence gates, sessions, `AGENT.md` | An agent completes a fixture sync by following `next` only |
| M3 | `verify` (baseline / upstream / result) | Fixture where upstream is broken is told apart from a broken merge |
| M4 | Intent markers + generated map + unmarked-change check | Fixture: an unmarked edit fails `check` |
| M5 | Release tracking + security category | Fixture with tagged releases and a GHSA reference |
| M6 | **Grain switches**: config written, CI moved, `Upstream/*.py` deleted, then the 72+ pending Handy commits synced with tether | Grain sync PR produced by tether |
| M7 | Carry mode (patch-set forks) with Graftfork's safety rules | Public fixture forks re-applied onto new releases |
| M8 | Built-in Weave merge driver (`semantic-merge` feature) | Fixture: a function-level conflict git cannot merge is merged; an unsure one is refused |
| M9 | Dashboard | User visual approval |

## 11. Moving Grain safely

1. `chore/fork-maintenance` stays the reference implementation; **no new
   features in Grain's `Upstream/`** while the port runs.
2. Golden tests compare tether against the Python on Grain's real history at
   each milestone; any difference is a bug until explained.
3. Grain switches only at M6, in one PR, after a dry run on a copy.
4. The history rewrite and public repository are separate work, after M6.

## 12. Risks and non-goals

- **Time** — two engines and a verify system are real work next to Grain.
  Mitigation: M1–M6 serve Grain directly; carry mode waits.
- **Competition** — Graftfork could open-source; GitHub could build it in.
  The product-fork layer and the agent contract are the parts least likely to
  be copied.
- **Non-goals** — our own conflict resolver, our own LLM calls, hosting, an
  MCP server, patch-stack editing UX (jj-fork, Electron do it), dashboards
  before M9.

## 13. Decisions

Decided 2026-11-25: name `tether` (user-facing, changeable), license MIT,
repository `Punit-Dethe/tether` (moves to an organisation later), no MCP
server, Weave used as a library dependency rather than forked.

Still open:

- Intent marker syntax per language (comment prefix vs a sidecar file).
- Whether `packet` trims context by syntax (tree-sitter, available through
  `weave-core`) in v1 or later.

## Sources

- Graftfork: <https://graftfork.com/>, evidence <https://github.com/craigm26/graft-evidence>
- Cohere fork maintenance: <https://cohere.com/blog/automating-fork-maintenance-with-ai-agents>
- GitHub Copilot conflicts / Agent Merge: <https://github.blog/changelog/2026-03-26-ask-copilot-to-resolve-merge-conflicts-on-pull-requests/>, <https://github.blog/changelog/2026-10-28-agent-merge-watches-and-fixes-pull-requests-in-the-cloud>
- Weave: <https://github.com/Ataraxy-Labs/weave> · Mergiraf: <https://mergiraf.org/>
- fork-parity: <https://mcp.directory/servers/fork-parity> · Git MCP: <https://github.com/modelcontextprotocol/servers/tree/main/src/git>
- jj-fork: <https://docs.rs/crate/jj-fork/0.2.8> · fork-kit: <https://github.com/BierDav/fork-kit> · Electron patches: <https://www.electronjs.org/docs/development/patches>
- MIP patch porting: <https://arxiv.org/abs/2604.01680>
- Practitioners: <https://lobste.rs/s/yav1ky/those_who_have_maintained_forks_oss_at>, <https://gitlab.com/gitlab-org/gitlab-web-ide-vscode-fork/-/issues/32>, <https://jdno.dev/how-long-lived-forks-break-open-source-collaboration/>
