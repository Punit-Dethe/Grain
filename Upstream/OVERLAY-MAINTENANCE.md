# Recording overlay maintenance contract

Current policy, audited 2026-10-04. Read this when upstream changes recording
windows, microphone readiness/levels, capture actions, streaming text, or pill
settings. The process remains [UPSTREAM.md](UPSTREAM.md); this document specifies
what its existing review and verification gates must cover. The pinned Handy
reference, implementation history and build evidence are in the
[migration record](../docs/HANDY-WEBVIEW-OVERLAY-MIGRATION.md).

## Ownership and integration seams

| Responsibility                                                                                                                                                            | Active owner                                                                                                                   | Sync rule                                                                                                                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Native window creation, monitor/DPI/text scaling, nonactivation, topmost/Spaces/layer-shell behavior, placement, readiness event, audio throttle, delayed-hide generation | `src-tauri/src/handy/overlay.rs`, compiled as `crate::overlay`                                                                 | Merge Handy fixes into this active module. Preserve only marked integration hooks; do not copy lifecycle algorithms into Grain.                                                  |
| Capture presentation, session/generation guards, snapshots, Agent ownership, notices/follow-up, cancellation destination                                                  | `src-tauri/src/grain_overlay.rs`                                                                                               | Grain adapter around the shared lifecycle. Re-thread upstream contracts here; do not alias it as `crate::overlay`.                                                               |
| First real sample, cancellation during arming, feedback and mute                                                                                                          | Shared recorder/audio-manager `RecordingReadiness`; `src-tauri/src/grain_capture.rs` adapts callers                            | Check Batch, Flow, Native ASR, Agent, extensions, action sessions and onboarding. Device-open success is not sample readiness. Preserve cancellation after every blocking stage. |
| Bounded PCM/journal callbacks and resampler tail                                                                                                                          | Shared marked recorder/audio-manager hooks; `grain_audio_journal.rs`, `grain_actions.rs`                                       | Preserve journal ownership, overflow failure and tail delivery. The unrelated ring-buffer and Earshot ports remain deferred.                                                     |
| Stream text and working phases                                                                                                                                            | Shared transcription manager emits `StreamTextEvent`/`StreamPhaseEvent`; owned overlay adapts public events/snapshot hydration | Committed prefix plus replaceable tentative tail; retain finalizing text, reject stale sessions, and keep Compact/None from opening.                                             |
| UI layout, caption expansion/scrolling, cleanup, waveform filter, colors and Agent tint                                                                                   | `src/app/overlay/RecordingOverlay.tsx`, `wave.ts`, `overlay.css`                                                               | Grain-owned. Review upstream UI commits as behavioral problem reports and adapt relevant fixes explicitly; never merge/restore the upstream frontend file.                       |
| App/site identity                                                                                                                                                         | `pill_icon.rs`, `surface_watch.rs`                                                                                             | Keep existing Rust detection and per-session cleanup. No foreground-app detection in the WebView.                                                                                |
| Preferences                                                                                                                                                               | `grain-core` settings/context, owned commands, settings store/components                                                       | Persist in Rust; frontend commands write, events/snapshot read. No frontend-only preference or new parallel settings system.                                                     |
| Build/event boundary                                                                                                                                                      | `lib.rs`, `grain_events.rs`, generated `src/app/bindings.ts`, `recording-overlay.html`, `vite.config.ts`                       | Compile the shared lifecycle and owned adapters separately. Export bindings after command/event/type changes. Keep the owned entry in both the native builders and Vite.         |

The shared overlay's marked hooks are limited to the owned entry document,
shared-bounds snapshot, supplemental notice/follow-up state access, public audio
delivery, and the Grain-owned 16 logical-pixel bottom clearance. Its two platform
offset hooks and updated bounds fixtures do not replace Handy placement logic.

## Product contract to preserve

| Saved preference      | Standard (Batch / Flow) | Streaming shortcut |
| --------------------- | ----------------------- | ------------------ |
| `none` (None)         | Hidden                  | Hidden             |
| `minimal` (Compact)   | Compact                 | Compact            |
| `live` (Live preview) | Compact                 | Live text          |

Agent capture is compact when enabled, with its blue/sky-blue base tint.
Its result/conversation window stays separate. Compact/Live preference changes
apply to the next capture; None hides the active pill. Top/Bottom placement is
independent of this preference. Selected models choose the internal Standard action, not the pill presentation.
`selected_model` drives Standard; `selected_asr_model` drives Streaming.
Alt+Space (Option+Space on macOS) starts Standard or reviewed Parakeet TDT
v2/v3 Flow automatically; all speech-to-text runs locally.
Translate to English retains Standard because Flow cannot translate.
The coordinator resolves through `grain_dictation_routing.rs` at start and
retains that action for stop/PTT release even if settings change.
`transcribe_realtime` remains an internal action with no binding or model-change
shortcut reconciliation. AI can borrow Standard or Streaming.

Preserve the always-dark `#1F1F21` surface and `#474749` border, neutral gray
accents, foreground icon, and original Grain waveform response. Waveform is the
only renderer: 13 round-capped 2px bars in a 51px slot, 1.7px gaps, original
RMS/noise-floor/attack/release/travel/spring math. Expanded text uses the retained
Handy-style transitions and scrolling. The compact close-button preference
removes only the X and its space; working labels remain, and expanded live text
keeps the X.

TinySkia runtime/supervisor, Dot Matrix/Pill Style, prompt switcher machinery and
hover controls are retired. Typed Agent capture expansion is unavailable in
production; its underlying submission capability is retained. Prompt Record
processing is activated by the saved `prompt_record` binding (F8),
shown last in Transcription → AI processing; it has no pill button. Do not revive retired code during
conflict resolution.

## How an upstream change is reviewed

1. Run preflight and follow the normal sync/close-out runbook. Read the deferred
   queue before interpreting ancestry as runtime parity.
2. `relocations.json` routes overlay, recorder, audio manager, transcription,
   actions and settings changes to the active Grain destinations. Record each
   touched source through `verdict.py --port`; the generated divergence table
   must stay in sync. Merging shared Rust is not evidence that owned callers/UI
   inherit the fix.
3. The frozen-frontend review covers upstream overlay components/CSS as problem
   reports. Record the failure mode, applicability, actual owned destination and
   verification with `--frontend-review`. Keep UI files Grain-owned.
4. Use `ported` for adopted behavior, `not-applicable` for excluded behavior or
   an already-covered bug class, and `deferred` for intentionally pending work.
   A deferred outcome is accepted only while the full commit SHA has a queue
   row in `DEFERRED-PORTS.md`. Completing it requires updating the port outcome
   and removing the queue row; otherwise the port audit fails.
5. `policy_check.py` gates the compiled shared module, separate adapters, owned
   entry wiring, required review routes and retirement of the native/Matrix
   renderer. It already runs in local preflight and runtime-port-policy CI.

## Verification when the affected behavior changes

- Run the existing upstream process tests and preflight. Do not accept budget
  growth before reviewing each shared-file diff; tighten budgets after commit.
- Frontend build/lint/tests, settings UI parity and Specta export for contracts.
- Rust overlay/session tests (including the 12 preference/mode combinations),
  first-sample/no-sample Stop tests, readiness cancellation, and settings
  migration/save/reload. Add focused regressions for the actual upstream bug.
- In the real app: exercise both shortcuts and both internal Standard paths with each preference, startup
  and stop/cancel, transcribing/processing, live text and scrolling, close-button
  visibility, Agent capture/result, clipboard/follow-up, missing microphone and
  rapid restart. Verify per-session listener/timer/frame cleanup.
- For platform changes: validate the affected OS, monitor/DPI/text scaling,
  fullscreen/Spaces or layer-shell, focus and placement. Record unvalidated
  platforms in the review; passing Windows tests is not cross-platform evidence.
- Measure RAM when relevant; replacing TinySkia with a WebView was an explicit
  maintenance compromise, not a measured RAM improvement.

Windows real-app visual behavior was accepted by the user after the final
preference correction on 2026-10-04. Cross-platform acceptance and hardware/RAM
measurements remain separate. Keep the user's running process intact if it
locks runtime DLLs; use the existing test-executable workaround or restart with
the user. Never create a browser-only visual replica.

## Measured divergence change

Compared `69807ccef1fcdfb70a796a981ad308733262adea` (branch base) with
`00eef3202de748d4f9eb4c43abe7968bfcfafdb5` (accepted implementation), using
blob-to-blob ratchet measurements against the same upstream merge base
`8f9cf53cd1410cda26beea39ff802ac306e39585`. These are actual measurements,
not old budget ceilings. Grain-only files are outside this shared-code metric.

| Shared Grain path                                     | Before | After | Reason                                                                                               |
| ----------------------------------------------------- | -----: | ----: | ---------------------------------------------------------------------------------------------------- |
| `src-tauri/Cargo.toml`                                |    142 |   141 | Retired native pill integration.                                                                     |
| `src-tauri/src/handy/actions.rs`                      |    688 |   654 | Restore shared readiness; replace native-presentation calls with owned seams.                        |
| `src-tauri/src/handy/audio_toolkit/audio/recorder.rs` |   1270 |  1281 | Real-sample acknowledgement restored within the existing bounded recorder.                           |
| `src-tauri/src/handy/input.rs`                        |     43 |    39 | Shared cursor-position helper is active again.                                                       |
| `src-tauri/src/handy/managers/audio.rs`               |    382 |   345 | Restore upstream readiness/generation contract around retained capture hooks.                        |
| `src-tauri/src/handy/managers/transcription.rs`       |   1084 |  1076 | Restore working/text delivery with owned public-snapshot hydration.                                  |
| `src-tauri/src/handy/overlay.rs`                      |      0 |    28 | Formerly inert; now compiled with narrow entry/bounds/audio/clearance hooks.                         |
| `src-tauri/src/lib.rs`                                |   1259 |  1269 | Shared WebView bootstrap plus owned command/event registrations; retired skin registrations removed. |
| `src-tauri/src/main.rs`                               |     16 |     9 | Retired native pill multicall startup.                                                               |

All other shared-file costs were unchanged. Total: 9813 → 9771 (-42 lines);
38 → 39 diverged files because the formerly inert overlay gained active hooks.
The follow-up maintenance audit removes misleading native-overlay comments;
its further budget tightening is recorded separately in `budget.json`.

Maintenance-audit verification: 16 upstream process guard tests cover valid
wiring, disconnected entries, missing readiness routes, native-renderer return,
tracked versus untracked deferrals, and legacy evidence handling. Policy,
runtime-port and full preflight gates pass. Removing obsolete comments tightens
`shortcut/mod.rs` from 388 to 387 and `lib.rs` from 1269 to 1266; runtime
algorithms and accepted UI are unchanged. `verdicts.json` keeps the existing CLI
serialization (its Prettier mismatch also exists at the baseline).

## Standard routing update (2026-10-04)

The two capture bindings are Standard (`transcribe`, Alt+Space / Option+Space)
and Streaming (`transcribe_native_asr`, Ctrl+Space).
Flow is selected internally; its old development binding is dropped and the
Standard default is enforced directly, without transferring custom Flow keys.
Model switches, downloads, rescans, deletion and translation changes no longer
reconcile a separate Flow hotkey. No extra engine or background service is added.

Verified: 209 grain-core tests, 8 coordinator tests (including stopping Flow
through the Standard trigger), 2 shortcut-conflict tests, 9 overlay tests,
118 frontend tests, Rust test compilation, production frontend build/lint,
Specta binding export, settings parity and upstream policy/preflight. Real-app
acceptance of the unified shortcut remains a user check; the earlier Windows
visual acceptance covers the pill presentation only.

Relative to `10dd1d41`, the shared-code ratchet decreases by 30 lines:
`commands/models.rs` 74→57, `shortcut/handler.rs` 47→40,
`shortcut/mod.rs` 387→383 and `transcription_coordinator.rs` 1621→1619.
The new routing policy and host installation seam remain Grain-owned.

## Standard/Streaming feedback update (2026-10-04)

Standard is the unified Alt+Space shortcut; Streaming uses Ctrl+Space. Both chords are supported by HandyKeys and Tauri.
Ctrl+Windows was considered and dropped because Tauri requires a main key.
The UI descriptions are “Speak, then paste text when you stop.” and
“See text live as you speak.”

Flow uses the same start/stop feedback calls as Standard and Streaming:
`grain_capture::announce_ready(..., true)` plays Start after real-sample
readiness and before mute; Stop calls the shared `play_feedback_sound` after
`remove_mute`. Shared Handy audio-feedback enable/theme/volume/output-device
settings and playback cleanup apply. Cancellation still invalidates readiness;
startup failure does not announce a ready capture.

Verified: Ctrl+Space parses with both shortcut backends and does not collide
with the other default capture/Agent keys (3 shortcut tests); real-sample/Stop
readiness (1), coordinator (8), overlay (9), settings migration (8), Specta
export, frontend build/lint/118 tests, settings parity, formatting and upstream
preflight pass. Rust test compilation used a command-local resource override
to avoid recopying DLLs locked by the running app; production configuration is
unchanged. Audible Flow feedback and OS hotkey activation remain real-app checks.

## Prompt Record shortcut and warning cleanup (2026-10-04)

The normal shortcut registry dispatches `PromptRecordAction` in `grain_actions.rs`.
It marks an ongoing Standard, Flow or Streaming capture once; it starts no new
recording and releases no capture key. The original capture key still stops it.
`grain-core::capture::supports_prompt_record` owns eligibility. The existing
`AudioRecordingManager::arm_prompt_record` hook holds the recording-state lock
through marking, excluding idle/stopping, Agent and Extension sessions and
serializing against Stop/Cancel/restart. Zero-length capture cannot arm. Registry calls use short-lived jobs on the existing runtime; no persistent
worker, service or listener is introduced. Existing split/ASR/AI paths consume
and clear the mark. Backend defaults seed the binding into existing profiles;
normal `change_binding` persistence, shortcut-backend switching and macOS
Secure Input fallback apply. `prompt_record` is a dynamic binding: no OS hotkey
at idle/startup. Normal dictation start claims it; matching Stop/Cancel/completion
releases it. The owned helper remembers the actual chord/backend, restores after
shortcut editing (including cancel/unmount), and excludes stale terminal events
from newer captures. The editor suspension flag also gates Carbon shadows. macOS
Carbon shadow registration uses the same active-capture gate. Rebinding cannot overwrite capture/Agent/cancel keys.

The removed native-pill reverse channel left an unused all-capabilities grant
and an obsolete Extension dismissal wrapper; both are removed. Scoped worker
authentication and native-window dismissal remain. Typed Agent submission and
typing are retained with narrowly documented dead-code allowances, as required
by the deferred expanded UI. Handy's `wait`, `show_transcribing_overlay` and
unregistered post-processing master setter remain compatibility APIs with local
allowances; do not re-expose that setter. `Emitter` is imported only on macOS.
`ContributedLayer` is now imported inside the tests that use it.

Verification: Windows library build resolves all ten reported compiler warnings;
test compilation retains only the pre-existing clamshell test-import warning.
221 grain-core tests, 144 targeted Tauri tests (context, Agent, authentication,
capabilities, coordinator, overlay, shortcut conflicts and stale-session cleanup),
124 frontend tests, frontend production build/lint, Specta export, settings
parity and runtime-port policy pass. The F8 default parses on both backends.
Real-microphone Prompt Record behavior, visual acceptance, and non-Windows host
activation remain real-application checks.

## Prompt Record tint and optional icon geometry (2026-10-04)

The owned `OverlayPresentation` context/snapshot includes `prompt_recording`
and `app_icon_enabled`. `PromptRecordAction` publishes confirmation only after
`arm_prompt_record` succeeds; the presentation accepts it only for the same
recording session, outside Agent/working states. Completion/cancel clears the
tint, and new capture construction resets it. No public SDK event, shared Handy
change, additional listener or animation worker is required.

Grain's WebView reveals an orange/amber/yellow variant of the Agent base tint
upward over 420 ms using opacity/transform. Reduced motion changes it immediately.
The app-icon setting updates the existing context immediately and is hydrated
from the saved setting. Compact listening removes its left slot and 12px gap
when disabled, independently of the compact X preference. With both hidden,
only the original 51px waveform and symmetric 10px padding remain. Expanded
Live retains its indicator (neutral fallback dot when icons are disabled),
timer and X; working states retain the spinner and measured label width.
An unavailable icon with the preference enabled still uses the fallback dot.
Keep these distinctions when reviewing upstream overlay changes.

Verification: the Rust library build, six overlay tests, Prompt Record and
shortcut-conflict tests, generated binding export, frontend build/lint and 124
unit tests pass. Settings parity, upstream policy and divergence ratchet pass.
Gradient timing and compact/expanded visual acceptance require the real app.

## Clipboard notice timing (2026-10-04)

`grain_overlay::on_event` delays only `PasteMissed` presentation by 400 ms,
allowing Handy's 300 ms recording-window hide to finish. Clipboard publication
and the deliver shortcut remain immediate. The existing notice generation
rejects pending presentation after a new capture, replacement notice, follow-up
or clear. `PasteMissedClear` now also clears pending/displayed feedback when the
held transcript is delivered, superseded or expires. The three-second notice
lifetime starts when shown. The timer uses the existing async runtime; shared
Handy lifecycle code and the experimental Agent typing branch are unchanged.

Verification: six overlay regression tests, normal Rust library build and
format/whitespace checks pass. Notice timing remains a real-app visual check.
