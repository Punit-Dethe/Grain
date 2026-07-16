# Upstream (Handy) Maintenance Guide

Grain is a heavily modified fork of [Handy](https://github.com/cjpais/handy).
We keep Handy's battle-tested STT core and build Grain's features on top.
This document is the single source of truth for how upstream updates are
absorbed and which deviations are deliberate.

## History model

Grain began as a **source download** of Handy (no shared git history). On
2026-07-16 we grafted a merge base: commit `33638cc` is an `ours`-merge of
upstream `0392b7b` (Handy as of 2026-04-11). Since then git can 3-way merge
upstream properly — upstream-only changes land automatically; only files
modified on both sides conflict.

## How to sync with upstream

Never rebase onto upstream. Never reset-and-reapply. **Merge release tags.**

```bash
git fetch upstream --tags
git checkout -b sync/vX.Y.Z
git merge vX.Y.Z          # one release tag at a time, oldest first
# resolve conflicts (see policy below)
bun install               # regenerate bun.lock
cargo check               # regenerate Cargo.lock files
# run the feature test checklist, then merge into main and push
```

Merging release-by-release keeps conflicts small and attributable. The CI
trial-merge report (`Upstream Tracking/merge-report.md`, refreshed every
2 hours) tells you in advance exactly which files will conflict.

### One-time per clone

The `merge=ours` rules in `.gitattributes` only work after:

```bash
git config merge.ours.driver true
git config rerere.enabled true      # record every conflict resolution…
git config rerere.autoupdate true   # …and auto-replay it on recurrence
```

`rerere` is the compounding win: a conflict resolved once (e.g. the same
locale or Cargo.toml hunk across successive syncs) never needs manual
resolution again on this clone.

## Runbook — for any maintainer, human or AI agent

Follow this sequence for every upstream item; no other context is required
beyond this file and [UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md).

1. **What's pending?** Read `Upstream Tracking/data.json` (`status:
   "Pending"`) and `Upstream Tracking/merge-report.md` (CI-generated, lists
   the files the next full merge would conflict on).
2. **Understand the commit.** `git show <sha>` on the upstream commit. Map
   each touched file through the divergence map. Decide: **Merge** (Grain
   benefits), **Ignore** (Grain replaced/removed that surface — record why),
   or **Defer** (needs its own session; record why).
3. **Apply.** `git cherry-pick -x <sha>` for single commits, oldest first;
   `git merge <release-tag>` for full syncs. Resolve conflicts per the
   divergence map. If the upstream change targets code Grain relocated
   (e.g. settings → grain-core), reimplement it in the Grain location and
   say so in the commit body.
4. **Verify.** Rust: `cargo check --lib` then `cargo test --lib` in
   `src-tauri`. Frontend: `./node_modules/.bin/tsc --noEmit`. Windows quirks
   on the primary dev machine: unset `LOCALAPPDATA` and `TEMP`, set
   `TMP=C:\Windows\Temp` (transcribe-cpp-sys junction workaround, see
   `scripts/run-tauri.ts`), and build with `CARGO_TARGET_DIR=C:\gtc` — the
   running Grain app locks the default target dir (`C:/gt`); NEVER kill the
   user's running app to free it.
5. **Record.** Update the entry in `data.json` (`status` + a one-line
   `notes` saying what was adapted). If the item created a new deliberate
   deviation, add it to the divergence map in the same commit.
6. **Commit and push.** One commit per upstream item, keeping the
   `(cherry picked from …)` line. The sync bot also pushes to `main` every
   2 hours — on a rejected push, `git pull --no-rebase` and push again.

Hard rules: never rebase or `git pull --rebase` (flattens the graft merge
`33638cc` and destroys the shared ancestry); never hand-edit
`src/bindings.ts` (generated); never re-add the Handy auto-updater or change
the `com.grain.app` identity.

## Conflict policy by area

| Area | Policy |
|---|---|
| `.gitattributes` `merge=ours` list (docs, workflows, tauri confs, lockfiles) | Auto-kept ours; regenerate lockfiles after the merge |
| `src-tauri/src/` STT core (`managers/`, `audio_toolkit/`, `transcription_coordinator.rs`) | **Prefer upstream**, then re-thread Grain hooks (rolling, router, pill events) |
| `src-tauri/src/` Grain modules (`rolling.rs`, `stt_router.rs`, `agent.rs`, `vault/`, `grain-*`) | Ours only — upstream has no counterpart |
| `src/` frontend | Grain modified (not rebuilt) Handy's UI — take upstream fixes where the component still exists, keep Grain styling/decoupling |
| `src/i18n/locales/` | Take upstream's new keys, keep Grain's rebranded strings ("Handy" → "Grain") |
| `src/bindings.ts` | Regenerate via specta/tauri-specta after the Rust side compiles, don't hand-merge |

## Deliberate deviations (do NOT "fix" these back to upstream)

- **Identity/rebrand**: `com.grain.app`; the Handy **auto-updater is fully
  removed** — never re-add its plugin, endpoint, or signing config.
- **Frontend/backend decoupling**: frontend→backend is Tauri commands only,
  backend→frontend is events only. The frontend must stay destroyable.
- **Multi-provider cloud STT**: `stt_router.rs` / `post_process_router.rs`
  replace upstream's single-provider client.
- **Native pill UI**: recording overlay is Grain's native `grain-pill`
  window (+ prompt switcher capsule, agent panel). Upstream overlay changes
  are usually irrelevant; upstream `RecordingOverlay.*` survives only as a
  fallback surface.
- **Rolling transcription**: `rolling.rs` + RCSR seam revision — no
  upstream counterpart; be careful when upstream touches chunking in
  `managers/transcription.rs`.
- **Grain-only subsystems**: Grain Space (vault, embeddings, notes editor,
  recall), context awareness, auto-dictionary, snippets/"scrap that",
  prompt record, agent. All Grain-owned.
- **CI**: Grain ships its own workflows (`grain-release.yml`, upstream
  sync); upstream workflow changes are ignored via `merge=ours`.
- **`tailwind.config.js`**: upstream deleted it (Tailwind v4 migration);
  Grain still uses it. Expect a modify/delete conflict → keep ours until
  Grain migrates.

When you make a new deliberate deviation, add it here in the same commit.

## Sync log

| Date | Upstream ref | Notes |
|---|---|---|
| 2026-04-11 | `0392b7b` | Import baseline (grafted 2026-07-16 as `33638cc`) |
| 2026-07-16 | 10 cherry-picks through `b00ae666` | Mic-init caches, settings salvage (reimplemented in grain-core), cancel-stalled-output (+ new cancel-generation infra), ampersands, hf-hub pin, auto timestamps (batch), tray state, 3 frontend fixes. Deferred: tauri bump (#1675), X11 push-to-talk (#1605), vsredist (#1577). |
