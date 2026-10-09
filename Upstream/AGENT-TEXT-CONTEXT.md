# Agent text-context retirement

## Scope and plan (2026-10-04)

1. Trace the Agent Context modes and shared readers before deleting code.
2. Remove Unique terms, Full field text and Whole window text from Agent UI,
   settings, commands, capture state and prompt composition.
3. Delete Agent-only readers and extraction; preserve active shared consumers.
4. Regenerate bindings and verify persistence, Agent framing, dictation context,
   frontend and upstream gates; independently audit the deletion.

## Removed

- `AgentContextMode`, `agent_context_mode`, its setter/registration, UI dropdown
  and frontend settings updater.
- `FieldContext`, its retained mutex, `capture_field_context`, summon-time
  background capture, full-field size cap and background prompt layers in both
  Quick Agent and the panel/tool conversation path.
- `extract_unique_terms`, its static English stop-list and caps, and the two
  tests of that deleted feature.
- `read_focused_text`, UIA `read_focused_value`, their only full-document helper
  `read_text_content`, and its `cap`/`MAX_TEXT_CHARS` machinery.

The removed functions had no remaining production caller. No compatibility
mode, hidden setting or replacement collector exists. Obsolete development
settings are ignored during deserialization and omitted on the next save;
unrelated settings remain intact.

## Retained consumers and boundaries

- Agent's explicitly selected text remains the subject of a user instruction.
  Conversation turns, tool calls, confirmation, paste, reply surfaces and
  existing LLM fallback remain supported.
- The separate opt-in **See my screen** image setting and image lifecycle stay
  supported. This request removed the text Context dropdown, not that feature.
- `capture_stop_context`, `read_caret_ranges`, `read_stop_caret`, surrounding
  character budgets and insertion prompt composition remain dictation-owned.
- `read_focus_probe` and `focused_has_selection` remain in use by Paste Catch
  and result actions. Their shared `read_value`/`read_name`/password checks and
  UIA/COM lifetime helpers remain in use by these paths and app/URL detection.
- `read_window_text` remains because `host_api` calls it for the granted
  extension `capture.screenText` API. The SDK still exposes that capability.
  It reads the accessibility tree, not OCR. Its window/element bounds,
  deduplication and password guards remain. Agent no longer calls it at summon.
- `context_screen` remains used by image capture and the extension
  `capture.screenImage` API. No unused OCR engine or dependency was found.

All changes live in Grain-owned modules/frontend/settings; Handy's tree is
unchanged. `lib.rs` only loses the retired command registration. The settings
relocation map points to AgentSection so upstream review preserves this
retirement. Never reintroduce automatic Agent field/window reads while adapting
Handy settings or context behavior.

## Verification

The persistence regression loads every obsolete Agent context mode, saves and
reloads, then verifies that it is omitted without resetting dictation context,
Agent image/Quick settings or language. The Agent prompt regression checks only
system, selected-text framing and supplied conversation turns, including role
normalization and absent/blank selections. Existing shared context tests cover
caret budgets, multibyte boundaries, focus verdicts and insertion contracts.

Real-app acceptance: close Grain, then run:

```powershell
cd C:\Users\watrm\.codex\worktrees\handy-webview-overlays\grain
$env:CARGO_TARGET_DIR='C:\gtc'
bun run dev:asr
```

Confirm the Agent Context row is absent, selection-based rewrite and follow-ups
still work, and normal dictation context remains available. The separate image
switch remains available. No browser-only harness or UI automation was used.


### Recorded results (2026-10-04)

- 213 grain-core unit tests and four capability benchmark tests pass, including
  obsolete context-mode persistence without resetting retained preferences.
- 101 targeted Tauri tests pass: Agent 6 and shared context 95, covering prompt
  framing, selection, focus/Paste Catch decisions, caret bounds and insertion.
- Full Tauri test-library compilation, Specta export, TypeScript/Vite production
  build, lint, 120 frontend tests, settings parity (83 fields), changed-source
  formatting, policy consistency and upstream preflight pass.
- Independent read-only audit found no outstanding correctness or cleanup
  findings. It verified retained shared consumers and complete retired wiring.
- Shared `lib.rs` divergence decreases from 1259 to 1258 lines because the
  retired command registration is deleted; budget.json records that reduction.
  Agent and shared context deletions are in Grain-owned files, outside this
  Handy shared-code metric.

Verification ran on Windows. A command-local Tauri resource override avoided
copying locked runtime DLLs; production config is unchanged. The audit was
static; real-app selection/image/clipboard behavior and Windows accessibility,
macOS and Linux runtime behavior were not exercised.
