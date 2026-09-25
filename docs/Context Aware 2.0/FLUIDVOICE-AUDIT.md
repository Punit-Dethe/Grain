# FluidVoice context awareness audit

2026-09-25. Source: the checked-in `Refrence/FluidVoice-latest` tree, compared with Grain's current backend. This is an implementation audit, not a claim about the behavior of FluidVoice's private runtime.

## What the reference actually does

| Concern | Public FluidVoice implementation | Evidence |
| --- | --- | --- |
| Target identity | Captures frontmost app name, bundle ID, window title, target PID, and focus target at recording start. Uses the bundle ID to route a selected or app-bound dictation prompt. | `ContentView.swift:2011, 2080-2113, 2289-2297`; `SettingsStore.swift:1157-1220` |
| App-specific style | The old automatic code/email/chat/browser prompt classifier is commented out. Active public routing uses explicit prompt selections and app bindings; an app binding covers the entire app, not a particular composer or caret position. | `ContentView.swift:2133-2240`; `SettingsStore.swift:805-870, 1157-1220` |
| Nearby text | When continuous spacing or smart capitalization is enabled, captures text **before** the caret at recording start. AX reads the focused value and selected range, with AppleScript fallbacks for Xcode and Notes. No text **after** the caret is passed to the formatter. | `ContentView.swift:2098-2113`; `TypingService.swift:1224-1315` |
| Seam formatting | After AI enhancement, uppercases the first letter if the preceding text is empty or ends a sentence; otherwise lowercases it. Optionally prepends and always appends a space. This is deterministic post-processing, not an LLM prompt with surrounding text. | `ContentView.swift:2777-2791`; `ASRService.swift:5886-5951` |
| AI input | The public cloud-provider path renders a dictation prompt plus transcript as one user message. Its prompt trace explicitly says selected context text is absent in dictation mode. The private provider receives app identity, not a public before/after caret payload. | `ContentView.swift:2289-2469`; `PrivateAIIntegrationService.swift:27-33` |
| Private boundary | FluidVoice's README says Fluid Intelligence, which provides advanced local formatting and context-aware capitalization, is separately maintained and private. We cannot verify its internal prompt, model, or text-context behavior from this repository. | `README.md:44-50` |

The checked-in `FluidVoice-main` copy has the same capture/formatting entry points. Fluid's useful public lesson is **capture target and caret before the overlay changes focus, then do a small seam pass after transcription**. It does not demonstrate full middle-of-paragraph awareness: its public pass sees only the left side and can append a stray space before existing right-side text. Lowercasing the first letter can also alter a proper name or acronym.

## Grain today

Grain's Windows context detection resolves the foreground executable and, for browsers, a focused-element-anchored site with confidence gating. Profiles select one app/site instruction; the shipped Email instruction already says to add greeting, sign-off, subject, or email layout only if dictated. Custom profiles and extension context can override that instruction. `context_detect.rs:159-164, 250-378, 1880-1965`; `prompt_stack.rs:429-575`.

The normal dictation path calls `detect_active_context()` **after recognition, immediately before LLM post-processing**. It passes app/site/region/profile layers and the transcript, but no caret neighborhood. `grain_post_process.rs:59-145`; `context_detect.rs:1584-1743`. Thus it cannot tell a new email draft from an insertion into an existing draft, or distinguish beginning, middle, and end of a sentence.

Grain already has a better bounded Windows UI Automation primitive than Fluid's full-prefix read: `CaretContext { before, after }`, a 200-character left / 80-character right read around the selection, password avoidance, and focus identity in a `FocusProbe`. It is currently used by Paste Catch, not ordinary dictation. `context_detect.rs:690-760, 824-914, 2429-2497, 2759-2801`; `paste_catch.rs:340-465`. Its sentence-fragment reduction is appropriate for prompt cost, but removes a completed left sentence entirely. A future seam formatter must retain separate boundary facts (sentence end, paragraph break, whitespace) before reducing text.

Grain's current `ActiveContext.field` is routing metadata, not a formatting instruction. Tests explicitly assert that a single-line field does not change the prompt. `context_detect.rs:1512-1545, 3423-3433`. On platforms other than Windows, `detect_active_context()` and `read_focus_probe()` return no context, so this audit's direct reuse applies to the Windows path first. `context_detect.rs:755-765, 1759-1770`.

## Diagnosis of the reported email behavior

An email profile is scoped to the application/site, so it fires equally for an empty draft and a caret inside an existing one. The shipped profile tries to prevent invented email structure, but a user-edited profile, a custom profile, an extension, or a model disregarding that wording can still format each utterance as a whole email. The code alone cannot identify which of those produced a particular observed output. The structural cause is clear: the prompt receives no fact that the caret is inside existing text.

## Recommended change, in order

1. **Capture once at record start.** Reuse `read_focus_probe()` when context awareness is enabled, before any Grain UI takes focus. Keep the existing focus identity plus bounded before/after fragments and explicit boundary flags. Release the snapshot after delivery; no background watcher or full-field read. Preserve current opt-in and password behavior.
2. **Validate before use.** At processing/delivery, compare the captured target with the actual paste target. If the field or app changed, drop the caret text. App/site profile selection should use the same target snapshot, avoiding start/end mismatches. Keep a conservative fallback for inaccessible or unsupported fields.
3. **Make continuation a host-owned rule.** Pass the short before/after fragments to the LLM as *reference context*, clearly separated from the transcript, and instruct it to output only the replacement span. Do not ask it to rewrite surrounding text or infer a new email from an existing draft. The rule should apply above any app profile, including edited/custom/extension profiles, while remaining below an explicit per-dictation user instruction. Review this against `PromptStack`'s tier contract before implementation.
4. **Finish the seam deterministically.** Use left and right boundary facts to handle initial capitalization, leading/trailing whitespace, and duplicate punctuation after the model output. Preserve names/acronyms and explicitly dictated casing. Avoid Fluid's unconditional trailing space. Keep this small and inline in the existing post-processing/delivery path.
5. **Test the actual matrix.** Empty field; start/middle/end of a sentence; paragraph break; selection replacement; existing email draft with greeting/sign-off; continuation into following text; punctuation on either side; one-line field; password/inaccessible field; app/focus change during recording; context awareness off; custom mandatory email profile and extension context. Verify both prompt content and final inserted span, including remote-provider failure fallback.

The smallest useful slice is steps 1, 2, and a bounded version of 3/4 for Windows. The broader Context Aware 2.0 plan already lists caret before/after and seam-aware insertion as P4 (`PLAN.md:608-613`); that plan's architecture sections are a design proposal, not proof this path is currently wired.
