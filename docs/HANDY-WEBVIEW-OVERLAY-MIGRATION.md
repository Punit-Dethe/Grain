# Handy WebView overlay migration

Status: implementation and Windows build verification complete; real-application visual, hardware, RAM and cross-platform acceptance pending. Reviewed 2026-10-04.

Current visual direction: Agent capture also has a static blue/sky-blue base gradient fading upward, and only the waveform pill remains. The user rejected the initial Grain-layout port on 2026-10-04. Use Handy's exact recording-card layout and styling, replacing only its waveform presentation and pink listening dot with Grain's waveform and app/site icon. The original visual plan below is historical and superseded by this correction.

Visual references and placement were confirmed on 2026-10-04. Grain's existing smooth waveform reaction is an explicit retained behavior; live-preview expansion, scrolling and timing follow Handy. Top/bottom placement is approved.

## Baselines and objective

- Branch: `codex/handy-webview-overlays`, created from GitHub `main` at `69807ccef1fcdfb70a796a981ad308733262adea`.
- Handy reference: `73ab851c2b6242283759a4c101b60f0ece132f08`, latest `main` fetched before implementation close-out. Its only change after the planning reference `ffbc9504cbf004ce4819d2ca872fcea92be0fddf` is Ubuntu troubleshooting documentation (#2206); the overlay/capture behavioral reference is unchanged.
- Local reference checkout: `C:\Projects\Grain\Reference\Handy`.
- Before implementation, Grain's `src-tauri/src/handy/overlay.rs` matched that Handy file byte for byte: SHA-256 `875C06C96EF8297C26622FF9B6143863B4350BDB7FACC63004430F3DCCBA27D7`. It is now compiled with narrow marked Grain presentation hooks; upstream platform lifecycle code remains active.

Replace the TinySkia overlay runtime with Handy's actual Tauri WebView window lifecycle and platform handling. Preserve Grain's visual design and foreground application/site icon. Follow `Upstream/UPSTREAM.md` and `Upstream/UPSTREAM-DIVERGENCE.md` for every upstream assessment and port. The user's explicit request establishes this new branch from `main` for the migration.

The branch base was independently checked against `git ls-remote origin refs/heads/main` and the available commit object. Work uses an isolated checkout and the machine's existing Git identity.

## Agreed scope

The user's clarification preserves the existing Agent result window and every other existing WebView surface. The Agent expansion being retired is the **TinySkia summon/input card**, including its typing expansion. The existing `AgentPanel.tsx` conversation and result behavior remains available.

| Surface or feature | Migration outcome |
| --- | --- |
| Compact dictation pill | Grain appearance and smooth waveform reaction; Handy recording, readiness, microphone event delivery, work-state, cancellation and window behavior. |
| Native ASR live preview | Grain appearance and smooth waveform reaction; Handy committed/tentative text handling, expansion speed, timer, scrolling, working state and visibility behavior. |
| Agent summon/input | Uses the same compact pill presentation as dictation, with an Agent-only blue/sky-blue base tint fading upward. Voice submission still opens the current Agent result surface according to the existing Agent settings. |
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

The pre-migration Grain baseline announced capture after `try_start_recording` returned `Ok(())`, used fixed start-cue delays in several paths, and drove the native process through `DaemonEvent`/WebSocket. Implementation restores Handy's readiness and delivery contracts. Grain's waveform dynamics are deliberately retained at the presentation layer under the user's latest instruction.

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
| Icons/themes | `pill_icon.rs`, `surface_watch.rs`, `grain_theme.rs`; reuse existing detection, caches, settings and resolved theme. |
| Switcher deletion | `master_key.rs`, `grain_actions.rs`, `grain-core/src/settings.rs`, `grain-core/src/capture.rs`, `grain-sdk/src/event.rs`, `CaptureModes.tsx`, related manifests/catalogues/tests. |
| Agent input setting | `grain_commands.rs`, command registration, settings schema/defaults, `settingsStore.ts`, `AgentSection.tsx`; preserve reply-panel settings and commands. |
| Native runtime retirement | `events_server.rs`, `events_auth.rs`, `bridge.rs`, `crates/grain-pill`, workspace/app Cargo files, lockfiles, packaging scripts and documentation. |
| Upstream bookkeeping | `relocations.json`, generated policy, `UPSTREAM.md`, `UPSTREAM-DIVERGENCE.md`, affected verdicts/deferred rows, divergence budgets after verification. |

The event server and headless bus also serve extensions. Retire only pill spawning, pill-only credentials/handshake/reverse actions and obsolete welcome frames. Preserve extension transport, filtering and public transcript/audio events. Restore targeted Handy microphone events without double-broadcasting them to WebViews; any retained daemon audio feed needs its own bounded cadence independent of overlay enablement.

## Execution order

1. **Capture baseline and finalize the contracts.** Recheck both remote pins before implementation. Record real-app reference images and the existing surface inventory. Review readiness and the deferred recorder rewrite as one capture contract; settle necessary dependencies before editing. Do not bulk-upgrade the native ASR library merely because current Handy changed it. Keep unrelated deferred VAD, tray and shortcut rewrites separate unless a demonstrated dependency requires them.

2. **Align settings and restore capture readiness.** Retain Handy's stored `OverlayStyle` only for enabled/disabled compatibility and use top/bottom placement. Grain selects compact/live presentation from the capture action chosen by the shortcut; there is no global Minimal/Live selector. The user approved Handy placement: migrate legacy `none` to disabled style with bottom position; migrate `center` to bottom for the affected pill, and preserve old top/bottom choices. Treat both legacy Minimal and Live as enabled; the Streaming shortcut always gets live preview. Preserve Linux's disabled default. Retain the app-icon preference; Wave is the only renderer, with no pill-style preference. Restore first-sample readiness and generation checks across all six caller families, including low-RAM Flow, Agent, extensions and onboarding. An unavailable microphone must fail voice-only Agent input cleanly.

3. **Activate the Handy window and implement Grain presentation.** Restore startup creation, enabled-cache updates, positioning commands and existing platform dependencies. Reuse upstream readiness, show/hide and microphone events. Port Grain's existing `WaveField`, amplitude mapping and bar geometry into a small frontend animation helper so both compact and live forms retain the current smooth response. Use Handy's live-preview expansion and transition timing, plus existing `StreamTextEvent`/`StreamPhaseEvent` for Native ASR; preserve model capability gating and compact fallback. Add Grain chrome, icon and the retained waveform to the owned component. Ensure all asynchronous listeners are actually released: Handy's current component calls an async setup function but does not return its eventual cleanup from the React effect, so copying that effect literally would violate Grain's cleanup requirement.

4. **Migrate every live TinySkia supplement.** Agent summon uses the same nonactivating compact pill; existing transient Enter submits voice through Rust and Escape/cancel performs the existing full cleanup. Remove native typing/click/Tab expansion and the Type to expand production setting. Preserve all existing Agent reply-window behavior, Quick Agent routing and follow-up destination. Move clipboard acknowledgement, follow-up capsule, extension owner indication and active error presentation into the shared window; reuse their backend logic and generation/expiry ownership. Never let a stale notice hide a new capture.

5. **Delete prompt switching and native-only controls.** Remove `prompt_next`, `prompt_prev`, `master_prompt_switch`, `switcher_prompt_next` and `switcher_prompt_prev` from actions, registration, seeded defaults and saved-binding migration. Remove `master_key` and switcher-only functions/events/reverse actions. Remove settings rows/search/localization references and obsolete tests. Preserve selected post-processing prompt behavior and prompt composition (`prompt_stack` is unrelated to the switcher). Preserve `arm_prompt_record`, split transcription and processing paths, but expose no Prompt Record button in the initial migration.

6. **Retire TinySkia and finish the upstream policy.** Remove `grain-pill`, `--pill`, supervisor/restart loops, process-wide stray-pill killing, native presentation code, bundled binaries and dependencies used exclusively by that crate. Retain shared dependencies where other components still use them. Regenerate Rust/JS/Nix lock derivatives and Specta bindings as applicable. Replace the overlay's inert relocation classification with its active shared-module policy, document Grain hooks/React mappings, and update relevant historical verdict notes only with actual completion evidence. No permanent parallel renderer or fallback native engine remains.

7. **Verify the real application and obtain visual approval.** Run the checks below, commit and push each completed phase, and provide the exact real-app command at each visual checkpoint. Cross-platform behavior is accepted only with evidence from that OS. Merge into `main` only on the user's explicit instruction.

8. **Later: Prompt Record shortcut.** Add a configurable recording-only shortcut through the existing action system, invoking the retained split-mark function. There is no current dedicated Prompt Record shortcut in settings on this baseline. Pick a binding separately, release it with session cleanup, and verify Batch/Flow/Native ASR split behavior. This phase follows the initial overlay migration.

## Visual reference

The four supplied screenshots are saved unchanged beside this plan:

| Reference | Role |
| --- | --- |
| [Grain compact](overlay-reference/grain-compact.png) | Current capsule, foreground app/site icon, neutral surface and pale waveform. |
| [Grain live preview](overlay-reference/grain-live.png) | Current caption typography, rounded card, fading text edge, bottom-left app icon, centered waveform and bottom-right cancel control. |
| [Handy compact](overlay-reference/handy-compact.png) | Handy compact form; its pink color is reference material, not Grain's palette. |
| [Handy live preview](overlay-reference/handy-live.png) | Handy live form and control layout; use the actual Handy code for temporal behavior. |

These references are sufficient for implementation. Keep Grain's screenshot appearance and existing waveform reaction; adopt Handy's preview opening/expansion speed, text lifecycle and window behavior. The unchanged clipboard notice can be translated from its existing code; no additional screenshot is required for planning.

The existing native code is the primary reference: near-black `#1E1E20`, alpha 242 for the pill/246 for cards, a one-pixel `#4B4B4D` rim, full-round compact capsules, Wave and Matrix skins, and Space Grotesk. The live card uses a 420px width, 16px corners and a 15.5px/21px caption scale. Translate these as visual tokens rather than retaining native window/pointer machinery. Handy's permanent cancel control must fit within the Grain presentation.

Preserve the existing waveform's presentation math from `crates/grain-pill/src/lib.rs`: finite/clamped RMS of the incoming microphone buckets, noise floor `0.012`, amplitude curve exponent `0.45`, attack/release `0.028s`/`0.14s`, center-to-edge travel `0.22s`, critically damped spring rate `26`, edge taper, bar spacing and minimum height. Its history is only 24 scalar levels at 60 Hz and the maximum is 48 bars. Port this bounded state into persistent frontend refs/arrays and update the existing bar elements from one visible-only animation frame loop. Reset between sessions and cancel the loop on hide/unmount. Do not add Handy's frontend exponential filter in front of this retained response: that would change the reaction the user explicitly likes. The backend still supplies Handy's normalized buckets and targeted, throttled event delivery; there is no new audio capture or DSP service.

Existing window bounds (Handy: compact 256×50, streaming 400×120) do not contain Grain's wider live card unchanged. Use a small geometry hook with one source of truth for CSS/native bounds; preserve upstream positioning math. Source tokens and the supplied Grain images define colors for each element: the existing `ACCENT` constant does not justify recoloring the pale waveform visible in these references.

Retain `pill_icon`'s foreground/site detection, cache, nonblocking lookup and stale-resolution guards. Its current payload is premultiplied RGBA; a WebView image needs straight-alpha PNG. Reuse its existing `png_data_url` conversion and encode only on icon changes. Keep the fallback dot and clear stale icons between sessions. Stop `surface_watch` and pending resolution on stop/cancel.

## Verification and acceptance

Meaningful automated tests cover settings migration; first-sample readiness/cancel races; stale hide/notice generations; session reset and stream text retention; cancellation of Batch/Flow/Native ASR/Agent; removal of switcher registrations; and preservation of Prompt Record's split processing. Verify the ported waveform against the current Rust behavior for attack/release, center-out travel, symmetry, rest/reset and a stalled frame. Exercise listener cleanup under React StrictMode and verify hidden timers/animation work stops. Avoid tests that merely mirror CSS or implementation details.

Run frontend type checks, lint, unit tests and the production build; Rust formatting, app `cargo check --lib`/`cargo test --lib`, affected `grain-core`/`grain-sdk` tests; and the existing dependency/Nix derivative checks. For shared-tree edits run `python Upstream/ratchet.py --worktree` before commit. Complete `preflight.py`, `policy_check.py`, `port_audit.py`, the applicable frontend/suppressed review audit and the ratchet after commit, following the runbook's fetch and budget rules.

Actual-app cases include: cold/on-demand and always-on microphones; no model/no device/permission failures; rapid start-stop-cancel-restart; all capture modes; disabled overlays; live text and polishing; top/bottom anchoring; theme/skin changes; app/site icon switching; clipboard notice overlap; Agent voice submission into the current result window; Quick Agent follow-up; extension capture; shutdown and owned-process cleanup. Verify hover does nothing and neither the switcher nor native Agent typing expansion can be opened from old saved settings or shortcuts.

Windows requires mixed-DPI/negative-origin monitors, accessibility text scaling and foreground preservation. macOS requires Spaces/fullscreen, work-area placement and Secure Input interactions. Linux requires Wayland layer-shell and X11/fallback placement. Measure the accepted WebView RAM cost and confirm idle/disabled overlays have no continuing animation or microphone-event flood.

The user authorizes the maintained `tests/agent-harness/` runner with isolated profiles, scoped credentials, owned-process cleanup and evidence. That runner exists in the original checkout's feature branch but is absent from this `main` baseline. Before acceptance, integrate the maintained runner and necessary host hooks through a reviewed dependency change or refresh this branch after their merge; do not copy the unrelated feature branch wholesale. Add overlay cases to that runner and exercise the real app only. No browser-only replica or Tauri mock visual harness is permitted.

Real application command for visual checkpoints from the migration worktree:

```powershell
Set-Location C:\Users\watrm\.codex\worktrees\handy-webview-overlays\grain
bun install --frozen-lockfile
$env:CARGO_TARGET_DIR='C:\gtc'
bun run dev:asr
```

Use the repository's Windows build environment/target-directory workaround when applicable. Do not terminate the user's existing Grain instance. User visual confirmation is required before accepting each UI phase. RTK was unavailable in this session; enable the installed wrapper when available, otherwise record that tooling limitation rather than treating a failed wrapper invocation as a passed check.

## Implementation evidence — 2026-10-04

- The actual `handy/overlay.rs` window is active. Marked hooks supply the Grain HTML entry, live-card bounds, supplemental clipboard/follow-up states and the independent public audio feed. Grain presentation, capture feedback and shared Windows child-process job helpers live in Grain modules.
- The owned WebView retains the Wave/Matrix skins, original waveform dynamics, font and app/site icons. Wave trajectory tests compare against values evaluated from the removed Rust implementation. Typed Tauri events/commands, listener disposal, visible-only animation/timers and bounded animation arrays replace the native socket/render path.
- First-sample readiness and generation checks cover Batch, Flow, Native ASR, Agent and extension capture callers. Onboarding reaches the same manager contract. Stop/cancel invalidate readiness; failed queued startup cannot leave a visible pill or dismiss a newer capture. Extension API entry points share the existing capture-start gate.
- Clipboard expiry is independent of capture completion, preserves live text/readiness and follow-up state, and cannot hide a replacement session. Live text remains through transcribing/polishing and is released at completion.
- `grain-pill`, multicall startup, its supervisor/credentials/reverse handlers and prompt-switching machinery are removed. Agent's typing preference/entry wiring is hidden; underlying typed submission and Prompt Record split processing remain. `AgentPanel.tsx` and existing extension/result WebViews are unchanged.
- Frontend: TypeScript/Vite production build, ESLint and 117 tests in 14 files passed. Core: 205 unit plus 4 integration tests passed. SDK: 87 tests passed. App: final full Rust suite passed (574 passed, 3 ignored, zero failures). Targeted wire-event, first-sample readiness and overlay lifecycle tests also passed.
- `bun run tauri build --debug --no-bundle` succeeded with the actual frontend embedded in `C:\gtc\debug\handy.exe`. Nix dependency validation and native transcribe-fork verification passed. Specta bindings were regenerated by the real ignored export test; the generated file is intentionally exempt from Prettier.
- Final formatting and UI parity checks passed. `Upstream/preflight.py --committed` passed divergence ancestry/convergence audit, frontend freeze, port audit, policy consistency and the ratchet (39 diverged files within the reviewed budget). Frontend/suppressed review audits passed for the branch range; it merges no new upstream commits. The existing 15-commit unrelated Handy backlog remains separately pending.
- Windows Rust test execution required the existing Common Controls v6 manifest workaround: compile the tests, embed a standard manifest with the Windows SDK `mt.exe` into the disposable test executable, then run it from `src-tauri`. No shipped executable/source workaround was added.
- The graph was unindexed in this worktree; structural review fell back to source tracing and diffs. No mock UI or alternate visual renderer was created. No user process, profile or original working-tree files were changed.

### Divergence budget review

The committed budget was already stale on `main`: `clipboard.rs` was 58 versus 46 allowed lines, `commands/history.rs` 13 versus 6, `shortcut/mod.rs` 388 versus 375, `transcription_coordinator.rs` 1621 versus 1616, and `lib.rs` 1259 versus 1022. These are pre-existing changes; stop-time caret insertion (`b4469e52`), history consistency (`7c3d34fc`), capture shortcut coordination (`69761ef8`) and shared embedding settings (`f70e30b1`) account for the affected backend areas. This migration does not modify clipboard/history/coordinator behavior.

Migration removes old native presentation hooks and reduces actions/manager/transcription/main divergence. Its deliberate growth is first-sample readiness in the recorder, 15 lines of narrow overlay hooks, and app composition/typed command-event registration. Rebaseline only committed, reviewed numbers with `ratchet.py --update --accept-growth`, as required by the runbook; preserve the separately deferred recorder ring-buffer rewrite and unrelated upstream backlog.

### Remaining acceptance

User visual approval, real microphone/foreground/monitor checks, WebView RAM measurement and macOS/Linux acceptance are pending. The maintained Agent acceptance runner is absent from this `main` baseline and depends on different APIs on the original feature branch; it was not copied or run. Automated real-app Agent acceptance requires that reviewed dependency first. Production packaging/signing was not exercised by the successful debug/no-bundle build. Prompt Record's configurable shortcut remains the explicitly deferred next phase.


## Visual correction — 2026-10-04

The user reported an invisible compact waveform, edge-clipped control rows and incomplete live-preview expansion. The first pass recreated Grain's card dimensions and structure; it has not received visual approval.

Correction uses the pinned Handy `RecordingOverlay.css` card rules and `styles/theme.css` palette in the owned overlay. The listening and working row markup, Minimal versus Live card branches, italic caption, caret, timer, placement reversal and transitions match Handy. Only the pink listening dot is replaced by the existing app/site image, and the waveform content uses the retained Grain WaveField/skin renderer. Clipboard/follow-up notices retain their separate presentation; Agent's result window is unchanged.

Remove the Grain 430×140 native override and return to Handy's compact 256×50 and streaming 400×120 window bounds. The Handy card is 172px at rest, 216px when working and 392px when expanded; its caption cap is 64px with upstream viewport clamping. The snapshot reports the shared native bounds instead of computing a second geometry. The retained waveform has a fixed 96px presentation width (25 WaveField bars), so its bar count cannot feed back into its own measured width and collapse. Async event cleanup and hidden animation teardown remain Grain's required integration adaptations.

Non-visual verification passed: the recording CSS prefix and palette match the local pinned Handy files (ignoring whitespace), and the WaveField math is unchanged. TypeScript/Vite build, ESLint, all 117 frontend tests, Rust check, formatting, UI parity and the real Tauri debug/no-bundle build passed. Visual confirmation in the real application is still required; no browser-only replica or UI automation was used.

### Waveform size and neutral accents

The next user correction restores the old compact row's approximately 50.75px slot (13 bars), replacing the 96px/25-bar row with a fixed 51px row in both compact and expanded cards. Bars remain 2px wide with 1.7px gaps, have capsule ends, and now range from 2–14px high; the WaveField dynamics are unchanged. Recording indicators, spinner, caret and related accent feedback use neutral gray, with a darker neutral for light-theme contrast.

The Wave skin's compact listening card is now 136px wide in both Minimal and the unopened Live form, after the user found the initial 124px spacing too tight. Compact rows use equal 10px outer padding and equal gaps between icon/waveform/cancel, removing Handy's extra 5px left inset; the optional Matrix skin retains its existing footprint to fit its wider dot field. Working cards remain 216px and the expanded Live card remains 392px with its existing caption geometry. Frontend build, lint and 117 tests passed for the waveform/accent pass; the full Tauri rebuild encountered a runtime DLL locked by the existing `C:\gtc\debug\handy.exe` process and was not completed.


### Dark palette, native waveform geometry and fixed working gaps

The latest correction fixes the recording surface at `#1F1F21` and its border at `#474749`, with pale gray waveform/spinner/caret and readable caption text in both app themes. Supplemental clipboard/follow-up pills also remain dark; Agent's separate result window is unaffected.

Native source comparison confirmed that the audio filter already matches the retired renderer. The 13-bar Wave skin now draws SVG round-capped 2px segments at exact 3.7px pitch (1.7px space), using the original compact visible height range of 2–23.6096px instead of 2–14px. The existing RMS, attack/release, delay and spring math are unchanged. SVG coordinates avoid competing CSS height animation and flex positioning. Tests compare both 13- and 25-bar attack/travel/release trajectories with the Rust fixture, plus native bar coordinates/heights. The visible-only animation frame loop and microphone listener retain explicit cleanup.

Closed recording and working rows use one explicit 12px gap, 10px symmetric outer padding and 18px/22px control slots. Working width is measured from intrinsic translated text, so Transcribing/Processing expands only by the required label width. Measurement observes the label only while a closed working row exists and disconnects on transitions/unmount. Expanded Live dimensions and caption transitions remain unchanged.

Bottom placement is raised by 8 logical pixels in the native bounds, avoiding clipping from translating content within the tight WebView window. The Grain-owned constant is added to Handy's existing per-platform bottom offset through two marked hooks; top placement, destination-monitor DPI/text scaling and upstream platform behavior remain intact. Windows bounds tests verify destination DPI, negative origins and accessibility text scaling with the new clearance. The overlay divergence budget grows from 14 to 28 lines solely for these two constant hooks and updated expected bounds/comments; no placement algorithm is duplicated.

Verification: TypeScript/Vite production build, ESLint, all 118 frontend tests, affected-file formatting, Rust `cargo check --lib`, all 8 overlay Rust tests and the upstream preflight gates passed. Rust test execution used the documented Common Controls v6 manifest workaround on the disposable test executable. The full Tauri debug/no-bundle rebuild was attempted and failed while copying `ggml-base.dll` into the shared debug output (Windows file lock, os error 32). No user-owned process was terminated. Real-app visual confirmation remains required by AGENTS.md.


### Compact close control preference and additional clearance

Application → Appearance now exposes “Hide close button on compact pill”, default off. The Grain-owned `pill_hide_close_button` boolean is backward compatible with settings files lacking the field, persisted through the existing core store, and delivered through a typed command/event plus the overlay startup snapshot. A stale startup snapshot cannot replace a newer preference event, and the event registration uses the overlay's existing disposal path. The settings updater handles command errors instead of displaying a false saved state.

Only closed recording/working cards omit the X control and its 22px column plus 12px gap. Waveform width, audio filtering, label text, working transitions and outer padding remain intact. Measured Transcribing/Processing widths account for whether the right control exists. As soon as live text opens the expanded card, its X/timer/control layout returns. Matrix uses the same compact preference; clipboard notices, follow-up pills and Agent's separate result UI are unchanged. The new strings use the existing English locale fallback.

Bottom clearance increases from 8 to 16 logical pixels above Handy's original platform offset. The shared placement algorithm is unchanged; expected Windows bounds were adjusted for destination DPI and accessibility text scaling. Handy-tree overlay divergence remains 28 lines; composition-root divergence grows by two registration lines (1270→1272), with the preference logic in owned modules.

The previous height change increased visible waveform excursion by approximately 80% (12px to 21.6096px above the 2px minimum). It did not change RMS normalization, noise floor, amplitude curve, attack/release, outward delay or spring dynamics; this pass makes no waveform changes.

Verification: TypeScript/Vite production build, ESLint, all 118 frontend tests, UI settings parity, affected-file formatting, eight overlay Rust tests, the typed event-name contract, explicit binding export and the isolated legacy-settings/save/reload test passed. The actual Rust test executable compiled successfully. A separate `cargo check --lib` hit Windows os error 32 while copying `ggml-base.dll` into the shared debug output used by the running user-owned Grain instance. It was left running; full native packaging/visual acceptance remains pending a user restart. RTK remains unavailable on this machine, so commands used the documented raw fallback. No visual harness or UI automation was used.


### Shortcut-owned pill routing correction

The migration incorrectly exposed Handy's global Minimal/Live selector and made Native ASR preview conditional on the saved Live preference. The selector is replaced by “Show recording pill” plus the existing Top/Bottom placement; legacy None remains disabled and both Minimal/Live mean enabled. The owned `OverlayPresentation::for_capture` constructor selects Streaming only for `SessionMode::NativeAsr`, and every other mode stays compact, independent of model capabilities or loaded-model identity.

The coordinator resolves and retains the action selected by the shortcut. Standard/Batch uses `transcribe` and `SessionMode::Batch`; Flow uses `transcribe_realtime` and `SessionMode::Dictation`; Streaming/Live Preview uses `transcribe_native_asr` and `SessionMode::NativeAsr`. The Standard/Flow model selection and the streaming selection remain separate (`selected_model` / `selected_asr_model`); the streaming model validation controls whether that shortcut can start, not which pill it chooses. Model selection and engine loading were audited and require no changes.

Verification: TypeScript/Vite build, lint, all 118 frontend tests, settings UI parity, formatting and all nine overlay Rust tests passed. The new regression covers every capture mode against all three legacy style values, text acceptance/rejection and session completion, including the previously failing Minimal + Native ASR combination. Shared-tree budgets are unchanged and the worktree ratchet passes. The Rust test executable compiled successfully; full native packaging and user visual acceptance remain pending the running application's known runtime DLL lock. No visual harness or UI automation was used; RTK is still unavailable.


### Agent capture tint and single waveform renderer

Agent capture cards now use their existing backend-owned `OverlayPresentation.agent` flag for a static sky-blue/light-blue/blue background, fading upward into the dark surface. The gradient adds no timers, elements, animation loops or services; it changes neither card geometry nor waveform sampling/filtering. New capture resets the flag, and Agent input hide clears it. Regular dictation, independent notices/follow-up capsules and Agent result/conversation windows retain their existing styling.

Dot Matrix is retired end to end: its renderer and CSS, Pill Style row/localization, persisted field/default, Tauri command/event, startup snapshot field, SDK type/event and composition registrations are deleted. Loading legacy `pill_skin` values removes the key and persists the cleaned file while preserving other settings. The isolated disk migration test covers both old Matrix and Wave values without requiring a later user edit. The historical skin references above describe earlier phases, not current production behavior.

Pill settings are Rust-owned `AppSettings` values persisted through `AppContext` in the existing settings file: visibility (`overlay_style`, with legacy Minimal/Live treated as enabled), Top/Bottom position, app-icon visibility and compact close-button visibility. React uses the existing settings store and generated Tauri commands; backend events and the startup snapshot deliver runtime presentation. No pill preference is stored in frontend-only state or localStorage. The owned visibility/app-icon commands now use the serialized core update path and propagate save failures; the store checks their typed results for optimistic rollback, matching the compact-close preference. Position continues through the existing Handy command and the same core persistence bridge. Agent's retained type-to-expand field remains hidden and inactive in production.

Verification: TypeScript/Vite production build, ESLint, all 118 frontend tests, nine overlay Rust tests, two typed event contract tests, explicit Specta binding export, all 13 core context tests (including settings reload and retired-style cleanup), all 84 SDK tests and settings UI parity passed. The Rust test executable compiled successfully. No visual harness or UI automation was used. Native packaging remains subject to the running application's previously observed runtime DLL lock; user visual acceptance in the real application is pending. Removing the obsolete module/command/event registrations reduces composition-root divergence from 1272 to 1269 lines; the Handy overlay module is unchanged in this phase.
