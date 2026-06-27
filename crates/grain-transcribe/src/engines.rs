//! All of Handy's `transcribe-rs` engines, behind the [`Asr`] trait.
//!
//! Mirrors the `LoadedEngine` dispatch in Handy's `managers/transcription.rs` so
//! that when upstream adds a model, inheriting it is a small `[GRAIN]`-marked
//! change here (and a registry entry in the decoupled `model.rs`). Every engine
//! in transcribe-rs implements [`SpeechModel`], so all but Parakeet are held as a
//! single `Box<dyn SpeechModel>` — a brand-new engine usually needs only a load
//! arm. Parakeet is special-cased to request `Word`-granularity timestamps, which
//! feed the assembler's time-based dedup path; the rest drive its text fallback.

use std::path::Path;

use anyhow::{anyhow, Result};
use rolling_window::WordTiming;
use transcribe_rs::onnx::canary::CanaryModel;
use transcribe_rs::onnx::cohere::CohereModel;
use transcribe_rs::onnx::gigaam::GigaAMModel;
use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant, StreamingModel};
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity};
use transcribe_rs::onnx::sense_voice::SenseVoiceModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

use crate::timing::{segments_to_words, synthesize_words, SAMPLE_RATE};
use crate::Asr;

/// The engine family of a model — 1:1 with Handy's `EngineType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    Whisper,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAM,
    Canary,
    Cohere,
}

impl EngineKind {
    pub const ALL: &'static [EngineKind] = &[
        EngineKind::Whisper,
        EngineKind::Parakeet,
        EngineKind::Moonshine,
        EngineKind::MoonshineStreaming,
        EngineKind::SenseVoice,
        EngineKind::GigaAM,
        EngineKind::Canary,
        EngineKind::Cohere,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            EngineKind::Whisper => "whisper",
            EngineKind::Parakeet => "parakeet",
            EngineKind::Moonshine => "moonshine",
            EngineKind::MoonshineStreaming => "moonshine-streaming",
            EngineKind::SenseVoice => "sense-voice",
            EngineKind::GigaAM => "gigaam",
            EngineKind::Canary => "canary",
            EngineKind::Cohere => "cohere",
        }
    }

    pub fn parse(s: &str) -> Option<EngineKind> {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "whisper" => Some(EngineKind::Whisper),
            "parakeet" => Some(EngineKind::Parakeet),
            "moonshine" => Some(EngineKind::Moonshine),
            "moonshinestreaming" => Some(EngineKind::MoonshineStreaming),
            "sensevoice" => Some(EngineKind::SenseVoice),
            "gigaam" => Some(EngineKind::GigaAM),
            "canary" => Some(EngineKind::Canary),
            "cohere" => Some(EngineKind::Cohere),
            _ => None,
        }
    }

    /// Best-effort guess from a model path's file/dir name (overridable).
    pub fn guess_from_path(path: &Path) -> Option<EngineKind> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let kind = if name.contains("parakeet") {
            EngineKind::Parakeet
        } else if name.contains("moonshine") && name.contains("streaming") {
            EngineKind::MoonshineStreaming
        } else if name.contains("moonshine") {
            EngineKind::Moonshine
        } else if name.contains("sense") {
            EngineKind::SenseVoice
        } else if name.contains("giga") {
            EngineKind::GigaAM
        } else if name.contains("canary") {
            EngineKind::Canary
        } else if name.contains("cohere") {
            EngineKind::Cohere
        } else if name.contains("whisper") || name.contains("ggml") || name.ends_with(".bin") {
            EngineKind::Whisper
        } else {
            return None;
        };
        Some(kind)
    }
}

/// A loaded model: Parakeet (word-timestamp path) or any other `SpeechModel`.
enum Loaded {
    Parakeet(ParakeetModel),
    Generic(Box<dyn SpeechModel>),
}

/// A `transcribe-rs` model wrapped as a Grain [`Asr`].
pub struct GrainModel {
    id: String,
    kind: EngineKind,
    loaded: Loaded,
}

impl GrainModel {
    pub fn kind(&self) -> EngineKind {
        self.kind
    }

    /// Load a model of `kind` from `path` (a model directory, or a `.bin` file
    /// for Whisper). Int8 quantization for the int8 ONNX bundles, matching Handy.
    pub fn load(kind: EngineKind, path: &Path) -> Result<Self> {
        let loaded = match kind {
            EngineKind::Parakeet => Loaded::Parakeet(
                ParakeetModel::load(path, &Quantization::Int8)
                    .map_err(|e| anyhow!("Parakeet load: {e}"))?,
            ),
            EngineKind::Moonshine => boxed(
                MoonshineModel::load(path, MoonshineVariant::Base, &Quantization::default())
                    .map_err(|e| anyhow!("Moonshine load: {e}"))?,
            ),
            EngineKind::MoonshineStreaming => boxed(
                StreamingModel::load(path, 0, &Quantization::default())
                    .map_err(|e| anyhow!("Moonshine streaming load: {e}"))?,
            ),
            EngineKind::SenseVoice => boxed(
                SenseVoiceModel::load(path, &Quantization::Int8)
                    .map_err(|e| anyhow!("SenseVoice load: {e}"))?,
            ),
            EngineKind::GigaAM => boxed(
                GigaAMModel::load(path, &Quantization::Int8)
                    .map_err(|e| anyhow!("GigaAM load: {e}"))?,
            ),
            EngineKind::Canary => boxed(
                CanaryModel::load(path, &Quantization::Int8)
                    .map_err(|e| anyhow!("Canary load: {e}"))?,
            ),
            EngineKind::Cohere => boxed(
                CohereModel::load(path, &Quantization::Int8)
                    .map_err(|e| anyhow!("Cohere load: {e}"))?,
            ),
            EngineKind::Whisper => {
                #[cfg(feature = "whisper")]
                {
                    boxed(
                        transcribe_rs::whisper_cpp::WhisperEngine::load(path)
                            .map_err(|e| anyhow!("Whisper load: {e}"))?,
                    )
                }
                #[cfg(not(feature = "whisper"))]
                {
                    let _ = path;
                    return Err(anyhow!(
                        "Whisper support requires building with: --features whisper"
                    ));
                }
            }
        };
        Ok(Self {
            id: kind.as_str().to_string(),
            kind,
            loaded,
        })
    }
}

fn boxed<M: SpeechModel + 'static>(model: M) -> Loaded {
    Loaded::Generic(Box::new(model))
}

impl Asr for GrainModel {
    fn id(&self) -> &str {
        &self.id
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<(String, Option<Vec<WordTiming>>)> {
        // Chunk duration from OUR owned timeline (transcribe-rs requires 16 kHz).
        let chunk_dur_sec = samples.len() as f64 / SAMPLE_RATE as f64;
        let result = match &mut self.loaded {
            Loaded::Parakeet(model) => {
                // SEGMENT (not WORD) granularity: per-word frame alignment costs
                // significant extra CPU per chunk (the rolling-mode spike), and we
                // don't need it — `segments_to_words` spreads each segment's words
                // across its span for the assembler's position dedup, with the
                // fuzzy text seam as backup. Matches the batch path's cost.
                let params = ParakeetParams {
                    timestamp_granularity: Some(TimestampGranularity::Segment),
                    ..Default::default()
                };
                model
                    .transcribe_with(samples, &params)
                    .map_err(|e| anyhow!("Parakeet transcribe: {e}"))?
            }
            Loaded::Generic(model) => model
                .transcribe(samples, &TranscribeOptions::default())
                .map_err(|e| anyhow!("transcribe: {e}"))?,
        };

        // Every engine feeds the assembler's TIME-based dedup path: use the
        // model's real segment timings when present, otherwise synthesize evenly
        // spaced word positions across the chunk we own. Dedup is then by
        // POSITION on our timeline, never by the model's (possibly inconsistent)
        // overlap text — so every model deduplicates cleanly.
        let words =
            segments_to_words(&result).or_else(|| synthesize_words(&result.text, chunk_dur_sec));
        Ok((result.text, words))
    }
}
