# GPU offloading audit — 2026-09-26

Baseline: freshly fetched Handy `8f9cf53cd1410cda26beea39ff802ac306e39585`.
Grain was already at zero commits behind. The shipping native fork is
`2d336ea878ded786f63dd53520beed8fc6192c61`, based on upstream transcribe.cpp
0.2.3 revision `63a44d9239d610b3908e8a66b384924cd4a77217`.

## Finding and correction

Grain retained an older generic-GPU fallback in the compiled Handy transcription
manager: when an exact stored device could not be resolved, it explicitly chose
CUDA, ROCm, then Vulkan (Metal on macOS). Current Handy delegates that case to
`Backend::Auto`. A missing/disconnected selected GPU therefore followed a
different policy in Grain even though both settings migrations and the selector
already used Auto or an exact device.

Restored Handy's backend selector, host helper, missing-device message and
existing host tests verbatim. Auto and unresolved/legacy GPU preferences now
delegate selection and CPU fallback to the native library. Available exact
devices remain honored; explicit CPU and Windows x64 emulation on ARM64 retain
strict CPU selection. No dependency/native-fork upgrade is required.

## Audited scope

- Fifteen shared backend functions match Handy after ignoring comments and
  whitespace: initialization, CLI device enumeration/index resolution, backend
  selection, exact GPU resolution, stable device keys/labels, host guard,
  device filtering, accelerator options and cached GPU enumeration.
- The model-load path passes the same backend/device pair into `ModelOptions`;
  there are no Grain GPU layer-count, aggressive-offload or flash-attention
  overrides. Production Flow checks out this same loaded session. Its CPU-only
  model loader appears only in an ignored fixture test.
- Compiled target features match Handy: Windows x64 and Linux use dynamic
  CPU/Vulkan modules; macOS uses Metal; native Windows ARM64 is static CPU-only.
  The Metal residency and Linux Vulkan-layer compatibility guards are upstream
  behavior, not Grain offloading policy.
- The native fork changes no ggml sources, Whisper sources, backend selection
  sources or native sys build script. Its backend wrapper adds ABI compatibility
  validation; its Parakeet changes implement Flow decoding and error handling.
- Grain's compiled settings migration and owned selector already represent
  Auto, CPU or an exact stable GPU identity. Upstream's inert settings file is
  not the runtime evidence; Grain's core migration tests cover the real settings.

Grain still intentionally uses transcribe.cpp GGUF models where Handy supports
separate ONNX engines and ORT acceleration. This audit aligns shared
transcribe.cpp offloading, not those different model runtimes, and does not
reintroduce ONNX. The native Flow extension remains necessary for Grain Flow.

## Verification

- Locked backend test build succeeds; 565 backend tests pass, 3 fixture tests
  remain ignored. All 23 transcription tests pass separately, including restored
  Auto/CPU/GPU and ARM64-emulation selection assertions.
- All 202 Grain core tests pass, including stable GPU identity and legacy/generic
  GPU migrations. TypeScript checking, upstream port audit, policy check and
  immutable native source/dependency verification pass.
- Windows test execution uses a disposable copy with Common Controls v6
  embedded, as required by the repository's existing test setup. The installed
  application is untouched.
- Divergence ratchet still has the same seven unrelated failures present at
  HEAD before this change. No budget is widened; transcription-manager
  divergence decreases from 1,164 to 1,084 lines against the current merge base.
- No new real-device transcription benchmark, macOS/Linux execution or native
  ARM64 run was performed. This is source-policy parity and automated backend
  verification, not a new claim about driver reliability or GPU utilization.
