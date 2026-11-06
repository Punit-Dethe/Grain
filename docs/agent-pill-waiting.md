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
