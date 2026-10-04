# Overlay warning cleanup and recent pill fixes

Date: 2026-10-04

## Scope

The ten reported Rust messages were compiler warnings about unused imports,
functions or an enum variant. They did not report compilation failures. The
cleanup was committed in `15350754`; its reviewed upstream hook budgets were
recorded in `287d93f8`. The subsequent pill refinements are in `d6330d70`.

## Reported warnings

| Warning | Cause | Resolution |
| --- | --- | --- |
| `context_detect.rs`: unused `ContributedLayer` import | Only tests used the type. | Moved the import into the test module. Production no longer imports it. |
| `handy/secure_input.rs`: unused `Emitter` import | Status emission uses this trait only on macOS. | Gated the import with `#[cfg(target_os = "macos")]`. |
| `agent.rs`: unused `input_submit_text` | The initial expanded Agent typing UI was withheld during the overlay migration, leaving its submission function without a caller. | Retained the requested underlying behavior with a narrowly documented `#[allow(dead_code)]`. This did **not** restore a typing UI. |
| `agent.rs`: unused `input_typing` | The same withheld UI no longer called the voice/typing transition function. | Retained the function with the same narrow, documented allowance for its deferred UI. |
| `events_auth.rs`: unused `CapabilitySet::All` | The retired native-pill reverse channel no longer needed an all-capabilities identity. | Removed the variant and associated branches. Scoped named-capability authentication remains; updated its test to assert the exact grant. |
| `grain_action_session.rs`: unused `dismiss` | The obsolete wrapper had no caller after the presentation changes. | Removed the wrapper. Kept `dismiss_from_view` and native-window cleanup used by the remaining Extension implementation. |
| `handy/overlay.rs`: unused `show_transcribing_overlay` | Grain's presentation uses `show_overlay_state` instead of this convenience API. | Preserved the Handy API with a local compatibility allowance and explanation. |
| `handy/managers/audio.rs`: unused `RecordingReadiness::wait` | Grain uses cancellable `wait_timeout` calls. | Preserved the Handy API with a local compatibility allowance and explanation. |
| `handy/managers/audio.rs`: unused `arm_prompt_record` | The native pill's former button was gone, leaving no production trigger for the existing audio split. | Added the F8 Prompt Record action, wired it to this function, and guarded marking against unsupported, stopping, empty or already-marked captures. |
| `handy/shortcut/mod.rs`: unused `change_post_process_enabled_setting` | Grain keeps the processing layer enabled and no longer exposes its master switch. | Left the command unregistered and preserved the Handy function with a local compatibility allowance. |

The retained allowances apply to specific functions; compiler warnings were
not disabled for entire files or the project. Shared Handy edits are documented
compatibility hooks, with implementation kept in Grain-owned modules.

## Prompt Record: restored trigger and lifecycle

- Default key: **F8**, editable under **Transcription → AI processing**.
- It is registered only during an eligible Standard, Flow or Streaming capture,
  and released on matching Stop, Cancel or completion. It does not reserve F8
  while Grain is idle.
- Pressing it marks the current audio position once. Earlier audio remains
  dictated content; subsequent audio becomes the spoken AI instruction. The
  original capture shortcut still stops recording.
- Agent and Extension captures cannot acquire this split. The marker is
  serialized against recording teardown and rejected before any samples exist.
- Shortcut editing, cancellation of editing and backend switching reconcile the
  actual registered key. Stale session cleanup cannot remove a newer capture's
  key. macOS fallback registration follows the same active-capture restriction.
- No additional persistent worker, service or animation listener was introduced.

## Subsequent visual fixes

### Prompt Record confirmation

After the recorder accepts F8, the existing overlay context confirms the mark
for that session. The pill reveals an orange/amber/yellow bottom-up tint over
420 ms. Reduced-motion preferences make the change immediate. Stale queued
confirmation cannot tint a newer capture or a processing/Agent state; new
captures and completion/cancellation clear the flag.

### Hidden application icon

Turning **Show app icon in pill** off now removes the icon/dot, its left slot
and its adjacent gap from compact listening. This works independently of the
compact X preference. With both disabled, the unchanged waveform has symmetric
10 px outer padding.

Processing and transcribing still expand for their spinner and status text.
Expanded Live preview retains its original indicator slot, neutral fallback
dot, timer and X. An unavailable application icon still uses the fallback dot
when the preference is enabled. The saved setting is read by the backend and
updates the existing overlay context.

## Verification and limits

Warning cleanup: Windows Rust library build resolved all ten reported warnings;
221 grain-core tests, 144 targeted Tauri tests, 124 frontend tests, production
frontend build, lint, generated bindings and settings/upstream checks passed.
Test compilation retained one pre-existing clamshell test-only unused import
warning; it was outside this cleanup.

Subsequent visual fixes: Rust library build, six overlay tests, Prompt Record
and shortcut-conflict tests, binding export, frontend build/lint and 124 unit
tests passed. Settings parity, upstream policy, divergence ratchet and committed
preflight passed. Gradient appearance/timing, real-microphone behavior and
non-Windows hotkey activation still require real-application acceptance; these
checks do not establish visual approval or cross-platform runtime acceptance.

For ownership seams and future upstream review, see
[the overlay maintenance guide](OVERLAY-MAINTENANCE.md).

## Later restoration of typed Agent input

The user subsequently requested the initial typing card back. It now uses a
destroyable, focusable WebView in `grain_agent_input.rs` and `AgentInput.tsx`,
opened by Tab during Agent voice input or a second Agent summon. Tab returns
typing to voice. No mode-switch indicator is rendered in the pill. The two
retained Agent functions above now have production callers, and their dead-code
allowances are removed. This later restoration does not change the reason they
were retained during the warning cleanup. The hidden `Type to expand` preference
remains unused; typing is explicitly opened rather than intercepting printable
keys globally.
