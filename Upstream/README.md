# Upstream/

Tooling and documentation for keeping Grain in sync with its upstream,
[Handy](https://github.com/cjpais/handy). Nothing here is shipped with the
app — this folder is maintainer infrastructure only.

**Start with [UPSTREAM.md](UPSTREAM.md).** It is the runbook: how updates
flow in, the four-layer architecture (isolation, merge machinery, automation,
guards), and the procedures for the common case (reviewing the
`sync/auto-upstream` PR) and the rare one (driving a conflicted merge).

## Documentation

| File | Purpose |
|---|---|
| [UPSTREAM.md](UPSTREAM.md) | The runbook. Single source of truth for upstream syncs. |
| [UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md) | Per-file map of deliberate Grain/Handy divergence; which side wins each conflict. |
| [UPSTREAMABLE.md](UPSTREAMABLE.md) | Grain-architecture fixes that must NOT go upstream, plus candidate extension hooks. |
| `merge-report.md` | CI-generated (gitignored): the next sync's conflict surface, written into the CI job summary. |

## Tooling

| File | Purpose |
|---|---|
| `sync_upstream.py` | Rebuilds the commit ledger (`data.json`, verdict.py's input) and audits ancestry drift. |
| `ratchet.py` | Divergence ratchet — enforces per-file line budgets vs the merge base. |
| `port_audit.py` | Port audit — flags upstream commits that touched an inert/relocated/parallel file (per `relocations.json`), so a fix that merged into the wrong (upstream-shaped) file can't silently miss the Grain code that replaces it. The one check a diff structurally cannot be. |
| `relocations.json` | Machine-readable map of upstream files → the Grain files that must carry their fixes. The audit's input; keep in step with `UPSTREAM-DIVERGENCE.md`. |
| `frontend_freeze.py` | Frontend freeze guard — `src/` is Grain-owned; fails if upstream files land there. |
| `frontend_allow.json` | The still-shared frontend files. May shrink, never grow. |
| `verdict.py` | Records human verdicts (`Ignored`, cherry-picks, notes) on upstream commits. |
| `rerere_cache.py` | Versions git's rerere cache through `rr-cache/` so resolutions replay everywhere. |
| `budget.json` | Ratchet budgets for every Handy-derived file. |
| `verdicts.json` | The one human-owned ledger input. |

## Generated (gitignored build products)

`data.json` (the commit ledger) and `merge-report.md` — recomputed every CI run
from upstream's commit list, our git ancestry, and `verdicts.json`. Never edit
by hand; regenerate with `python Upstream/sync_upstream.py`. Sync health is
surfaced in the CI job summary, the auto-sync PR, and `python
Upstream/preflight.py` — the GitHub Pages dashboard was retired 2026-08-10.

## One-time per-clone setup

```bash
git config merge.ours.driver true
git config rerere.enabled true
git config rerere.autoupdate true
git config merge.directoryRenames true
python Upstream/rerere_cache.py restore
```
