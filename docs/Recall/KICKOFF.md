# Grain Recall kickoff

## Branch and baseline

- Implementation branch: `feat/grain-meet`.
- Parent branch: `cleanup/retire-legacy-extension-platform`.
- Verified parent commit: `30a4e74d4b189b374b2664121de0def2e56e0b25`.
- Native runtime: `Punit-Dethe/transcribe.cpp` at
  `eeffcc3887018015d04d20e61d3698a33d790f24`, version `0.4.0`,
  contract revision `5`, patch identity `grain-flow-tdt`.

The parent and native pin match the uploaded [execution plan](EXECUTION-PLAN.md).
The uploaded plan is preserved with trailing whitespace normalized. This branch was created directly from
the cleanup commit. At kickoff, no meeting application code or model benchmarks had been
implemented. See [progress](PROGRESS.md) for subsequent implementation.

## Initial review

The proposed order puts the uncertain native integration before capture and UI.
Keep Step 0 as the first implementation milestone: saved audio through the real
worker, with reproducible timing, resource, cancellation, and role-transition
evidence. Do not describe source inspection as successful inference validation.

The checked baseline confirms the following integration work:

- `handy/engine_supervisor/worker.rs` stores an ASR `Session`; the Grain request
  hook dispatches only when that session exists. Diarization requires actual
  role-aware loading and dispatch, not just a new request name.
- `grain_engine.rs` accepts an ASR session and currently exposes Flow-model,
  reset, and detokenization queries. Keep meeting policy in Grain-owned modules.
- The supervisor uses a 30-second floor for Grain queries. Long meeting tasks
  need an explicit duration-aware deadline through the same worker lifecycle.
- `tdt_flow.rs` renders final text. Structured token/word results need regression
  evidence that existing Flow text and boundary behavior remain unchanged.

The long-file memory envelope, model quality, platform capture support, and
external reference claims still require the plan's runtime checks. In particular,
Sortformer's complete-track input API does not establish bounded processing RAM.

## Current validation limits

The cloud machine has Bun and Rust for frontend and headless tests. Desktop
validation remains blocked by missing Linux GTK/WebKit/ALSA development libraries,
unavailable administrator access, and no desktop display session. Results from
onboarding on the previous checkout do not validate this cleanup baseline or
Grain Recall. Native/model execution and hardware acceptance are unrun.

Resolve those prerequisites and use licensed saved-audio fixtures for Step 0.
Record actual measurements and the exact Handy hook inventory before advancing
to production storage, capture, or UI. Preserve the backend boundary and use the
existing inference supervisor. UI integration must follow the user's requested
Recall branch and applicable repository guidance without mixing in an unrelated
UI overhaul.
