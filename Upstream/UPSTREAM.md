# Upstream (Handy) — how updates flow into Grain

Grain is a friendly fork of [Handy](https://github.com/cjpais/handy). Handy's
battle-tested STT core lives **verbatim** in `src-tauri/src/handy/`; everything
Grain is lives outside it. This document is the single source of truth for how
upstream updates are absorbed. The per-file conflict policy lives in
[UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md); fixes we should send _to_
Handy live in [UPSTREAMABLE.md](UPSTREAMABLE.md).

**Before the next sync, read [DEFERRED-PORTS.md](DEFERRED-PORTS.md) and the
[recording overlay maintenance contract](OVERLAY-MAINTENANCE.md).** The first
lists the exact Handy commits whose runtime behavior Grain has deliberately not
implemented yet.

The tooling is [tether](https://github.com/Punit-Dethe/tether). Its policy and
records are committed in [`.tether/`](../.tether/); see
[README.md](README.md) for what each file holds.

## The short version

**The common case is reviewing a PR, not running a procedure.** Every 2 hours
CI previews the next sync. If Handy moved and the sync is clean, CI opens (or
refreshes) the **`sync/auto-upstream` PR** with the sync already done as one
squashed commit, the commit list in the body, and `tether check` already run on
the result. Review it, merge it. Commits inside the sync file themselves as
merged, because the sync commit's `Upstream-Synced:` line _is_ the record.

`merge=ours` decides file ownership, not whether an upstream fix matters. Every
upstream change under the frozen frontend or another Grain-owned path must be
reviewed as a problem report: what broke, whether Grain shares it, which Grain
paths adapt or already cover it, and how that was verified. tether's review
gate enforces those records. For transcribe.cpp changes,
`docs/TRANSCRIBE-CPP-FORK.md` and `native/transcribe-fork.json` are the binding
contract and paired immutable source pin. Preserve both fork patches/locks and
the current target features when adapting Handy dependency changes; a native
upgrade is a separately tested fork release.

Working locally? One command answers everything:

```bash
tether status     # fetches, then: where things stand and the one next step
```

Only a _conflicted_ sync needs a human driver — and the CI job summary will
have listed the exact conflicting files up to 2 hours in advance, with
[UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md) saying which side wins in
each one.

## The architecture (four layers)

### 1. Isolation — the layout does most of the work

```
src-tauri/src/handy/   Handy's tree, byte-preserved + small marked [GRAIN]
                       hooks. Declared from lib.rs via #[path = "handy/..."],
                       so crate paths AND file contents are unchanged — files
                       still diff 1:1 against upstream. DON'T ADD FEATURES HERE.
src-tauri/src/         Grain: composition roots (lib.rs, main.rs) + grain_*
                       modules, agent, bridge, rolling, grain_space…
crates/                Grain crates (grain-core, grain-sdk, grain-tdt,
                       grain-editor).
```

`.tether/config.toml` records the move as `[[moved]]` (`src-tauri/src/` →
`src-tauri/src/handy/`, plus the frontend's `src/` → `src/app/`), so every
tether report speaks in both path spaces.

Two files inside `handy/` are **inert** — byte-identical to upstream but
never compiled (no `mod` declaration): `settings.rs`, `llm_client.rs`.
Grain's replacements are `grain_settings.rs`, `grain_llm_client.rs`, aliased in
`lib.rs` so `crate::settings::…` etc. still resolve. Upstream edits to inert
files merge with low conflict risk but high silent-port risk, so the port audit
requires a structured outcome and evidence for every mapped runtime
destination.

Because git recorded the folder move as 100% renames, merges map upstream's
`src-tauri/src/X` into our `src-tauri/src/handy/X` automatically. One caveat,
guarded by `tether check` (strays): a **new** upstream file at the `src/`
_root_ lands at our root — the root itself was never fully renamed, since
`lib.rs`/`main.rs` and the Grain modules live there — so `git mv` it into
`handy/` and add its `#[path]` declaration if it is a new module.

### 2. Merge machinery — plain git, driven by tether

- **The sync point is a commit-message line.** Every sync is one squashed,
  single-parent commit carrying `Upstream-Synced: <handy sha>`. tether finds
  the newest such line in Grain's history and merges three-way against that
  Handy commit, grafting the base in for the merge when Grain's history does
  not contain it. Never trust a version written in docs; `tether status`
  reports the sync point.
- **`merge=ours` attributes** (`.gitattributes`): docs, workflows, identity
  configs, lockfiles, `Upstream/` and `.tether/` keep Grain's side.
- **Owned areas** (`[[owned]]`): the frozen frontend (`src/`, except the moved
  `src/app/` exemptions listed there). Conflicts inside resolve to Grain
  automatically, and upstream work that rename detection routed _into_
  Grain's tree is reverted — git's directory-rename detection sees
  `src/` → `src/app/` and would otherwise apply upstream edits to moved files
  with **no conflict at all** (on 2026-08-02 only the type-checker caught it).
- **Merge settings** (`[sync.git_config]`): `merge.ours.driver` and
  `merge.directoryRenames` are passed to every sync, so a fresh clone or CI
  runner behaves exactly like a configured one.
- **rerere, shared** (`[sync] rerere_cache = ".tether/rr-cache"`): tether
  restores the committed resolutions before each sync and saves new ones
  after it, so CI and every clone replay past resolutions. Replayed text stays
  unstaged for review; an old resolution is not automatically correct forever.

We evaluated the heavier tools the ecosystem uses for this problem —
[Copybara](https://dagster.io/blog/monorepos-the-hub-and-spoke-model-and-copybara),
[josh](https://josh-project.github.io/josh/faq.html) and `git subtree` — and
rejected them: they solve _mapping a subtree across repositories_. Grain
doesn't have that problem, because `#[path]` kept upstream's paths
merge-compatible. GitHub's own
[friendly-fork guidance](https://github.blog/developer-skills/github/friend-zone-strategies-friendly-fork-management/)
reaches the same conclusion: frequent, small, scheduled merges beat clever
tooling.

### 3. Automation — CI does the waiting

[`upstream-sync.yml`](../.github/workflows/upstream-sync.yml), every 2 hours
(and on any push touching `.tether/`):

1. **Preview** (`tether sync --dry-run`) into the CI **job summary**: how far
   behind, the conflicts that need a person, possible security fixes not in
   Grain yet, and upstream commits already applied here by hand.
2. **Auto-PR**: new commits + no conflicts + nothing applied by hand → the
   `sync/auto-upstream` branch is rebuilt with `tether sync`, new rerere
   resolutions are committed, `tether check` runs on the result, and a PR is
   opened or updated with the commit list and a review checklist. Conflicts
   and hand-applied commits suppress the PR rather than producing a misleading
   one.

[`upstream-check.yml`](../.github/workflows/upstream-check.yml) on every
push/PR touching the Rust tree, the frontend or `.tether/`: `tether check`, plus
`scripts/grain_seams.py` for Grain's own seams (alias coverage, overlay
wiring, the transcribe.cpp pin).

#### The records — one folder, all human decisions

Nothing is generated into the repo. tether derives each commit's state from
upstream's history, the sync point and `.tether/decisions/`:

| Input                       | Answers                                                    |
| --------------------------- | ---------------------------------------------------------- |
| upstream's commit list      | what happened                                              |
| the sync point              | what we absorbed — everything up to it is merged           |
| `.tether/decisions/<sha>.toml` | what only a human knows: ignored, port evidence, reviews, notes |

Record decisions with the tool, never by hand:

```bash
tether inbox                                     # what is pending
tether packet <sha|#PR>                          # everything one commit needs
tether decide <sha|#PR> ignored --note "why"     # Grain replaced that surface
tether decide <sha|#PR> --note "detail"          # just annotate
tether decide <sha|#PR> --clear                  # back to derived
```

**A synced commit needs no decision at all.**

### 4. Guards — the boundary is enforced, not hoped for

`tether check` is the CI gate. It fails on:

- **Budgets** (`.tether/budgets.toml`): every Handy-derived file has a
  divergence budget (added+removed lines against the sync point, measured
  blob-to-blob so the folder move can't fool it). A grown budget, a newly
  diverged file or an outright-deleted upstream file fails. Budgets tighten
  with `tether check --update`; growth needs `--update --accept-growth` after
  semantic review and an explanation in the commit.
- **Strays**: an upstream file sitting outside `handy/`.
- **Adopted frontend files**: upstream files that landed in the frozen
  `src/`. The count may shrink, never grow.
- **Port audit** (`[relocations."<upstream file>"]`): budgets and diffs prove
  the _shared_ `handy/` surface is on par with Handy, but they are blind to
  fixes that merge into an **inert** file, into a file whose logic Grain
  **relocated** (`actions.rs` → `grain_post_process.rs`), or that share a bug
  class with a Grain-only **parallel** implementation (the rolling engine). In
  all three the upstream-shaped file matches perfectly while the code Grain
  runs lacks the fix. The audit fails when a synced upstream commit touched a
  relocated file without a port record, and when a relocation points at a
  Grain file that no longer exists.
- **Reviews** (`[[review]]`): synced upstream changes to the frozen frontend
  and other Grain-owned paths need a problem-level review record.
- **Unmarked changes**: a Grain edit inside Handy-derived code with no
  `[GRAIN] <why>` comment on or just above it. The accepted count per file
  lives in `budgets.toml [unmarked]` and may only shrink. `tether map` renders
  the divergence map from those markers.
- **Drift**: a Handy file that no longer matches the blob it was synced at.

Record runtime port decisions with:

```bash
tether decide <sha> ported --port <upstream file> --evidence "where it landed, how it was verified"
tether decide <sha> not-applicable --port <upstream file> --evidence "why"
tether decide <sha> deferred --port <upstream file> --evidence "why, and the plan"
tether decide <sha> --review <area> --problem "what broke" --dest <grain path> --evidence "how verified"
```

`deferred` is reviewed pending work, not implementation. It counts only while
the commit's full SHA has a row in `DEFERRED-PORTS.md` (`[port_audit]
deferred_queue`); tether refuses an unqueued deferral. On completion, record
the adopted behavior and verification before removing that row. The
overlay/capture review routes and acceptance checklist are in
[OVERLAY-MAINTENANCE.md](OVERLAY-MAINTENANCE.md).

## Runbook

### A. The auto-PR is open (common case)

1. Read the PR body's commit list and the `tether check` output. Anything
   Grain does **not** actually take needs saying so —
   `tether decide <sha> ignored --note "why"` (the divergence map says where
   Grain replaced that surface). Every port, review and marker the check asks
   for gets recorded on the branch; `tether packet <sha>` shows what each
   needs.
2. Review affected recording-window/capture changes using
   [OVERLAY-MAINTENANCE.md](OVERLAY-MAINTENANCE.md), including owned callers
   and frozen-frontend behavior. CI must be green (build, tests, Upstream
   Check). If the check flags a stray file, `git mv` it into `handy/` on the
   branch.
3. Merge the PR with a **merge commit or rebase — never squash**: squashing
   rewrites the sync commit's message, so the `Upstream-Synced:` line can be
   lost and the next sync re-fights these commits.

### B. The preview reports conflicts (rare case)

```bash
tether status                           # fetches; how far behind + what conflicts
git checkout -b sync/upstream-YYYY-MM-DD
tether sync                             # merges; owned areas resolve themselves
# resolve ONLY what it lists, per UPSTREAM-DIVERGENCE.md; git add each file
tether sync --continue                  # one squashed commit with the marker
git add .tether/rr-cache && git commit -m "Share new conflict resolutions"
bun install && cargo check              # regenerate lockfiles
tether status                           # ports, reviews, markers still owed
```

For a staged sync, pass `--target <commit>` to `tether sync`. It still fetches
first and rejects targets outside Handy's history. Resolve, review, test and
finish each batch before selecting the next boundary. `tether sync --abort`
throws a prepared sync away.

**Never run a bare `git merge upstream/main`.** It brings in Handy's history
instead of one squashed commit, merges against a possibly stale
remote-tracking ref (how the first 2026-08-02 sync "completed" while 16
commits stayed behind), and skips the owned-area rules. tether fetches first.

### B1. Before you commit anything in the Handy tree

Mark every change: a `// [GRAIN] <why>` comment on or just above it. For a
file that cannot hold comments, `tether map --reason <path> --why "<why>"`.
`tether check` measures the committed tree, so run it right after committing.

If upstream changed code Grain relocated (settings → `crates/grain-core`,
post-processing → `grain_post_process.rs`, LLM client → `grain_llm_client.rs`,
final-text stage → `audio_toolkit/grain_text.rs`), **port the change into the
Grain location by hand** and record it with `tether decide --port`. The
relocations in `.tether/config.toml` list every destination.

### C. Closing out commits without a sync (do not skip)

Once every commit up to some Handy revision has a decision
(`tether inbox` comes back empty for it):

```bash
tether closeout <rev>              # read-only: is every commit decided?
tether closeout <rev> --execute    # moves the sync point, changes no file
```

`closeout` refuses undecided commits, dirty worktrees and non-upstream
targets, and its commit changes no file. **Never** move the sync point by
hand over commits you have not assessed — it silently locks their fixes out
forever, with no conflict to warn you.

### D. "We're N behind, but we already did those" — hand-applied commits

Symptoms: the behind count stays stubbornly non-zero; the same file
conflicts in _every_ preview.

**Cause:** the work was applied by cherry-pick or by hand. Git tracks
history, not content — the content is in the tree but the sync point never
moved, so every future sync replays those commits and re-raises their
conflicts.

**Detection (automatic):** `tether inbox` marks a commit as applied here from
exact patch IDs and explicit `Upstream-Commit:` trailers. CI lists them in the
job summary and **suppresses the auto-sync PR** while any exist, because
auto-syncing in that state would replay resolved work.

**Fix:** verify the content really is present (spot-check the files each
commit touched), record it with `tether decide <sha> merged`, then run a
close-out (C) to move the sync point past it.

**Prevention:** sync rather than cherry-pick. If an urgent fix must be
cherry-picked or adapted, put the exact full SHA in the commit body as
`Upstream-Commit: <sha>`, then close it out afterwards.

### Verification (every sync)

- Rust: `cargo check --lib` then `cargo test --lib` in `src-tauri/`
- Frontend: `./node_modules/.bin/tsc --noEmit`
- Boundary: `tether check` and `python scripts/grain_seams.py`
- Dependencies: `bun install --frozen-lockfile`; regenerate intentionally if
  `package.json` changed, then run the existing Nix lock-derivative check
- Platform fixes: run the native CI target where available; otherwise record
  the unvalidated OS explicitly in the decision note and PR checklist
- Windows quirks on the primary dev machine: unset `LOCALAPPDATA` and `TEMP`,
  set `TMP=C:\Windows\Temp` (transcribe-cpp-sys junction workaround), and
  build with `CARGO_TARGET_DIR=C:\gtc` — the running Grain app locks the
  default target dir; NEVER kill the user's running app to free it.

## Deliberate deviations (do NOT "fix" these back to upstream)

- **Identity/rebrand**: `com.grain.app`. Grain's updater is owned by
  `grain_update.rs` with Grain's release policy; do not merge Handy's updater
  endpoint, signing identity, or frontend flow into it.
- **Frontend/backend decoupling**: frontend→backend is Tauri commands only,
  backend→frontend is events only. The frontend must stay destroyable.
- **Frontend freeze (2026-07-31)**: `src/` is Grain-owned — `merge=ours` plus
  tether's owned area and review gate, because `merge=ours` only wins
  conflicts. Everything genuinely valuable in upstream's frontend commits is
  _backend knowledge that happens to be written in TSX_ (which accelerators this
  host can run, which locale a system tag resolves to). Port those into Rust —
  which we still merge in full — and the freeze costs nothing. See
  [`docs/UI 2.0/PLAN.md`](../docs/UI%202.0/PLAN.md).
- **AI text provider fallback**: `grain_llm_client.rs` owns transport;
  `grain_llm_fallback.rs` and Agent try enabled, configured providers in saved
  order when Fallback is on. Rotation, scoring, quotas and the routing crate
  are retired. Preserve the settings/transport seams in
  [LLM-FALLBACK.md](LLM-FALLBACK.md). Speech-to-text is local only; see
  [LOCAL-TRANSCRIPTION.md](LOCAL-TRANSCRIPTION.md).
- **Recording pills**: compiled `handy/overlay.rs` owns WebView window lifecycle
  and platform handling. `grain_overlay`/`grain_capture` adapt Grain capture
  paths; `src/app/overlay` owns appearance and waveform response. Upstream
  frontend files remain frozen; review behavioral changes against the owned UI.
  The [maintenance contract](OVERLAY-MAINTENANCE.md) maps readiness, streaming,
  settings, app icons, Agent capture and retired surfaces to their active owners.
  The policy gate protects module/entry wiring; runtime-port review includes
  recorder/audio-manager changes, even when the shared merge is clean.
- **Rolling transcription**: `rolling.rs` + RCSR seam revision — no upstream
  counterpart; be careful when upstream touches chunking in
  `handy/managers/transcription.rs`.
- **Grain-only subsystems**: Grain Space, context awareness,
  snippets/"scrap that", prompt record, agent. Agent automatic text-context
  modes are retired; preserve active shared readers as recorded in
  [AGENT-TEXT-CONTEXT.md](AGENT-TEXT-CONTEXT.md).
- **CI**: Grain ships its own workflows; upstream workflow changes are
  ignored via `merge=ours`.
- **`tailwind.config.js`**: converged 2026-07-17 (deleted, matching upstream).

When you make a new deliberate deviation, add it to the divergence map in the
same commit.

## Sync log

### November 2 catalogue checkpoint

The onboarding stage through `0e5e9ca9` (#2303) preserves Grain's existing
design, adds durable final-step completion through the owned Core store,
adopts native rate progress and regional Chinese output, and retains the
shared silence status/microphone-settings improvements. Excludes the replacement
Try-it wizard, OS-language JS injection and upgrade guesses. Windows checks
pass 712 backend tests, 245 Core tests and 102 frontend tests; frontend build,
binding export and all five upstream gates pass. The measured baseline is 39
files. Native UI/macOS acceptance remains explicit in `SYNC-2026-11-02.md`;
audio-file import is the next and last approved feature block, Settings-only.

Integrated #2299 through `93c9ee05` on `sync/handy-audit-20261102`, retaining
Grain UI and the current paired native fork. The shared models 0.2.2 snapshot
contains the same ASR entries as fetched Handy main's 0.2.3; language detection
remains held. Package parity, Rust compilation, 704 backend tests (23 opt-in
skips), 16 final-snapshot catalogue tests, 102 frontend tests, 28 upstream
process tests and all five preflight gates pass. The existing Windows Cargo
manifest runner is required. Regenerated the existing Nix derivative/hash;
Linux Nix evaluation, native macOS/Linux execution and real model inference
are not claimed. The staged baseline is 38 reviewed diverged files. Seven
newer Handy commits remain outside ancestry; see `SYNC-2026-11-02.md`.

The same branch separately adapts #2336 Debian OpenSSL packaging and #2340
Intel Mac CPU-only features, retaining Apple Silicon Metal and the paired
fork. Both native layers' target features resolve correctly for both Mac and
both Windows architectures; 11 contract tests and the Windows Rust check
pass. Windows #2326 has no matching GradeChip surface in Grain. Final
post-platform preflight passes all five gates. Native Mac/Linux acceptance
remains pending; these two adapted ports do not close out intervening commits.

The next staged merge integrates #2302 through `5a9451e7`: shared clipboard
transactions stay serialized and macOS input injection stays on the main
thread; Standard, Flow and Streaming wait through Grain's existing blocking
pool instead of sleeping on the UI thread. Preserve the options/Agent/Paste
Catch hooks and session-tagged completion, with tray/terminal-error guards.
Windows Rust check and 704 backend tests pass (23 opt-in skips), as do all
five final preflight gates. Desktop paste responsiveness/restoration and
macOS native acceptance remain in the sync plan. Six newer commits remain
outside ancestry; the reviewed baseline stays at 38 files.

### October 17 full integration

Merged the complete 34-commit range through `9e57a0d7` on
`sync/handy-2026-10-17` (merge `1cf50ec1`); target behind count is zero.
Native fork 0.4.0 `517885822a5f42562972796c53ae496a52c143b6` passes its
Windows/Linux/macOS CI. Grain's worker/capture/chunking/dictionary paths and
final frontend benefit review are integrated. Windows checks pass 599 backend,
359 workspace and 117 frontend tests; explicit process/streaming/Flow/VAD gates
and installer extraction/runtime checks also pass. All 10 frontend and 13
suppressed-path reviews are recorded, mapped ports pass, and the divergence
ratchet is remeasured at 37 files. See `SYNC-2026-10-17.md` for scope and limits.
Main remains unchanged pending the required real-application UI acceptance.
Older deferred platform/lifecycle rows remain explicit; zero behind is not a
claim that those separate behaviors are complete. Entries below describe earlier
checkpoints and retain their then-current pending counts/pins.

October 17 batch 2: merged five Handy commits through `29bd2c0d`; 28 remain.
The immutable native pair is now 0.4.0 at `51788582`, contract 3. Adopted filler
capitalization, shortcut parse validation, shared missing-model capture guards
and lazy startup device reporting. The source-built backend suite passes 585
tests (3 opt-in skips); the preceding native checkpoint also passes real v2/v3
production journal/replay gates. Exact scope, lint limits and reviewed
new-base budget differences are recorded in the active plan and divergence map.

October 17 staged sync, batch 1: merged Handy `8ef8dd43` (#2156), preserving
Ha Long Bay/Ha Noi during English filler cleanup. `cargo check --lib` passes;
582 backend tests pass, with 3 existing opt-in tests ignored, after the
documented disposable Windows test-manifest workaround. Port audit,
frontend review, policy, divergence audit and post-merge ratchet pass; no budget
growth was accepted. The pre-merge ratchet saw the incoming upstream line
against the old base; that difference disappears after the real ancestry merge.
The native pin remains 0.2.3. There are 33 pending Handy commits; see the active
execution plan for subsequent native/capture and final frontend stages.

| Date          | Upstream ref                       | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-04-11    | `0392b7b`                          | Import baseline (grafted 2026-07-16 as `33638cc`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-07-16    | 10 cherry-picks through `b00ae666` | Mic-init caches, settings salvage (reimplemented in grain-core), cancel-stalled-output (+ new cancel-generation infra), ampersands, hf-hub pin, auto timestamps (batch), tray state, 3 frontend fixes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-07-17    | `438582fc`, `f1359706`, `5a7c0eac` | X11 push-to-talk deferral; vsredist app-local bundling; tauri 2.10.2 → 2.11.5 (cjpais runtime fork dropped for a tao rev pin). **Backlog zero through v0.9.3.**                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-07-17    | `v0.9.3` closed out                | Merge base advanced via `git merge -s ours v0.9.3` (tree unchanged); trial-merge conflicts 57 → **0**.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-07-19/20 | —                                  | **Handy Isolation phases 1-7**: audio chain re-baselined onto upstream text; inert files; Grain code extracted to `grain_*` modules; divergence ratchet CI; folder move to `src/handy/` (R100 renames; merge mapping verified with simulated upstream commits). Divergence 5561 → ~3580 lines / 26 files. Three upstreamable fixes catalogued.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 2026-07-20    | infra                              | This architecture: auto-sync PRs, shared rerere cache, stray-file guard, `Upstream/` as the single home for all sync machinery.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-07-22    | tracker repairs                    | Ledger was losing commits: one 30-commit API page (no paging) and a **subject** dedup key that swallowed upstream's repeated subjects. Re-keyed on SHA + PR number; two commits it had dropped (#1529, #1447) recovered, and commits already in our ancestry now pre-file as `Merged` instead of padding the review queue. Dashboard opened to an error off the filesystem (`fetch()` is blocked on `file://`) — it now falls back to a generated `data.js`, and shows behind-count / conflicts / drift from `status.json`. Ratchet was red on `main` (extension-platform `[GRAIN]` hooks landed unbudgeted), re-baselined — while red it also gated the auto-sync PR.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-07-20    | `cdbc2239` closed out              | #1697/#1701/#1708/#1709 were applied by cherry-pick, so git still counted them unmerged — the cause of the recurring `es/translation.json` conflict. Content verified (3 files byte-identical; the 4 keys the Spanish pick dropped belong to upstream's replaced model-list UI and are referenced nowhere in Grain), then recorded with `merge -s ours`. Trial merge: 1 conflict / 4 behind → **clean / 0 behind**. Ancestry-drift detection added so this cannot recur silently.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-07-31    | frontend frozen                    | `src/` left the shared-code category ahead of the UI 2.0 rewrite: `src/** merge=ours` + `frontend_freeze.py` (census + sync purity), because `merge=ours` only wins conflicts — a clean upstream edit to an untouched file, or a new upstream file, would otherwise land silently. Measured first: 22 upstream commits, 17 backend, 7 frontend, **4 frontend outside i18n** (`86616891` updater overlay and `cdf5028b` Sidebar restructure are both moot for Grain; `46d6a2ae` Vulkan gating and `ea3c20a3` `zh-Hant` resolution are backend facts written in TSX). Baseline: 140 files still shared, target 0 at cutover. Backend intake unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-07-28    | tracker rebuilt                    | Three faults, one cause — the tracker's data was _committed_. (1) The site never refreshed from CI: the sync job pushed with `GITHUB_TOKEN`, and GitHub does not fire `on: push` workflows for those pushes, so the separate `deploy-pages.yml` never ran. (2) Verdicts hand-typed into the generated `data.json` left `data.js` holding an older answer; the page silently preferred whichever it could read (two rows were drifted when this was found). (3) `checked_at` changed every run, so a `chore: sync upstream commits` commit landed on `main` every 2 hours carrying no information — ~60 of them. Fix: derived files (`data.json`, `data.js`, `status.json`, `merge-report.md`) are gitignored build products, rebuilt each run as a pure function of upstream's commits + our ancestry + the new hand-owned `verdicts.json`, and deployed to Pages _by the same job_. `verdict.py` replaces hand-editing; ancestry alone files a merged commit as `Merged`. Also: dropped the dashboard's PAT-in-`localStorage` trigger button for a link to the workflow, added a "Built <ago>" stamp linking to the run, and CI now `rerere_cache.py save`s the resolutions it learns instead of discarding them. Verified: regenerated ledger matches the last committed one on all 83 rows bar 4 stale hand-typed dates and one PR number (`#1261` → `#1310`, the PR that actually landed it).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-02    | `ea3c20a3` (22 commits)            | Backlog to **zero**. Eight conflicts; the rest merged clean. Grain and upstream had independently written the same #1639 symlink-pruning in `build.rs`, so ours was dropped for upstream's — **62 → 12 lines** of divergence. Kept #1731's Vulkan host gating in `get_available_accelerators`, dropping only the ORT branch (no engine behind it since the ONNX removal). `tray_i18n.rs` **converged to 0**: it was byte-identical already, and merging finally recorded the ancestry, so the recurring delete/modify conflict is gone. Upstream's new `managers/model/download.rs` landed at the src root (new dir ⇒ no directory-rename detection) and was `git mv`'d into `handy/`. Frontend arrivals (Sidebar, UpdateChecker, i18n, en+da) landed at `src/`, which we vacated in the `src/app` move, so they appeared as deletable additions instead of silently overwriting ours. 387 Rust tests pass.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-08-02    | `76736d5a` (16 commits)            | **The 08-02 sync above was wrong.** It reported "0 behind" against an `upstream/main` last fetched on 07-28, so these 16 commits — already upstream at the time — were invisible to the merge, the ratchet and the audit. The CI dashboard fetches every run and correctly showed 16 pending / 11 trial conflicts; the disagreement looked like a broken dashboard. Fix: `upstream_ref.py` fetches before anything measures, from `ratchet.py`, `audit_divergence.py` and `sync_upstream.py`. 11 conflicts: 6 frozen-frontend (auto), 5 real. Took `secure_input` (verbatim), `paste_tx`, `autostart` (SMAppService) — all three `git mv`'d out of the src root. Kept BOTH suspend/resume APIs (upstream's `_all_shortcuts` for `handy_keys.rs`, Grain's per-binding for the UI); kept Grain's single tray icon while adopting the warning badge; dropped `should_use_streaming_overlay` (no `OverlayStyle` in Grain). `handy-keys` 0.3.2→0.3.3 applied by hand — it sat outside the conflict and would have been lost. **Caught two silent adoptions**: directory-rename detection routed 5 new upstream frontend files into `src/app/` and applied upstream's slider-reset edits to 3 moved files plus 2 translation files with no conflict at all. The freeze census cannot see these (it matches upstream's `src/` paths, not `src/app/`); only the type-checker did. `merge_upstream.py` now reverts that class automatically. 398 Rust + 258 crate + 69 frontend tests pass.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-08-08    | `db003f38` (v0.9.5, 19 commits)    | Backlog to **zero**. Five real conflicts, the rest frozen-frontend/`merge=ours`. Kept BOTH sides in `recorder.rs`/`audio.rs`: Grain's rolling hooks (`sample_cb`/`conditioning`/`recorded_len`/`prompt_mark`) plus upstream's #1254 `selected_channel` and #1716 lock-free `recording_active` (all state writes now route through `set_state()`). #1873 idle-skip adopted by gating the resampler in `if recording` around Grain's conditioning closure. #1846's `memory.rs` (glibc allocator tuning) `git mv`'d into `handy/` (upstream's sibling `mod overlay;` dropped — Grain aliases `grain_overlay`), and its `trim_freed_memory()` `FinishGuard::drop` hook **ported by hand** (the diverged `actions.rs` didn't auto-merge it). #1254's `selected_channel` setting ported into `crates/grain-core` (inert `settings.rs`); upstream's `ChannelSelector.tsx` dropped, the command kept + registered. #2211's `stream: false` threaded into `grain_llm_client` (landed inert upstream). #1823 (reqwest source-chain diagnostics) initially deferred, then **adopted on re-evaluation**: Grain's `grain_llm_client` swallowed transport causes the same way, so the helper was ported into a shared payload-safe `net_diag.rs` and applied to BOTH cloud clients — including the Grain-only `stt_client` (5 send sites), which the merge can never reach. Only #1865 js-yaml stays deferred (dev-only transitive dep of `@eslint/eslintrc`, `merge=ours` lockfile, zero runtime exposure). Caught the freeze edge case `merge_upstream.py` misses: 3 NEW upstream frontend files (`ErrorBoundary`, `ChannelSelector`, `compat.ts`) that rename-detection routed into `src/app/` as adds — the script's `checkout HEAD` no-ops on a file absent from HEAD, so they were `git rm`'d by hand. `cargo check --lib` clean (only upstream's own `Emitter` warning), `tsc` clean, ratchet + audit + freeze green.                                                                                                                                                                                     |
| 2026-08-04    | `b1b2d9f9` (2 commits)             | Clean, **behind 0**. #1838 microphone-stream recovery (`audio_toolkit/audio/recorder.rs`, `managers/audio.rs`) + #1847 reliable paste (`paste_tx/windows.rs`) — all Handy-tree, mapped into `handy/` with no strays and no frontend adoption. Budgets re-baselined; the bump also folds in Grain's own `lib.rs` growth this session (`disable_drag_drop_handler` on the main window so in-app note drag-and-drop reaches the DOM, and the `grain_space_delete_folder` registration). Exported the five 08-02 real-conflict rerere resolutions that had been recorded locally but never shared, and reconciled `bun.lock` (vitest / vite-plugin-singlefile were declared in `package.json` but unrecorded). 399 Rust lib tests pass; `tsc` clean; ratchet green.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-08-10    | dashboard retired                  | The GitHub Pages sync dashboard was removed: chronically stale (the API-scraping ledger step gated the deploy, so any hiccup froze the site) and fully redundant with the CI job summary, the auto-sync PR, and `preflight.py`. Deleted `index.html`; stripped the Pages permissions/environment/concurrency + the three deploy steps and the `status.json` block from `upstream-sync.yml`; trimmed `sync_upstream.py` to emit only `data.json` (still verdict.py's input + the drift source), dropping `data.js`/`status.json`. All sync intelligence — trial merge, ledger, ancestry-drift gate, auto-PR, ratchet, port audit, verdicts, rerere — is unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-10    | `b50b52a8` (10 commits)            | Backlog to **zero**. Five conflicts: two frozen-frontend (auto), three real. The substantive one is **#1738 filler-word removal** — a real upstream STT-core feature, adopted end to end. Upstream reworked filler removal into a universal tier + language-gated tier keyed on `OutputLanguageEvidence` (user/model/text-detected), split `filter_transcription_output` into `remove_filler_words` + `normalize_transcription_output`, added a `filler_word_removal_enabled` master toggle, and `whatlang`/`isolang` text-LID (`lang_id.rs`). `text.rs` refactor merged clean; the setting was ported into **grain-core** `AppSettings` (inert `settings.rs`), the toggle command + registration kept (`change_vad_enabled_setting` stayed unregistered as before). Grain's diverged `transcribe()` (`with_engine_session`, transcribe-cpp only, `context_bias` initial_prompt) was **kept and extended** to resolve output-language evidence in-closure and thread it + the model language list into `finalize_batch_text`; the relocated `grain_text.rs` (`finalize_transcript`/`finalize_batch_text`) was rewritten onto the new API, and the rolling + cloud callers now key filler removal on the transcription language (`selected_language`), not the UI language — the exact bug #1738 fixes. Native streaming finalize threads upstream's new `FinalizedStreamText` (clean merge; Grain never diverged `StreamCmd`). #1548 compressed API responses (reqwest `gzip`/`brotli`/`deflate`, **kept** Grain's `multipart`). #1866/#1756 Linux paste fixes and #1700 Wayland overlay merged (overlay.rs inert — Grain's pill is native, no port; verdict-noted). **#1659 theme dropped on resolution**: Grain removed the backend theme command long ago (frontend owns theming, pill is native), so `change_theme_setting`/`apply_window_theme` + the startup call are not re-added. `cargo check --lib` clean (only upstream's own `secure_input` `Emitter` warning), `cargo test --lib` **426 passed**, grain-core **73 passed**, `tsc` clean, ratchet + audit + freeze + port-audit green. |
| 2026-08-15    | `98a4d80c` (6 commits)             | Backlog to **zero**. Two frozen Handy web-overlay conflicts were auto-dropped; one real `managers/audio.rs` conflict was resolved per policy. Adopted #37a26fd6's Secure Input/tray rebuild race fix (including atomic template-icon updates), #1908 portable `HF_HOME` under `Data/huggingface`, #1911 layout-aware macOS Cmd+V key resolution (active keyboard layout with ANSI-V fallback), and #1902's pre-init `GGML_METAL_NO_RESIDENCY=1` shutdown workaround (`HANDY_METAL_RESIDENCY=1` opts back in). #1907's Turkish semantic corrections were **manually ported** into Grain's owned locale schema with Grain branding. #549cbde3's webview arming visualization and compiled first-sample recorder API were **not adopted**: Grain's overlay is the native pill, and the upstream API would cut across batch, rolling, Agent, onboarding, and extension capture paths; the inert `handy/overlay.rs` was nevertheless taken verbatim. Decision recorded in `verdicts.json` and the per-file map. `cargo check --lib` passed (only upstream's documented Windows `Emitter` warning); `cargo test --lib` **510 passed, 1 ignored**; `bun install` unchanged; `tsc --noEmit` clean; ratchet + audit + freeze + port-audit green.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 2026-08-22    | `0e503672` (10 commits)            | Audited every Handy commit after 2026-08-15 and closed ancestry to **zero behind**. Fixed four missed runtime/product changes: #1874 race-safe default-microphone fallback + UI refresh; #1406 phrase support across all Grain dictionary entry points; #1817/#1892 explicit xdotool modifier keyup cleanup; handy-keys 0.3.4. Completed #1924 beyond the earlier partial port by taking the generated catalog/architecture changes verbatim while preserving the exact 0.2.0 vendor baseline and isolated rolling/TDT patch contract. Adapted three suppressed docs fixes (Bluetooth input tradeoff, Apple-only fn/Globe limitation, stale macOS Accessibility grants). The new frontend/suppressed gates require problem, applicability, Grain destinations, and evidence and include upstream merge commits; d55ea7ef's conflict-marker accident is valid only with repair merge 0e503672. `cargo test --lib`: **622 passed, 1 ignored**; grain-core **111 passed**; frontend **81 passed**; TypeScript, policy, semantic audits, ancestry, and ratchet green.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-09-23    | `8f9cf53c` (42 commits)            | Bulk merge driver advanced ancestry to zero behind. It auto-resolved 14 frozen-frontend conflicts and reverted 13 routed UI changes; ten backend/product conflicts were resolved under the divergence policy. Ported model-file recovery, empty-recording unload, transcript-log redaction, Norwegian language alias, compound-key names, Wayland paste fixes, Nix updater guard, and Windows Vulkan default. Frontend, suppressed-path, relocation, divergence, policy, and ratchet gates pass after reviewing and locking 26 intentional temporary divergence increases. Seven larger tray/capture/shortcut/platform ports remain explicitly deferred in [REVIEW-2026-09-23.md](REVIEW-2026-09-23.md); zero behind does not imply feature parity. Bun production build and eight focused frontend tests passed; Rust check stopped in the vendored transcribe.cpp CMake build before Grain code compiled.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

### 2026-11-02 staged completion

On branch `sync/handy-audit-20261102`, onboarding #2303 and Settings-only
file transcription #2315 are integrated after the catalogue, platform and paste
checkpoints. See [SYNC-2026-11-02.md](SYNC-2026-11-02.md) for exact scope,
exclusions, 721 backend / 245 Core / 102 frontend test evidence, five passing
boundary gates, and native desktop acceptance still required. No release or
main merge. Language detection and older deferred ports remain held; the user's
upcoming single dictation model selection is a separate product change.

### 2026-11-02 deferred follow-up ports

Adopted shortcut busy-input parity and hold-or-toggle (#1910/#1971), unchanged
Carbon shadow preservation (#2002), and pre-run macOS Accessory launch (#2003).
These commits were already in ancestry: this is a runtime adaptation, not a new
upstream merge or a close-out of held features. Shared coordinator/sequence tests
are canonical, with small Grain effect adapters and a structured relocation rule.
754 backend / 245 Core / 102 frontend tests and the frontend build pass. Native
macOS and the short new real-app shortcut checks remain pending; the exact
procedures are in SYNC-2026-11-02.md. Language detection and tray serialization
remain deferred. No release or main merge; hero edits remain local.
