//! The streaming backend/session contracts and the audio they consume.
//!
//! A [`NativeAsrBackend`] owns the loaded model and mints [`AsrSession`]s; a
//! session owns the per-utterance incremental recognizer state. The host drives
//! exactly one session at a time on a dedicated worker thread, feeding it
//! [`AudioFrame`]s and forwarding the returned [`AsrRawEvent`]s into the
//! [`crate::stabilizer::TranscriptStabilizer`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::events::AsrRawEvent;
use crate::model::{AsrCapabilities, AsrModelSpec};

/// The PCM format the host's fan-out delivers to every backend.
///
/// This is the model-agnostic boundary. The host captures and resamples to ONE
/// fixed delivery format ([`HOST_DEFAULT`](Self::HOST_DEFAULT)) and stamps every
/// [`AudioFrame`] with it; each backend then adapts that to whatever its own
/// model needs (Sherpa-ONNX resamples internally from the frame's rate to the
/// model's rate; a backend that can't would resample in its adapter). Making the
/// contract a single explicit value keeps a 16 kHz/mono assumption from leaking,
/// unstated, into every call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl AudioFormat {
    /// Grain's capture/fan-out delivery format today: 16 kHz mono `f32`.
    pub const HOST_DEFAULT: AudioFormat = AudioFormat {
        sample_rate_hz: 16_000,
        channels: 1,
    };
}

/// A block of mono PCM handed to a session, tagged with its true sample rate.
///
/// `samples` is an `Arc<[f32]>` so the host's audio fan-out can hand the SAME
/// frame to Rolling and Native ASR without copying the PCM (the plan's frame
/// fan-out, Milestone 3). The session must not assume it owns the buffer.
///
/// `sample_rate_hz` is the rate of THIS buffer (the host's delivery rate), NOT
/// the model's native rate. A backend that resamples must read this field rather
/// than assume a constant — that is what keeps the backend model-agnostic.
#[derive(Clone, Debug)]
pub struct AudioFrame {
    pub samples: Arc<[f32]>,
    pub sample_rate_hz: u32,
}

impl AudioFrame {
    pub fn new(samples: impl Into<Arc<[f32]>>, sample_rate_hz: u32) -> Self {
        Self {
            samples: samples.into(),
            sample_rate_hz,
        }
    }
}

/// Optional biasing hints handed to a session at start. Backends may ignore any
/// hint they do not support — they are best-effort, never required.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextHints {
    /// Custom words / domain vocabulary to bias recognition toward (Grain's
    /// existing custom-words feature feeds this).
    pub vocabulary: Vec<String>,
    /// Recently committed text, for backends that accept a left-context prompt.
    pub preceding_text: Option<String>,
}

/// Per-session configuration handed to [`NativeAsrBackend::start_session`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AsrSessionConfig {
    /// Host-assigned id, echoed onto every [`crate::events::AsrEvent`].
    pub session_id: u64,
    /// Requested language tag, or `None` to let the model decide.
    pub language: Option<String>,
    pub hints: ContextHints,
    /// Ask the backend for per-word timing when it supports it.
    pub want_word_timestamps: bool,
}

/// One live streaming recognition session. Owns incremental decoder state.
///
/// `Send` (not `Sync`): a session lives on a single worker thread. The host
/// serializes all calls; implementations need no internal locking.
pub trait AsrSession: Send {
    /// Feed one audio frame; return any events produced (often empty — the
    /// backend may buffer several frames before revising its hypothesis).
    fn push_audio(&mut self, frame: AudioFrame) -> anyhow::Result<Vec<AsrRawEvent>>;
    /// Force the backend to decode buffered audio now (e.g. on a VAD endpoint),
    /// without ending the session.
    fn flush(&mut self) -> anyhow::Result<Vec<AsrRawEvent>>;
    /// End the session: decode any remaining audio and emit final events. After
    /// this the session is spent and must be dropped.
    fn finish(&mut self) -> anyhow::Result<Vec<AsrRawEvent>>;
}

/// A loaded Native ASR backend that mints sessions for one model.
///
/// `Send` so the owning worker thread can hold it; the host guarantees a single
/// active session per backend (the low-RAM, one-heavyweight-engine rule).
pub trait NativeAsrBackend: Send {
    /// Stable identifier for logs/metrics (e.g. `"sherpa-onnx"`).
    fn backend_id(&self) -> &'static str;
    /// Capabilities the backend supports in principle, independent of any model.
    fn static_capabilities(&self) -> AsrCapabilities;
    /// Load `model` into memory; return the RESOLVED capabilities for this exact
    /// model package + execution provider (see [`AsrCapabilities`]).
    fn load(&mut self, model: &AsrModelSpec) -> anyhow::Result<AsrCapabilities>;
    /// Drop all loaded model resources. Must be safe to call when not loaded.
    fn unload(&mut self);
    /// Begin a new session against the loaded model.
    fn start_session(&mut self, config: AsrSessionConfig)
        -> anyhow::Result<Box<dyn AsrSession>>;
}
