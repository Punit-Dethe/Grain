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
| `grain_space_decay_half_life_days` | Recall recency-decay tuning (`grain_space/recall.rs`). Has a sensible default; exposing a half-life in days asks the user a question they cannot answer. Revisit only if recall ranking needs field tuning.                                                                                                                                                                                                  |
| `selected_model`                   | **Reachable, just not as a setting** — triaged 2026-08-15. Not dead and not upstream-only: it is the live Batch/Rolling model id (`rolling.rs`, `lib.rs` preload, `actions.rs` language resolution). Its control is the Model Library, which writes it through the `set_active_model` command rather than the generic settings updater — and a command writer is what the gate cannot see. Nothing to build. |

## Gaps — no UI today, and that is not deliberate

Not exceptions so much as a to-do list the gate keeps honest. Kept here so the
gate is green; move them out when UI 2.0 gives them a home.

| Field                 | Reason                                                                                                                                                                                                                                   |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `custom_filler_words` | A real user-facing override — it replaces Grain's built-in filler-word list in `finalize_transcript` — with no control anywhere. It predates the gate. **Candidate to surface in UI 2.0** next to Custom Words, which already has a row. |

### Revealed 2026-08-15 — triaged

These six are not judgements. They surfaced the moment `ui_parity.py`'s
generated-bindings exclusion was fixed: it compared a tree-relative name against
a `src/`-relative path, so after the UI 2.0 move to `src/app/` it stopped
matching and `bindings.ts` — which names every field — was admitted as evidence.
Every field looked reachable because its own type declaration counted as a UI
control. Rows are here to keep the gate green while it is honest again; each one
still needs a real decision.

| Field | Reason |
| ----- | ------ |

Triaged 2026-08-15. Two left the list: `paste_catch_enabled` already had a
control (`OutputPane.tsx`) and the row was simply wrong — the gate said so in
its own output; `selected_model` is reachable through the Model Library and
moved to _Deliberately not surfaced_ above. The four below are real, ranked by
what a user loses today.

| Field                         | Reason                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `filler_word_removal_enabled` | **Most urgent of the four.** The master switch for filler-word removal, default ON, live in `finalize_transcript` — so Grain is editing every transcript (dropping "um", "uh", …) with no way to stop it. Its companion `custom_filler_words` is the word list, equally unreachable, so the whole feature is off-limits rather than just the tuning. One toggle plus a word list, next to Custom Words which already has both. Build them together. |
| `paste_catch_hold_ms`         | How long a caught transcript stays on the clipboard (default 20s). Lower urgency than it looks: the feature's own switch IS surfaced, and this is the tuning knob behind it. Ship the default; surface only if 20s proves wrong in use.                                                                                                                                                                                                             |
| `selected_channel`            | Which input channel to record from a multi-channel microphone (`None` = average all). Live — `managers/audio.rs` applies it to the real recorder, and `set_selected_channel` exists. Matters only to people on interfaces or stereo inputs where speech is on one channel; for everyone else the default is right. A picker belongs beside the Microphone selector when someone asks for it.                                                        |
| `reliable_paste`              | Upstream's receipt-sequenced paste: restore the clipboard once the target app has actually read the transcript, instead of after a fixed delay. Defaults OFF and its own doc calls it debug-gated beta. **Decide, do not surface**: either finish it and replace `paste_delay_ms`, or pin it off and stop carrying the branch in `clipboard.rs`.                                                                                                    |
