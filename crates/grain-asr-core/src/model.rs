//! What a Native ASR model *is*, and what a loaded backend can actually *do*.
//!
//! This is intentionally separate from Handy's Batch/Rolling model registry
//! (`selected_model`): Native ASR models have a different file topology
//! (multi-file Sherpa transducer bundles), different runtime state, and
//! different capabilities. Overloading the existing registry would couple two
//! lifecycles that should stay independent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which backend family a model targets. The host uses this to pick the right
/// [`crate::session::NativeAsrBackend`] implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsrBackendKind {
    /// Sherpa-ONNX (the first real backend; see `crates/grain-asr-sherpa`).
    SherpaOnnx,
}

/// What a backend+model combination can actually do at runtime.
///
/// Two sources fill this in, and they can legitimately disagree:
///   * [`AsrModelSpec::capabilities`] — the *declared* capabilities from model
///     metadata (a hint, used before load to drive UI/registry decisions).
///   * the return value of [`crate::session::NativeAsrBackend::load`] — the
///     *resolved* capabilities, after the backend has inspected the actual model
///     package and execution provider.
///
/// The resolved value is authoritative; the declared value must never be trusted
/// over it once a model is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrCapabilities {
    /// Backend emits incremental partial hypotheses (not just one final).
    pub partials: bool,
    /// A `BackendFinal` is immutable once emitted, so the stabilizer can trust
    /// it verbatim instead of running SAPrefix over it.
    pub immutable_final: bool,
    /// Backend performs its own endpoint detection (emits `Endpoint`).
    pub endpointing: bool,
    /// Backend emits per-word timing on its events.
    pub word_timestamps: bool,
}

impl AsrCapabilities {
    /// A conservative, true-streaming default: partials yes, final NOT assumed
    /// immutable (so SAPrefix runs), no endpointing, no word timing. Backends
    /// override the fields they actually support.
    pub const fn streaming_minimal() -> Self {
        Self {
            partials: true,
            immutable_final: false,
            endpointing: false,
            word_timestamps: false,
        }
    }
}

/// Rough resident-memory cost of a loaded model, for the lifecycle manager's
/// low-RAM accounting. Approximate by design — exact figures vary by EP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub approx_mb: u32,
}

/// How a transducer backend searches for the best token sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodingMethod {
    /// One best path — fastest, deterministic.
    Greedy,
    /// Keep `num_active_paths` hypotheses — higher accuracy. Cheap on these
    /// models because the decoder/joiner are tiny relative to the encoder, so
    /// the extra search barely moves the real-time factor.
    ModifiedBeamSearch { num_active_paths: u32 },
}

/// [GRAIN] Per-model runtime tuning — the model-specific "audio/decoding
/// profile" a backend applies to maximize *this* model's accuracy and keep it
/// real-time. The shape is backend-agnostic (any streaming ASR engine has these
/// concepts); the sherpa backend maps it onto `OnlineRecognizerConfig`.
///
/// Extend this — not the host audio pipeline — when a knob is genuinely
/// per-model. Some model-specific audio concerns are NOT yet expressible here
/// and are tracked in the architecture plan (host pre-roll duration, and whether
/// the model wants the recorder's high-pass/AGC conditioning bypassed); they
/// live on the global recorder today and want to become part of this profile.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsrTuning {
    /// Intra-op threads for the ONNX session. Heavier encoders need more to
    /// process each streaming chunk within its real-time budget (a model that
    /// can't keep up drops frames → worse accuracy).
    pub num_threads: u32,
    /// Decoding search strategy.
    pub decoding: DecodingMethod,
    /// Trailing silence (seconds) after speech that closes a segment — the
    /// model's own endpointer (maps to sherpa `rule2_min_trailing_silence`).
    /// Lower = snappier segment finalization (commits flow sooner); higher =
    /// more context per segment.
    pub endpoint_trailing_silence_secs: f32,
    /// Hard cap (seconds) on one utterance before a segment boundary is forced
    /// even without a pause (maps to sherpa `rule3_min_utterance_length`).
    pub endpoint_max_utterance_secs: f32,
}

impl Default for AsrTuning {
    /// Conservative defaults that reproduce the pre-tuning behavior: single
    /// thread, greedy, sherpa's stock endpoint timings.
    fn default() -> Self {
        Self {
            num_threads: 1,
            decoding: DecodingMethod::Greedy,
            endpoint_trailing_silence_secs: 1.2,
            endpoint_max_utterance_secs: 20.0,
        }
    }
}

/// On-disk file layout of a Native ASR model. An enum (not a flat struct) so each
/// backend topology is explicit and the host can validate the right file set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsrModelFiles {
    /// Sherpa-ONNX streaming transducer: encoder/decoder/joiner + tokens, with
    /// an optional extra config file.
    SherpaTransducer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
        tokens: PathBuf,
        config: Option<PathBuf>,
    },
}

/// A fully described Native ASR model: identity, backend, files, audio format,
/// language coverage, declared capabilities, and memory profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsrModelSpec {
    pub id: String,
    pub name: String,
    pub backend: AsrBackendKind,
    pub files: AsrModelFiles,
    /// Native sample rate the model expects (Hz). The host resamples to this.
    pub sample_rate_hz: u32,
    /// BCP-47-ish language tags the model claims to support.
    pub languages: Vec<String>,
    /// Declared capabilities (a hint — see [`AsrCapabilities`]).
    pub capabilities: AsrCapabilities,
    pub memory: MemoryProfile,
    /// [GRAIN] The model's runtime profile — decoding/endpoint/threading knobs
    /// the backend applies to maximize this model's accuracy. Every model in the
    /// catalog declares one; anything constructed without a specific profile uses
    /// [`AsrTuning::default`] (the standardized fallback), so the pipeline stays
    /// model-agnostic and no model is left un-tuned.
    pub tuning: AsrTuning,
}
