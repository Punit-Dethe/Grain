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
