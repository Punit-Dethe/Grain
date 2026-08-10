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
Review it, merge it. Done — merged commits file themselves as `Merged` on the
dashboard, because being in our ancestry *is* the record.

Working locally? One command answers everything:

```bash
python Upstream/preflight.py     # fetches, then: behind-count, conflicts, all gates
```

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

[`upstream-sync.yml`](../.github/workflows/upstream-sync.yml), every 2 hours
(and on any push touching `Upstream/`):

1. **Trial merge** → [merge-report.md](merge-report.md): the next sync's
   conflict surface, always known in advance. Its machine-readable twin
   `status.json` (behind count, trial result, conflicting files, ancestry
   drift) is what puts sync health on the dashboard, so "are we keeping up?"
   never requires reading CI logs.
2. **Ledger**: [sync_upstream.py](sync_upstream.py) rebuilds `data.json` — the
   per-commit board [index.html](index.html) renders. It pages the API back to
   `TRACKING_FLOOR_TS` (a single page silently dropped commits whenever
   upstream landed a burst) and keys on **SHA**, because subjects repeat
   (`update catalog`, `bump tauri global shortcut`).
3. **Ancestry audit**: flags upstream commits that are already applied here but
   unrecorded (see D below) — and gates step 5 on it.
4. **Publish**: the dashboard is uploaded to GitHub Pages *in this same job*.
   It used to be a second, push-triggered workflow, which never fired: GitHub
   does not run `on: push` workflows for pushes made with `GITHUB_TOKEN`, so
   the site only refreshed when a human happened to push. Deploying inline
   removes the chain rather than patching it.
5. **Auto-PR**: clean merge + new commits + no ancestry drift → the
   `sync/auto-upstream` branch is (re)built, the ratchet must pass on the
   merged result, and a PR is opened/updated with the commit list and a review
   checklist. Conflicted merges, ratchet failures and drift all suppress the
   PR rather than producing a misleading one.

[`divergence-ratchet.yml`](../.github/workflows/divergence-ratchet.yml) on
every push/PR touching `src-tauri/`: the boundary cannot silently erode.

#### The ledger is derived — one file is yours

`data.json`, `data.js`, `status.json` and `merge-report.md` are **gitignored
build products**. Nothing commits them; every run recomputes them from three
inputs:

| Input | Answers |
| --- | --- |
| upstream's commit list | what happened |
| our git ancestry | what we absorbed — a commit reachable from `HEAD` is `Merged` |
| [verdicts.json](verdicts.json) | what only a human knows: `Ignored`, cherry-picks, notes |

So the output is a pure function of its inputs: idempotent, and impossible to
drift. That matters because it used to drift constantly — verdicts were typed
into the generated `data.json` while `data.js` kept an older answer, and a bot
commit landed on `main` every 2 hours carrying nothing but a new timestamp.

Record the rare verdict with the tool, never by hand:

```bash
python Upstream/verdict.py --pending                    # what still needs one
python Upstream/verdict.py <sha|#PR> Ignored "why"      # Grain replaced that surface
python Upstream/verdict.py <sha|#PR> --note "detail"    # just annotate
python Upstream/verdict.py <sha|#PR> --clear            # back to derived
```

It refuses to store a status ancestry already implies, so `verdicts.json` only
ever holds real decisions. **A merged sync needs no verdict at all.**

**The dashboard is a plain file — open `Upstream/index.html` and it works.**
Browsers forbid `fetch()` on a `file://` origin, so the page also ships a
generated `data.js` (ledger + status baked into a `<script>`) and falls back to
it, labelling itself `offline copy`. Both are written by the same call, so they
cannot disagree. Regenerate everything with `python Upstream/sync_upstream.py`.
The header shows when the page was **built**, linking to the run that built it
— a stale deploy is visible instead of looking like a quiet upstream.

### 4. Guards — the boundary is enforced, not hoped for

[ratchet.py](ratchet.py) + [budget.json](budget.json): every Handy-derived
file has a divergence budget (added+removed lines vs the merge base, measured
blob-to-blob so the folder move can't fool it). CI fails on: a grown budget, a
newly-diverged file, an outright-deleted upstream file, or a **stray**
upstream file sitting outside `handy/`. Budgets tighten via
`python Upstream/ratchet.py --update` — run it *after* committing (it
measures HEAD, not the working tree).

[port_audit.py](port_audit.py) + [relocations.json](relocations.json): the
ratchet and a diff both prove the *shared* `handy/` surface is on par with
Handy — but they are blind to the fixes that merge into an **inert** file
(`llm_client.rs`, `settings.rs`, `overlay.rs` — byte-identical to upstream but
uncompiled), into a file whose logic Grain **relocated** (`actions.rs` →
`grain_post_process.rs`), or that share a bug class with a Grain-only
**parallel** implementation (`stt_client`, the rolling engine). In all three
the upstream-shaped file matches perfectly while the code Grain actually runs
lacks the fix, and no diff spans the gap. The port audit reads the relocation
map and fails when a merged upstream commit touched a mapped file without a
[verdicts.json](verdicts.json) note recording where the fix landed (or why it
does not apply) — turning "did we forget to port it?" from unbounded worry into
a bounded, gated list. Wired into `preflight.py`; clear each finding with
`python Upstream/verdict.py <sha> --note "..."`.

## Runbook

### A. The auto-PR is open (common case)

1. Read the PR body's commit list. Anything Grain does **not** actually take
   needs saying so — `python Upstream/verdict.py <sha> Ignored "why"` (the
   divergence map says where Grain replaced that surface). Everything merged
   needs nothing: ancestry files it.
2. CI must be green (build, tests, ratchet). If the ratchet flags a stray
   file, `git mv` it into `handy/` on the branch.
3. Merge the PR with a **merge commit — never squash** (squashing discards
   the recorded ancestry, so the board would forget these commits were merged
   *and* the next sync re-fights them).

### B. The trial merge reports conflicts (rare case)

```bash
python Upstream/preflight.py            # fetches; says how far behind + what conflicts
python Upstream/rerere_cache.py restore
git checkout -b sync/upstream-YYYY-MM-DD
python Upstream/merge_upstream.py       # merges; auto-resolves the frozen frontend
# resolve ONLY what it lists, per UPSTREAM-DIVERGENCE.md
git commit --no-edit
python Upstream/merge_upstream.py --finish   # re-baselines budgets + runs every gate
python Upstream/rerere_cache.py save    # share NEW resolutions; commit them
bun install && cargo check              # regenerate lockfiles
python Upstream/verdict.py --pending    # anything left unassessed? record it
```

**Never run a bare `git merge upstream/main`.** The remote-tracking ref is a
local cache; merging against a stale one is how the first 2026-08-02 sync
"completed" while 16 commits it could not see stayed behind. Every tool here
now fetches first — `git merge` does not.

Three things `merge_upstream.py` does that hand-merging misses:

- **Frozen-frontend conflicts resolve themselves.** Upstream keeps developing
  its `src/`; we deleted it. Six of the eleven conflicts in the 08-02 sync were
  that, and the answer never varies.
- **Silent frontend adoption is reverted.** This is the one to understand.
  Git's directory-rename detection saw `src/` → `src/app/` and *helpfully*
  routes upstream's frontend work into Grain's tree — new files land under
  `src/app/`, and edits to files that moved are applied with **no conflict at
  all**. `frontend_freeze.py` cannot see either: it matches paths shared with
  upstream's `src/`, and `src/app/...` is not one. On 08-02 only the
  type-checker caught it.
- **Strays are surfaced.** New upstream modules land at the `src-tauri/src/`
  root and must be `git mv`'d into `handy/` with a `#[path]` declaration.

### B1. Before you commit anything in the Handy tree

```bash
python Upstream/ratchet.py --worktree
```

The budget is measured against `HEAD`, so the plain ratchet can only fail
*after* you commit — which then needs a second commit or an amend to fix.
`--worktree` measures the files on disk and answers before you commit.

If upstream changed code Grain relocated (settings → `crates/grain-core`,
post-processing → `grain_post_process.rs`, LLM client → `grain_llm_client.rs`,
final-text stage → `audio_toolkit/grain_text.rs`), **port the change into the
Grain location by hand** and say so in the commit body. The divergence map
lists every relocation.

### C. Closing out a release (do not skip)

Once every commit of a release has a verdict (`python Upstream/verdict.py
--pending` comes back empty for it):

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

### D. "We're N behind, but we already did those" — ancestry drift

The single most common way this fork's bookkeeping goes wrong. Symptoms:

- `git rev-list --count HEAD..upstream/main` stays stubbornly non-zero;
- the same file (historically `es/translation.json`) conflicts in *every*
  trial merge;
- the board says `Merged` for those very commits (a `verdicts.json` override
  said so, since ancestry could not).

**Cause:** the work was applied by cherry-pick or by hand. Git tracks
*ancestry*, not content — so the content is in the tree, the ledger is
correct, and git still believes we never took those commits. It therefore
replays them (and re-raises their conflicts) into every future merge, forever.
`git cherry` won't spot it either: an adapted cherry-pick has a different
patch-id, so it reports the commit as missing.

**Detection (automatic):** `python Upstream/sync_upstream.py` matches unmerged
upstream commits against our own subjects since the merge base and prints a
loud `ALREADY APPLIED` block. CI runs this every 2 hours, surfaces it in the
job summary, and — importantly — **suppresses the auto-sync PR** while drift
exists, because auto-merging in that state would replay resolved work.

**Fix:** verify the content really is present (spot-check the files each
commit touched), then record it:

```bash
git merge -s ours upstream/main      # tree untouched; ancestry says "assessed"
git diff HEAD~1 --stat               # MUST be empty
python Upstream/ratchet.py --update  # budgets re-baseline to the new base
```

Measured 2026-07-20: four i18n commits (#1697, #1701, #1708, #1709) sat
applied-but-unrecorded. Recording them took the trial merge from "1 conflict,
4 behind" to **clean, 0 behind** without changing a single line of code.

**Prevention:** prefer `git merge` over `git cherry-pick` for upstream work.
If you must cherry-pick (a single urgent fix), close it out afterwards.

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
- **Frontend freeze (2026-07-31)**: `src/` is Grain-owned — `merge=ours` plus
  [frontend_freeze.py](frontend_freeze.py), because `merge=ours` only wins
  conflicts. Everything genuinely valuable in upstream's frontend commits is
  *backend knowledge that happens to be written in TSX* (which accelerators this
  host can run, which locale a system tag resolves to). Port those into Rust —
  which we still merge in full — and the freeze costs nothing. See
  [`docs/UI 2.0/PLAN.md`](../docs/UI%202.0/PLAN.md).
- **Multi-provider cloud STT + LLM**: `stt_router.rs` /
  `post_process_router.rs` / `grain_llm_client.rs` replace upstream's
  single-provider client.
- **Native pill UI**: the recording overlay is Grain's native `grain-pill`
  window (+ prompt switcher capsule, agent panel). Upstream's webview overlay
  files are inert; upstream `RecordingOverlay.*` (frontend) stays deleted.
- **Rolling transcription**: `rolling.rs` + RCSR seam revision — no upstream
  counterpart; be careful when upstream touches chunking in
  `handy/managers/transcription.rs`.
- **Grain-only subsystems**: Grain Space, context awareness,
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
| 2026-07-22 | tracker repairs | Ledger was losing commits: one 30-commit API page (no paging) and a **subject** dedup key that swallowed upstream's repeated subjects. Re-keyed on SHA + PR number; two commits it had dropped (#1529, #1447) recovered, and commits already in our ancestry now pre-file as `Merged` instead of padding the review queue. Dashboard opened to an error off the filesystem (`fetch()` is blocked on `file://`) — it now falls back to a generated `data.js`, and shows behind-count / conflicts / drift from `status.json`. Ratchet was red on `main` (extension-platform `[GRAIN]` hooks landed unbudgeted), re-baselined — while red it also gated the auto-sync PR. |
| 2026-07-20 | `cdbc2239` closed out | #1697/#1701/#1708/#1709 were applied by cherry-pick, so git still counted them unmerged — the cause of the recurring `es/translation.json` conflict. Content verified (3 files byte-identical; the 4 keys the Spanish pick dropped belong to upstream's replaced model-list UI and are referenced nowhere in Grain), then recorded with `merge -s ours`. Trial merge: 1 conflict / 4 behind → **clean / 0 behind**. Ancestry-drift detection added so this cannot recur silently. |
| 2026-07-31 | frontend frozen | `src/` left the shared-code category ahead of the UI 2.0 rewrite: `src/** merge=ours` + `frontend_freeze.py` (census + sync purity), because `merge=ours` only wins conflicts — a clean upstream edit to an untouched file, or a new upstream file, would otherwise land silently. Measured first: 22 upstream commits, 17 backend, 7 frontend, **4 frontend outside i18n** (`86616891` updater overlay and `cdf5028b` Sidebar restructure are both moot for Grain; `46d6a2ae` Vulkan gating and `ea3c20a3` `zh-Hant` resolution are backend facts written in TSX). Baseline: 140 files still shared, target 0 at cutover. Backend intake unchanged. |
| 2026-07-28 | tracker rebuilt | Three faults, one cause — the tracker's data was *committed*. (1) The site never refreshed from CI: the sync job pushed with `GITHUB_TOKEN`, and GitHub does not fire `on: push` workflows for those pushes, so the separate `deploy-pages.yml` never ran. (2) Verdicts hand-typed into the generated `data.json` left `data.js` holding an older answer; the page silently preferred whichever it could read (two rows were drifted when this was found). (3) `checked_at` changed every run, so a `chore: sync upstream commits` commit landed on `main` every 2 hours carrying no information — ~60 of them. Fix: derived files (`data.json`, `data.js`, `status.json`, `merge-report.md`) are gitignored build products, rebuilt each run as a pure function of upstream's commits + our ancestry + the new hand-owned `verdicts.json`, and deployed to Pages *by the same job*. `verdict.py` replaces hand-editing; ancestry alone files a merged commit as `Merged`. Also: dropped the dashboard's PAT-in-`localStorage` trigger button for a link to the workflow, added a "Built <ago>" stamp linking to the run, and CI now `rerere_cache.py save`s the resolutions it learns instead of discarding them. Verified: regenerated ledger matches the last committed one on all 83 rows bar 4 stale hand-typed dates and one PR number (`#1261` → `#1310`, the PR that actually landed it). |
| 2026-08-02 | `ea3c20a3` (22 commits) | Backlog to **zero**. Eight conflicts; the rest merged clean. Grain and upstream had independently written the same #1639 symlink-pruning in `build.rs`, so ours was dropped for upstream's — **62 → 12 lines** of divergence. Kept #1731's Vulkan host gating in `get_available_accelerators`, dropping only the ORT branch (no engine behind it since the ONNX removal). `tray_i18n.rs` **converged to 0**: it was byte-identical already, and merging finally recorded the ancestry, so the recurring delete/modify conflict is gone. Upstream's new `managers/model/download.rs` landed at the src root (new dir ⇒ no directory-rename detection) and was `git mv`'d into `handy/`. Frontend arrivals (Sidebar, UpdateChecker, i18n, en+da) landed at `src/`, which we vacated in the `src/app` move, so they appeared as deletable additions instead of silently overwriting ours. 387 Rust tests pass. |
| 2026-08-02 | `76736d5a` (16 commits) | **The 08-02 sync above was wrong.** It reported "0 behind" against an `upstream/main` last fetched on 07-28, so these 16 commits — already upstream at the time — were invisible to the merge, the ratchet and the audit. The CI dashboard fetches every run and correctly showed 16 pending / 11 trial conflicts; the disagreement looked like a broken dashboard. Fix: `upstream_ref.py` fetches before anything measures, from `ratchet.py`, `audit_divergence.py` and `sync_upstream.py`. 11 conflicts: 6 frozen-frontend (auto), 5 real. Took `secure_input` (verbatim), `paste_tx`, `autostart` (SMAppService) — all three `git mv`'d out of the src root. Kept BOTH suspend/resume APIs (upstream's `_all_shortcuts` for `handy_keys.rs`, Grain's per-binding for the UI); kept Grain's single tray icon while adopting the warning badge; dropped `should_use_streaming_overlay` (no `OverlayStyle` in Grain). `handy-keys` 0.3.2→0.3.3 applied by hand — it sat outside the conflict and would have been lost. **Caught two silent adoptions**: directory-rename detection routed 5 new upstream frontend files into `src/app/` and applied upstream's slider-reset edits to 3 moved files plus 2 translation files with no conflict at all. The freeze census cannot see these (it matches upstream's `src/` paths, not `src/app/`); only the type-checker did. `merge_upstream.py` now reverts that class automatically. 398 Rust + 258 crate + 69 frontend tests pass. |
| 2026-08-08 | `db003f38` (v0.9.5, 19 commits) | Backlog to **zero**. Five real conflicts, the rest frozen-frontend/`merge=ours`. Kept BOTH sides in `recorder.rs`/`audio.rs`: Grain's rolling hooks (`sample_cb`/`conditioning`/`recorded_len`/`prompt_mark`) plus upstream's #1254 `selected_channel` and #1716 lock-free `recording_active` (all state writes now route through `set_state()`). #1873 idle-skip adopted by gating the resampler in `if recording` around Grain's conditioning closure. #1846's `memory.rs` (glibc allocator tuning) `git mv`'d into `handy/` (upstream's sibling `mod overlay;` dropped — Grain aliases `grain_overlay`), and its `trim_freed_memory()` `FinishGuard::drop` hook **ported by hand** (the diverged `actions.rs` didn't auto-merge it). #1254's `selected_channel` setting ported into `crates/grain-core` (inert `settings.rs`); upstream's `ChannelSelector.tsx` dropped, the command kept + registered. #2211's `stream: false` threaded into `grain_llm_client` (landed inert upstream). #1823 (reqwest source-chain diagnostics) initially deferred, then **adopted on re-evaluation**: Grain's `grain_llm_client` swallowed transport causes the same way, so the helper was ported into a shared payload-safe `net_diag.rs` and applied to BOTH cloud clients — including the Grain-only `stt_client` (5 send sites), which the merge can never reach. Only #1865 js-yaml stays deferred (dev-only transitive dep of `@eslint/eslintrc`, `merge=ours` lockfile, zero runtime exposure). Caught the freeze edge case `merge_upstream.py` misses: 3 NEW upstream frontend files (`ErrorBoundary`, `ChannelSelector`, `compat.ts`) that rename-detection routed into `src/app/` as adds — the script's `checkout HEAD` no-ops on a file absent from HEAD, so they were `git rm`'d by hand. `cargo check --lib` clean (only upstream's own `Emitter` warning), `tsc` clean, ratchet + audit + freeze green. |
| 2026-08-04 | `b1b2d9f9` (2 commits) | Clean, **behind 0**. #1838 microphone-stream recovery (`audio_toolkit/audio/recorder.rs`, `managers/audio.rs`) + #1847 reliable paste (`paste_tx/windows.rs`) — all Handy-tree, mapped into `handy/` with no strays and no frontend adoption. Budgets re-baselined; the bump also folds in Grain's own `lib.rs` growth this session (`disable_drag_drop_handler` on the main window so in-app note drag-and-drop reaches the DOM, and the `grain_space_delete_folder` registration). Exported the five 08-02 real-conflict rerere resolutions that had been recorded locally but never shared, and reconciled `bun.lock` (vitest / vite-plugin-singlefile were declared in `package.json` but unrecorded). 399 Rust lib tests pass; `tsc` clean; ratchet green. |
| 2026-08-10 | `b50b52a8` (10 commits) | Backlog to **zero**. Five conflicts: two frozen-frontend (auto), three real. The substantive one is **#1738 filler-word removal** — a real upstream STT-core feature, adopted end to end. Upstream reworked filler removal into a universal tier + language-gated tier keyed on `OutputLanguageEvidence` (user/model/text-detected), split `filter_transcription_output` into `remove_filler_words` + `normalize_transcription_output`, added a `filler_word_removal_enabled` master toggle, and `whatlang`/`isolang` text-LID (`lang_id.rs`). `text.rs` refactor merged clean; the setting was ported into **grain-core** `AppSettings` (inert `settings.rs`), the toggle command + registration kept (`change_vad_enabled_setting` stayed unregistered as before). Grain's diverged `transcribe()` (`with_engine_session`, transcribe-cpp only, `context_bias` initial_prompt) was **kept and extended** to resolve output-language evidence in-closure and thread it + the model language list into `finalize_batch_text`; the relocated `grain_text.rs` (`finalize_transcript`/`finalize_batch_text`) was rewritten onto the new API, and the rolling + cloud callers now key filler removal on the transcription language (`selected_language`), not the UI language — the exact bug #1738 fixes. Native streaming finalize threads upstream's new `FinalizedStreamText` (clean merge; Grain never diverged `StreamCmd`). #1548 compressed API responses (reqwest `gzip`/`brotli`/`deflate`, **kept** Grain's `multipart`). #1866/#1756 Linux paste fixes and #1700 Wayland overlay merged (overlay.rs inert — Grain's pill is native, no port; verdict-noted). **#1659 theme dropped on resolution**: Grain removed the backend theme command long ago (frontend owns theming, pill is native), so `change_theme_setting`/`apply_window_theme` + the startup call are not re-added. `cargo check --lib` clean (only upstream's own `secure_input` `Emitter` warning), `cargo test --lib` **426 passed**, grain-core **73 passed**, `tsc` clean, ratchet + audit + freeze + port-audit green. |
