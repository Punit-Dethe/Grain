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

### Added in session 2 (gap check)

| Unit | Tier | Files | Notes |
| --- | --- | --- | --- |
| M7 Agent per-call confirmation | 2 | `action_exec.rs` `Dispatch`, `run_or_confirm(_opt)`, `PendingCalls::insert`, `MAX_PENDING`, `execute_revalidated`, `confirm_interaction`, `to_agent_confirm`, test-only `run_agent_call`; `agent.rs` pending-action run control (`publish_pending`, `take_pending`, `clear_pending_action`, `AgentConfirm`, `confirm_action`), `resume`, `discard`, `CONFIRM_TTL_MS`, `agent_confirm_action` command; frontend `AgentPanel.tsx` confirm card | The Agent auto-approves enabled tools (`run_agent_calls`), and nothing in production inserts a pending call, so the confirm card cannot appear. Removing it touches the Agent panel UI; needs user approval. |
| Prompt priority leftovers | 3 | `toggle_seq`/`builtin_toggle_seq` in grain-core (contributed-layer and shortcut arbitration order), `Contributions`/`ContributedLayer` in `context_detect::compose_prompt`, `prompt_contributions` (M3) | `toggle_seq` also orders enabled cards on the Extensions page (`sortExtensionCards`), so keep or replace that ordering deliberately. Grain's own priority line (spoken instruction > context profile > main prompt) is core and stays. |

Corrections to the session 1 inventory: `Submission` in registry tools is not
dead (`publish` parses `submission.toml` with it). `extension_settings_sections`
is still called by `ExtensionsPage` (`refresh`), so it belongs to M6, not L3.
`grain_embed.rs:367` still mentions `match.semantic`; fix it with M1.

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

**31 October 2026, session 2 (Windows machine with Rust).**

- Toolchain: cargo/rustc 1.96 (MSVC), VS 18 BuildTools. The app builds into
  `C:/gt` (gitignored `src-tauri/.cargo/config.toml`, short path for
  ggml-vulkan); the workspace builds into `C:\t`.
- **App unit tests need a comctl32 v6 manifest on Windows.** The `cargo test`
  executable imports `TaskDialogIndirect` but has no embedded manifest, so
  Windows refuses to start it with `STATUS_ENTRYPOINT_NOT_FOUND`
  (`0xc0000139`). That is the failure recorded in
  `Upstream/REVIEW-2026-09-23.md`. Local workaround (not committed): copy the
  test exe out of `deps`, place a side-by-side `.exe.manifest` that depends on
  `Microsoft.Windows.Common-Controls` 6.0.0.0, and run it with `C:\gt\debug`
  on `PATH`. A permanent fix would embed that manifest into test builds.
- Baseline on `4ca98265`: workspace `check --all-targets` and `test` pass;
  workspace `clippy --all-targets -D warnings` fails on two pre-existing lints
  in `grain-sdk/src/manifest.rs` test code (3194, 3242); app `check` passes;
  app `clippy --all-targets` exits 0 with 74 lib / 104 lib-test warnings; app
  lib tests 771 pass / 23 ignored; `cargo fmt --check` has one pre-existing
  diff (`grain_audio_journal.rs:144`); `Upstream/ratchet.py` already fails
  (`lib.rs` 1302 → 1376 and `Cargo.toml` 142 → 156 lines vs merge base). The
  frontend matched session 1.
- Done, each verified and pushed:
  - **L8** `9a5cd01c`: lib.rs comments only.
  - **L5** `a681e3e4`: five always-false guards, the test-only recommendation,
    activation, hand-off and session-stage helpers and their 10 tests. App
    tests 771 → 761; Clippy lib-test warnings 104 → 103.
  - **L1** `c3b60c0a`: `extension_companion.rs`, `grain_process.rs`,
    `RuntimeKind`, companion branches in spawn/kill/detach,
    `companion_gave_up`, the unused `spawn_worker` activation argument, and
    the load-unpacked Native arm (now refused by `validate_dev`; the refusal
    test also covers a missing binary). App tests 761 → 759; Clippy warnings
    72 lib / 101 lib-test; no new fmt diffs; `lib.rs` shrinks.
- New finding (not in the inventory): `extension_icons::materialize_pack`
  still writes a 64² `pill.rgba` for every install, but nothing reads it since
  recommendation delivery was retired. Removing it is a small Tier 1 follow-up.

**Next:** L2 → L3 → L7 → L4 → L6.

**31 October 2026, session 2 continued: batch 1 (rest of Tier 1).** The user
asked for batches with one real-app test pass per batch, and for full
retirement rather than leaving machinery in place.

- Commits: L2 `9fc06d06` (Recommendation Lab), L3 `c9e68d62` (five caller-less
  stub commands), L7 `c190e2bb` (registry `retire`), L4 `3eb5d5f6` (retired
  host API arms, document store, settings namespace, open/app approval, the
  extension LLM completions and the Windows UIA whole-window text reader), L6
  `d4407140` (slot takeover dialog, `extension_take_slot`, dead frontend
  helpers, retired capability labels), `3c4b7c66` (unused `pill.rgba`),
  `e148a877` (`layer_matches` and `ActiveContext.field`).
- Deliberate behaviour choice: uninstall purge removes only
  `<id>.storage.json`; old `<id>.docs` directories are no longer touched
  (clean-break policy).
- Verification after the batch: workspace tests pass; `grain-registry-tools`
  strict Clippy (`--no-deps`) clean; workspace strict Clippy still fails only
  on pre-existing lints in untouched crates (`grain-extension-checks`
  `listing.rs:33,146`, `grain-sdk` `manifest.rs:3194,3242`); app Clippy exit 0
  with 69 lib / 99 lib-test warnings (baseline 74/104); app lib tests 748 pass /
  23 ignored (each drop accounted for by removed tests); vitest 19 files / 115
  tests (11 removed tests); tsc pass; eslint only the pre-existing error;
  parity OK; no new rustfmt diffs; `src-tauri/src/handy/` and
  `src-tauri/Cargo.toml` untouched; `lib.rs` 1584 → 1571 lines.
- Tooling notes: regenerate bindings with `C:\t\logs\regen-bindings.ps1`
  (local, uncommitted). It runs the export test through the comctl32 manifest
  workaround, strips trailing whitespace, and restores 7 doc lines that
  `agent.rs` carries double-encoded so they keep their committed spelling.
  `agent.rs` itself has that encoding damage; fixing it is a separate cleanup.
  `prettier --check` warnings on CRLF working copies are line-ending noise
  (`.prettierrc` is `endOfLine: lf`); the LF-normalised files pass.

**Next: batch 2 (Tier 2),** ordered lowest risk first: M3 no-op pipeline stubs
→ M5 headless `--eval` → M4 startup migrations → M2 sessions and contributed
shortcuts (`handy/` hooks, run `Upstream/ratchet.py`) → M1 Extension Mode →
M6 settings contribution UI → M7 Agent confirmation. M6 and M7 change visible
UI and need the user's decision first.
