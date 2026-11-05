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

| `agent_input_type_to_expand` | Retained for future typed Agent input; production voice pills do not expand or expose this preference. Existing Agent result/conversation settings remain available. |

| Field                     | Reason                                                                                                                                                                |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `settings_schema_version` | Internal settings-file format version used to run migrations after upgrades; never user-configurable.                                                                 |
| `onboarding_completed`    | Internal first-run completion state, saved by the Finish action through `complete_onboarding`; it controls startup routing and is not a user-configurable preference. |

## Write-only by design

| Field                   | Reason                                                                                                                                                                   |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `post_process_api_keys` | Write-only key-presence contract; values are written through `pp_upsert_provider` and never returned to the renderer.                                                    |
| `mcp_oauth_client_ids`  | Managed through the Developer section's `mcp_set_client_credentials` command. The UI displays `mcp_provider_status.client_id_configured` and never reads the stored map. |

## Deliberately not surfaced

`post_process_enabled` remains readable for compatibility, but Grain's settings
load policy keeps it enabled. Its master toggle and Tauri command are removed;
capture policy controls when AI processing runs.

| Field                 | Reason                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `custom_filler_words` | Handy keeps this power-user override backend-only and exposes only the master filler-removal toggle. Grain mirrors that product boundary instead of inventing a fork-only editor.                                                                                                                                                                                                                            |
| `paste_catch_hold_ms` | Internal expiry for Recover Missed Text Insertion. The feature switch is surfaced; its 20-second safety window is implementation tuning, not a second user preference.                                                                                                                                                                                                                                       |
| `selected_model` | The single dictation model, selected by Handy's catalogue via `set_active_model` rather than a generic settings updater. Shared by streaming, batch and the reviewed Flow exception. |
