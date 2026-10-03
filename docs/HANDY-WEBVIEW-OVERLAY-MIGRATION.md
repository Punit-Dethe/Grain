# Handy WebView overlay migration

Status: planning complete; product implementation has not started. Reviewed 2026-10-04.

## Baselines and objective

- Branch: `codex/handy-webview-overlays`, created from GitHub `main` at `69807ccef1fcdfb70a796a981ad308733262adea`.
- Handy reference: `ffbc9504cbf004ce4819d2ca872fcea92be0fddf`, latest `main` when inspected; `git describe` reports `v0.9.8-3-gffbc950`.
- Local reference checkout: `C:\Projects\Grain\Reference\Handy`.
- Grain's `src-tauri/src/handy/overlay.rs` already matches that Handy file byte for byte: SHA-256 `875C06C96EF8297C26622FF9B6143863B4350BDB7FACC63004430F3DCCBA27D7`.

Replace the TinySkia overlay runtime with Handy's actual Tauri WebView window lifecycle and platform handling. Preserve Grain's visual design and foreground application/site icon. Follow `Upstream/UPSTREAM.md` and `Upstream/UPSTREAM-DIVERGENCE.md` for every upstream assessment and port. The user's explicit request establishes this new branch from `main` for the migration.

The branch base was independently checked against `git ls-remote origin refs/heads/main` and the available commit object. Work uses an isolated checkout and the machine's existing Git identity.

## Agreed scope

The user's clarification preserves the existing Agent result window and every other existing WebView surface. The Agent expansion being retired is the **TinySkia summon/input card**, including its typing expansion. The existing `AgentPanel.tsx` conversation and result behavior remains available.

| Surface or feature | Migration outcome |
| --- | --- |
| Compact dictation pill | Grain appearance; Handy recording, readiness, waveform, work-state, cancellation and window behavior. |
| Native ASR live preview | Grain appearance; Handy committed/tentative text handling, timer, scrolling, working state and visibility behavior. |
| Agent summon/input | Uses the same compact pill presentation as dictation. Voice submission still opens the current Agent result surface according to the existing Agent settings. |
| TinySkia Agent typed expansion | Unavailable in production. Preserve the underlying typed submission functions; remove the native input renderer when retiring TinySkia. |
| `Type to expand` setting | Remove its production row and native-input wiring. An old persisted value must not enable expansion. |
| Prompt switcher | Delete the UI, cycling actions, shortcuts, transient key registrations, timers and protocol machinery. Ordinary prompt editing/selection and post-processing remain. |
| Hover controls | Remove hover-driven expansion, reveals, controls and animations from the migrated surfaces. |
| Prompt Record | Remove its buttons and hover UI. Preserve audio split marks, transcription and spoken-instruction processing. Add a configurable shortcut only in a later phase. |
| Clipboard acknowledgement | Preserve appearance, copy/hold behavior and its independent three-second notice. Move presentation into the shared WebView. |
| Quick Agent follow-up capsule | Migrate its TinySkia presentation and retain its existing click/shortcut, expiry and result-window destination. No hover reveal. |
| Extension-owned capture indicator | Preserve the current owner indication and capture lifecycle in the migrated pill. |
| Agent reply panel, extension interaction/confirmation windows | Preserve their existing UI and behavior. |

The old native extension chooser already ignores `ExtensionRecommend` events; current recommendations use `extension_view`. `ActionChoice`/`ActionResult` remain in the SDK, but no production Rust emitter was found on this baseline. Do not recreate dormant surfaces or remove unrelated SDK contracts solely because they appear in the old renderer.

## What Handy currently does

Sources: [overlay backend](https://github.com/cjpais/Handy/blob/ffbc9504cbf004ce4819d2ca872fcea92be0fddf/src-tauri/src/overlay.rs), [overlay component](https://github.com/cjpais/Handy/blob/ffbc9504cbf004ce4819d2ca872fcea92be0fddf/src/overlay/RecordingOverlay.tsx), [capture actions](https://github.com/cjpais/Handy/blob/ffbc9504cbf004ce4819d2ca872fcea92be0fddf/src-tauri/src/actions.rs).

- Creates one hidden `recording_overlay` window at startup and reuses/resizes it. Compact and streaming layouts share this window.
- Uses transparent, undecorated, topmost, taskbar-skipping, nonactivating windows. Geometry and monitor access run on the main thread.
- Windows placement uses the destination monitor's physical coordinates, DPI and accessibility text scaling; it reasserts bounds/topmost status without taking focus.
- macOS uses a nonactivating NSPanel, joins Spaces/fullscreen, and accounts for the work area. Linux uses GTK layer-shell anchors and size requests, with the normal-window fallback.
- Shows an arming presentation before actual capture readiness. `RecordingReadiness` is satisfied by real microphone samples; generation checks prevent a cancelled/old capture from announcing readiness or playing its start cue.
- Sends `mic-level` to the overlay specifically, throttled to approximately 30 FPS, and skips that WebView traffic when overlays are disabled. The frontend smooths the 16 buckets with `previous * 0.7 + target * 0.3`.
- Uses `show-overlay`, `hide-overlay`, `recording-ready`, `StreamTextEvent` and `StreamPhaseEvent`. Stream text is a cumulative committed prefix plus a replaceable tentative tail.
- Starts the live timer at readiness, opens the text panel when text exists, retains that text while finalizing/polishing, and pauses auto-follow when the user scrolls back.
- Guards the delayed native hide with a show-generation counter so it cannot hide a newer session. Hidden React content does not continue its visible animation/timer loop.

Grain presently announces capture after `try_start_recording` returns `Ok(())`, uses fixed start-cue delays in several paths, renders its own waveform dynamics, and drives the native process through `DaemonEvent`/WebSocket. Merely displaying a React pill would leave these behavioral gaps.

## Architecture and blast radius

Reactivate `handy/overlay.rs` as the compiled `crate::overlay` module. Keep Grain additions in a small `grain_overlay` integration module. Shared-tree changes must be narrow, marked `[GRAIN]` hooks: Grain entry URL, required visual bounds/supplementary presentation entry points, and preservation of the public audio-event feed where necessary. Platform positioning/show/hide algorithms remain upstream code.

Build the Grain-owned renderer under `src/app/overlay/`, with a real `recording-overlay.html` Vite entry. It consumes Tauri events and sends Tauri commands. It does not open a loopback socket, create its own microphone stream, discover foreground apps in JavaScript, or allocate another background service.

Native Rust fixes can then flow directly into the active shared module. Grain's frontend freeze still means upstream React behavior changes require an explicit review and adaptation into the owned renderer. Record this mapping and the pinned behavioral reference in the upstream policy; do not claim automatic frontend inheritance.

The graph reported high impact and 42 additional files within two hops. Its results were truncated and included unrelated entities, so this is a warning about breadth, not an exact change count. Direct source tracing identifies these concrete areas:

| Area | Paths and responsibility |
| --- | --- |
| Window composition | `src-tauri/src/lib.rs`, `main.rs`, `handy/overlay.rs`, `grain_overlay.rs`; activate the shared window and remove multicall startup. |
| Capture contract | `handy/audio_toolkit/audio/recorder.rs`, `handy/managers/audio.rs`; restore upstream readiness while preserving bounded capture, sample callbacks, Prompt Record marks and resampler tail drain. |
| Capture callers | `handy/actions.rs`, `grain_actions.rs`, `agent.rs`, `extension_session.rs`, `grain_action_session.rs`, `grain_onboarding.rs`; start, failure, stop, cancel and terminal cleanup. |
| Live text/work states | `handy/managers/transcription.rs`, `grain_actions.rs`; use existing stream events and restore working/polishing transitions. Review Flow/rolling lifecycle consumers too. |
| Grain presentation | New `src/app/overlay/*`, `recording-overlay.html`, `vite.config.ts`, overlay capabilities, `grain_events.rs`, generated `src/app/bindings.ts`. |
| Icons/themes/skins | `pill_icon.rs`, `surface_watch.rs`, `pill_skin.rs`, `grain_theme.rs`; reuse existing detection, caches, settings and resolved theme. |
| Switcher deletion | `master_key.rs`, `grain_actions.rs`, `grain-core/src/settings.rs`, `grain-core/src/capture.rs`, `grain-sdk/src/event.rs`, `CaptureModes.tsx`, related manifests/catalogues/tests. |
| Agent input setting | `grain_commands.rs`, command registration, settings schema/defaults, `settingsStore.ts`, `AgentSection.tsx`; preserve reply-panel settings and commands. |
| Native runtime retirement | `events_server.rs`, `events_auth.rs`, `bridge.rs`, `crates/grain-pill`, workspace/app Cargo files, lockfiles, packaging scripts and documentation. |
| Upstream bookkeeping | `relocations.json`, generated policy, `UPSTREAM.md`, `UPSTREAM-DIVERGENCE.md`, affected verdicts/deferred rows, divergence budgets after verification. |

The event server and headless bus also serve extensions. Retire only pill spawning, pill-only credentials/handshake/reverse actions and obsolete welcome frames. Preserve extension transport, filtering and public transcript/audio events. Restore targeted Handy microphone events without double-broadcasting them to WebViews; any retained daemon audio feed needs its own bounded cadence independent of overlay enablement.

## Execution order

1. **Capture baseline and finalize the contracts.** Recheck both remote pins before implementation. Record real-app reference images and the existing surface inventory. Review readiness and the deferred recorder rewrite as one capture contract; settle necessary dependencies before editing. Do not bulk-upgrade the native ASR library merely because current Handy changed it. Keep unrelated deferred VAD, tray and shortcut rewrites separate unless a demonstrated dependency requires them.

2. **Align settings and restore capture readiness.** Add Handy's `OverlayStyle` and top/bottom position contract to Grain's real settings. Migrate legacy `none` to disabled style with bottom position; migrate `center` to bottom for the affected pill, and preserve old top/bottom choices. Default enabled legacy installations to Live so existing Native ASR preview remains available, while retaining Linux's disabled default. Keep Wave/Matrix skin and app-icon preferences separate. Restore first-sample readiness and generation checks across all six caller families, including low-RAM Flow, Agent, extensions and onboarding. An unavailable microphone must fail voice-only Agent input cleanly.

3. **Activate the Handy window and implement Grain presentation.** Restore startup creation, enabled-cache updates, positioning commands and existing platform dependencies. Reuse upstream readiness, show/hide and waveform events. Use existing `StreamTextEvent`/`StreamPhaseEvent` for Native ASR; preserve model capability gating and compact fallback. Add Grain chrome, icon and skin rendering to the owned component. Ensure all asynchronous listeners are actually released: Handy's current component calls an async setup function but does not return its eventual cleanup from the React effect, so copying that effect literally would violate Grain's cleanup requirement.

4. **Migrate every live TinySkia supplement.** Agent summon uses the same nonactivating compact pill; existing transient Enter submits voice through Rust and Escape/cancel performs the existing full cleanup. Remove native typing/click/Tab expansion and the Type to expand production setting. Preserve all existing Agent reply-window behavior, Quick Agent routing and follow-up destination. Move clipboard acknowledgement, follow-up capsule, extension owner indication and active error presentation into the shared window; reuse their backend logic and generation/expiry ownership. Never let a stale notice hide a new capture.

5. **Delete prompt switching and native-only controls.** Remove `prompt_next`, `prompt_prev`, `master_prompt_switch`, `switcher_prompt_next` and `switcher_prompt_prev` from actions, registration, seeded defaults and saved-binding migration. Remove `master_key` and switcher-only functions/events/reverse actions. Remove settings rows/search/localization references and obsolete tests. Preserve selected post-processing prompt behavior and prompt composition (`prompt_stack` is unrelated to the switcher). Preserve `arm_prompt_record`, split transcription and processing paths, but expose no Prompt Record button in the initial migration.

6. **Retire TinySkia and finish the upstream policy.** Remove `grain-pill`, `--pill`, supervisor/restart loops, process-wide stray-pill killing, native presentation code, bundled binaries and dependencies used exclusively by that crate. Retain shared dependencies where other components still use them. Regenerate Rust/JS/Nix lock derivatives and Specta bindings as applicable. Replace the overlay's inert relocation classification with its active shared-module policy, document Grain hooks/React mappings, and update relevant historical verdict notes only with actual completion evidence. No permanent parallel renderer or fallback native engine remains.

7. **Verify the real application and obtain visual approval.** Run the checks below, commit and push each completed phase, and provide the exact real-app command at each visual checkpoint. Cross-platform behavior is accepted only with evidence from that OS. Merge into `main` only on the user's explicit instruction.

8. **Later: Prompt Record shortcut.** Add a configurable recording-only shortcut through the existing action system, invoking the retained split-mark function. There is no current dedicated Prompt Record shortcut in settings on this baseline. Pick a binding separately, release it with session cleanup, and verify Batch/Flow/Native ASR split behavior. This phase follows the initial overlay migration.

## Visual reference

The existing native code is the primary reference: near-black `#1E1E20`, alpha 242 for the pill/246 for cards, a one-pixel `#4B4B4D` rim, full-round compact capsules, Wave and Matrix skins, and Space Grotesk. The live card uses a 420px width, 16px corners and a 15.5px/21px caption scale. Translate these as visual tokens rather than retaining native window/pointer machinery. Handy's permanent cancel control must fit within the Grain presentation.

Existing window bounds (Handy: compact 256×50, streaming 400×120) do not contain Grain's wider live card unchanged. Use a small geometry hook with one source of truth for CSS/native bounds; preserve upstream positioning math. The current code's accent is `255,93,30`. Use supplied reference screenshots to verify the icon treatment, colors and geometry at visual approval; code is the primary reference until those images are available.

Retain `pill_icon`'s foreground/site detection, cache, nonblocking lookup and stale-resolution guards. Its current payload is premultiplied RGBA; a WebView image needs straight-alpha PNG. Reuse its existing `png_data_url` conversion and encode only on icon changes. Keep the fallback dot and clear stale icons between sessions. Stop `surface_watch` and pending resolution on stop/cancel.

## Verification and acceptance

Meaningful automated tests cover settings migration; first-sample readiness/cancel races; stale hide/notice generations; session reset and stream text retention; cancellation of Batch/Flow/Native ASR/Agent; removal of switcher registrations; and preservation of Prompt Record's split processing. Exercise listener cleanup under React StrictMode and verify hidden timers/animation work stops. Avoid tests that merely mirror CSS or implementation details.

Run frontend type checks, lint, unit tests and the production build; Rust formatting, app `cargo check --lib`/`cargo test --lib`, affected `grain-core`/`grain-sdk` tests; and the existing dependency/Nix derivative checks. For shared-tree edits run `python Upstream/ratchet.py --worktree` before commit. Complete `preflight.py`, `policy_check.py`, `port_audit.py`, the applicable frontend/suppressed review audit and the ratchet after commit, following the runbook's fetch and budget rules.

Actual-app cases include: cold/on-demand and always-on microphones; no model/no device/permission failures; rapid start-stop-cancel-restart; all capture modes; disabled overlays; live text and polishing; top/bottom anchoring; theme/skin changes; app/site icon switching; clipboard notice overlap; Agent voice submission into the current result window; Quick Agent follow-up; extension capture; shutdown and owned-process cleanup. Verify hover does nothing and neither the switcher nor native Agent typing expansion can be opened from old saved settings or shortcuts.

Windows requires mixed-DPI/negative-origin monitors, accessibility text scaling and foreground preservation. macOS requires Spaces/fullscreen, work-area placement and Secure Input interactions. Linux requires Wayland layer-shell and X11/fallback placement. Measure the accepted WebView RAM cost and confirm idle/disabled overlays have no continuing animation or microphone-event flood.

The user authorizes the maintained `tests/agent-harness/` runner with isolated profiles, scoped credentials, owned-process cleanup and evidence. That runner exists in the original checkout's feature branch but is absent from this `main` baseline. Before acceptance, integrate the maintained runner and necessary host hooks through a reviewed dependency change or refresh this branch after their merge; do not copy the unrelated feature branch wholesale. Add overlay cases to that runner and exercise the real app only. No browser-only replica or Tauri mock visual harness is permitted.

Real application command for visual checkpoints from the migration worktree:

```powershell
Set-Location C:\Users\watrm\.codex\worktrees\handy-webview-overlays\grain
bun install --frozen-lockfile
bun run dev:asr
```

Use the repository's Windows build environment/target-directory workaround when applicable. Do not terminate the user's existing Grain instance. User visual confirmation is required before accepting each UI phase. RTK was unavailable in this session; enable the installed wrapper when available, otherwise record that tooling limitation rather than treating a failed wrapper invocation as a passed check.
