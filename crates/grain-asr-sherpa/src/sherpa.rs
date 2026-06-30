//! The Sherpa-ONNX `OnlineRecognizer` adapter.
//!
//! Maps a streaming transducer's decode loop onto the protocol's
//! [`AsrRawEvent`] stream: feed audio → `is_ready`/`decode` drain → `get_result`
//! → emit `Partial` while text grows, and `BackendFinal` + `Endpoint` when the
//! recognizer's endpointer fires (then `reset` and advance the segment). The
//! M6 stabilizer turns these into the committed/volatile UI contract.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use grain_asr_core::events::{AsrRawEvent, EndpointReason};
use grain_asr_core::model::{AsrCapabilities, AsrModelFiles, AsrModelSpec};
use grain_asr_core::session::{AsrSession, AsrSessionConfig, AudioFrame, NativeAsrBackend};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};

/// Resolved capabilities of a Sherpa streaming transducer: it streams partials,
/// its finals are immutable, and it has its own endpointer. Word timing is
/// available as subword tokens but is not mapped to words yet, so we advertise
/// `word_timestamps: false` rather than emit misleading per-token "words".
const CAPS: AsrCapabilities = AsrCapabilities {
    partials: true,
    immutable_final: true,
    endpointing: true,
    word_timestamps: false,
};

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A loaded Sherpa-ONNX recognizer. Shared with each session it mints so the
/// model stays resident across utterances (one heavyweight engine, per the
/// lifecycle policy).
pub struct SherpaOnnxBackend {
    recognizer: Option<Arc<OnlineRecognizer>>,
}

impl Default for SherpaOnnxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SherpaOnnxBackend {
    pub fn new() -> Self {
        Self { recognizer: None }
    }
}

impl NativeAsrBackend for SherpaOnnxBackend {
    fn backend_id(&self) -> &'static str {
        "sherpa-onnx"
    }

    fn static_capabilities(&self) -> AsrCapabilities {
        CAPS
    }

    fn load(&mut self, model: &AsrModelSpec) -> Result<AsrCapabilities> {
        // Single-variant today; the irrefutable bind turns into a compile error
        // if a second `AsrModelFiles` layout is ever added (a useful reminder).
        let AsrModelFiles::SherpaTransducer {
            encoder,
            decoder,
            joiner,
            tokens,
            ..
        } = &model.files;

        for p in [encoder, decoder, joiner, tokens] {
            if !p.exists() {
                anyhow::bail!("sherpa-onnx model file missing: {}", p.display());
            }
        }

        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer.encoder = Some(path_str(encoder));
        config.model_config.transducer.decoder = Some(path_str(decoder));
        config.model_config.transducer.joiner = Some(path_str(joiner));
        config.model_config.tokens = Some(path_str(tokens));
        // Edge defaults: single intra-op thread, CPU EP. GPU EPs are a later,
        // capability-gated option.
        config.model_config.num_threads = 1;
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;

        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
            anyhow::anyhow!("failed to create sherpa-onnx recognizer (check model files / EP)")
        })?;
        self.recognizer = Some(Arc::new(recognizer));
        log::info!("[GRAIN] sherpa-onnx backend loaded model '{}'", model.id);
        Ok(CAPS)
    }

    fn unload(&mut self) {
        self.recognizer = None;
    }

    fn start_session(&mut self, _config: AsrSessionConfig) -> Result<Box<dyn AsrSession>> {
        let recognizer = self
            .recognizer
            .clone()
            .ok_or_else(|| anyhow::anyhow!("sherpa-onnx backend not loaded"))?;
        let stream = recognizer.create_stream();
        Ok(Box::new(SherpaOnnxSession {
            recognizer,
            stream,
            segment_id: 0,
            revision: 0,
            last_text: String::new(),
        }))
    }
}

/// One streaming session over a shared recognizer + its own decoder stream.
struct SherpaOnnxSession {
    recognizer: Arc<OnlineRecognizer>,
    stream: OnlineStream,
    /// Current utterance index (advances on each endpoint).
    segment_id: u64,
    /// Partial revision within the current segment.
    revision: u64,
    /// Last text we emitted, to suppress no-change partials.
    last_text: String,
}

impl SherpaOnnxSession {
    /// Drain the recognizer until it has consumed all ready frames.
    fn decode_ready(&self) {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
    }

    fn current_text(&self) -> String {
        self.recognizer
            .get_result(&self.stream)
            .map(|r| r.text)
            .unwrap_or_default()
    }
}

impl AsrSession for SherpaOnnxSession {
    fn push_audio(&mut self, frame: AudioFrame) -> Result<Vec<AsrRawEvent>> {
        // Tell Sherpa the TRUE rate of this buffer; it resamples to the model's
        // native rate internally. Honoring the frame (not a constant) is what
        // keeps this adapter correct if the host's delivery rate ever changes.
        self.stream
            .accept_waveform(frame.sample_rate_hz as i32, &frame.samples);
        self.decode_ready();
        let text = self.current_text();

        let mut out = Vec::new();
        if self.recognizer.is_endpoint(&self.stream) {
            if !text.trim().is_empty() {
                out.push(AsrRawEvent::BackendFinal {
                    segment_id: self.segment_id,
                    text,
                    words: Vec::new(),
                });
            }
            out.push(AsrRawEvent::Endpoint {
                segment_id: self.segment_id,
                reason: EndpointReason::Backend,
                audio_end_ms: None,
            });
            self.recognizer.reset(&self.stream);
            self.segment_id += 1;
            self.revision = 0;
            self.last_text.clear();
        } else if text != self.last_text {
            out.push(AsrRawEvent::Partial {
                segment_id: self.segment_id,
                revision: self.revision,
                text: text.clone(),
                words: Vec::new(),
            });
            self.revision += 1;
            self.last_text = text;
        }
        Ok(out)
    }

    fn flush(&mut self) -> Result<Vec<AsrRawEvent>> {
        self.decode_ready();
        let text = self.current_text();
        let mut out = Vec::new();
        if !text.is_empty() && text != self.last_text {
            out.push(AsrRawEvent::Partial {
                segment_id: self.segment_id,
                revision: self.revision,
                text: text.clone(),
                words: Vec::new(),
            });
            self.revision += 1;
            self.last_text = text;
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<AsrRawEvent>> {
        self.stream.input_finished();
        self.decode_ready();
        let text = self.current_text();
        let mut out = Vec::new();
        if !text.trim().is_empty() {
            out.push(AsrRawEvent::BackendFinal {
                segment_id: self.segment_id,
                text,
                words: Vec::new(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture smoke test + Windows build/link validation.
    ///
    /// Building this crate with `--features backend` is itself the link
    /// validation. This test additionally loads a REAL model and drives a short
    /// buffer through the pipeline — but only when `GRAIN_SHERPA_TEST_MODEL`
    /// points at an extracted model directory (it skips otherwise, so CI without
    /// a model still passes). Point it at the default registry model's bundle dir.
    #[test]
    fn loads_and_runs_when_model_present() {
        let Ok(dir) = std::env::var("GRAIN_SHERPA_TEST_MODEL") else {
            eprintln!("GRAIN_SHERPA_TEST_MODEL unset — skipping sherpa smoke test");
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let entry = &grain_asr_core::registry::builtin_catalog()[0];
        let spec = entry.to_spec(&dir);

        let mut backend = SherpaOnnxBackend::new();
        let caps = backend.load(&spec).expect("load sherpa model");
        assert!(caps.partials && caps.immutable_final);

        let mut session = backend
            .start_session(AsrSessionConfig::default())
            .expect("start session");

        // 0.5 s of silence — must drive the decode loop without panicking.
        let silence: Arc<[f32]> = Arc::from(vec![0.0f32; 8000].into_boxed_slice());
        let _ = session
            .push_audio(AudioFrame::new(silence, 16_000))
            .expect("push_audio");
        let _ = session.finish().expect("finish");

        backend.unload();
    }
}
