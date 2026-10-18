//! Grain's model queries carried by Handy's existing worker transport.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    FlowModel,
    ResetFlow,
    Detokenize { tokens: Vec<i32> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    FlowModel {
        arch: String,
        variant: String,
        supported: bool,
    },
    Text(String),
    Reset,
}

pub fn dispatch(
    request: Request,
    session: &mut transcribe_cpp::Session,
) -> Result<Response, String> {
    let model = session.model();
    match request {
        Request::ResetFlow => session
            .reset()
            .map(|()| Response::Reset)
            .map_err(|error| error.to_string()),
        Request::FlowModel => Ok(Response::FlowModel {
            arch: model.arch(),
            variant: model.variant(),
            supported: model.accepts_ext(
                transcribe_cpp::ExtSlot::Run,
                transcribe_cpp::ParakeetTdtBufferedOptions::KIND,
            ),
        }),
        Request::Detokenize { tokens } => model
            .detokenize(&tokens)
            .map(Response::Text)
            .map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_supervisor::{DeviceSelector, EngineSupervisor, LoadSpec};
    use crate::grain_audio_journal::PcmJournal;
    use crate::grain_transcription::FlowSession;
    use crate::tdt_flow::{Inference, TdtAccumulator, TdtRunConfig};
    use std::sync::atomic::AtomicBool;

    /// Run against the real Grain executable's worker, not an in-process model.
    #[test]
    #[ignore = "requires Grain worker executable, model and exact-text fixtures"]
    fn worker_flow_journal_preserves_exact_text_and_recovers_after_cancel() {
        let engine = EngineSupervisor::new(true);
        let backend = match std::env::var("GRAIN_FLOW_TEST_BACKEND")
            .as_deref()
            .unwrap_or("cpu")
        {
            "cpu" => transcribe_cpp::Backend::Cpu,
            "vulkan" => transcribe_cpp::Backend::Vulkan,
            _ => panic!("expected cpu or vulkan"),
        };
        let info = engine
            .load(LoadSpec {
                path: std::env::var_os("GRAIN_FLOW_TEST_MODEL").unwrap().into(),
                backend,
                device: DeviceSelector::Auto,
            })
            .unwrap();
        assert_eq!(info.on_gpu, backend == transcribe_cpp::Backend::Vulkan);
        let cancelled = AtomicBool::new(false);
        let mut session = FlowSession {
            engine: &engine,
            mark: engine.cancel_mark(),
            cancelled: &cancelled,
        };
        let model = session.flow_model().unwrap();
        assert_eq!(model.arch, info.arch);
        assert_eq!(model.variant, info.variant);
        assert!(model.supported);
        let config = TdtRunConfig {
            model_id: info.variant,
            language: None,
        };
        let cases: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(std::env::var_os("GRAIN_FLOW_TEST_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            // Match the offline oracle's PCM16 conversion exactly. The generic
            // Handy WAV helper divides by 32767, perturbing quantized decoding.
            let mut wav = hound::WavReader::open(case["path"].as_str().unwrap()).unwrap();
            assert_eq!(
                (
                    wav.spec().sample_rate,
                    wav.spec().channels,
                    wav.spec().bits_per_sample
                ),
                (16000, 1, 16)
            );
            assert_eq!(wav.spec().sample_format, hound::SampleFormat::Int);
            let pcm: Vec<f32> = wav
                .samples::<i16>()
                .map(|s| s.unwrap() as f32 / 32768.0)
                .collect();
            let journal = PcmJournal::create().unwrap();
            let mut reader = journal.reader().unwrap();
            let mut accumulator = TdtAccumulator::new(&session, &config).unwrap();
            let ceiling = accumulator.max_input_samples();
            let mut audio = Vec::with_capacity(ceiling);
            for block in pcm.chunks(8001) {
                journal.append(block).unwrap();
                accumulator
                    .process_stable(
                        &mut session,
                        &config,
                        &journal,
                        &mut reader,
                        &mut audio,
                        journal.frame_count(),
                    )
                    .unwrap();
            }
            journal.close().unwrap();
            let text = accumulator
                .render(
                    &mut session,
                    &config,
                    &journal,
                    &mut reader,
                    &mut audio,
                    journal.frame_count(),
                )
                .unwrap();
            assert_eq!(
                text,
                case["expected"].as_str().unwrap(),
                "full casing/punctuation must agree"
            );
            assert!(audio.capacity() <= ceiling);
        }
        engine.cancel();
        assert!(
            session.flow_model().is_err(),
            "old epoch must not revive a cancelled session"
        );
        let fresh = FlowSession {
            engine: &engine,
            mark: engine.cancel_mark(),
            cancelled: &cancelled,
        };
        assert!(
            fresh.flow_model().unwrap().supported,
            "a fresh cancellation epoch must be usable"
        );
        engine.unload().wait();
        drop(fresh);
        drop(session);
        assert!(engine.loaded().is_none());
        assert!(
            !engine.grain_has_resident_model(),
            "cleanup must not reload an unloaded worker"
        );
    }

    #[test]
    fn grain_model_queries_round_trip_without_native_handles() {
        let request = Request::Detokenize {
            tokens: vec![0, 10, 442],
        };
        let json = serde_json::to_vec(&request).unwrap();
        assert!(matches!(serde_json::from_slice::<Request>(&json).unwrap(),
            Request::Detokenize { tokens } if tokens == [0, 10, 442]));
    }
}
