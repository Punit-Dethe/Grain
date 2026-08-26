# UI parity exceptions

Settings that `scripts/ui_parity.py` cannot reach from the UI tree, with the
reason each one is allowed to stay unreachable. **A field only belongs here once
someone has decided it should be unreachable** — that is the whole point of the
gate: losing a setting becomes a decision that got written down, not an accident
nobody noticed for three months.

Format matters: the script parses rows of `| `field` | reason |`.

## Quick Panel deletion — audited 2026-07-31, clear to delete

The plan required this before the Quick Panel (11 files, ~90 KB) could be
removed. Measured two ways across its files vs the rest of `src/`:

| Check                                              | Result                     |
| -------------------------------------------------- | -------------------------- |
| Backend commands / raw invokes / stores it reaches | **0** reachable only there |
| Settings keys it reads or writes (10 in total)     | **0** reachable only there |

So it is a second face on controls that all exist elsewhere — deleting it drops
no capability. Re-run before the deletion commit if it has been touched since:

```bash
python scripts/ui_parity.py            # settings still reachable
python scripts/ui_parity.py --commands # what nothing calls any more
```

## Backend-only — internal bookkeeping, never user-facing

| Field                           | Reason                                                                                                          |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `extensions_imported_v1`        | One-shot migration flag (`grain-core/context.rs`): marks that the bundled packs were imported. Nothing to show. |
| `settings_schema_version`       | Internal settings-file format version used to run migrations after upgrades; never user-configurable.           |
| `post_process_quota_reset_date` | Local date the post-process daily quotas last rolled over; the router resets lazily at routing time.            |
| `stt_quota_reset_date`          | Same, for the STT pool.                                                                                         |

## Write-only by design

| Field                   | Reason                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `stt_api_keys`          | Key material, write-only by construction. Written through `stt_upsert_provider`'s `apiKey` argument; `get_app_settings` no longer serializes either key map to the renderer at all (`grain_settings::get_settings_for_renderer`), so the UI asks `providers_with_keys` for the only thing it may know — _which_ providers have a key. `post_process_api_keys` gets the same treatment; the legacy panel that read it in plaintext is deleted. PLAN.md §6.2 closed. |
| `post_process_api_keys` | Same write-only key-presence contract as `stt_api_keys`; values are written through `pp_upsert_provider` and never returned to the renderer.                                                                                                                                                                                                                                                                                                                       |

## Deliberately not surfaced

| Field                              | Reason                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `auto_send_disabled`               | **Reachable, just not as a raw setting** — the deny-list stays backend-owned so the UI cannot widen an author's eligibility. Each eligible extension exposes an “Allow Auto-send” switch in Extensions, which calls `change_auto_send_for_extension`; its effective state comes from `ExtensionCard.auto_send_enabled`. |
| `custom_filler_words`              | Handy keeps this power-user override backend-only and exposes only the master filler-removal toggle. Grain mirrors that product boundary instead of inventing a fork-only editor.                                                                                                                                                                                                                            |
| `grain_space_decay_half_life_days` | Recall recency-decay tuning (`grain_space/recall.rs`). Has a sensible default; exposing a half-life in days asks the user a question they cannot answer. Revisit only if recall ranking needs field tuning.                                                                                                                                                                                                  |
| `paste_catch_hold_ms`              | Internal expiry for Recover Missed Text Insertion. The feature switch is surfaced; its 20-second safety window is implementation tuning, not a second user preference.                                                                                                                                                                                                                                       |
| `selected_model`                   | **Reachable, just not as a setting** — triaged 2026-08-15. Not dead and not upstream-only: it is the live Batch/Rolling model id (`rolling.rs`, `lib.rs` preload, `actions.rs` language resolution). Its control is the Model Library, which writes it through the `set_active_model` command rather than the generic settings updater — and a command writer is what the gate cannot see. Nothing to build. |
