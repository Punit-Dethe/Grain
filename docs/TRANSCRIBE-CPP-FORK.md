# Grain native fork integration

Grain consumes the existing safe Rust wrapper and native sys package from the [maintained source fork](https://github.com/Punit-Dethe/transcribe.cpp/tree/codex/grain-0.2.3). Both packages retain upstream version 0.2.3 and resolve to one immutable source revision. There is no new service, download engine, persistent Flow handle or inference algorithm in Grain.

## Source and runtime contract

`native/transcribe-fork.json` records the canonical repository, full tested source SHA, upstream base, contract revision, patch identity and ABI digest. Both `[patch.crates-io]` tables and both lockfiles must agree: `src-tauri` and `scripts/flow-audit` are independent workspaces. The exact-version declarations and per-target backend features are preserved.

The fork has upstream source layout, generated Rust FFI, and its existing safe wrapper. Cargo finds both packages inside the same Git checkout and compiles the native library through upstream CMake. Users install Grain's compiled application and bundled shared libraries/backend modules; no source checkout or runtime download is needed.

Contract revision **1**, patch identity **`grain-flow-v1`**, semantic ABI **`a3b263c9e59d3ca9`**. Native queries use static storage and work without `.git`. Rust validates version, contract identity/revision, runtime digest and 16 size/alignment pairs before model loading or backend setup; the result is cached once. The previous version-only gate could not distinguish an unpatched 0.2.3 DLL. Missing exports fail in the OS loader; incompatible values produce an explicit compatibility error. This checks compatibility, not cryptographic authenticity.

Grain retains capture, journal, window cursor, token merging, replay, cancellation, inference lease and post-processing. v2 uses ordinary bounded native runs; v3 opts into PKFW; both detokenize merged IDs through the native tokenizer. Native decoder, encoder and ggml behavior are unchanged from the audited vendor patch.

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

## Validation evidence

- Windows native static CPU/CLI build: all **39 enabled CTest cases pass**. Tiny deterministic ggml weights exercise the actual PKFW decoder: blank/repeated zero-duration advance, duration boundaries, token/tail budgets, fresh-state reuse, graph-setup failure, model/head/blank/duration gates and invalid extensions. Exported detokenization tests cover split UTF-8 tokens, buffer capacities, no implicit NUL, invalid arguments, exception containment and reuse.
- Safe Rust: **14 unit/no-model tests pass**, Clippy denies warnings, formatting/bindgen/semantic digest checks pass. The source package audit checks platform-neutral paths, actual Cargo target directories, native sources, ABI data and both FluidAudio notices; the sys package is about **3.7 MiB compressed**. Registry publishing is not part of this integration.
- Windows Cargo source build with shared CPU/Vulkan modules: contract, ABI and actual CPU/Vulkan discovery pass. The final pinned revision builds a local NSIS debug installer; all 13 packaged native DLLs match staging hashes, and the extracted payload passes contract and CPU/Vulkan discovery. Its application differs from the built executable only by Tauri's expected `UNK` → `NSS` bundle marker. Updater artifact signing is disabled only in an external local verification config; production signing/configuration is unchanged. The installer was inspected, not installed over the user's application.
- Grain backend suite: **565 passed, 3 ignored** on Windows. Its disposable lib-test executable needs the previously documented Common Controls v6 manifest; without it Windows fails before Rust executes. DLL inspection confirms that failure is separate from native fork compatibility.
- Local v2 short and 27/84-second recordings: old-vendor and fork results match **exactly** for transcript, token IDs, confidence values, timestamps and window traces on the same CPU or Vulkan backend. Timings are excluded from equality. This proves extraction parity on these fixtures, not WER improvement. Private audio/transcripts stay local.
- Three repeated CPU runs of the 83.76-second fixture produce identical outputs; sampled peak working set is approximately **1.20 GiB for both** (vendor 1,286,483,968 bytes, fork 1,285,554,176). This is a local process measurement, not a claim of reduced memory. Retained application memory and VRAM need separate evidence. The real production journal test also passes for three local short/long cases with incremental appends and fresh replay, preserving bounded audio capacity.
- [Fork CI for the exact pin](https://github.com/Punit-Dethe/transcribe.cpp/actions/runs/36242159304) **passes on all three hosted Windows/Linux/macOS jobs**: native suite/CLI, safe Rust source build, generated ABI/format checks, package audit, shared dynamic consumption, Apple Metal compilation and a Linux shared source archive without `.git`. This run verifies the exact pinned SHA; earlier failed generator checks were fixed for UTF-8, CRLF and Clang enum representation differences.

The implementation branch is reviewable; no production fork release tag is published yet. Remaining evidence includes real v3 multilingual/byte-fallback model runs, retained memory/VRAM measurements, Windows ARM64, Nix offline builds, and installed package checks on the supported platforms. Absence of those fixtures/devices is not a passing gate. The original vendor trees remain **frozen rollback sources**, outside dependency resolution, until the retirement gates in the researched plan are satisfied. Do not keep updating two native implementations.

The four pin/bootstrap regression tests, both audit tests, audit Clippy/format checks, frontend production build and upstream policy check pass with fork pins and preserved provenance. The divergence ratchet reports pre-existing Handy-tree growth in seven files; this task changes no Handy Rust files and does not accept or reset that unrelated baseline.

## Upgrade and rollback

1. Start an unpublished branch from a verified upstream tag. Keep published fork commits immutable.
2. Review the complete source diff and overlaps with detokenization, PKFW dispatch/decoder, tokenizer, cleanup, private helpers, bindings, build scripts and ggml. The detailed 0.2.2/0.2.3/0.2.4 audit is in `TRANSCRIBE-CPP-FORK-PLAN.md`.
3. For 0.2.4, check predictor failure returns and the new fallible `joint_step` result in the custom decoder. A compile-time signature guard forces an explicit joint-helper port; adapt error handling rather than merely changing the assertion. Signature-preserving predictor changes still require review and failure tests. Keep encoder/conformer/backend improvements upstream-owned, then measure behavior and memory.
4. Regenerate all affected bindings/digest with pinned libclang 18.1.1 and clang-format 22.1.5. Run native semantics/failure tests, model/window/replay tests, resource/backend and package gates. Revise the contract identity/revision for semantic changes.
5. Push the tested fork commit, update the JSON, both Git patch pairs and both locks together, and run the dependency check. Publish a new immutable release tag only after promotion gates pass. Do not consume a moving branch/tag in Cargo.

Rollback restores the integration commit's previous paired manifests/locks; known vendor sources are recoverable at Grain commit `811313a6489cb98b932ab7e733c3547f76a33f73`. Rebuild/stage/package from that source pair rather than mixing DLLs or ggml modules. Removing the inactive vendor directories is a later gated commit. When upstream APIs are semantically equivalent, remove our patches or retire the fork after the same tests.

**Consensus:** the fork is worth maintaining for the existing native Flow patch. It gives the patch original upstream history, focused tests, a checked runtime contract and immutable source integration. It adds bounded release/CI work and does not itself improve RAM or accuracy. Keep the current source-based packaging and stable safe API; avoid adding a second wrapper hierarchy or binary distribution infrastructure.
