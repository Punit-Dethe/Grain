//! [GRAIN] Model-agnostic Native ASR protocol — the pure-Rust foundation.
//!
//! Native ASR is the THIRD engine path beside Batch
//! (`src-tauri/src/managers/transcription.rs`) and Rolling
//! (`src-tauri/src/rolling.rs`). Unlike those two — which run a chunked
//! *batch* model over a window — Native ASR is for backends that own true
//! incremental recognizer state and stream partial → committed text as the user
//! speaks.
//!
//! This crate is deliberately inert: **no Tauri, no Sherpa, no network, no audio
//! backend, no model files.** It defines only the contracts and the policy that
//! sit between a streaming backend and the UI:
//!
//! * [`events`] — the two event layers. Backends speak [`AsrRawEvent`]; the
//!   UI/host consumes the stabilized [`AsrEvent`].
//! * [`model`] — [`AsrModelSpec`] / [`AsrCapabilities`]: what a model is and
//!   what a *loaded* backend can actually do.
//! * [`session`] — the [`NativeAsrBackend`] / [`AsrSession`] traits, the
//!   [`AudioFrame`] they consume, and session config / hints.
//! * [`stabilizer`] — [`TranscriptStabilizer`]: the SAPrefix policy that turns a
//!   noisy stream of raw hypotheses into a stable committed-prefix +
//!   volatile-tail contract.
//! * [`testing`] — a scripted fake backend/session so the stabilizer, the
//!   lifecycle manager, and the event bridge can all be exercised without a real
//!   model, mic, or network.
//!
//! Everything is `Send` so a host can drive a session from a dedicated worker
//! thread (the plan's "isolate blocking inference from async/audio threads").

pub mod audio_input;
pub mod events;
pub mod model;
pub mod registry;
pub mod session;
pub mod stabilizer;
pub mod testing;

pub use audio_input::{BoundedFrameQueue, PreRollRing, PushOutcome};
pub use events::{AsrEvent, AsrRawEvent, AsrWord, EndpointReason, Stability};
pub use registry::{
    builtin_catalog, catalog_entry, AsrDownload, AsrModelCatalogEntry, SherpaTransducerLayout,
};
pub use model::{
    AsrBackendKind, AsrCapabilities, AsrModelFiles, AsrModelSpec, MemoryProfile,
};
pub use session::{
    AsrSession, AsrSessionConfig, AudioFormat, AudioFrame, ContextHints, NativeAsrBackend,
};
pub use stabilizer::{StabilizerConfig, TranscriptStabilizer};
