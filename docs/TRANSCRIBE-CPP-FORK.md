# Grain native fork integration

Grain consumes the safe Rust wrapper and native sys package from the [maintained source fork](https://github.com/Punit-Dethe/transcribe.cpp/tree/sync/grain-0.4.0). Both packages retain upstream version 0.4.0 and resolve to one immutable source revision. There is no new service, download engine, persistent Flow handle or inference algorithm in Grain.

## Source and runtime contract

`native/transcribe-fork.json` records the canonical repository, full tested source SHA, upstream base, contract revision, patch identity and ABI digest. Both `[patch.crates-io]` tables and both lockfiles must agree: `src-tauri` and `scripts/flow-audit` are independent workspaces. The exact-version declarations and per-target backend features are preserved.

The fork has upstream source layout, generated Rust FFI, and its existing safe wrapper. Cargo finds both packages inside the same Git checkout and compiles the native library through upstream CMake. Users install Grain's compiled application and bundled shared libraries/backend modules; no source checkout or runtime download is needed.

Contract revision **3**, patch identity **`grain-flow-v1`**, semantic ABI **`3669c8465a11b2f5`**. Native queries use static storage and work without `.git`. Rust validates version, contract identity/revision, runtime digest and 30 size/alignment pairs before model loading or backend setup; the result is cached once. Upstream ABI IDs 0–28 are preserved; PKFW is appended as ID 29. Missing exports fail in the OS loader; incompatible values produce an explicit compatibility error. This checks compatibility, not cryptographic authenticity.

Grain retains capture, journal, window cursor, token merging, replay, cancellation, inference lease and post-processing. v2 uses ordinary bounded native runs; v3 opts into PKFW; both detokenize merged IDs through the native tokenizer. The fork ports only the cumulative Grain patch onto upstream 0.4.0, retaining upstream's new encoder, ggml, VAD, language ID, diarization and vocabulary APIs. Predictor/joint failure handling is already upstream. Confidence and some timestamps differ from 0.2.3; bit-exact extraction parity is not claimed for this upgrade.

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

## October 17 upgrade evidence

Native candidate `517885822a5f42562972796c53ae496a52c143b6` is based on
upstream 0.4.0 `f1dcd15858c4bbba87ca97862b4e448b2cf15b8f`.
The static Windows CPU suite passes 59 tests, with one diarization fixture
smoke skipped. The shared CPU/Vulkan build passes six native tests and discovers
both the CPU and RTX 3080. Safe Rust with serde passes 13 library and 10 no-model
tests; PKFW run options survive JSON and postcard worker transport. Clippy,
formatting, regenerated Rust/Python/TypeScript bindings, Swift ABI and package
checks pass (sys archive: 1,718 files, about 4.3 MiB compressed).

Explicit real v2 Q8 and v3 Q4 lifecycle tests pass cancellation/reset,
invalid-window recovery, model lifetime and detokenization with public JFK
audio. The unchanged maintained Flow audit compares public 11/35/83-second
fixtures against 0.2.3. Full cased/punctuated Batch, Flow, independent-window and
diagnostic-window strings agree on CPU and Vulkan. Token IDs agree in the
matching shared CPU v2 comparison; confidence and some frame/timestamp values
differ. These fixtures do not prove multilingual accuracy or general
sentence-continuation correctness; timing overlaps with builds prevent using
these runs as performance benchmarks.

Capability queries are now fallible: model loading and Batch propagate errors;
the streaming error branch returns its engine and clears/drains the route.
The dependency checker verifies the paired source pin and target features; all
ten pin/runtime-path tests and 28 upstream tooling tests pass. The official
source-built backend suite passes **582 tests, 3 ignored**. Its production Flow
journal gate is then run explicitly with both v2 and v3: all three public clips
pass incremental partitioning and fresh replay, exact cased/punctuated output,
and bounded audio capacity. The initial v2 fixture setup mistakenly selected
the audit's diagnostic PKFW output; correcting it to the production independent
window output resolves that test setup failure. Native CI passes Windows,
Linux and macOS at the exact source pin:
https://github.com/Punit-Dethe/transcribe.cpp/actions/runs/39026846446.
Current Grain worker integration evidence is in Upstream/SYNC-2026-10-17.md.
Installer and real microphone acceptance remain separate from these tests.

The capability error handling adds 17 measured divergence lines to the current
Handy manager baseline (1076 → 1093). This is reviewed adaptation of the 0.4
fallible ASR API, including streaming cleanup, rather than a new Grain feature.
The forthcoming supervisor integration replaces these in-process call sites;
do not retain a second legacy engine to preserve this checkpoint.

## Retired implementation

The inactive 0.2.3 vendor copies and their contract are removed. Git retains
the previous implementation; no second native baseline is built or maintained.
The current paired pin and fork contract above are authoritative.
