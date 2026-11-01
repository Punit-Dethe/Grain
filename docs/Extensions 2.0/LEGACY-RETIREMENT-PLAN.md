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

## Current checkpoint — 1 November 2026

Implementation in scope is complete: Tier 1, M1–M6 and Tier 3, plus the
embedding, catalogue, API-admission and package-layout/archive follow-ups below.
The final stamped real-app checkpoint passed 11 cases; both injected negative
controls failed at the intended assertions, and all seven runs cleaned up.

Remaining acceptance: user visual confirmation of Settings → Application and
ordinary microphone/dictation start/cancel. These are not certified by the
extension-focused automated checkpoint. M7 remains explicitly excluded by the
user. Unrelated overlay/default-prompt/secrets migrations remain outside scope.
Existing listing-module Clippy, settingsStore.test.ts ESLint and Cargo.toml
ratchet failures remain; see the final continuity entry for exact checks/evidence.

The inventory below is the original audit; completion and corrected decisions
are recorded in the continuity log, including superseded earlier holds.

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

**31 October 2026, session 2 continued: batch 2 (Tier 2).**

- Commits: M3 `e024fab9` (prompt contributions, extension prompt tier,
  transform hook), M5 `1dc2496d` (headless `--eval`, core `eval`/
  `recommend_eval`/`matching`, `main.rs` back to upstream), M4 `a569ef39`
  (startup migrations, `tool_only_migration_version`, `extensions_imported_v1`,
  the ignored `ExtensionsRegistry::load` argument, prompt-pack apply/remove;
  harness `native.legacy-migration` became `native.legacy-quarantine`), M2
  `0307d8fb` (extension sessions and contributed shortcuts with all their
  `handy/` hooks), M1 `52acc2ed` (Extension Mode, V1 recommendation pool,
  grain-core `recommend`/`action_router`, embed warm witness, action
  vocabulary biasing, auto-send, `extension_mode` binding), M6 `3fb036db`
  (settings contribution UI, extension settings route, destination routing,
  `has_detail`, extension secret store).
- Decisions: settings load now drops a saved `extension_mode` binding through
  the existing retired-binding list, so no backend registers a dead chord.
  `cancel_session` always emits `SessionCancelled`. `net.fetch` refuses the
  retired `secret` parameter (fail closed); `auth: true` is the only
  host-held credential. Old `extension_secrets` entries are dropped from the
  credential file on its next write. The extension-owned prompt grouping in
  Post-processing is gone; the "Your prompts" group label stays. Kept as
  non-extension: `migrate_overlay_settings`, `RETIRED_DEFAULT_PROMPT_IDS`, the
  `load_secrets` legacy bare map, `settings_schema_version` migrations.
- Fix carried in M1: `Upstream/relocations.json` and its generated table still
  listed `extension_session.rs`/`grain_action_session.rs` after M2, so
  `Upstream/policy_check.py` failed. Both rows are removed; the check passes.
- **M7 dropped by the user.** Agent auto-approval and the dormant
  confirmation card stay as they are; do not touch them.
- New finding: after M1 nothing uses the embedding engine. The capability
  index never receives dense scores (`dense: None`), and Extension Mode was
  the only consumer. The ~130 MB model download in Settings now has no effect,
  and `grain_embed.rs` engine functions compile as dead code. Needs a user
  decision (remove the setting and engine, or wire the dense tier).
- Verification: app lib tests 699 pass / 23 ignored; grain-core 268; vitest 18
  files / 102 tests; tsc pass; eslint only the pre-existing error; parity,
  policy OK; ratchet fails only on the pre-existing `Cargo.toml` 142 → 156 and
  `lib.rs` 1302 → 1305 (was 1328); handy files shrink (`handler.rs` 40 → 12,
  `secure_input.rs` 216 → 212). Remaining app warnings are pre-existing
  (`action_exec` confirm path, clamshell, handy managers) plus the new
  `grain_embed` dead engine.

**Next: batch 3 (Tier 3)** without M7: SDK manifest fields and tombstones,
slot registry, prompt-layer approvals/fingerprints and `recommend_approved`,
`toggle_seq` prompt arbitration, SDK `ExtensionRecommend` events, card
`recommend`/`needs`/`kind`.

**31 October 2026, session 2 end: batch 3 (Tier 3) in progress.**

- User decisions this session: **M7 is dropped.** Leave Agent auto-approval,
  `action_exec` confirm code, `agent.rs` pending-action control and the
  AgentPanel confirm card exactly as they are. A cancelled sub-agent had
  rewritten them and gutted `tests/agent-harness/`; all of that was discarded
  (`git checkout`), nothing from it was committed. Do the rest of Tier 3.
- Batch 3 commits so far (local, pushed with this log):
  - `5a69272c` SDK: removed `DaemonEvent::ExtensionRecommend`/`Clear`,
    `RecommendCandidate`, and the `PillAction` chooser variants
    (`ExtensionChoose`/`Cancel`/`DownloadModel`). Wire is externally-tagged
    JSON, so no renumbering risk. grain-sdk tests 82 pass.
  - `743bb161` ExtensionCard lost `kind`, `recommend`, `needs`,
    `prompt_layers`, `slots` (+ `RecommendInfo`, `PromptLayerInfo`,
    `PackFacts` fields); the enable approval error lost `needsPromptLayers`/
    `needsRecommendation`; the TS `ApprovalRequest` parser and its tests
    followed. Bindings regenerated; tsc clean; vitest 101 pass.
- **Next unit (T5, planned, not started; tree clean at `743bb161`):**
  grain-core registry cleanup. Remove the built-in toggle order
  (`builtin_toggle_seq`, `touch_builtin_toggle`, `toggle_seq(id)`, no
  production callers), the slot registry (`CORE_DEFAULT`, `slot_claims`,
  `slot_occupant`, `slots_held`, `slot_conflict`, `SlotConflict`,
  `take_slot`, `release_slots_locked`, `set_slot_claim`, `heal_slots`, slot
  parts of `RecordEditSnapshot`, `ExtensionRecord.slots`, the `slots`
  parameter of `install::plan_record`), and the legacy approval fingerprints
  (`prompt_layers_approved`, `recommend_approved`,
  `prompt_layers_fingerprint`, `recommendation_fingerprint`,
  `ApprovalDigests.prompt_layers/recommend`). Keep `ExtensionRecord.toggle_seq`
  /`next_toggle_seq` (card sort), `actions_approved`,
  `authentication_*`, and do not change what `actions_fingerprint` hashes.
  Files: `crates/grain-core/src/extensions.rs`, `install.rs`,
  `src-tauri/src/extension_host.rs` (dev-reload `slots_changed`, record
  construction, `record_for` test helper), `grain_commands.rs` (record
  construction at three sites, the approve path setting the fields to
  `None`), plus the two test calls of `plan_record` in
  `src-tauri/src/grain_store.rs` (~1766, ~2098: drop the 4th `vec![]`).
  Keep `toggle_order_is_enable_order_and_reenable_moves_to_end` rewritten on
  `record(id).toggle_seq`. About 15 slot/prompt/recommend tests go
  (grain-core 268 → ~253). A full line-by-line plan is in the local file
  `C:\t\logs\t5-continuity.md` (not in the repo).
- **Remaining Tier 3 after T5, in order:**
  1. `grain-extension-checks` `check_recommendation`, `check_activations`
     (rewrite as an actions-required check); trim the `grain-ext-cli`
     scaffold (`kind`, `recommend`, `auto_send`, `needs`, `activation`,
     `surfaces`, `slots`, `variant_slots`, `settings`, `session_mode`,
     `companion`).
  2. `grain-registry-tools` `extends()` use (main.rs ~1020): it always yields
     `[]`; removing it changes the signed index `extends` metadata, so decide
     whether to drop the index field or keep it empty.
  3. SDK manifest fields and dead validators (`recommend`, `autoSend`,
     `needs`, `kind`, `activation`, `surfaces`, `slots`, `variant_slots`,
     `KNOWN_SLOTS`, settings/shortcuts/sessionMode with `SettingDecl`,
     `promptLayers`, `PackPayloads.prompts`/`pill_theme`,
     `settings_schema.rs`, `ANCHORS`, `extends()`, retired
     `KNOWN_CAPABILITIES`, `validate_phase3`/`validate_classification`/
     `validate_prompt_layers` arms). Then `companion` (update
     `capability_index.rs:985-989`), then `Tier::Pack`/`Native` (grain_commands
     tier strings, `IndexEntry.tier`, `validate_inner`). Do NOT add
     `deny_unknown_fields`: the store blob fixture
     (`src-tauri/tests/fixtures/store/blob/10046c61….grainpack`, hash-named)
     contains `activation`, the harness lifecycle fixture contains
     `kind`/`activation`/`surfaces`/`slots`/`variant_slots`/`when`, and
     `ExtensionProjectManifest` uses `#[serde(flatten)]`. Removed keys are
     simply ignored, so old artifacts still parse; the precise "Retired
     extension feature" errors for them disappear (accepted, clean break).
     Check `validate_prompt_contribution_text` and `settings_schema::Accepted`
     callers first. Regenerate the lifecycle fixture.
  4. Ask the user before `ActionDecl.when`/`agent_rules`/`LayerWhen`:
     dropping them from `actions_fingerprint` forces re-approval of every
     installed extension.
  5. Ask the user about the embedding model (no consumer since M1).
- Verification recipe is unchanged (see "Verification requirements" and the
  session 2 tooling notes). Local helper scripts live in `C:\t\logs\`
  (`edit.ps1`, `cut.ps1`, `regen-bindings.ps1`, `run-app-tests.ps1`).
- The user has not yet run the real-app checks for batch 2 or batch 3.

**1 November 2026, session 3: T5 registry retirement complete.**

- Removed the slot registry/API and claims from enable, replacement, developer
  override/unload, uninstall, quarantine and rollback; removed built-in toggle
  bookkeeping and prompt-layer/recommendation approval fields and fingerprints.
  `plan_record` no longer accepts slots; both store test callers were updated.
  No migration, tombstone or compatibility parameter was added.
- Retained installed-card `toggle_seq`/`next_toggle_seq`, action/authentication
  approvals and account ownership, revision/generation checks, bounded atomic
  persistence, refusal behavior and rollback. The action fingerprint algorithm is
  unchanged. Removed 15 obsolete tests and added one action/authentication
  independence regression: grain-core 268 -> 254 tests. M7 stays untouched.
- Verification: workspace check and tests pass; grain-core strict Clippy passes;
  normal app all-target and `agent-harness` feature checks pass; app Clippy exits
  zero (69 lib / 86 lib-test warnings, none in changed logic); app tests 699 pass /
  23 ignored; tsc, 101 Vitest tests, production frontend build, parity and upstream
  policy pass. Workspace strict Clippy still fails at the two existing SDK test
  lints; ESLint retains its existing settingsStore.test.ts raw-invoke error;
  ratchet retains the unchanged Cargo.toml 142 -> 156 and lib.rs 1302 -> 1305 failures.
- The maintained Windows test runner still exited `0xc0000139` here despite its
  embedded manifest. The previously recorded local copy/sidecar workaround in
  `C:\t\logs\run-app-tests.ps1` successfully ran the complete app tests.
- Adapted the maintained harness greeting to automatic Agent execution and reused
  its existing first-round-schema model adapter for registry recovery. Initial
  runs exposed the old confirmation-card and forced-discovery assumptions;
  neither was a product regression. No product Agent/confirmation UI changed.
  Runner self-tests: 80 pass. Final real-app registry checkpoint: 3/3 pass, with
  exact result/dispatch, restart/quarantine and real Windows save-lock assertions.
  The `lost-pointer` negative control failed at its expected pointer assertion.
  Both final positive and negative reports record cleanup Pass; ordinary profiles
  and credentials were not used. Evidence (ignored):
  `tests/agent-harness/.runs/retirement-t5-20261101/run-S7eAgU/evidence/report.md`
  and `run-VGGxyZ/evidence/report.md`; earlier failures remain preserved.
- This checkpoint accepts the affected registry/native-call boundary, not the
  entire batch 2/3 or ordinary microphone/dictation visual checks.

**Next:** remove recommendation/activation checks from the author checker, require
declared actions directly, then trim the SDK manifest and CLI scaffold together.
The index `extends` decision, action conditions/rules decision and unused embedding
model decision remain as listed above. Do not revisit M7.

**1 November 2026, session 3 continued: author checker retirement complete.**

- Removed recommendation advice, near-duplicate phrase scoring and activation
  event/capability/shortcut validation. `doctor` and packaging now require a
  declared action directly (`E_ACTIONS`); MCP descriptor checks are unchanged.
  Removed the now-unused warning severity/advice path; findings remain stable,
  deterministically sorted errors. Fixtures describe current tools only.
- Removed seven obsolete tests and added a missing-actions packaging refusal
  regression. Full workspace check/tests pass; checker tests 29 -> 23, CLI 23,
  registry tools 99. Clippy exits zero with only the two existing `listing.rs`
  warnings; strict Clippy fails at those unchanged lines (33, 146). Scoped diff
  review, rustfmt and whitespace checks pass. No runtime/UI changes in this unit.
- Next: SDK manifest fields and CLI scaffold together (the scaffold constructs
  `ExtensionManifest` directly, so its fields must change with that struct).
  Three remaining product decisions have been presented to the user; SDK edits
  independent of those decisions can proceed. No change to M7.

**1 November 2026, session 3 continued: SDK manifest/scaffold retirement.**

- Removed classification/recommend/auto-send/needs, activation, slots/variants,
  authored surfaces, contributed settings/shortcuts/sessions/prompt layers and
  prompt/theme payload fields, their declarations, obsolete validators and
  SDK exports. Deleted `settings_schema.rs` (no external consumer) and its 12
  tests. The CLI scaffold and lifecycle fixture use the narrower contract.
  Unknown manifest keys retain normal Serde behavior; no tombstones or new
  compatibility validators were added. Authentication still denies unknown keys.
- Removed `extends()` derivation from manifests and packages. Signed index
  metadata still emits its existing empty `extends` field pending the user's
  decision; there is no remaining host-position derivation or SDK stub.
- Core discovery found: `actions_from_manifest` still consumed old recommendation
  aliases/examples/entities in the live native tool index. Removed that legacy
  projection; native tools use the current extension name/description and action
  title/utterance/parameter metadata. Updated the real projection/retrieval test;
  the retriever itself is unchanged. Packages relying on retired recommendation
  metadata no longer get those extra discovery terms.
- Narrowed `KNOWN_CAPABILITIES` to storage/auth (bounded `net:<host>` remains
  separately validated). Removed `flagged.rs` and its eight tests: every warning
  watched only retired capture/transcript/native-executable permissions. Removed
  the unused StoreEntry flags DTO and the obsolete registry-site warning markup;
  no current Grain UI rendered that field. Regenerated bindings. Permission
  admission, action/auth approvals, artifact limits, trust and revocation remain.
- Verification: workspace all-target check/tests pass (core 254, SDK 44,
  checker/CLI 23 each, registry tools 99); SDK lost 38 tests net: 18 manifest,
  12 settings-schema, eight flags. Strict SDK/core/CLI/registry Clippy passes;
  fixed the existing SDK test's needless borrow while updating that module.
  The checker retains only its two existing listing.rs strict-Clippy failures.
  App all-target check, app tests 699 pass / 23 ignored, regenerated bindings,
  tsc, 101 Vitest tests, frontend build, parity/policy, rustfmt and diff checks
  pass. ESLint retains the existing raw-invoke test import failure.
- Rebuilt the real isolated host and repeated the registry/native-call checkpoint:
  3/3 Pass; lost-pointer negative control failed at the expected assertion;
  both cleanup Pass. Evidence (ignored):
  `tests/agent-harness/.runs/retirement-t7-20261101/run-Ene9Dx/evidence/report.md`
  and `run-Bd7RPb/evidence/report.md`. These are scoped registry/native-call
  results, not whole batch/dictation or live-provider certification.
- Remaining: companion declarations and Pack/Native tier branches; signed index
  `extends` contract; action `when`/`agentRules`/LayerWhen and approval hash;
  unused embedding model/engine. The last three choices remain pending. M7 stays.

**1 November 2026, session 3 continued: companion/runtime contracts retired.**

- Removed `CompanionDecl`, the manifest companion field, `Tier::Pack` and
  `Tier::Native`, `allow_native` validation and the runtime-kind helpers. The
  signed/current wire still declares `tier: "scripted"`; unsupported runtime
  names now fail parsing. Developer projects are always bounded JavaScript
  entries; native executable paths are never inspected. Removed companion
  platform eligibility from native tool projection (JavaScript is portable).
- Simplified author packaging, enable-grant collection and card tier projection
  to the current runtime. Permissions, action/auth consent, identities, source
  size/path checks, trusted namespace admission and MCP acquisition remain.
  Updated catalogue/developer negative tests to assert refusal at parsing;
  no compatibility runtime or refusal tombstone was added. Bindings regenerated
  with no output diff. No product UI or M7 change.
- Verification: workspace tests pass (core 254, SDK 44, checker/CLI 23 each,
  registry tools 99); normal app all-target check and 699 app tests pass / 23
  ignored; strict SDK/core/CLI/registry Clippy passes; app Clippy exits zero
  with unchanged 69/86 warnings. tsc, 101 Vitest tests, parity/policy, rustfmt,
  diff checks and 80 runner self-tests pass. The pre-existing listing.rs
  strict-Clippy, settingsStore.test.ts ESLint and two ratchet failures remain.
- Rebuilt the isolated host and ran seven real-app cases: registry 3/3, native
  signed-store 3/3, legacy quarantine/recovery 1/1 Pass. The quarantine case now
  reuses the existing current-policy greeting model adapter. Lost-pointer
  negative control failed at its expected assertion; all four run cleanups
  Pass and no harness process remained. Evidence (ignored), all under
  `tests/agent-harness/.runs/retirement-t8-20261101/`:
  `run-J8QCm7` (registry), `run-SGxbJa` (store), `run-2mVVR5` (quarantine),
  `run-xg5ytM` (negative), each `evidence/report.md`.
- Still pending the user's three presented choices: remove signed-index
  `extends` or retain it empty; remove action conditions/rules and change the
  approval digest or retain them; remove unused embedding engine/download UI
  or retain it for a separate discovery feature. No answer was inferred.
- Final catalogue cleanup should include the residual retired-built-in id
  denylist and obsolete `prompts`/`dictation` category vocabulary. Also audit
  the DESCRIPTION/README fallback and native artifact-kind encoding with the
  current publisher and signed fixtures: these were not extended here, and
  needed current native/MCP protocol support must survive. Keep this work in
  the coherent signed-catalogue unit after the `extends` decision.
- Full batch 2/3 acceptance, ordinary dictation/microphone and any remaining
  user visual checks are not certified by these scoped automated results.

**1 November 2026, session 4: embedding workflow fully retired.**

- The user explicitly authorized removing the unused embedding workflow,
  including Settings/download/install machinery, and continuing the remaining
  retirement. Removed `grain_embed.rs`, its four commands/four events and generated
  bindings, the Application settings group/component and English copy, and the
  Agent-window teardown hook. Agent confirmation/auto-approval behavior is unchanged.
  No startup migration or model-cleanup service was added. Shared `hf-hub` stays
  because Handy's live ASR model downloads use it; no Handy source was changed.
- Removed Candle core/nn/transformers and tokenizers dependencies. Cargo pruned
  57 packages from the app lockfile, with zero added packages/version upgrades.
  Removed the dormant dense retrieval arm, cosine floor, hybrid/RRF fusion,
  injected-score context, ranks/provenance and evaluation metrics/API. Production
  callers had always passed `None`; exact/lexical retrieval remains model-free.
  Lexical assembly also no longer allocates fusion rank maps/id sets/result vectors.
- Differential verification against the actual `e4413e79` retriever: all 410
  comparisons across 41 corpus requests, five K values (including zero) and both
  query sources match tool IDs, provenance, lexical ranks, exact score bits,
  truncation and exclusions. Evidence: `C:\t\logs\t9-retrieval-parity.txt`;
  the local probe imports the original source, not a recreated scoring algorithm.
- Verification: workspace all-target check/tests pass (core 250 + three benchmark
  tests, SDK 44, checker/CLI 23 each, registry tools 99); removed four dense unit
  tests and one dense benchmark. App all-target check and 696 app tests pass /
  23 ignored (three deleted embedding tests); core strict Clippy passes; app
  Clippy exits zero with 57 lib / 84 lib-test warnings (was 69/86). A redundant
  nested close condition exposed by removing the hook was merged, preserving
  its short-circuit and existing pending-action cleanup. Bindings regenerated;
  tsc, 101 Vitest tests, frontend build, parity/policy, rustfmt, scoped UI
  Prettier and whitespace checks pass. Impeccable detector: no findings.
- Existing lint/ratchet failures remain. Additional SDK-follow-up finding:
  `grain-extension-checks::build_pack` still uses a now-redundant
  `..PackPayloads::default()` after the payload struct became icon-only; remove
  that with the remaining contract cleanup. The corpus JSON already failed
  Prettier on HEAD; only its stale embedding explanation changed here.
- Fresh real-app registry/native-call checkpoint: 3/3 Pass; lost-pointer fault
  failed at its expected assertion; both cleanup Pass. Evidence (ignored):
  `tests/agent-harness/.runs/retirement-t9-20261101/run-Pv7AQZ/evidence/report.md`
  and `run-Ij8lHq/evidence/report.md`. This is scoped acceptance, not ordinary
  microphone/dictation or full product certification. User visual confirmation
  of Settings -> Application remains required; run `bun run tauri dev` from the
  repository to inspect the real application.
- Next: retire action conditions/rules and their hash inputs under the clean-break
  policy (fresh action approval required), then the signed-catalogue cleanup.

**1 November 2026, session 4 continued: action contract cleanup (T10).**

- Removed unused `LayerWhen`, action `when`/`agentRules`, their validation,
  ceilings and approval DTO fields. Action digests now cover the current tool
  contract directly; existing nonempty action approvals require renewed approval.
  Added a regression against the former digest. No migration/hash fallback.
- Kept risk, utterance, parameter, Unicode and network-sink checks. Repaired the
  sink test to use a supported exact-host grant, proving safe raw text is refused
  while a confirm declaration succeeds. Removed obsolete lifecycle fixture keys
  and the redundant icon-only PackPayloads struct update.
- Verification: workspace all-target tests pass (SDK 43, core 250 + three
  benchmarks), five focused approval tests pass; SDK/core strict Clippy passes.
  App 696 tests pass / 23 ignored. Regenerated bindings, tsc, 101 Vitest tests,
  parity/policy and whitespace checks pass. Source graph and scoped diff reviewed.
  Real-app acceptance will cover this together with the signed-catalogue unit.
- Catalogue trace confirmed the old README publisher is isolated from the current
  reviewed DESCRIPTION signing path and has no maintained caller. Remove it with
  `extends`, retired IDs/categories and the README fallback in the next unit.

**1 November 2026, session 4 continued: signed catalogue cleanup (T11).**

- Removed IndexEntry/StoreEntry `extends`, retired built-in ID denylist and old
  category vocabulary. Removed the isolated README `publish` command, its
  Submission parser, manifest/media helpers and obsolete refusal test. The live
  reviewed DESCRIPTION/native/MCP publishing and signing path stays intact.
- Removed README document fallback, renamed the current host command/DTO to
  `store_description`/`listing_document`, regenerated bindings and updated the
  real UI/acceptance callers. Metadata-only cards remain valid, but media now
  requires a bounded DESCRIPTION listing. Every media request, including cache
  hits, requires a current signed reference and exact size; no hash-only fallback.
  Native's existing wire encoding (omitted discriminator) remains supported along
  with explicit MCP discrimination and refusal of unknown artifact formats.
- Verification: workspace checks/tests pass (core 250 + three benchmarks, SDK 44,
  registry 98 + CLI credential test); app check and 696 tests pass / 23 ignored.
  SDK/core strict Clippy passes; app Clippy exits zero, unchanged 57/84 warnings.
  Workspace strict Clippy retains the two existing listing.rs failures. tsc,
  101 Vitest tests, frontend build, 80 harness tests, scoped formatting and policy
  pass. ESLint's settingsStore.test.ts error persists. The lib.rs ratchet now
  shrinks below baseline; only existing Cargo.toml growth remains.
- Fresh real-app native store 3/3 Pass: `retirement-t11-20261101/run-VGmVpF`.
  MCP store 3/3 Pass: `run-L7R8gv`; both cleanup Pass. Each path reads the signed
  DESCRIPTION; native also proves an unlisted hash is rejected without network.
  Full reports are the ignored `evidence/report.md` inside those run directories.
- MCP acceptance first stopped on the Windows Python Store alias; reran with the
  installed Python311/cryptography on PATH. A later run exposed an obsolete test
  requiring Developer Mode for verified store updates. Updated only the oracle to
  the existing source-aware policy: a genuine update with Developer Mode off
  disables execution, retains unchanged account credentials, then explicitly
  re-enables and performs one actual tool call. The first added oracle incorrectly
  required fresh OAuth for a version-only update; corrected it after tracing
  account_binding_changed and the existing account preservation tests. Product
  account/access semantics were not changed. Earlier runs remain failure evidence.
- Final audit found two compatibility-only empty API requirement allowances in
  SDK package/catalogue admission, plus an Extension Mode binding tombstone and
  stale overview comments. Retire those in the final contract sweep; unrelated
  overlay/default-prompt/secrets migrations and Agent confirmation (M7) remain
  outside this extension-platform task.

**1 November 2026, session 4 continued: explicit API admission (T12).**

- Removed the legacy-only allowance for an empty package grainApi and catalogue
  minimum. All pack admission paths now refuse missing/empty/unsupported profiles;
  exact/caret grammar and future-profile refusal stay unchanged. Updated fixture
  constructors to explicitly declare the current profile so their existing
  permission/auth/hash/publication tests reach the intended guard.
- Extended the existing API tests to refuse omitted and empty metadata at all
  runtime boundaries. Updated the existing real-app API scenario to preserve a
  working owner after missing/empty/unsupported profile refusal; reused the
  current first-round greeting adapter. No compatibility migration/default was
  added. Empty deserialization defaults remain invalid and never authorize use.
- Removed the Extension Mode binding tombstone and obsolete locale reservation;
  corrected API/icon docs and overview comments about retired built-in handling.
  Unrelated overlay/default-prompt/secrets migrations remain outside this task.
- Verification: workspace all-target checks/tests, SDK/core strict Clippy, app
  all-target check and 696 app tests / 23 ignored pass; tsc, 80 harness tests and
  whitespace checks pass. Real-app API/registry/store checkpoint follows T13.
- Audit follow-up: current import/store writers always publish content-addressed
  artifacts, but extension_host still reads two old installation layouts and
  uninstall still cleans a flat-file/approved-app format. Remove those fallback
  paths next, retaining current immutable publication/ownership guarantees.

**1 November 2026, session 4 continued: installed layout retirement (T13).**

- Removed flat-file and version-only package loader fallbacks. Runtime loads only
  the exact recorded content-addressed artifact; hash/id/version verification is
  mandatory in the reader signature. Missing/corrupt current bytes fail closed.
  Tests prove matching old-layout bytes cannot substitute, and a changed old
  shadow cannot affect the current owner. Added explicit wrong-id/version checks.
- Removed uninstall's flat-package helper and retired approved-app cleanup.
  Corrected install/artwork comments and generated uninstall docs. Current scoped
  storage, account/worker retirement and optional artwork purge remain. Immutable
  published artifacts remain untouched: deleting them here could race another
  install that staged those bytes before committing its registry owner.
- Verification: workspace tests and 696 app tests / 23 ignored pass; bindings
  regenerated, source graph/diff and whitespace checked. Final normal checks and
  real-app checkpoint will include the archive-removal follow-up below.
- Final source trace found pack.rs's unused archive extractor, obsolete extraction
  limits, and install.rs's ZIP manifest reader. The current stage function already
  refuses all non-JSON artifacts; its existing 2 MiB package limit must survive
  removal of the legacy ZIP machinery/dependency (T14).

**1 November 2026, session 4 completed: archive retirement and final checkpoint (T14).**

- Deleted the unused 455-line ZIP extractor and its 11 archive-only tests,
  ZIP manifest reader, PackShape/PackError/ExtractLimits plumbing and zip 8.6
  dependency. Both lockfiles pruned zip, typed-path, zlib-rs and zopfli, with zero
  added packages/version upgrades. Together with T9, the app lockfile drops 61
  packages. No Handy-derived source was changed in this session.
- The signed-store installer still refuses non-JSON before filesystem writes and
  retains its 2 MiB package ceiling. Added a regression for ZIP/other shapes,
  exact-limit acceptance and one-byte-over refusal. Approval digests come directly
  from the already validated manifest: no second parse or best-effort digest
  fallback. Current immutable publication and registry race behavior stay intact.
- Final verification: locked workspace/app all-target checks pass; workspace
  tests pass, with the added boundary test separately passing (core now 240 +
  three benchmarks, SDK 44, checker/CLI 23 each, registry 98 + one credential test).
  App 696 tests pass / 23 ignored. SDK/core strict Clippy passes; app Clippy exits
  zero, unchanged 57 lib / 84 lib-test warnings. tsc, 101 Vitest tests, production
  frontend build, 80 harness self-tests, parity/policy and scoped formatting pass.
  Workspace strict Clippy still fails two pre-existing listing.rs diagnostics;
  settingsStore.test.ts:2 raw-invoke ESLint and Cargo.toml ratchet growth persist.
- Rebuilt the actual isolated acceptance host after all source changes. Final
  evidence (ignored), all under `tests/agent-harness/.runs/retirement-final-20261101/`,
  each with `evidence/report.md` and JSON:
  - `run-ioe69W`: API admission 1/1 Pass (missing/empty/future profiles, working
    owner preserved and actual greeting calls).
  - `run-peRyW6`: registry refusal/preservation/save-failure 3/3 Pass.
  - `run-ohFIbo`: native signed-store 3/3 Pass, including signed DESCRIPTION,
    unlisted media refusal without network, updates/close/offline and real calls.
  - `run-eQaOBG`: MCP store management/accounts/revocation 3/3 Pass, including
    actual OAuth/vault ownership, destination changes, Developer Mode-independent
    verified updates, explicit re-enable, cancellation and real tool calls.
  - `run-fGusBB`: quarantine/recovery 1/1 Pass.
  - `run-MoXv7u`: unsupported-api-as-current negative control failed at its
    expected missing-rejection assertion.
  - `run-bqrwNK`: lost-pointer negative control failed at its expected preserved
    native-account pointer assertion.
  All seven cleanup verdicts Pass, including scoped vaults; no harness process
  remained. Evidence records precommit HEAD plus dirty inputs and exact build/
  runner fingerprints; source is unchanged by the final documentation/commit.
- In-scope implementation is complete. M7 remains excluded by the user's earlier
  decision. Unrelated overlay/default-prompt/secrets migrations remain outside
  this task. Manual ordinary dictation/microphone and the user's visual approval
  are still pending; do not infer them from extension acceptance. Launch the real
  app with `bun run tauri dev` from `C:\Projects\Grain\grain` and verify Application
  settings have no embedding section and remaining groups render correctly.

**1 November 2026, session 5: production verification follow-through (P1).**

- The user authorized continuing production-grade implementation. Retirement
  source work is complete; the next bounded units close known verification gaps,
  repair the Windows test runner and stale Rust CI, and review the stale manifest
  ratchet. M7 and unrelated settings migrations remain excluded. User visual and
  microphone/dictation results have been requested; no Pass is inferred.
- Removed the test's raw invoke import while retaining the real generated-binding
  error/transport coverage via a hoisted mock. Moved listing verification before
  its test module and simplified the equivalent media-count comparison.
- Verification: full workspace strict Clippy now passes; full frontend ESLint and
  tsc pass; all 101 Vitest tests and checker/CLI tests pass. Scoped formatting and
  whitespace pass. No runtime/visual behavior changed.

**1 November 2026, session 5 continued: reproducible Windows backend tests (P2).**

- Reproduced the maintained Cargo runner's STATUS_ENTRYPOINT_NOT_FOUND launch
  failure after its mt.exe mutation. It now runs a unique owned copy outside
  deps with the checked-in Common Controls sidecar and native DLL search paths,
  restores its environment, preserves the child exit code and removes only its
  staging directory. Cargo's cached executable is never rewritten.
- Added four standard unittest regressions using actual tiny Rust test binaries:
  filter/argument forwarding, failing-test exit 101, non-Tauri passthrough and
  concurrent staging. They also prove the original binary hash and unrelated
  sibling directory survive. All four pass, including paths containing spaces.
- Documented a Cargo array runner configuration in BUILD.md, verified verbatim
  against all 696 real backend tests / 23 ignored. No local C:/t helper is needed;
  no staging directory remained. This fixes test infrastructure, not product UI.

**1 November 2026, session 5 continued: real CI and reviewed ratchet (P3).**

- Replaced the broken CI source-copy/transcribe-rs deletion step with the real
  locked backend build/tests and the maintained native build's Vulkan SDK setup.
  Added current workspace strict Clippy/tests, frontend lint/tests/build and
  Windows runner regression jobs. Paths cover both roots, frontend inputs and
  runner scripts; jobs have read-only permissions, bounded time/resources, and
  no signing/release/installer operation. Rust matches the maintained CLI pin.
- Reviewed every Cargo.toml difference against the actual upstream merge base
  9e57a0d7929fcc6417ea0d6e54c1b57f62635edd using UPSTREAM.md and the per-file map.
  The existing composition seam measures 152 rather than the stale 142 budget:
  native/MCP/OAuth, assets/vault, paired immutable native fork and gated harness
  declarations are still required. Documented that semantic review and used the
  runbook's explicit --update --accept-growth procedure. Seven existing hook/root
  budgets tightened by 69 lines. No Handy source, dependency or native pin changed.
- Ratchet and upstream policy now pass. Actionlint 1.7.7 (checksum-verified local
  tool), workflow structure checks and formatting pass. Full workspace tests and
  production frontend build pass locally. Hosted CI execution follows the push;
  do not infer a Linux backend result from the local Windows test pass.

**1 November 2026, session 5 continued: hosted cross-platform discovery (P4).**

- Dispatched the repaired workflow on Grain explicitly (41300107744). Frontend
  and Windows runner regressions passed. Linux workspace Clippy exposed the
  pre-existing tracked root .cargo/config.toml with a Windows C:\t target path;
  Cargo failed joining that colon-containing path into Linux LD_LIBRARY_PATH.
- Removed only that machine-local file from Git's index and ignored it, matching
  the existing per-app local configuration policy. The actual Windows file and
  cache path remain unchanged. Hosted checkouts now use platform-native Cargo
  defaults. Added .cargo inputs to CI path triggers. No application code changed.
- Cancelled the superseded owned run before native compilation continued; it
  remains failure evidence. Actionlint/whitespace pass. Dispatch the fixed commit
  for the actual Linux workspace/backend checkpoint; no Pass is inferred yet.
