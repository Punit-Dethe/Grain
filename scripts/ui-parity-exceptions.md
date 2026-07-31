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

| Check | Result |
|---|---|
| Backend commands / raw invokes / stores it reaches | **0** reachable only there |
| Settings keys it reads or writes (10 in total) | **0** reachable only there |

So it is a second face on controls that all exist elsewhere — deleting it drops
no capability. Re-run before the deletion commit if it has been touched since:

```bash
python scripts/ui_parity.py            # settings still reachable
python scripts/ui_parity.py --commands # what nothing calls any more
```

## Backend-only — internal bookkeeping, never user-facing

| Field | Reason |
|---|---|
| `extensions_imported_v1` | One-shot migration flag (`grain-core/context.rs`): marks that the bundled packs were imported. Nothing to show. |
| `post_process_quota_reset_date` | Local date the post-process daily quotas last rolled over; the router resets lazily at routing time. |
| `stt_quota_reset_date` | Same, for the STT pool. |
| `dictionary_candidates` | Auto-dictionary's pending learned respellings — the feature's own working state, surfaced (when it is surfaced) as suggestions, not as a settings field. |

## Write-only by design

| Field | Reason |
|---|---|
| `stt_api_keys` | Key material, write-only by construction. Written through `stt_upsert_provider`'s `apiKey` argument; `get_app_settings` no longer serializes either key map to the renderer at all (`grain_settings::get_settings_for_renderer`), so the UI asks `providers_with_keys` for the only thing it may know — *which* providers have a key. `post_process_api_keys` gets the same treatment; the legacy panel that read it in plaintext is deleted. PLAN.md §6.2 closed. |

## Deliberately not surfaced

| Field | Reason |
|---|---|
| `grain_space_decay_half_life_days` | Recall recency-decay tuning (`grain_space/recall.rs`). Has a sensible default; exposing a half-life in days asks the user a question they cannot answer. Revisit only if recall ranking needs field tuning. |

## Gaps — no UI today, and that is not deliberate

Not exceptions so much as a to-do list the gate keeps honest. Kept here so the
gate is green; move them out when UI 2.0 gives them a home.

| Field | Reason |
|---|---|
| `custom_filler_words` | A real user-facing override — it replaces Grain's built-in filler-word list in `finalize_transcript` — with no control anywhere. It predates the gate. **Candidate to surface in UI 2.0** next to Custom Words, which already has a row. |
