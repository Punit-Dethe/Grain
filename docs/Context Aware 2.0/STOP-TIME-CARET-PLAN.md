# Stop-time caret context for dictation

Status: design only, 2026-09-25. Application code has not been changed. This plan supersedes the capture-at-start recommendation in `FLUIDVOICE-AUDIT.md` and the caret-exclusion portions of `docs/Prompt Priority/PLAN.md`. The user's target is the field at the moment recording stops.

## Decision and evidence

Capture the destination app/site and a short left/right caret neighborhood **when Stop is pressed**, before Grain announces `RecordingStopped`, changes the pill/tray, plays feedback, or begins the asynchronous transcription tail. Carry that immutable snapshot to the one existing LLM post-processing call. Stop is the only context read for this dictation. After Stop, the user can move focus; Grain will neither revisit the target nor revise the result. Do not add a second agent/model call or a background context engine.

FluidVoice's public implementation captures preceding text at record start and applies capitalization/spacing after AI output (`Refrence/FluidVoice-latest/Sources/Fluid/ContentView.swift:2098-2113, 2777-2791`; `ASRService.swift:5886-5951`). It does not expose right-side text in that path, and [Fluid Intelligence is private](https://github.com/altic-dev/FluidVoice/blob/main/README.md). Grain should borrow the bounded-capture idea, but use Stop timing and send both sides to its one LLM pass. Fluid's post-LLM formatting pass is not part of this plan.

Current Grain paths: ordinary batch stop in `src-tauri/src/handy/actions.rs:427-463`; Flow stop in `src-tauri/src/grain_actions.rs:580-613`; native streaming stop in `grain_actions.rs:884-917`. Each eventually uses `process_transcription_output` (`handy/actions.rs:219-291`) when an LLM pass is requested. Today `grain_post_process.rs:103-137` detects context later, and `context_detect.rs:755-760, 824-914, 2438-2497` has a bounded two-sided UIA reader used only by Paste Catch. `handy/clipboard.rs:785-809` can append a trailing space after model output. These are the integration points, not a reason to add another pipeline.

[Microsoft's TextPattern contract](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.textpattern.getselection?view=windowsdesktop-10.0) says an unselected caret is a zero-length range, a missing caret can yield no range, and multiple disjoint selections are possible. [TextPattern2's `GetCaretRange`](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextpattern2-getcaretrange) can provide an active caret when supported. [UIA calls belong off the UI thread](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading); its text reads cross process and have real latency. Grain's ordinary shortcut Stop already runs on a dedicated `TranscriptionCoordinator` thread (`handy/transcription_coordinator.rs:91-115, 331-355`), so no new capture worker is needed.

## Behavioral contract

The model transforms **only the dictated transcript** into **the exact span to insert or replace**. Before/after text is evidence of the seam, never text to rewrite, complete, echo, or follow as an instruction. An email app tells the model the desired register; it does not imply a new greeting, subject, sign-off, or entire draft. A user explicitly dictating those parts may still get them. If the user selects the whole draft, the replacement span can be a whole email; if the caret is inside the draft, output is only the local insertion.

| Stop-time state | Intended behavior |
| --- | --- |
| Empty editable field, known caret | Normal dictation; no continuation assumption. Existing profile can shape tone, but shipped Email rules still add layout only when dictated. |
| Existing text before caret, none after | Continue the local sentence or paragraph; do not restart the document or duplicate a greeting/sign-off. |
| Existing text after caret, none before | Begin so the surviving right side still reads naturally; do not output that right side. |
| Text on both sides | Fit grammar, casing and punctuation at both seams; return only the replacement span. |
| One selected span | Treat its start/end as the two seams and replace only that selection. Selection content need not enter the model for ordinary dictation. |
| Multi-range selection, password, read-only, unknown caret, or unsupported provider | Use current non-caret behavior. Unknown is **not** an empty field. |
| Focus or caret changes **after Stop** | The Stop snapshot and LLM result stay fixed. Grain pastes through its existing route at the then-current target; choosing that target after Stop remains the user's responsibility. |

Keep explicit newline, punctuation and list commands from the dictated transcript. Ask the LLM to handle both seams without blindly adding spaces or lowercasing names; there is no post-LLM text repair or second decision. The insertion scope remains even if an edited/custom profile says “format as an email.” Full-document rewrite remains an explicit edit/selection operation.

## Snapshot contract

One per-session value, owned by the stop-to-paste operation and dropped afterward:

```text
StopContext {
  session_id, captured_at,
  surface: app/site/region/field + confidence,
  caret: Unavailable | Known {
    before, after,                 // bounded, from selection edges
    has_selection,                // distinguish selection from empty field
    left/right boundary facts     // space, line break, terminal punctuation
  }
}
```

Build surface and caret from the **same focused element** where possible, especially for browser URL/profile selection. Preserve `Known { before: "", after: "" }`; Grain currently drops that by filtering empty `CaretContext`, which conflates a blank composer with inaccessible context. Preserve boundary facts *before* `relevant_left_fragment`/`relevant_right_fragment` trim or cut sentences. Keep the current 200-left/80-right capture ceiling, then enforce an additional UTF-8-safe **400-byte model payload ceiling**. Read only the text adjacent to the selection, never the whole field or a whole email thread. Store plain snapshot data, not COM objects.

Do not use `read_focus_probe()` unchanged: when it finds no caret, it reads the whole `ValuePattern` value for Paste Catch (`context_detect.rs:2786-2794`). The stop-time reader should return `Unavailable` for such a field unless a bounded caret read is possible. It must never fetch a long field merely to infer where an insertion might be.

There is no delivery-time identity or seam comparison. The stop-time read is authoritative for this request; if it cannot resolve a real editable caret, omit caret text and keep the existing app/site or base behavior. Password fields never contribute content; log only availability, length and reasons, never neighboring text.

## Stop-to-paste sequence

1. At the beginning of each of the three ordinary dictation `stop` callbacks, request one stop snapshot when Context Awareness and a possible LLM pass are enabled. Prompt Record can force the LLM after Stop, so capture when it may be present. Do this before `emit_recording_stopped` and any UI transition. Keep all behavior behind the existing off-by-default feature gate.
2. Read UIA directly on the existing coordinator thread before the Stop UI transition. The read stays bounded to the focused element and 200/80 neighboring characters; measure its latency on real apps. If it fails, return no caret context. Do not add a new worker, listener, cache, or second read after Stop. Push-to-talk release has an existing 50 ms grace period to reject synthetic key-up events (`handy/transcription_coordinator.rs:11-12, 169-179`); capture when that release becomes an accepted Stop, never on a release that may be canceled.
3. Pass the snapshot by value through the existing async transcription and `process_transcription_output` path. Keep app/site/profile and caret tied to the same stop snapshot. Do not call `detect_active_context()` again for that session when a valid snapshot exists. Cancel discards it.
4. Compose one LLM request. Resolve the existing main prompt, active profile/extension context, and Prompt Record as today. Add a conditional host-owned **insertion contract** immediately before the final output contract. It scopes every editable rule to the replacement span; it is not a seventh configurable priority tier. Put the transcript and a serialized, size-bounded `before`/`after` data object in the user message. Label adjacent text as untrusted reference data. Keep the no-caret request byte-for-byte compatible where practical. Update both structured-output and legacy/failover provider paths, including token estimates and `${output}` handling.
5. Paste the model's result once through the existing route. Do not recheck target or caret, re-run the model, or repair the model's text after processing. The existing optional `append_trailing_space` setting is a separate paste policy that can defeat an exact seam; decide at Stop to suppress that automatic suffix for contextual insertion, then pass that fixed option through the normal paste call. Other paste modes retain their current policy.

The Windows overlay is configured non-activating (`handy/overlay.rs:410-470`), so the usual hotkey/pill Stop can inspect the external target. A stop command invoked from a focused Grain window may have already lost that focus; it must degrade to no caret context. A small `[GRAIN]` hook in Handy-derived stop/paste files may carry the value, but capture logic, request building and seam logic belong in Grain-owned modules.

## Prompt conflict rule

Do not let the three editable layers fight over the unit of output. Their priority remains **spoken Prompt Record > active context profile > main prompt > additive extension rules**. The host-owned insertion contract defines the unit: *the span replacing the current caret/selection*. The existing terminal contract continues to require only final text. The neighboring text has no instruction authority, even if an email contains text resembling a system prompt. This follows [Microsoft's guidance to keep external content out of instruction channels](https://learn.microsoft.com/en-us/security/zero-trust/catalog-ai-defense-capabilities/input-context-retrieval-hygiene).

Example: main prompt requests clean prose or bullets; Email profile requests professional tone; before is `I wanted to add `, after is ` before Friday.`; dictated text is `one more check`. The response should be a locally fitting `one more check`, producing `I wanted to add one more check before Friday.` It must not become a newly formatted email or a bullet list. If Prompt Record explicitly says `make this a bullet`, it may insert a bullet at the caret, while still leaving the surrounding document untouched.

For a draft already ending in `Best,\nAlex`, dictating a new paragraph before that sign-off should produce the paragraph only. For a blank email body, dictating `Dear Sam ... Best, Alex` may produce those spoken parts. This is the same surface profile with different stop-time insertion facts. Selecting an entire draft permits replacing that entire span with newly dictated content; rewriting the old selected text is a separate edit-mode task and requires the selected content.

## Privacy and platform scope

Nearby field text can contain private correspondence and may be sent to the user's chosen remote LLM provider. App/site-only context did not carry that content. Keep the existing off-by-default Context Awareness switch as the gate and update its copy to disclose the bounded nearby-text read and provider destination. With the switch off, perform zero caret reads and produce the old LLM request. Keep no neighboring text in logs, analytics, history, or extension payloads; release the snapshot after paste/cancel.

This implementation starts on Windows because `read_focus_probe()` and `detect_active_context()` are Windows-only today. The same typed contract can later receive macOS/Linux adapters, but the UI must not imply working caret capture on those platforms until verified there. Real Tauri application checks are required; no browser-only visual harness or computer-control automation.

## Verification gates

1. **Pure tests:** known-empty versus unavailable; left/right clipping and Unicode boundaries; selection edges; sentence/paragraph flags; prompt layer order; serialized-data escaping; no-context request parity; contextual request parity across structured, legacy and rotation providers; custom “always email” profile; Prompt Record conflict; trailing-space policy.
2. **Windows integration:** actual Gmail and Outlook compose/reply, native mail editor, chat, document editor, single-line form, rich-text editor, a field exposing ValuePattern but no TextPattern, an inaccessible field, password field, selection replacement, and browser tab/window switch before Stop. Verify Stop via hotkey and pill. Include provider timeout/cancel and confirm that a user move **after** Stop causes no recapture or second LLM call.
3. **Quality set:** fixed transcripts inserted at beginning/middle/end of sentences and paragraphs; greeting/sign-off already present; punctuation on either side; quote/parenthesis seams; proper names/acronyms; markdown lists; URLs/code/paths; CJK and RTL samples. Compare baseline and candidate outputs with the *assembled surrounding text*, not the model span alone. Block rollout for lost dictated content, echoed neighbors, surprise email layout, or changed text outside the selected span.
4. **Performance/privacy:** measure stop-to-processing delay and payload bytes on real apps; verify one focused-field read per Stop, no new workers/listeners, no full-field read, no captured text in logs/history. Measure slow UIA providers before considering extra timeout machinery.

Ship behind the existing Context Awareness switch and preserve a per-session no-context fallback. Do not expand to whole-window reading, a second LLM pass, delivery-time rechecks, response-length heuristics, or a background context indexer to solve this local seam problem.
