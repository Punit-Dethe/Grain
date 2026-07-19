# Upstream (Handy) — how updates flow into Grain

Grain is a friendly fork of [Handy](https://github.com/cjpais/handy). Handy's
battle-tested STT core lives **verbatim** in `src-tauri/src/handy/`; everything
Grain is lives outside it. This document is the single source of truth for how
upstream updates are absorbed. The per-file conflict policy lives in
[UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md); fixes we should send *to*
Handy live in [UPSTREAMABLE.md](UPSTREAMABLE.md).

## The short version

**The common case is reviewing a PR, not running a procedure.** Every 2 hours
CI trial-merges `upstream/main`. If Handy moved and the merge is clean, CI
opens (or refreshes) the **`sync/auto-upstream` PR** with the merge already
done, the commit list in the body, and the divergence ratchet already run.
Review it, record verdicts in the ledger, merge it. Done.

Only a *conflicted* merge needs a human driver — and
[merge-report.md](merge-report.md) will have told you the exact conflicting
files up to 2 hours in advance, with
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
                       modules, agent, bridge, rolling, routers, grain_space…
crates/                Grain crates (grain-core, grain-pill, provider-router,
                       rolling-window).
```

Three files inside `handy/` are **inert** — byte-identical to upstream but
never compiled (no `mod` declaration): `settings.rs`, `llm_client.rs`,
`overlay.rs`. Grain's replacements are `grain_settings.rs`,
`grain_llm_client.rs`, `grain_overlay.rs`, aliased in `lib.rs` so
`crate::settings::…` etc. still resolve. Upstream edits to inert files merge
with **zero risk**.

Because git recorded the folder move as 100% renames, merges map upstream's
`src-tauri/src/X` into our `src-tauri/src/handy/X` automatically (verified
with simulated upstream commits, 2026-07-20). One caveat, guarded by the
ratchet: a **new** upstream file at the `src/` *root* lands at our root — the
root itself was never fully renamed, since `lib.rs`/`main.rs` and the Grain
modules live there — so `git mv` it into `handy/` and add its `#[path]`
declaration if it is a new module. If rename detection ever fails wholesale on
a huge upstream refactor, fall back to `git merge -s subtree` or map by hand.

### 2. Merge machinery — plain git, deliberately

- **Grafted ancestry** (`33638cc`, an `ours`-merge of upstream `0392b7b`):
  3-way merges work; upstream-only changes land automatically. The merge base
  advances only at release close-outs; it currently sits at **v0.9.3**.
- **`merge=ours` attributes** (`.gitattributes`): docs, workflows, identity
  configs, lockfiles auto-keep Grain's side.
- **rerere, shared**: every conflict resolution is recorded, and — via
  `Upstream/rr-cache/` + [rerere_cache.py](rerere_cache.py) — versioned in the
  repo, so CI and every clone replay past resolutions instead of re-resolving
  the same locale/Cargo.toml hunks each sync.
- **`merge.directoryRenames=true`**: maps new upstream files inside moved
  directories into `handy/` aggressively rather than conservatively.

One-time per clone:

```bash
git config merge.ours.driver true
git config rerere.enabled true
git config rerere.autoupdate true
git config merge.directoryRenames true
python Upstream/rerere_cache.py restore
```

We evaluated the heavier tools the ecosystem uses for this problem —
[Copybara](https://dagster.io/blog/monorepos-the-hub-and-spoke-model-and-copybara)
(explicit cross-repo transforms),
[josh](https://josh-project.github.io/josh/faq.html) (fast implicit tree
filtering, as adopted by
[Rust](https://blog.rust-lang.org/inside-rust/2026/06/04/how-josh-helps-rust-manage-code-across-multiple-repositories/)),
and `git subtree` — and rejected them: they solve *mapping a subtree across
repositories*. Grain doesn't have that problem, because `#[path]` kept
upstream's paths merge-compatible inside one shared history. Plain `git merge`
plus rename detection is simpler than any of them and loses nothing. GitHub's
own [friendly-fork guidance](https://github.blog/developer-skills/github/friend-zone-strategies-friendly-fork-management/)
reaches the same conclusion: frequent, small, scheduled merges beat clever
tooling.

### 3. Automation — CI does the waiting

[`upstream-sync.yml`](../.github/workflows/upstream-sync.yml), every 2 hours:

1. **Ledger**: [sync_upstream.py](sync_upstream.py) pulls new upstream commits
   into [data.json](data.json) as `Pending` — rendered by
   [index.html](index.html), the tracker dashboard.
2. **Trial merge** → [merge-report.md](merge-report.md): the next sync's
   conflict surface, always known in advance.
3. **Auto-PR**: clean merge + new commits → the `sync/auto-upstream` branch is
   (re)built, the ratchet runs against it, and a PR is opened/updated with the
   commit list and a review checklist. Conflicted merges never auto-open a PR.

[`divergence-ratchet.yml`](../.github/workflows/divergence-ratchet.yml) on
every push/PR touching `src-tauri/`: the boundary cannot silently erode.

### 4. Guards — the boundary is enforced, not hoped for

[ratchet.py](ratchet.py) + [budget.json](budget.json): every Handy-derived
file has a divergence budget (added+removed lines vs the merge base, measured
blob-to-blob so the folder move can't fool it). CI fails on: a grown budget, a
newly-diverged file, an outright-deleted upstream file, or a **stray**
upstream file sitting outside `handy/`. Budgets tighten via
`python Upstream/ratchet.py --update` — run it *after* committing (it
measures HEAD, not the working tree).

## Runbook

### A. The auto-PR is open (common case)

1. Read the PR body's commit list. For each commit, set a verdict in
   [data.json](data.json): `Merged`, or `Ignored` + a one-line note (Grain
   replaced that surface — the divergence map says where).
2. CI must be green (build, tests, ratchet). If the ratchet flags a stray
   file, `git mv` it into `handy/` on the branch.
3. Merge the PR with a **merge commit — never squash** (squashing discards
   the recorded ancestry and the next sync re-fights everything).

### B. The trial merge reports conflicts (rare case)

```bash
git fetch upstream --tags
python Upstream/rerere_cache.py restore
git checkout -b sync/vX.Y.Z
git merge vX.Y.Z            # or upstream/main; oldest release first
# resolve per UPSTREAM-DIVERGENCE.md — rerere replays known resolutions
python Upstream/rerere_cache.py save   # share NEW resolutions; commit them
python Upstream/ratchet.py             # strays + drift
bun install && cargo check             # regenerate lockfiles
# verify (below), record data.json verdicts, merge into main, push
```

If upstream changed code Grain relocated (settings → `crates/grain-core`,
post-processing → `grain_post_process.rs`, LLM client → `grain_llm_client.rs`,
final-text stage → `audio_toolkit/grain_text.rs`), **port the change into the
Grain location by hand** and say so in the commit body. The divergence map
lists every relocation.

### C. Closing out a release (do not skip)

Once every commit of a release has a verdict in `data.json` (zero `Pending`):

```bash
git merge -s ours vX.Y.Z              # tree untouched; ancestry says "assessed"
python Upstream/ratchet.py --update   # budgets re-baseline to the new merge base
```

Verify the tree is unchanged (`git diff HEAD~1 --stat` must be empty) and
commit the regenerated `budget.json` with the close-out. **Never** run
`-s ours` over commits you have not assessed — it silently locks their fixes
out forever, with no conflict to warn you. Cherry-picks record no ancestry
(measured 2026-07-17: 13 cherry-picks, conflict surface unchanged at 57) —
close-outs are what advance the merge base.

### Verification (every sync)

- Rust: `cargo check --lib` then `cargo test --lib` in `src-tauri/`
- Frontend: `./node_modules/.bin/tsc --noEmit`
- Boundary: `python Upstream/ratchet.py`
- Windows quirks on the primary dev machine: unset `LOCALAPPDATA` and `TEMP`,
  set `TMP=C:\Windows\Temp` (transcribe-cpp-sys junction workaround), and
  build with `CARGO_TARGET_DIR=C:\gtc` — the running Grain app locks the
  default target dir; NEVER kill the user's running app to free it.

## Deliberate deviations (do NOT "fix" these back to upstream)

- **Identity/rebrand**: `com.grain.app`; the Handy **auto-updater is fully
  removed** — never re-add its plugin, endpoint, or signing config.
- **Frontend/backend decoupling**: frontend→backend is Tauri commands only,
  backend→frontend is events only. The frontend must stay destroyable.
- **Multi-provider cloud STT + LLM**: `stt_router.rs` /
  `post_process_router.rs` / `grain_llm_client.rs` replace upstream's
  single-provider client.
- **Native pill UI**: the recording overlay is Grain's native `grain-pill`
  window (+ prompt switcher capsule, agent panel). Upstream's webview overlay
  files are inert; upstream `RecordingOverlay.*` (frontend) stays deleted.
- **Rolling transcription**: `rolling.rs` + RCSR seam revision — no upstream
  counterpart; be careful when upstream touches chunking in
  `handy/managers/transcription.rs`.
- **Grain-only subsystems**: Grain Space, context awareness, auto-dictionary,
  snippets/"scrap that", prompt record, agent, master-key chords.
- **CI**: Grain ships its own workflows; upstream workflow changes are
  ignored via `merge=ours`.
- **`tailwind.config.js`**: converged 2026-07-17 (deleted, matching upstream).

When you make a new deliberate deviation, add it to the divergence map in the
same commit.

## Sync log

| Date | Upstream ref | Notes |
|---|---|---|
| 2026-04-11 | `0392b7b` | Import baseline (grafted 2026-07-16 as `33638cc`) |
| 2026-07-16 | 10 cherry-picks through `b00ae666` | Mic-init caches, settings salvage (reimplemented in grain-core), cancel-stalled-output (+ new cancel-generation infra), ampersands, hf-hub pin, auto timestamps (batch), tray state, 3 frontend fixes. |
| 2026-07-17 | `438582fc`, `f1359706`, `5a7c0eac` | X11 push-to-talk deferral; vsredist app-local bundling; tauri 2.10.2 → 2.11.5 (cjpais runtime fork dropped for a tao rev pin). **Backlog zero through v0.9.3.** |
| 2026-07-17 | `v0.9.3` closed out | Merge base advanced via `git merge -s ours v0.9.3` (tree unchanged); trial-merge conflicts 57 → **0**. |
| 2026-07-19/20 | — | **Handy Isolation phases 1-7**: audio chain re-baselined onto upstream text; inert files; Grain code extracted to `grain_*` modules; divergence ratchet CI; folder move to `src/handy/` (R100 renames; merge mapping verified with simulated upstream commits). Divergence 5561 → ~3580 lines / 26 files. Three upstreamable fixes catalogued. |
| 2026-07-20 | infra | This architecture: auto-sync PRs, shared rerere cache, stray-file guard, `Upstream/` as the single home for all sync machinery. |
