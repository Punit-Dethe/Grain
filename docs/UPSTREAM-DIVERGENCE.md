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

## Rust backend

| File | Divergence | Merge guidance |
|---|---|---|
| `actions.rs` | Heavy (+1263/−306): pill session events, Prompt Record, cloud-routing model warm-up, cancel-generation output guard, no webview overlay calls | Keep Grain structure; thread upstream fixes into it |
| `shortcut/mod.rs` | Heavy (+434/−128): Grain bindings (agent summon, Grain Space recall, send-to-AI), cancel shortcut lifecycle | Keep Grain; take upstream's key-handling fixes |
| `llm_client.rs` | Heavy (+440/−80): multi-provider + smart rotation (upstream is single-provider) | Grain authoritative; upstream provider fixes usually n/a |
| `managers/transcription.rs` | Heavy (+354/−441): transcribe-cpp unification, shared model across Batch/Rolling/Native ASR, stream router | Keep Grain; upstream decode-parameter fixes DO matter — port them |
| `managers/model.rs` | Deliberate (−461): legacy ONNX model entries (Parakeet/Moonshine/SenseVoice/GigaAM/Canary via transcribe-rs) REMOVED — every family ships as GGUF via the catalog | Ours for the model list; take upstream download/verify-flow fixes |
| `settings.rs` (src-tauri) | Rewritten: thin facade over grain-core `AppContext` | Ours always; real logic lives in `crates/grain-core` — port upstream settings fixes THERE |
| `overlay.rs` | Rewritten: webview overlay retired, native pill only (audio-level fan-out remains) | Ours always |
| `tray.rs` | Moderate: single branded icon (no theme/state variants), non-panicking icon load, Grain menu | Keep Grain icon model; take upstream menu/state logic |
| `transcription_coordinator.rs` | Light: Grain's `stop_with_intent` (send-to-AI) alongside upstream's PTT deferral | Merge upstream freely |
| `lib.rs` | Moderate: Grain service bootstrap (rolling, routers, events server, pill supervisor, Grain Space) | Merge; keep the [GRAIN] bootstrap block intact |
| `audio_toolkit/audio/recorder.rs` | Moderate: sample_cb (rolling), conditioning (high-pass+AGC), recorded_len (Prompt Record); no VadPolicy in Cmd::Start | Keep Grain hooks; take upstream capture/latency fixes |
| `audio_toolkit/text.rs` | Moderate (+104): custom-words extensions (auto-dictionary substrate) | Merge; watch word-boundary semantics |
| `managers/audio.rs` | Moderate: prompt_mark, cancel_generation, Grain mode handling | Merge; keep [GRAIN] fields |
| `commands/models.rs`, `managers/history.rs`, `clipboard.rs`, `cli.rs`, `utils.rs` | Light (≤45 lines each) | Merge normally |
| `catalog/*`, `managers/gguf_meta.rs`, `managers/model_capabilities.rs`, `managers/transcription_mock.rs`, `audio_toolkit/*mod.rs`, `resampler.rs` | Converged (≈ upstream as of v0.9.3) | Merge freely |
| `Cargo.toml` / `build.rs` | Grain deps (grain-core, WS, embeddings) + transcribe-lib staging. `[patch.crates-io]` now matches upstream (tao rev pin; the cjpais tauri-runtime fork is gone) | Merge; never drop Grain deps |

## Grain-only subsystems (no upstream counterpart — never expect upstream changes)

`crates/*` (grain-core, grain-pill, grain-editor, provider-router),
`src-tauri/src/{rolling,stt_router,post_process_router,rotation_state,agent,bridge,events_server,context_detect,grain_space/**,stt_client}.rs`,
`Upstream Tracking/`, `docs/`.

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
`Upstream Tracking/`.
