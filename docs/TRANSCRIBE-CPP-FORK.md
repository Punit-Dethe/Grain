# Grain native fork integration

Grain consumes the safe Rust wrapper and native sys package from the [maintained source fork](https://github.com/Punit-Dethe/transcribe.cpp/tree/flow/parakeet-v3-stateful). Both packages retain upstream version 0.4.0 and resolve to one immutable source revision.

## Source and runtime contract

`native/transcribe-fork.json` records the canonical repository, full tested source SHA, upstream base, contract revision, patch identity and ABI digest. Both `[patch.crates-io]` tables and both lockfiles must agree: `src-tauri` and `scripts/flow-audit` are independent workspaces. The exact-version declarations and per-target backend features are preserved.

The fork has upstream source layout, generated Rust FFI, and its existing safe wrapper. Cargo finds both packages inside the same Git checkout and compiles the native library through upstream CMake. Users install Grain's compiled application and bundled shared libraries/backend modules; no source checkout or runtime download is needed.

Contract revision **6**, patch identity **`grain-flow-tdt`**, semantic ABI **`54c13920a14bb4b6`**. Native queries use static storage and work without `.git`. Rust validates version, contract identity/revision, runtime digest and 31 size/alignment pairs before model loading or backend setup; the result is cached once. Upstream ABI IDs 0–28 are preserved; PKFW is appended as ID 29; v2/v3 buffered TDT (PKBF) is ID 30; the Nemotron-3 diarize-stream params are ID 31. Missing exports fail in the OS loader; incompatible values produce an explicit compatibility error. This checks compatibility, not cryptographic authenticity.

Grain owns capture, the PCM journal, window scheduling, cancellation and post-processing. The native wrapper owns inference sessions, decoding and tokenization.

Full source SHA and artifact hashes belong to build/release provenance. The optional native short Git stamp can be `unknown` in source archives; it is not an identity gate. Do not embed a commit's own SHA in its committed source.

## Co-located development checkout

`native/transcribe.cpp/` is an independent Git repository ignored by Grain. Its native commits are pushed to the fork; Grain tracks only integration/configuration. Production builds fetch the pinned Git source and do not use this ignored working folder.

```powershell
uv run --no-project scripts/transcribe_fork.py --bootstrap
uv run --no-project scripts/transcribe_fork.py --check --metadata --official
```

Bootstrap clones the canonical fork at the pinned SHA with an upstream remote. For an existing checkout it verifies its independent root, origin, SHA and clean state. It never resets, pulls, overwrites or discards local work. After changing a pin, review/commit/stash native work and switch the development checkout manually. It is normal for an active native development branch to differ from the shipping pin; bootstrap reports that difference instead of changing it.

Official source builds reject `TRANSCRIBE_DIR`, `TRANSCRIBE_CMAKE_ARGS` and `CMAKE_ARGS` overrides. A deliberate prebuilt development test must be explicit and separately validate its library. A local path override is development-only and fails the dependency check. No Cargo configuration or Git identity is changed by bootstrap.

```powershell
# Inspect the actual staged or installed shared library and its backend modules.
uv run --no-project scripts/transcribe_fork.py --runtime-dir src-tauri/transcribe-libs
```

On Linux, start this command with loader paths appropriate to the inspected directory, such as `LD_LIBRARY_PATH=/absolute/native/lib`; static Apple/ARM builds use the fork's Rust `grain_contract` example instead. Runtime inspection verifies the expected identity/digest/version and actual CPU fallback discovery. It does not replace full installer/resource verification. Existing `DEP_TRANSCRIBE_CPP_*` forwarding, Tauri staging, SONAME handling, Windows resource layout and Linux rpaths are preserved.

## Flow window policies

- **Parakeet TDT v2 and v3** use buffered continuation with 10 seconds left context, 10 seconds of owned audio and 4.96 seconds of right context. Only owned frames advance the native predictor, so each token is appended once. The v2 operating point follows published long-form results. The same policy gives v3 bounded long-form decoding, with WER dependent on the model and audio. Finalization and session drop release the carried state before returning the inference lease.
- **Fluid Speech Pro Q8_0** uses its own published independent-window profile for ordinary Flow dictation. Grain pads short audio, decodes overlapping windows and merges their timestamped tokens. Paths that retain structured window evidence use buffered continuation.

See the [Flow conformance tool](../scripts/flow-audit/README.md) for the buffered-profile checks. Profile selection lives in [the Flow session](../src-tauri/src/tdt_flow.rs).

## Local diarization

Grain's fork carries the Nemotron-3 diarization architecture port from
glimpse-hq/transcribe.cpp. Recall uses the offline diarization API to label
speakers in saved recordings. The native fork's GRAIN-FORK.md documents the
port and its interface.
