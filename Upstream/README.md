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
| [merge-report.md](merge-report.md) | CI-generated: the next sync's conflict surface, known in advance. |

## Tooling

| File | Purpose |
|---|---|
| `sync_upstream.py` | Rebuilds the commit ledger (`data.json`/`data.js`) and audits ancestry drift. |
| `ratchet.py` | Divergence ratchet — enforces per-file line budgets vs the merge base. |
| `verdict.py` | Records human verdicts (`Ignored`, cherry-picks, notes) on upstream commits. |
| `rerere_cache.py` | Versions git's rerere cache through `rr-cache/` so resolutions replay everywhere. |
| `budget.json` | Ratchet budgets for every Handy-derived file. |
| `verdicts.json` | The one human-owned ledger input. |
| `index.html` | Sync dashboard (open it directly — works offline via `data.js`). |

## Generated (gitignored build products)

`data.json`, `data.js`, `status.json`, `merge-report.md` — recomputed every CI
run from upstream's commit list, our git ancestry, and `verdicts.json`. Never
edit by hand; regenerate with `python Upstream/sync_upstream.py`.

## One-time per-clone setup

```bash
git config merge.ours.driver true
git config rerere.enabled true
git config rerere.autoupdate true
git config merge.directoryRenames true
python Upstream/rerere_cache.py restore
```
