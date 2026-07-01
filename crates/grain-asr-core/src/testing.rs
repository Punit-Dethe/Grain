//! A scripted fake backend/session — the deterministic test double the rest of
//! the Native ASR stack is built against.
//!
//! It is compiled into the normal crate (not behind `#[cfg(test)]`) on purpose:
//! the engine-lifecycle manager (Milestone 2) and the event bridge (Milestone 6)
//! both need to drive a real [`NativeAsrBackend`] without a model, mic, or
//! network. A [`ScriptedBackend`] replays a fixed list of [`AsrRawEvent`] batches:
//! batch *i* is returned from the *i*-th `push_audio`, and everything left over
//! is drained on `finish` (or `flush`).

use std::collections::VecDeque;

use crate::events::AsrRawEvent;
use crate::model::{AsrCapabilities, AsrModelSpec};
use crate::session::{AsrSession, AsrSessionConfig, AudioFrame, NativeAsrBackend};

/// A backend that mints [`ScriptedSession`]s replaying a fixed event script.
pub struct ScriptedBackend {
    caps: AsrCapabilities,
    /// One inner `Vec` per `push_audio` call; cloned into each new session.
    script: Vec<Vec<AsrRawEvent>>,
    loaded: bool,
}

impl ScriptedBackend {
    /// Build a backend whose sessions replay `script` (batch per `push_audio`).
    pub fn new(caps: AsrCapabilities, script: Vec<Vec<AsrRawEvent>>) -> Self {
        Self {
            caps,
            script,
            loaded: false,
        }
    }
}

impl NativeAsrBackend for ScriptedBackend {
    fn backend_id(&self) -> &'static str {
        "scripted-fake"
    }

    fn static_capabilities(&self) -> AsrCapabilities {
        self.caps
    }

    fn load(&mut self, _model: &AsrModelSpec) -> anyhow::Result<AsrCapabilities> {
        self.loaded = true;
        Ok(self.caps)
    }

    fn unload(&mut self) {
        self.loaded = false;
    }

    fn start_session(
        &mut self,
        _config: AsrSessionConfig,
    ) -> anyhow::Result<Box<dyn AsrSession>> {
        if !self.loaded {
            anyhow::bail!("ScriptedBackend: start_session called before load");
        }
        Ok(Box::new(ScriptedSession {
            queue: self.script.iter().cloned().collect(),
        }))
    }
}

/// A session that pops one scripted batch per `push_audio` and drains the rest
/// on `flush`/`finish`.
pub struct ScriptedSession {
    queue: VecDeque<Vec<AsrRawEvent>>,
}

impl AsrSession for ScriptedSession {
    fn push_audio(&mut self, _frame: AudioFrame) -> anyhow::Result<Vec<AsrRawEvent>> {
        Ok(self.queue.pop_front().unwrap_or_default())
    }

    fn flush(&mut self) -> anyhow::Result<Vec<AsrRawEvent>> {
        Ok(self.queue.pop_front().unwrap_or_default())
    }

    fn finish(&mut self) -> anyhow::Result<Vec<AsrRawEvent>> {
        Ok(self.queue.drain(..).flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AsrEvent;
    use crate::model::{AsrBackendKind, AsrModelFiles, AsrModelSpec, MemoryProfile};
    use crate::session::ContextHints;
    use crate::stabilizer::{StabilizerConfig, TranscriptStabilizer};
    use std::sync::Arc;

    fn dummy_spec() -> AsrModelSpec {
        AsrModelSpec {
            id: "fake".into(),
            name: "Fake".into(),
            backend: AsrBackendKind::SherpaOnnx,
            files: AsrModelFiles::SherpaTransducer {
                encoder: "enc".into(),
                decoder: "dec".into(),
                joiner: "join".into(),
                tokens: "tok".into(),
                config: None,
            },
            sample_rate_hz: 16_000,
            languages: vec!["en".into()],
            capabilities: AsrCapabilities::streaming_minimal(),
            memory: MemoryProfile { approx_mb: 64 },
            tuning: crate::model::AsrTuning::default(),
        }
    }

    fn frame() -> AudioFrame {
        AudioFrame::new(Arc::from(vec![0.0f32; 160].into_boxed_slice()), 16_000)
    }

    /// Backend → session → stabilizer, end to end, no model/mic/network.
    #[test]
    fn scripted_backend_drives_stabilizer_to_final() {
        let caps = AsrCapabilities::streaming_minimal();
        let script = vec![
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 0,
                text: "hello".into(),
                words: Vec::new(),
            }],
            vec![AsrRawEvent::Partial {
                segment_id: 0,
                revision: 1,
                text: "hello world".into(),
                words: Vec::new(),
            }],
            vec![AsrRawEvent::BackendFinal {
                segment_id: 0,
                text: "hello world".into(),
                words: Vec::new(),
            }],
        ];
        let mut backend = ScriptedBackend::new(caps, script);
        let resolved = backend.load(&dummy_spec()).unwrap();
        assert_eq!(resolved, caps);

        let mut session = backend
            .start_session(AsrSessionConfig {
                session_id: 42,
                language: Some("en".into()),
                hints: ContextHints::default(),
                want_word_timestamps: false,
            })
            .unwrap();

        let mut stab = TranscriptStabilizer::new(42, caps, StabilizerConfig::default());
        let mut ui = Vec::new();
        // Three push_audio calls drain the three scripted batches.
        for _ in 0..3 {
            for raw in session.push_audio(frame()).unwrap() {
                ui.extend(stab.ingest(raw));
            }
        }
        ui.push(stab.session_final());

        // The session must have produced a SegmentFinal and a SessionFinal whose
        // text is the full utterance.
        let seg_final = ui.iter().any(|e| {
            matches!(e, AsrEvent::SegmentFinal { text, .. } if text == "hello world")
        });
        assert!(seg_final, "expected SegmentFinal 'hello world'");
        match ui.last().unwrap() {
            AsrEvent::SessionFinal { text, session_id } => {
                assert_eq!(text, "hello world");
                assert_eq!(*session_id, 42);
            }
            other => panic!("expected SessionFinal, got {other:?}"),
        }
    }

    #[test]
    fn start_session_requires_load() {
        let mut backend = ScriptedBackend::new(AsrCapabilities::streaming_minimal(), vec![]);
        assert!(backend.start_session(AsrSessionConfig::default()).is_err());
    }
}
