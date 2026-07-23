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
| `managers/transcription.rs` | Heavy (+354/−441): transcribe-cpp unification, shared model across Batch/Rolling/Native ASR, stream router | Keep Grain; upstream decode-parameter fixes DO matter — port them |
| `managers/model.rs` | Deliberate (−461): legacy ONNX model entries (Parakeet/Moonshine/SenseVoice/GigaAM/Canary via transcribe-rs) REMOVED — every family ships as GGUF via the catalog | Ours for the model list; take upstream download/verify-flow fixes |
| `settings.rs` (src-tauri) | **Reclassified 2026-07-20**: byte-identical to upstream but UN-COMPILED (no `mod settings;`). Grain's facade over grain-core `AppContext` is `grain_settings.rs` (aliased as `crate::settings`) | Take upstream verbatim — the file is inert. Port upstream settings fixes into `crates/grain-core` |
| `actions.rs` | Grain's actions, post-processing and settings commands extracted (2026-07-20). What remains is upstream's shape + thin `[GRAIN]` hooks. **One deliberate hole**: upstream's `post_process_transcription` is absent (it cannot compile against Grain's `llm_client` signature) — expect a modify/delete conflict there and port into `grain_post_process.rs` | Merge upstream freely; re-thread the marked hooks |
| `shortcut/mod.rs` | Grain's 24 settings commands moved to `grain_commands.rs` (2026-07-20). Registration/dispatch is upstream's, plus the send-to-AI helpers | Merge upstream freely |
| `audio_toolkit/text.rs` | Grain's `finalize_transcript` moved to `audio_toolkit/grain_text.rs` (2026-07-20); 4 lines of divergence left | Merge upstream freely |
| `audio_toolkit/audio/resampler.rs` | Grain-architecture fix: `finish()` drains the `FftFixedIn` delay line so cloud STT doesn't clip the tail word. Not an upstream candidate (Handy's local STT tolerates it). See UPSTREAMABLE.md | Keep Grain's fix; re-thread if upstream rewrites `finish()` |
| `overlay.rs` | **Reclassified 2026-07-19**: byte-identical to upstream but UN-COMPILED. Grain's pill mic-level fan-out lives in `grain_overlay.rs` (aliased as `crate::overlay`) | Take upstream verbatim — the file is inert |
| `tray.rs` | Moderate: single branded icon (no theme/state variants), non-panicking icon load, Grain menu | Keep Grain icon model; take upstream menu/state logic |
| `transcription_coordinator.rs` | Light: Grain's `stop_with_intent` (send-to-AI) alongside upstream's PTT deferral | Merge upstream freely |
| `lib.rs` | Moderate: Grain service bootstrap (rolling, routers, events server, pill supervisor, Grain Space) | Merge; keep the [GRAIN] bootstrap block intact |
| `audio_toolkit/audio/recorder.rs` | **Re-baselined 2026-07-19** onto upstream text (VadPolicy restored). Additive `[GRAIN]` hooks only: with_sample_callback (rolling), conditioning, recorded_len | Merge upstream freely; keep the marked hooks |
| `managers/audio.rs` | **Re-baselined 2026-07-19** onto upstream text (Stopping state, cancellable buffer, VadPolicy args restored). Additive `[GRAIN]` hooks: prompt_mark, set_conditioning, rolling wiring | Merge upstream freely; keep the marked hooks |
| `audio_toolkit/vad/*`, `audio_toolkit/bin/cli.rs` | Converged 2026-07-19 (byte-identical to upstream) | Take upstream verbatim |
| `audio_toolkit/text.rs` | Moderate (+104): custom-words extensions (auto-dictionary substrate) | Merge; watch word-boundary semantics |
| `commands/models.rs`, `managers/history.rs`, `clipboard.rs`, `cli.rs`, `utils.rs` | Light (≤45 lines each) | Merge normally |
| `catalog/catalog.json` | Converged 2026-07-17 (byte-identical). It is **generated upstream** by `scripts/gen_catalog.py`, which Grain deliberately does not vendor — so Grain must never hand-edit it | **Take upstream's version verbatim.** A 2026-07-17 audit found it had drifted (reformatted by hand, and upstream #1648's Moonshine language descriptions silently lost), which is exactly what hand-editing causes |
| `catalog/mod.rs`, `managers/gguf_meta.rs`, `managers/model_capabilities.rs`, `managers/transcription_mock.rs`, `audio_toolkit/*mod.rs`, `resampler.rs` | Converged (byte-identical to upstream as of v0.9.3) | Merge freely. Do not "fix" upstream's comments here (e.g. `catalog/mod.rs` refers to `gen_catalog.py`, which is correct in upstream's tree) — editing them would re-open a conflict for no gain |
| `Cargo.toml` / `build.rs` | Grain deps (grain-core, WS, embeddings) + transcribe-lib staging. `[patch.crates-io]` now matches upstream (tao rev pin; the cjpais tauri-runtime fork is gone) | Merge; never drop Grain deps |

## Grain-only subsystems (no upstream counterpart — never expect upstream changes)

`grain_actions.rs`, `grain_commands.rs`, `grain_post_process.rs`,
`grain_settings.rs`, `grain_llm_client.rs`, `grain_overlay.rs`,
`audio_toolkit/grain_text.rs` (all `grain_*` files are Grain-owned by
convention — upstream has no counterpart, so they never conflict),
`crates/*` (grain-core, grain-pill, grain-editor, provider-router),
`src-tauri/src/{rolling,stt_router,post_process_router,rotation_state,agent,bridge,events_server,context_detect,grain_space/**,stt_client}.rs`,
`Upstream/`, `docs/`.

## Frontend

| Area | Divergence | Merge guidance |
|---|---|---|
| `App.tsx`, `main.tsx`, `App.css` | Grain shell/branding, decoupled-frontend boot | Keep Grain layout; take upstream logic fixes |
| `components/settings/**` | Grain UI on Handy's component skeleton | Component-by-component judgment: Grain styling, upstream behavior fixes |
| `components/onboarding/**` | Grain flat layout, standard-models-only filter | Same |
| `overlay/RecordingOverlay.*` | DELETED (native pill replaced the webview overlay) | Modify/delete conflict each sync → always keep deleted |
| `stores/settingsStore.ts` | Moderate: Grain settings fields | Merge normally |
| `bindings.ts` | GENERATED from Grain's Rust (specta) | Never hand-merge — regenerate |
| `i18n/locales/*` | "Handy"→"Grain" string rebrand + Grain keys | Take upstream's new keys, keep Grain strings |
| `tailwind.config.js` | DELETED 2026-07-17, converging with upstream: Grain was already on Tailwind v4 (`@theme` in App.css) and nothing referenced the file | Converged — conflict gone |

## Repo meta (all `merge=ours` via .gitattributes)

Docs (`README`, `AGENTS.md`, `CLAUDE.md`, `BUILD.md`, `CRUSH.md`,
`CONTRIBUTING*`), `.github/workflows/**`, `tauri.conf.json` +
`tauri.windows.conf.json` (identity `com.grain.app`, **no auto-updater —
never re-add**), lockfiles (regenerate after merges), `website/`, `docs/`,
`Upstream/`.
