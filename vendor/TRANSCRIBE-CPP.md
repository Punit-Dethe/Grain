# Grain transcribe.cpp vendor patch

These published `0.2.3` wrapper/sys trees are the frozen pre-fork rollback
baseline. Grain's active dependencies now resolve to the paired immutable Git
fork in `native/transcribe-fork.json`; see `docs/TRANSCRIBE-CPP-FORK.md`. Do not
update both implementations. Retire these directories only after the remaining
model/platform/package gates pass. This record retains their original additive
Parakeet TDT Flow patch and pristine provenance.

## Pristine baseline

- Upstream: <https://github.com/handy-computer/transcribe.cpp>
- Published crates: `transcribe-cpp 0.2.3`
- Upstream commit: `63a44d9239d610b3908e8a66b384924cd4a77217`
- Release/tag commit: `v0.2.3`, `63a44d9239d610b3908e8a66b384924cd4a77217`
- Published crate SHA-256:
  - `transcribe-cpp-0.2.3.crate`: `b405c121ef674311b9e8eba83e45d859663da681952c7b95b730792ffde358e7`
  - `transcribe-cpp-sys-0.2.3.crate`: `00e81030804f0ce2dd83761ba18e90e0c0c0c2794d68ca008e24245ed6d2702e`
- `.cargo_vcs_info.json` in both trees records the same release commit.

The archive hashes were independently verified against downloaded official
crate payloads and crates.io records on 2026-09-26. The earlier recorded hashes
were incorrect; source comparison confirms the narrow functional patch below.

`src-tauri/Cargo.toml` pins both dependencies to exact `=0.2.3` and redirects
them with `[patch.crates-io]`. The wrapper path dependency points only to the
matching sibling sys tree.

## Grain patch contract

The permitted divergence is deliberately narrow:

1. Generic `transcribe_detokenize`: a two-pass, caller-buffer API that decodes a
   complete token-ID sequence. Flow uses it instead of concatenating individual
   token strings, which is required for Parakeet v3 byte-fallback Unicode.
2. `TRANSCRIBE_EXT_PARAKEET_TDT_WINDOW` (`PKFW`): one stateless Parakeet window
   request carrying decode start/end frame, absolute timestamp offset, and final
   tail policy. Native code accepts only architecture `parakeet` with variants
   `tdt-0.6b-v2` or `tdt-0.6b-v3`.
3. The opt-in decoder mirrors FluidAudio's TDT controls: fixed duration bins,
   10 symbols per frame, 150 main-loop tokens, repeated-nonblank zero-duration
   advance, blank predictor-projection reuse, post-advance activity checking,
   and the bounded final-tail loop. Each call starts with fresh predictor state.

Grain keeps capture, journaling, window scheduling, overlap merge, replay,
cancellation, model catalog policy, and post-processing outside this vendor
tree. There is no native Flow handle, queue, timeline, or state-carry protocol.

The decoder behavior is adapted from FluidAudio commit
`3fd63887eef1dc25edea8263ce4b44aa854d898b`. Attribution and the Apache-2.0
license are retained in `GRAIN-FLUIDAUDIO-NOTICE` and
`GRAIN-FLUIDAUDIO-LICENSE`; both are included in the sys package allowlist.

## Binding and ABI regeneration

The Rust xtask is restored from the exact upstream release. Its only portability
change accepts Windows and Unix path separators in the bindgen header allowlist.
Run from the repository root:

```text
cargo run --manifest-path vendor/transcribe-cpp-sys-0.2.3/bindings/rust/xtask/Cargo.toml -- bindgen
cargo run --manifest-path vendor/transcribe-cpp-sys-0.2.3/bindings/rust/xtask/Cargo.toml -- bindgen --check
```

The semantic C ABI digest was regenerated with pinned `libclang==18.1.1` and is
`418591c6f5ca103b`. The generated Rust binding embeds the same digest; do not
hand-edit it.

## Validation evidence

- The complete native v0.2.3 tree builds through the wrapper and through Grain.
- The safe wrapper materialization test verifies the exact `PKFW` request bytes.
- Wrapper Clippy passes with warnings denied; bindgen `--check` passes.
- `cargo check --manifest-path src-tauri/Cargo.toml --offline` passes with the
  patched shared/dynamic backend configuration.

On this Windows checkout, CMake's Visual Studio generator cannot use the deep
Cargo path reliably. Validation used Ninja, disabled ggml ccache, and the
existing short `C:\\t` target directory. Cargo's Windows lib-test executable has
no embedded application manifest, so it initially resolved legacy Common
Controls without `TaskDialogIndirect` and stopped before the Rust harness.
Injecting the standard Common Controls v6 manifest into that disposable test
artifact allowed the journal, TDT routing/timing, and rolling-service tests to
run. PE inspection separately confirmed the staged transcribe.cpp and DirectML
imports/exports; the failure was not a TDT DLL regression.

## Updating

For a later release, replace both directories from verified published crate
payloads, confirm commit and hashes, build pristine copies, then reapply and
review this patch. Regenerate bindings and the ABI digest. Never mix wrapper and
sys releases or carry forward obsolete native Flow APIs.
