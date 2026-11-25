# Upstream/

Documentation for keeping Grain in sync with its upstream,
[Handy](https://github.com/cjpais/handy). Nothing here is shipped with the
app — this folder is maintainer infrastructure only. The tooling is
[tether](https://github.com/Punit-Dethe/tether); its policy and records live
in [`.tether/`](../.tether/).

**Start with [UPSTREAM.md](UPSTREAM.md).** It is the runbook: how updates
flow in, what tether enforces, and the procedures for the common case
(reviewing the `sync/auto-upstream` PR) and the rare one (driving a
conflicted sync).

## Documentation

| File                                             | Purpose                                                                             |
| ------------------------------------------------ | ----------------------------------------------------------------------------------- |
| [UPSTREAM.md](UPSTREAM.md)                       | The runbook. Single source of truth for upstream syncs.                             |
| [UPSTREAM-DIVERGENCE.md](UPSTREAM-DIVERGENCE.md) | Per-file map of deliberate Grain/Handy divergence; which side wins each conflict.   |
| [DEFERRED-PORTS.md](DEFERRED-PORTS.md)           | The queue of Handy fixes Grain has reviewed but not ported yet.                     |
| [OVERLAY-MAINTENANCE.md](OVERLAY-MAINTENANCE.md) | The recording overlay/capture contract every sync must respect.                     |
| [UPSTREAMABLE.md](UPSTREAMABLE.md)               | Grain-architecture fixes that must NOT go upstream, plus candidate extension hooks. |

## Policy and records (`.tether/`)

| File                         | Purpose                                                                                                     |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `.tether/config.toml`        | The policy: moved folders, owned areas, review areas, relocations (upstream file → Grain files), settings. |
| `.tether/budgets.toml`       | Divergence budgets per Handy-derived file, and the accepted count of unmarked changes.                     |
| `.tether/decisions/<sha>.toml` | One record per upstream commit that needed a decision: ignored, port evidence, reviews, notes.          |
| `.tether/rr-cache/`          | Shared conflict resolutions; tether restores and saves them around each sync.                              |
| `.tether/AGENT.md`           | The loop an AI agent follows to drive tether.                                                              |

Grain's own seams with the Handy tree (alias coverage, overlay wiring, the
transcribe.cpp pin) are checked by `scripts/grain_seams.py`.

## Setup

No per-clone git config is needed: tether applies the merge settings and the
shared rerere cache itself. Install it once:

```bash
cargo install --locked --git https://github.com/Punit-Dethe/tether tether
tether doctor
```
