# Divergence Map (Grain vs Handy)

The complete, file-level map of where Grain has deliberately diverged from
upstream. Regenerate the raw list any time with:

```bash
comm -12 <(git diff --name-only 0392b7b main | sort) \
         <(git diff --name-only 0392b7b upstream/main | sort)
```

Audited 2026-07-16 against upstream v0.9.3. Sizes are `git diff upstream/main
main -- <file>`. When you resolve a conflict in one of these files, this table
tells you which side is authoritative. Files not listed here follow the
default: 3-way merge normally, prefer upstream in the STT core.

> **Paths below are upstream's.** Since the phase-7 folder move the same files
> live at `src-tauri/src/handy/…` in Grain (declared via `#[path]`, contents
> unchanged). `Upstream/ratchet.py` maps between the two.

## Rust backend

| File | Divergence | Merge guidance |
|---|---|---|
| `actions.rs` | Heavy (+1263/−306): pill session events, Prompt Record, cloud-routing model warm-up, cancel-generation output guard, no webview overlay calls | Keep Grain structure; thread upstream fixes into it |
| `shortcut/mod.rs` | Heavy (+434/−128): Grain bindings (agent summon, Grain Space recall, send-to-AI), cancel shortcut lifecycle | Keep Grain; take upstream's key-handling fixes |
| `llm_client.rs` | **Reclassified 2026-07-19**: byte-identical to upstream but UN-COMPILED (no `mod llm_client;`). Grain's multi-provider client lives in `grain_llm_client.rs` (aliased as `crate::llm_client`) | Take upstream verbatim — the file is inert; port relevant provider fixes to `grain_llm_client.rs` by hand |
| `managers/transcription.rs` | Heavy (+354/−441): transcribe-cpp unification, shared model across Batch/Rolling/Native ASR, stream router. Plus one `[GRAIN]` hook (2026-07-28): the whisper `initial_prompt` source is `context_bias::for_transcription` rather than `custom_words.join(", ")`, because the prefix needs a byte budget — whisper drops tokens past ~224 from the FRONT, silently | Keep Grain; upstream decode-parameter fixes DO matter — port them. Re-thread the `initial_prompt` hook if upstream rewrites that block |
| `managers/model.rs` | Deliberate (−461): legacy ONNX model entries (Parakeet/Moonshine/SenseVoice/GigaAM/Canary via transcribe-rs) REMOVED — every family ships as GGUF via the catalog | Ours for the model list; take upstream download/verify-flow fixes |
| `settings.rs` (src-tauri) | **Reclassified 2026-07-20**: byte-identical to upstream but UN-COMPILED (no `mod settings;`). Grain's facade over grain-core `AppContext` is `grain_settings.rs` (aliased as `crate::settings`) | Take upstream verbatim — the file is inert. Port upstream settings fixes into `crates/grain-core` |
| `actions.rs` | Grain's actions, post-processing and settings commands extracted (2026-07-20). What remains is upstream's shape + thin `[GRAIN]` hooks. **One deliberate hole**: upstream's `post_process_transcription` is absent (it cannot compile against Grain's `llm_client` signature) — expect a modify/delete conflict there and port into `grain_post_process.rs` | Merge upstream freely; re-thread the marked hooks |
| `shortcut/mod.rs` | Grain's 24 settings commands moved to `grain_commands.rs` (2026-07-20). Registration/dispatch is upstream's, plus the send-to-AI helpers. **+3 lines 2026-08-02**: `change_post_process_enabled_setting` re-registers `transcribe_send_to_ai`, not upstream's `transcribe_with_post_process` — Grain retired that binding, so upstream's line left Grain's AI key unregistered until restart. Kept to one `.get()` argument plus a `[GRAIN]` marker | Merge upstream freely; on conflict keep Grain's binding id |
| `audio_toolkit/text.rs` | Grain's `finalize_transcript` moved to `audio_toolkit/grain_text.rs` (2026-07-20); 4 lines of divergence left | Merge upstream freely |
| `audio_toolkit/audio/resampler.rs` | Grain-architecture fix: `finish()` drains the `FftFixedIn` delay line so cloud STT doesn't clip the tail word. Not an upstream candidate (Handy's local STT tolerates it). See UPSTREAMABLE.md | Keep Grain's fix; re-thread if upstream rewrites `finish()` |
| `overlay.rs` | **Reclassified 2026-07-19**: byte-identical to upstream but UN-COMPILED. Grain's pill mic-level fan-out lives in `grain_overlay.rs` (aliased as `crate::overlay`) | Take upstream verbatim — the file is inert |
| `tray.rs` | Moderate: single branded icon (no theme/state variants), non-panicking icon load, Grain menu | Keep Grain icon model; take upstream menu/state logic |
| `transcription_coordinator.rs` | Light: Grain's `stop_with_intent` (send-to-AI) alongside upstream's PTT deferral | Merge upstream freely |
| `lib.rs` | Moderate: Grain service bootstrap (rolling, routers, events server, pill supervisor, Grain Space). This file is the registration point, so every Grain module costs it a few lines by construction — `mod` line, `collect_commands!` entries, `collect_events!` entries (+4 on 2026-08-02 for `grain_update`, +2 on 2026-08-15 for `installed_apps`/`app_icon`). Growth here is expected; growth anywhere else in `handy/` is not. The 2026-08-15 pair is the floor for that feature: its module is declared from `context_detect` rather than here, and its two `windows` crate features were dropped for local `PROPERTYKEY` consts, so `Cargo.toml` took nothing | Merge; keep the [GRAIN] bootstrap block intact |
| `audio_toolkit/audio/recorder.rs` | **Re-baselined 2026-07-19** onto upstream text (VadPolicy restored). Additive `[GRAIN]` hooks only: with_sample_callback (rolling), conditioning, recorded_len. **2026-08-15:** #549cbde3's first-sample acknowledgement was not adopted; it changes the shared start API for Handy's retired webview arming state | Merge upstream freely except #549cbde3's readiness API; keep the marked hooks |
| `managers/audio.rs` | **Re-baselined 2026-07-19** onto upstream text (Stopping state, cancellable buffer, VadPolicy args restored). Additive `[GRAIN]` hooks: prompt_mark, set_conditioning, rolling wiring. **2026-08-15:** #549cbde3's `RecordingReadiness`/`capture_generation` layer was not adopted because every Grain capture owner would need a coordinated native contract | Keep Grain's current start contract and marked hooks; treat a future native-pill arming state as a Grain feature spanning every capture owner |
| `audio_toolkit/vad/*`, `audio_toolkit/bin/cli.rs` | Converged 2026-07-19 (byte-identical to upstream) | Take upstream verbatim |
| `audio_toolkit/text.rs` | Moderate (+104): custom-words extensions | Merge; watch word-boundary semantics |
| `commands/models.rs`, `managers/history.rs`, `clipboard.rs`, `cli.rs`, `utils.rs` | Light (≤45 lines each) | Merge normally |
| `catalog/catalog.json` | Converged 2026-07-17 (byte-identical). It is **generated upstream** by `scripts/gen_catalog.py`, which Grain deliberately does not vendor — so Grain must never hand-edit it | **Take upstream's version verbatim.** A 2026-07-17 audit found it had drifted (reformatted by hand, and upstream #1648's Moonshine language descriptions silently lost), which is exactly what hand-editing causes |
| `catalog/mod.rs`, `managers/gguf_meta.rs`, `managers/model_capabilities.rs` | Converged (byte-identical to upstream) — re-verified by blob hash 2026-08-02 | Merge freely. Do not "fix" upstream's comments here (e.g. `catalog/mod.rs` refers to `gen_catalog.py`, which is correct in upstream's tree) — editing them would re-open a conflict for no gain |
| `audio_toolkit/audio/resampler.rs` | **Not converged (+166/−13)** — the row above used to claim it was, for ~2 weeks. It carries the resampler tail-drop drain (`output_delay`), a real fix logged as [UPSTREAMABLE.md](UPSTREAMABLE.md) #1: `FftFixedIn` holds `output_delay()` frames in its FFT delay line, so end-of-stream dropped the last ~30–60 ms — inaudible to local ASR, a clipped final word for cloud STT | Merge upstream freely; **keep the drain**. Do not restore upstream's `finish()` |
| `audio_toolkit/mod.rs`, `audio_toolkit/audio/mod.rs` | Light (4 and 2 lines): `pub mod` + re-export lines only, for Grain's `snippets`/`grain_text` and `conditioner` modules. Also previously mis-filed as byte-identical | Merge upstream freely; keep the `[GRAIN]` module lines |
| `secure_input.rs`, `paste_tx/*`, `autostart.rs` | **New from upstream 2026-08-02, taken verbatim.** All three arrived at the `src-tauri/src/` ROOT (new dirs get no directory-rename detection) and were `git mv`'d into `handy/` with `#[path]` declarations. `secure_input.rs` is byte-identical to upstream; its unused-`Emitter` warning on Windows is upstream's own (the emit sits behind `#[cfg(target_os = "macos")]`) and is NOT worth diverging to silence | Merge upstream freely; keep them in `handy/` |
| `tray.rs` (2026-08-02) | Upstream added a `warning` parameter and a `windows_taskbar_theme()` helper. Grain keeps its single-icon model but adopted the new SIGNATURE and the warning badge — "shortcuts are being swallowed" is worth surfacing, and upstream ships the artwork. `windows_taskbar_theme` is `#[allow(dead_code)]` rather than deleted: keeping it costs a suppressor, deleting it costs a conflict every time upstream touches it | Keep Grain's icon model; keep the signature in step with upstream |
| `actions.rs` (2026-08-02) | Upstream's `should_use_streaming_overlay` is NOT ported: it switches on `OverlayStyle`, which Grain does not have (the webview overlay was replaced by the native pill). `strip_think_block` is kept but `#[allow(dead_code)]` — Grain's post-processing lives in `grain_post_process` | Do not re-add the overlay helper |
| `shortcut/mod.rs` (2026-08-02) | Carries BOTH suspend/resume APIs: upstream's `suspend_all_shortcuts`/`resume_all_shortcuts` (called by `handy_keys.rs`) and Grain's per-binding `suspend_binding`/`resume_binding` (called by `GlobalShortcutInput`). They are not duplicates — one is bulk, one is per-key | Keep both; do not collapse |
| `memory.rs` | **New from upstream 2026-08-08 (#1846), taken verbatim.** glibc allocator tuning (`init_allocator`/`trim_freed_memory`, no-ops off glibc). Arrived at the `src-tauri/src/` ROOT and was `git mv`'d into `handy/` with a `#[path]` declaration; upstream's sibling `mod overlay;` line was dropped (Grain aliases `grain_overlay as overlay`). The `trim_freed_memory()` call in `FinishGuard::drop` did NOT auto-merge (Grain's `actions.rs` FinishGuard is diverged, `[GRAIN] pub(crate)`) — **ported by hand** | Merge upstream freely; keep it in `handy/`. Re-thread the `FinishGuard::drop` call if upstream rewrites it |
| `audio_toolkit/audio/recorder.rs` + `managers/audio.rs` (2026-08-08) | #1254 input-channel selection (`selected_channel`) and #1716 lock-free `is_recording` (`recording_active` mirror via `set_state()`) merged as additive fields ALONGSIDE Grain's hooks (`sample_cb`/`conditioning`/`recorded_len`, `prompt_mark`). #1873 idle-skip wraps the resampler in `if recording` — Grain's conditioning/rolling closure kept inside it. The `selected_channel` **setting** lives in `crates/grain-core` (upstream `settings.rs` is inert); upstream's `ChannelSelector.tsx` was dropped (frontend frozen), the `set_selected_channel` command kept + registered | Keep both sides' fields; route every state write through `set_state()` so `recording_active` never drifts |
| `grain_llm_client.rs` (2026-08-08) | Ports of upstream fixes that landed in the inert `llm_client.rs`: `stream: false` (#2211da65) on `ChatCompletionRequest` at all four call sites; #1823 reqwest source-chain diagnostics (via `net_diag.rs` below); and #1809 **reasoning-rejection retry** in `post_chat` — a 400/422 to a request carrying reasoning-disable fields retries once without them and remembers the endpoint (`url|model`) process-wide. Grain gates reasoning by provider, so #1809 only bites a `custom` endpoint pointed at a strict API | Grain-owned client; port only relevant provider fixes by hand |
| `net_diag.rs` (Grain-only, 2026-08-08) | Shared reqwest transport-error diagnostics ported from upstream #1823 (which lives in the inert single-provider `llm_client.rs`). `report_reqwest_error` walks the `.source()` chain to surface the real cause (cert/connection/proxy failure) that reqwest's `Display` omits, while sanitizing URLs and suppressing decode-error sources that can quote transcription content. Applied to BOTH `grain_llm_client` and the Grain-only `stt_client` (5 send sites) — the STT client is a parallel path the merge can never reach, per the "check parallel implementations" rule | Grain-owned; keep in step with upstream #1823 if it evolves |
| `managers/transcription.rs` (2026-08-10) | #1738 filler-word removal adopted into Grain's diverged `transcribe()`: the `with_engine_session` decode closure now resolves `OutputLanguageEvidence` (`resolve_output_language_evidence` + `with_model_detected_language`, both merged clean) and returns it plus the model language list, threaded into `finalize_batch_text` → the 5-arg `post_process_transcription_text`. Native streaming finalize carries upstream's new `FinalizedStreamText` (Grain never diverged `StreamCmd`, so it merged clean) | Keep Grain's decode structure; re-thread the output-language pieces if upstream reshapes them |
| `shortcut/mod.rs` (2026-08-10) | Adopted #1738 `change_filler_word_removal_enabled_setting` (+ registered in `lib.rs`). Did NOT re-add upstream #1659 `change_theme_setting`/`apply_window_theme` or the `theme-changed` emit: Grain removed the backend theme command long ago (frontend owns theming via `data-theme`; the pill is native), so the HEAD-empty side wins. `change_vad_enabled_setting` likewise stays absent/unregistered | Keep Grain's binding set; do not re-add the theme command or VAD toggle |
| `audio_toolkit/grain_text.rs` (2026-08-10, Grain-only) | Rewritten onto #1738's `text.rs` API: `finalize_transcript` and `finalize_batch_text` gained `filler_word_removal_enabled` + output-language/supported-language args and call `remove_filler_words` + `normalize_transcription_output` (old `filter_transcription_output` is gone). Rolling/cloud callers (`grain_actions.rs`, `stt_router.rs`) key filler removal on `selected_language`, not `app_language` | Grain-owned; keep in step with `text.rs` |
| `Cargo.toml` / `build.rs` | Grain deps (grain-core, WS, embeddings) + transcribe-lib staging. `[patch.crates-io]` now matches upstream (tao rev pin; the cjpais tauri-runtime fork is gone). 2026-08-10: reqwest keeps Grain's `multipart` while adopting #1548 `gzip`/`brotli`/`deflate`; `whatlang`/`isolang` added for #1738 text-LID alongside Grain's `sourcemap` | Merge; never drop Grain deps |
| `filler_word_removal_enabled` (grain-core, 2026-08-10) | #1738 master toggle for filler removal (default `true`). Lives in `crates/grain-core` `AppSettings` because upstream's `settings.rs` is inert | Keep in grain-core; port future filler-setting changes there by hand |
| `actions.rs` + recorder/audio readiness (2026-08-15) | #549cbde3 tied start-chime timing and a `recording-ready` event to the first captured sample for Handy's web overlay. Grain kept its existing capture contract: the native pill is announced after the recording request succeeds and receives real `AudioLevel` events once samples flow. The compiled `actions.rs`, recorder, and manager hunks were dropped; inert `handy/overlay.rs` remains byte-identical to upstream | Do not partially re-add #549cbde3. If Grain needs an explicit arming state, design one native `DaemonEvent` and update batch, rolling, Native ASR, Agent, onboarding, and extension sessions together |
| `input.rs`, `portable.rs`, `lib.rs` (2026-08-15) | Adopted upstream verbatim/inline: #1911 resolves macOS Cmd+V from the active layout with ANSI-V fallback; #1908 sets portable `HF_HOME` to `Data/huggingface`; #1902 sets `GGML_METAL_NO_RESIDENCY=1` before engine initialization unless `HANDY_METAL_RESIDENCY=1` opts back in | Merge these compatibility fixes freely; keep the Metal environment setup before any transcribe-cpp initialization |
| `secure_input.rs` + `tray.rs` (2026-08-15) | #37a26fd6 suppresses redundant Secure Input tray rebuilds and atomically applies the macOS template flag with icon replacement, removing recording start/stop races. Grain kept its branded single-icon model while adopting the state-transition fix | Preserve the warning-state comparison and `set_icon_with_as_template`; keep Grain's icon/menu model |
| `resources/default_settings.json` (budgeted 2026-08-15) | Grain deliberately defaults `push_to_talk` to `false`; upstream defaults it to `true`. The setting remains user-configurable | Keep Grain's off-by-default product choice |
| `commands/transcription.rs` (budgeted 2026-08-15) | Grain manages `TranscriptionManager` as `Arc<TranscriptionManager>`; both model-status commands therefore request `State<Arc<TranscriptionManager>>`. Upstream's bare state type fails at runtime because it is not managed | Keep the `Arc` state type and imports unless Grain's composition root changes |

## Grain-only subsystems (no upstream counterpart — never expect upstream changes)

`grain_actions.rs`, `grain_commands.rs`, `grain_post_process.rs`,
`grain_settings.rs`, `grain_llm_client.rs`, `grain_overlay.rs`,
`audio_toolkit/grain_text.rs` (all `grain_*` files are Grain-owned by
convention — upstream has no counterpart, so they never conflict),
`crates/*` (grain-core, grain-pill, grain-editor, provider-router),
`src-tauri/src/{rolling,stt_router,post_process_router,rotation_state,agent,bridge,events_server,context_detect,context_bias,context_screen,grain_space/**,stt_client}.rs`,
`Upstream/`, `docs/`.

## Frontend — FROZEN 2026-07-31 (UI 2.0)

| Area | Divergence | Merge guidance |
|---|---|---|
| `src/**` | **Grain-owned. Never merge.** `merge=ours` in `.gitattributes`; guarded by [frontend_freeze.py](frontend_freeze.py) (`merge=ours` only wins *conflicts* — a clean upstream edit or a new upstream file would otherwise land silently) | **Keep ours, always.** On a modify/delete conflict, `git rm`. If an upstream frontend commit carries *backend* knowledge (host capabilities, locale resolution, permissions), port it into Rust by hand and `verdict.py --note` it |
| `src/app/i18n/locales/tr/translation.json` (2026-08-15) | #1907's corrected Turkish semantics were manually adapted to Grain's older owned schema: transcribe.cpp acceleration, `Ekran Katmanı` terminology, and ggml acknowledgement copy with Grain branding | Treat future upstream translations as review input only; manually port relevant wording without merging or restoring upstream's frontend file |
| `bindings.ts` | GENERATED from Grain's Rust (specta) | Never hand-merge — regenerate |

The rationale, the measurements behind it, and the work that pays for it (moving
the last UI-resident policy into Rust) are in
[`docs/UI 2.0/PLAN.md`](../docs/UI%202.0/PLAN.md). In short: of upstream's last
22 commits, 17 touched the backend and 4 touched the frontend outside i18n — two
for features Grain deleted, two carrying backend facts that belong in Rust, where
we still merge everything. The freeze does not touch the backend relationship in
any way.

`frontend_allow.json` records the files still shared with upstream (140 at the
freeze). That number may shrink, never grow; it reaches zero at the UI 2.0
cutover, when `"strict": true` makes any frontend change in a sync a hard
failure.

## Repo meta (all `merge=ours` via .gitattributes)

Docs (`README`, `AGENTS.md`, `CLAUDE.md`, `BUILD.md`, `CRUSH.md`,
`CONTRIBUTING*`), `.github/workflows/**`, `tauri.conf.json` +
`tauri.windows.conf.json` (identity `com.grain.app`, **no auto-updater —
never re-add**), lockfiles (regenerate after merges), `website/`, `docs/`,
`Upstream/`.

## Build & repo-root files

Absent from this map until 2026-08-02, though every one of them conflicted in a
real trial merge against `upstream/main`. Audited by blob hash, not by memory.

| File | Divergence | Merge guidance |
|---|---|---|
| `.gitignore` | Necessary (+66): Grain's own ignores (target dir, keys, tracker build products). Both forks *append*, so the conflicts were never real disagreements | **Now `merge=union`** — keeps both sides' lines, no conflict. A duplicate ignore is harmless; a lost upstream ignore is not |
| `.nix/bun.nix`, `.nix/bun-lock-hash` | Necessary: both are **generated from `bun.lock`**, which is already `merge=ours`. `bun.nix` says "Autogenerated by bun2nix" in line 1 | **Now `merge=ours`.** Taking upstream's would describe a dependency set we do not have, and `nix-check.yml` regenerates + diffs them. Regenerate with `bunx bun2nix -o .nix/bun.nix` after changing deps |
| `package.json` | Necessary (+22): Grain's `version` (independent of Handy's), `run-tauri.ts` script wrappers, CodeMirror + vitest deps, and `react-markdown`/`remark-gfm` dropped | Merge normally — **do not** `merge=ours`. Upstream dependency bumps are worth seeing; expect a one-line `version` conflict each release and keep ours |
| `src-tauri/.gitignore` | Necessary but trivial (4 lines): a **comment** describing where transcribe libs install (`/usr/lib/Grain` vs `/usr/lib/Handy`). The ignore rule itself is identical | Keep ours (rebrand). Not worth a merge driver — it conflicts only when upstream edits that comment |
| `build.rs` | Necessary: same rebrand, but in code — the deb/rpm rpath is `$ORIGIN/../lib/Grain`. Grain already carries upstream's #1639 app-private-lib fix, renamed | Take upstream's logic, keep `Grain` in the paths |
| `scripts/gen_catalog.py`, `scripts/ci/stage-transcribe-libs.sh` | **Deleted by us, deliberately.** Grain does not vendor upstream's catalogue generator (see the `catalog.json` row) and does not run upstream's CI | Each upstream edit raises a *delete/modify* conflict — a merge driver cannot resolve those. The answer is always `git rm`. Do not restore them: dead scripts we never run are exactly what the tree is being kept clear of |

## Ancestry drift (looks like divergence, is not)

`handy/tray_i18n.rs` is **byte-identical to `upstream/main`** yet still carries a
60-line budget, because the zh-Hant/zh-TW locale-resolution change was applied
here by hand rather than merged. Git tracks ancestry, not content, so it replays
the change into every merge — and because the file *also* moved into `handy/`,
similarity falls to **36%**, under git's 50% rename threshold, so it lands as a
`deleted by us / modified by them` conflict instead of a clean rename.

Resolution while it lasts: **keep ours** (`git checkout --ours` on the `handy/`
path, `git rm` the root path) — the blobs are equal, so nothing is lost. It
clears permanently at the next real merge or `-s ours` close-out, after which
`ratchet.py --update` drops the 60.

Run [audit_divergence.py](audit_divergence.py) to re-check this and the
"converged" claims above against real blobs — both had silently gone stale
before it existed.
