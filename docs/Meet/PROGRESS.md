# Grain Meet implementation progress

## Increment 1: structured bounded TDT evidence

Implemented in `src-tauri/src/tdt_flow.rs` on `feat/grain-meet`, based on
`30a4e74d4b189b374b2664121de0def2e56e0b25`.

- Added an opt-in accumulator constructor and structured final result. Native
  transcripts retain token IDs/text, raw text, confidence (including unavailable
  NaN), timing (including zero-duration points), word/segment membership and
  original word rows. Each batch retains its exact analysis-audio window.
- Final text still uses one detokenization of the complete ordered token ID
  sequence, followed by the existing trim policy. No per-window text joining,
  word timestamp fabrication, or repeated-token deduplication was introduced.
- Existing Flow construction retains compact tokens only. Full evidence is
  collected only when requested before decoding. Audio buffers remain bounded
  by the existing window geometry; retained transcript evidence grows with output.
- Invalid timing commits no partial batch. Failed native calls and finalization
  make an accumulator terminal; replay requires a new accumulator so the native
  decoder resets instead of reusing potentially advanced state.
- No Handy-derived file, model pin, application manifest, lockfile, UI, or capture
  path changed. The structured API is a foundation for the pending meeting
  pipeline; it is not yet connected to a meeting command.

Native word rows are **window-local evidence**, not final joined meeting words.
Words/byte-fallback sequences may cross windows. Canonical word assembly needs
native tokenization semantics and real-model boundary fixtures before use for
speaker assignment. Source/track/session mapping belongs to the later meeting
pipeline; current window coordinates refer only to its 16 kHz analysis journal.

## Validation

The actual production adapter and journal modules compiled against a clean
checkout of the exact pinned `transcribe.cpp` source, with its real CPU native
library. No native API replacement or Tauri UI mock was used.

Reproducible command (Cargo, CMake and a C++ toolchain required):

```sh
python3 scripts/test_tdt_adapter.py --native-source /path/to/transcribe.cpp
```

The runner verifies the native commit and tracked-source cleanliness, creates a
disposable Cargo test target outside the checkout, imports the production
modules, and preserves Cargo's exit status. It leaves application dependency
files unchanged. Test-only dependencies resolve separately from the desktop
application; this is not a full application build or packaging check.

Result: **16 passed, 0 failed, 1 ignored**. The ignored test requires explicitly
provided local Parakeet models and WAV fixtures. Tests cover compact Flow replay,
repeated tokens, incremental versus saved-journal window partitioning, native
metadata preservation, point timing, malformed timing, empty input, failed-call
reuse rejection, buffer capacity, and journal lifetime/range behavior.

Also passed: `python3 scripts/transcribe_fork.py --check`, Python compilation of
the runner, Rust formatting of the changed module, and `git diff --check`.

The tests inject inference outputs to exercise adapter logic. Compiling/linking
the native library does not establish successful model execution or real-model
text parity. Frontend files were untouched; no new frontend acceptance is claimed.

## Step 0 remains open

No Sortformer/Parakeet model execution, real-worker role switching, final word
assembly, cancellation during native execution, accelerated fallback, recognition
quality, or 10/60/120-minute process-tree memory measurements have passed yet.
No production duration or RAM envelope is established.

Next required work is a role-aware extension of the existing worker, followed
by saved-audio model tests. Resolve the buildable real-worker prerequisite first:
this cloud image lacks the documented Linux desktop development libraries and
administrator access. A licensed/reproducible meeting fixture set and verified
Sortformer/Parakeet artifacts are also needed for the model gates. Neither the
CPU adapter runner nor documentation closes those gates. Do not expose capture
or UI as ready until the execution plan's dependencies and acceptance checks pass.
