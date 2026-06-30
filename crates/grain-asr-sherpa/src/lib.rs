//! [GRAIN] Sherpa-ONNX streaming backend for Native ASR (Milestone 5).
//!
//! Implements `grain_asr_core::NativeAsrBackend` / `AsrSession` over the official
//! [`sherpa-onnx`](https://docs.rs/sherpa-onnx) crate's `OnlineRecognizer`. It is
//! the first REAL backend behind the model-agnostic protocol built in M1 — the
//! worker, stabilizer, and event bridge (M6) consume it unchanged; only the
//! backend object differs from the scripted fake.
//!
//! ## Why the `backend` feature gate
//!
//! The `sherpa-onnx` crate links a native library; its build script downloads a
//! prebuilt archive at build time unless `SHERPA_ONNX_LIB_DIR` points at a local
//! copy. That is a heavy, network-touching, platform-sensitive build step we do
//! NOT want in the default workspace/CI build. So the entire native backend —
//! and the `sherpa-onnx` dependency — sits behind the off-by-default `backend`
//! feature. Without it this crate compiles to nothing; with it you get
//! [`SherpaOnnxBackend`].
//!
//! Enable with `--features backend`, ideally after setting `SHERPA_ONNX_LIB_DIR`
//! for a reproducible, offline build.

#[cfg(feature = "backend")]
mod sherpa;

#[cfg(feature = "backend")]
pub use sherpa::SherpaOnnxBackend;
