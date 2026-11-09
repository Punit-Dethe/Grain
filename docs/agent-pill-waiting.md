# First Agent reply handoff

Keep the existing warm Agent renderer hidden while it executes the first turn,
with the submitted capture pill showing an animated blue/violet/orange base and
rotating working labels until a reply or error has committed in React.
Bind queued instructions, errors, execution admission and reveal to the summon
generation so Stop cannot restart a late transcript or reopen a closed card.
Retain only task-owned state and the visible label timer, respect reduced motion,
and release Enter while waiting before rearming reply controls on presentation.

Validation: 26 Agent tests, seven overlay tests, four waveform tests, generated
command export, production frontend build, ESLint, formatting and Rust build;
Clippy retains the existing warning baseline.
Real-application visual approval remains pending: verify capture colour,
submission-to-working continuity, reply/error reveal and Stop before replying.

Enter regression: HandyKeys delivers the submit callback on its manager thread,
but shortcut registration synchronously waits for that manager's response.
Queue the working-state shortcut update on the existing main loop, guarded by
summon generation and shortcut ownership, so the callback can return and the
manager can process registration and subsequent dictation hotkeys.

Dictation with Agent open: only active Agent input or a busy recorder blocks
dictation startup. An Agent reply window can stay open while dictating into
another app. In the focused, expanded panel, dictated text appends to the
follow-up draft while a reply is running. Sending waits for that reply to finish.

## Top notch

Agent now opens one window at the top center of the monitor work area. It
shows the microphone waveform during capture, rotates status text after
submission, then grows the same shell when the first reply or error arrives.
The starting logical sizes are 300 × 54 for capture and 580 × 460 for replies.
Both are clamped to the work area. Long replies and follow-ups scroll inside
the expanded notch.

The notch reuses the dictation pill's waveform and explicit dimension
transitions. Agent no longer uses the dictation pill or either legacy reply
layout. The first reply stays tied to its summon generation, and closing the
notch releases the Agent capture or request independently of dictation.

Visual handoff: use `bun run dev:asr` and check the top-edge entry, the
recording-to-working handoff, the reply expansion and scrolling, and dismissal
from capture and working states.

## Notch refinement

The recording notch is 300 × 38 logical pixels and submits with Enter. The
reply notch is 680 × 360, with a borderless composer below the scrolling
conversation and reply actions aligned to its right. The composer stays
available for typing or dictation while Agent is replying. Enter sends a
nonempty draft when Agent is ready. Shift+Enter inserts a newline.

Removed the follow-up shortcut and its temporary ownership of other chords.
The composer is available as soon as the first reply is shown.

## Windows expansion flash

The notch opens with its reply-sized WebView viewport on Windows. A native
region limits drawing and mouse input to the compact notch during capture and
the first reply wait. At reveal, Grain releases the region and lets CSS grow
the shell around an already sized reply. Grain focuses the visible window once
before the transition when dictation is idle.

Live check: submit an Agent request, watch the first reply expansion, and
confirm you can click the app under the area beside or below the compact notch
while Agent is working.

## Compact notch startup

The fixed Windows region above was retired after it clipped the compact notch
on the user's machine. The native window starts at the compact dimensions
again, with a transparent entry document that paints the shell before React
loads. The shell is queued before selection and microphone setup. It shows
Starting until the microphone opens, and Enter leaves startup alone until
there is a capture to submit. At reply time, one native bounds change
precedes the CSS expansion. The window is shown once and uses no shadow.

## Windows reply handoff tests

Preserving Windows client pixels and disabling automatic reply focus both
left the glitch visible in live testing. Restored the original resize flags
and automatic focus. The user saw the compact notch appear to shrink or
disappear before the reply expanded. React retains the same shell across
the handoff. The remaining transition to isolate is the native viewport
resize before CSS starts the expansion.
