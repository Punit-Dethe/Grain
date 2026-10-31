# Legacy extension platform retirement: plan and continuity

Branch: `cleanup/retire-legacy-extension-platform`, created from `main` at
`1ae53597` (31 October 2026). Authority: the clean-break policy in `AGENTS.md`
and "B5c physical retirement — authorized" in
[MCP-EXTENSION-REUSE-EXECUTION-PLAN.md](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md).
No migration, tombstone or compatibility code is required for old extensions,
settings or registry data.

Goal: remove the machinery of the pre-tool-only platform (Extension Mode,
recommendation routing, companions, sessions, contributed shortcuts/settings/
slots/prompt layers, retired host APIs) without changing current behaviour.
Order: lowest severity first, one coherent commit per unit, verified before the
next unit.

## Must stay (live tool-only path)

- `extension-host.html`, `src/app/extension-host.ts`, `src/app/extension-runtime.ts`,
  `capabilities/extension-host.json`, Vite `extensionHost` input. This hidden
  webview **is** the native tool worker runtime.
- `extension_host.rs` worker lifecycle, native call admission, Capability Index V2
  (`collect_capability_actions`, directory/actions lookups), dev reload, quarantine
  on invalid artifacts.
- `host_api.rs` arms `log.*`, `storage.*`, `net.fetch`, `auth.*` and the
  `authorize` allowlist. `events_server.rs` / `events_auth.rs` (Worker and
  DevControl roles). `bridge.rs`.
- `context_screen.rs` (Agent screen image uses it; the `lib.rs` comment saying
  "no core caller" is wrong). `grain_dictation_routing.rs` (live dictation).
- `extension_icons.rs` (store install and overview cards), `dev_extensions.rs`
  scripted path, `grain-core` `interaction`/`execution`/`capability_*`.
- SDK/core security: `validate_tool_only` API + permission allowlist,
  `tool_permission_allowed`, ids/versions/icons/size limits, `AuthenticationDecl`
  (`deny_unknown_fields`), distribution/trust/revocation, registry review/signing.

## Inventory by severity

Reachability: (a) dead, (b) reachable only from retired entry points, (c) wired
into live paths but inert.

### Tier 1 — low (dead code, no live caller; removal cannot change behaviour)

| Unit | Files | Notes |
| --- | --- | --- |
| L1 Native companion runtime | `extension_companion.rs`; `grain_process.rs` (only companion uses it; its `lib.rs` comment is wrong); `extension_host.rs` `RuntimeKind::Companion`, `companion_launch` in `spawn_worker`, companion stop in `kill_worker_checked`, companion branch in `detach_connection`, `companion_gave_up`, `is_companion`, `detach_companion`, kind filters in `scripted_tokens`/`scripted_connected_tokens`, tests ~4464 and ~4674; `dev_extensions.rs` Native arm + `companion_path`; `lib.rs` `mod` line 47 and `stop_all` at 1382 | `validate_tool_only` rejects `Tier::Native` before any companion code runs. Keep dev test `native_companions_are_refused_even_in_developer_mode` passing (let `validate_dev` produce the error). |
| L2 Recommendation Lab | `extension_lab.rs`; `grain_commands.rs` `extension_recommendation_lab_install/remove`, `unload_recommendation_lab_records`, `lab_count` in developer status, `remove_materialized` in `extension_set_developer_mode`; `lib.rs` 50, 1498-1499; `tests/RECOMMENDATION_LAB.md` | No frontend caller. Check `DeveloperSection` for `lab_count` before dropping the field; regenerate bindings. |
| L3 Retired stub commands with no caller | `extension_shortcuts_status`, `extension_setting_set`, `extension_pick_app`, `extension_capture_app`, `extension_export_pack` (grain_commands.rs ~2151-2510); `lib.rs` handler list | Confirm `extension_import_pack` callers before touching it (sub-agents disagreed). |
| L4 Unreachable host API arms | `host_api.rs` `doc.*`, `settings.*`, `llm.complete`, `embed`, `match.*`, `capture.*`, `session.start`, `open.*` dispatch + `validate_request` arms, helpers (`param_match_*`, `param_scored_candidates`, `param_decide_policy`, `semantic_match`, `cosine`, `validate_open_url`, approved-app helpers, `ExtStorage::doc_*`/`settings_*`, `EMBED_MAX_BATCH`), their tests (`open_url_*`, `open_app_*`, document-store tests) | `authorize` refuses them first; keep `retired_host_methods_are_unreachable_even_with_old_or_all_grants`. Cascade: `grain_post_process::complete_for_extension(_with_image)`, `context_detect::read_window_text` (+ uia impl) lose their only caller. `extension_session::start` becomes unused (remove with Tier 2 sessions or now). `ExtStorage::purge` must still delete `.docs` dirs of old data, or drop that line deliberately. Do **not** touch the `net.fetch` `secret` option here (Tier 2). |
| L5 Test-only / always-false scaffolding | `extension_host.rs` `HAS_ACTIVATIONS`, `HAS_TRANSFORMS`, `HAS_PROMPT_LAYERS`, `HAS_CONTEXT_SLOT`, `HAS_MAIN_SLOT`; `#[cfg(test)]` `collect_recommendation`, `declared_event_variants`, `declares_transform`, `declares_startup`, `has_resident_grant`, `has_grant`, `parse_session_stage_output`, `validate_request_reply_shape`, `parse_handoff_outcome` and their tests (~3047-3300, 3453) | `HandOffOutcome`/`SessionStageOutput` stay until Tier 2 removes their callers. |
| L6 Frontend dead helpers | `src/app/extensions/extensionRuntime.ts` `matchToolRecommendations`, `studioShelfMode`, `promptLayerScope` (verify), retired `CAPABILITY_LABELS` entries, slot-conflict helpers (`SLOT_LABELS`, `SlotConflict`, `slotLabel`, `parseSlotConflict`); `ExtensionsPage.tsx` slot dialog/`takeOver`; Rust `extension_take_slot`; matching tests in `extensionRuntime.test.ts` | Slots are always empty for non-quarantined records, so `slotConflict` cannot be produced. |
| L7 Registry tools legacy signer | `crates/grain-registry-tools/src/main.rs` `Cmd::Retire`, `retire()`, unlock list entry (~661), dead `Submission` struct (~503); README line ~737 | Runbook already forbids `retire`; replacement is signed renewal/revocation. Keep `CheckSubmission`. |
| L8 Stale comments | `lib.rs` comments for `context_screen`, `events_server`, `grain_process`, `extension_icons` | Comment-only. |

### Tier 2 — medium (touches live start/cancel, startup or hook paths)

| Unit | Files | Required edits on live paths |
| --- | --- | --- |
| M1 Extension Mode | `grain_action_session.rs`, `extension_view.rs`, `extension_misroutes.rs`, `grain_action_log.rs`; `grain_actions.rs` `ExtensionModeAction`, `extension_mode_started`, ACTION_MAP entry; commands `grain_action_listen`, `grain_action_log`, `grain_extension_mode_*`, `extension_view_*`, auto-send commands; `grain_events::ExtensionRecommendation`; `extension_host` V1 pool (`recommend`, `searchable_*`, `auto_send_eligible`, `recommendation_display`, `RECOMMEND_VECTORS`, `semantic_scores`, `collect_actions`, `Index.actions/recommendations/auto_send_eligible`, `action_vocabulary`, `hand_off`, `HandOffOutcome`); frontend `extension-view.html/.tsx/-model(.test).ts/.css`, `capabilities/extension-view.json`, Vite input, `tests/agent-harness/stamp.mjs:63`; ExtensionsPage auto-send row; `settingsStore` `auto_send_enabled` updater; core settings `auto_send_*`, default `extension_mode` binding, `capture::EXTENSION_MODE_AVAILABLE` / `shortcut_is_withheld` | `grain_actions.rs:98` drop `supersede`; `cancel_session` must emit `SessionCancelled` unconditionally; delete `extension_view::*` calls in `extension_host` kill/stop paths; remove `context_bias` `ACTION_TERMS` from `take_terms`; remove `grain_embed` Extension-Mode warmth from the reaper (keep Agent panel check). Check `handy/` hooks that call `shortcut_is_withheld`. |
| M2 Extension sessions + contributed shortcuts | `extension_session.rs`, `extension_shortcuts.rs`, `grain_actions::extension_session_started`, `grain_post_process::run_extension_session_stage`, `extension_host` `run_session_stage`/`wake_for_session`/`cancel_session_stage`/`wake_for_shortcut`/`SessionStageOutput`, `sync` call in `refresh_index` (re-reads every manifest), `forget` in uninstall | `[GRAIN]` hooks in `handy/shortcut/handler.rs` 60-64, 81; `handy/shortcut/mod.rs` 249, 277, 353, 412, 458, 477, 489; `handy/commands/mod.rs` 191. Hook-only edits; run `Upstream/ratchet.py`. |
| M3 No-op pipeline stubs | `extension_host::run_transforms` (hook `handy/actions.rs:255`), `prompt_contributions` (`grain_post_process.rs:135`) | Then prompt-stack contribution plumbing becomes removable (Tier 3). |
| M4 Startup migrations | `reconcile_builtin_packs` archive/retire steps, `RETIRED_BUILTINS`, `retire_builtin_packs`; core `archive_retired_*`, `retire_extension_*`, `finish_tool_only_migration`, `tool_only_migration_version`, `heal_slots`, `RETIRED_AGENT_CENTER_VARIANT_ID`, `load(_settings_file_preexisted)` | Keep validate-and-quarantine of invalid artifacts. |
| M5 Headless `--eval` | `grain_eval.rs`, `grain_recommend_eval.rs`, `grain_actions` submodules, `lib.rs` 140-147/990-992/1129-1133, `main.rs` pre-clap hook; core `eval.rs`, `recommend_eval.rs`, `recommend.rs` (`capability_index.rs` doc link) | After M1. |
| M6 Retired settings contribution UI | `ExtensionSettings.tsx` (null components) and mounts in `ExtensionsPage`, `PostProcessingSettings.tsx:17,402`, `CapturePane.tsx:11,26`; `ExtensionSettingsPage`, `extension-settings` route (`GrainApp.tsx`, `navigation.ts` + tests), destination routing, sections fetch; commands `extension_settings_schema/sections` and DTOs; `setting_decl`, `net.fetch` `secret` option, `resolve_net_secret`, extension secret storage | Check `card.has_detail`: if it can be true the "Open settings" button is visible and its removal needs user visual approval. Decide whether `net.fetch` should reject a `secret` param after removal (fail closed). |

### Tier 3 — high (schema and cross-crate contract changes)

- Manifest fields/types in `grain-sdk/manifest.rs`: `kind`, `recommend`, `autoSend`,
  `needs`, `activation`, `surfaces`, `slots`, `variant_slots`, `companion`,
  `contributes.settings/shortcuts/sessionMode/promptLayers`, `agent_rules`,
  `when`, `Tier::Pack/Native`, `PackPayloads.prompts/pill_theme`, settings schema
  module, `ANCHORS`, `KNOWN_SLOTS`, `extends()`, dead `validate_inner`/
  `validate_phase3` arms, `KNOWN_CAPABILITIES` retired entries. Replace tombstones
  with `deny_unknown_fields` (check `#[serde(flatten)]` in `ExtensionProjectManifest`).
  Ripples into core, CLI scaffold, extension-checks (`check_activations`,
  `check_recommendation`), registry tools, seeds/fixtures and the frontend
  approval parser (`promptLayers`, `recommendation`).
- `grain-core` slot registry (`slot_claims`, `take_slot`, `CORE_DEFAULT`, …) with
  callers in `agent.rs`, `context_detect.rs`, `grain_actions.rs`, `handy/overlay.rs`
  hook and the frontend; prompt-layer approvals/fingerprints; `recommend_approved`.
- `matching.rs` / `action_router.rs` once `match.*` and eval are gone.
- SDK `DaemonEvent::ExtensionRecommend(Clear)`, `RecommendCandidate`.
- Possibly unrelated old migrations (`migrate_overlay_settings`,
  `RETIRED_DEFAULT_PROMPT_IDS`, `load_secrets` legacy map, `extensions_imported_v1`):
  confirm scope with the user first.

## Verification requirements

Per unit: `cargo check --all-targets` and affected `cargo test` for `src-tauri`
and `crates/*`, strict Clippy, regenerate `src/app/bindings.ts` with the ignored
`binding_export::export_bindings` test after command changes, then
`node_modules/.bin/tsc --noEmit`, `npx vitest run`, `npx eslint src`,
`py scripts/ui_parity.py`, `Upstream/ratchet.py` for hook edits, and a scoped
diff review. Real-app smoke (Agent tool call, dictation start/cancel) after M1/M2.

## Continuity log

**31 October 2026, session 1 (analysis only, no code changed).**

- Created the branch from `main` `1ae53597` and inventoried the old machinery
  with four parallel code traces (backend Extension Mode, worker host, crates,
  frontend). The `code-review-graph` MCP tools were not available; tracing used
  grep and direct reads. Results are the tiers above.
- Frontend baseline on the untouched branch: `tsc --noEmit` pass; `vitest run`
  19 files / 126 tests pass; `eslint src` 1 pre-existing error
  (`settingsStore.test.ts` raw `invoke` import); `prettier --check src` reports
  210 pre-existing unformatted files; `py scripts/ui_parity.py` OK. Use
  `node_modules/.bin/tsc` (plain `npx tsc` resolves a wrong package) and `py`
  (`python` is a Store alias that fails). `bun install --frozen-lockfile` done.
- **Blocker: no Rust toolchain on this machine.** `cargo`/`rustc` are absent,
  Visual Studio Community 2026 (`M:\VSCODE PRODUCT`) has no C++ workload or
  Windows SDK, and the configured target dir `C:\t` does not exist. Tier 1 is
  mostly Rust, so it was not started blind.
- Verification options found:
  1. Docker Desktop works (12 CPUs, ~4 GB VM). A Linux container (rust 1.96 +
     webkit2gtk/appindicator/rsvg/alsa/ssl/gtk-layer-shell/openblas/x11 deps,
     cmake, `glslc`, `libvulkan-dev`) can run `cargo check`/`test`. Keep the
     target dir on a named volume. `transcribe-cpp-sys` builds C++ via CMake;
     `TRANSCRIBE_DIR=<prebuilt prefix>` skips the source build. Memory is tight.
  2. CI: `.github/workflows/test.yml` is red on `main` (copies a
     `src/managers/transcription_mock.rs` that no longer exists).
     `extension-cli.yml` runs on any branch push touching `grain-sdk`,
     `grain-ext-cli` or `grain-extension-checks`. `build-test.yml` with
     `github-extension-alpha: true` compiles the Windows app (~40 min, no tests).
  3. Install Rust plus the VS C++ workload locally (needs the user's consent and
     likely admin rights).
- Docker Desktop was started for this probe; stop it when not needed.

**Next session:** choose a verification route, then implement Tier 1 in order
L8 → L5 → L1 → L2 → L3 → L7 → L4 → L6, one commit each, verifying after each.
Report Tier 2/3 decisions (M6 visual change, `net.fetch` `secret` behaviour,
Tier 3 scope) to the user before starting them.
