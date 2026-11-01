//! Grain's model queries and model roles carried by Handy's existing worker
//! transport.
//!
//! The worker holds at most one resident model role: Handy's ASR `Session`, or
//! the diarization session in [`WorkerSlot`]. Loading either releases the
//! other first. The supervisor keeps the role in its `LoadSpec`, so a crashed,
//! hung or cancelled worker is replaced with the same role, model and device
//! policy instead of a stale dictation model.
use crate::engine_supervisor::{DeviceInfo, DeviceSelector, LoadSpec, LoadedInfo};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use transcribe_cpp::{Backend, DiarizeOptions, SpeakerSegment};

/// Analysis audio is 16 kHz mono float32 PCM, the native diarizer contract.
pub const ANALYSIS_SAMPLE_RATE: u32 = 16_000;

/// Grain's per-track processing cap: 90 minutes (86.4 M float32 samples).
/// Like FluidMeet, a track is diarized as one run so speaker slots stay
/// continuous; it is never cut into independently diarized pieces. 90 minutes
/// keeps one run's measured worker peak (about 1.2 GiB per hour of audio)
/// under the 2 GiB processing target. Longer tracks are kept but refused for
/// processing with this limit stated.
pub const MAX_ANALYSIS_SAMPLES: u64 = 90 * 60 * ANALYSIS_SAMPLE_RATE as u64;

/// The native role a worker load creates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Handy's transcription session (dictation, Flow and meeting ASR).
    #[default]
    Asr,
    /// A standalone speaker-diarization session.
    Diarize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    FlowModel,
    ResetFlow,
    Detokenize {
        tokens: Vec<i32>,
    },
    /// Replace any resident model with a diarize-only model. Sent only by the
    /// supervisor's load path for a [`Role::Diarize`] spec.
    LoadDiarizer {
        path: PathBuf,
        backend: Backend,
        device: DeviceSelector,
    },
    /// Diarize one validated analysis WAV read by the worker. The file is
    /// passed by reference so a long track is never copied through the PCM
    /// frame; `samples` is checked against the file before inference.
    Diarize {
        audio: PathBuf,
        samples: u64,
        options: DiarizeOptions,
    },
}

impl Request {
    /// Audio this request processes, for duration-aware worker deadlines.
    /// `None` keeps the fixed query deadline.
    pub fn audio_samples(&self) -> Option<usize> {
        match self {
            Request::Diarize { samples, .. } => {
                Some(usize::try_from(*samples).unwrap_or(usize::MAX))
            }
            _ => None,
        }
    }
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
    /// A non-ASR role finished loading; the supervisor records it exactly as
    /// it records an ASR load.
    Loaded(LoadedInfo),
    /// Native speaker segments, in the diarizer's arrival-order speaker ids.
    Speakers {
        segments: Vec<SpeakerSegment>,
        max_speakers: i32,
    },
}

/// The worker load request for a non-ASR role; `None` keeps Handy's ASR load.
pub fn load_request(spec: &LoadSpec) -> Option<Request> {
    match spec.grain_role {
        Role::Asr => None,
        Role::Diarize => Some(Request::LoadDiarizer {
            path: spec.path.clone(),
            backend: spec.backend,
            device: spec.device.clone(),
        }),
    }
}

/// Build a diarization request for an analysis file, validating it before any
/// worker or model is involved.
pub fn diarize_request(audio: &Path, options: DiarizeOptions) -> Result<Request, String> {
    let samples = analysis_samples(audio)?;
    Ok(Request::Diarize {
        audio: audio.to_path_buf(),
        samples,
        options,
    })
}

/// Validate an analysis file: an absolute regular file of raw little-endian
/// 16 kHz mono float32 samples (Grain's `PcmJournal` layout, also read by the
/// bounded Parakeet adapter). Returns its sample count.
fn analysis_samples(audio: &Path) -> Result<u64, String> {
    if !audio.is_absolute() {
        return Err("Analysis audio must be an absolute path".into());
    }
    let metadata = std::fs::symlink_metadata(audio)
        .map_err(|error| format!("Analysis audio is unavailable: {error}"))?;
    if !metadata.is_file() {
        return Err("Analysis audio must be a regular file".into());
    }
    if metadata.len() % 4 != 0 {
        return Err("Analysis audio is not whole float32 samples".into());
    }
    let samples = metadata.len() / 4;
    if samples == 0 {
        return Err("Analysis audio is empty".into());
    }
    if samples > MAX_ANALYSIS_SAMPLES {
        return Err(format!(
            "Analysis audio is {} minutes; one track can be processed for at most {} minutes",
            samples / (60 * u64::from(ANALYSIS_SAMPLE_RATE)),
            MAX_ANALYSIS_SAMPLES / (60 * u64::from(ANALYSIS_SAMPLE_RATE)),
        ));
    }
    Ok(samples)
}

/// Read an analysis file in the worker into one exact-capacity buffer. The
/// size is re-checked because the file may have changed since the request.
fn read_analysis(audio: &Path, declared: u64) -> Result<Vec<f32>, String> {
    use std::io::Read;
    let samples = analysis_samples(audio)?;
    if samples != declared {
        return Err(format!(
            "Analysis audio changed: expected {declared} samples, found {samples}"
        ));
    }
    let mut file = std::io::BufReader::with_capacity(
        64 * 1024,
        std::fs::File::open(audio)
            .map_err(|error| format!("Analysis audio is unavailable: {error}"))?,
    );
    let mut pcm = Vec::with_capacity(samples as usize);
    let mut bytes = [0u8; 4];
    for _ in 0..samples {
        file.read_exact(&mut bytes)
            .map_err(|error| format!("Analysis audio is truncated: {error}"))?;
        let sample = f32::from_le_bytes(bytes);
        if !sample.is_finite() {
            return Err("Analysis audio contains a non-finite sample".into());
        }
        pcm.push(sample);
    }
    Ok(pcm)
}

/// The worker's non-ASR model slot. Never resident beside Handy's `Session`.
#[derive(Default)]
pub struct WorkerSlot {
    diarizer: Option<Diarizer>,
}

struct Diarizer {
    // Keeps the native model alive through its shared handle.
    session: transcribe_cpp::DiarizeSession,
    max_speakers: i32,
    on_gpu: bool,
}

impl WorkerSlot {
    /// Release the resident non-ASR model (before any other load).
    pub fn clear(&mut self) {
        self.diarizer = None;
    }
}

/// Worker-side dispatch for every Grain request. `asr` is Handy's resident
/// session slot; only a role load may clear it.
pub fn dispatch(
    request: Request,
    asr: &mut Option<(transcribe_cpp::Session, LoadedInfo)>,
    slot: &mut WorkerSlot,
) -> Result<Response, String> {
    match request {
        Request::LoadDiarizer {
            path,
            backend,
            device,
        } => {
            // Free every resident model first so two are never held at once.
            *asr = None;
            slot.clear();
            let (diarizer, info) = load_diarizer(&path, backend, device)?;
            slot.diarizer = Some(diarizer);
            Ok(Response::Loaded(info))
        }
        Request::Diarize {
            audio,
            samples,
            options,
        } => {
            let Some(diarizer) = slot.diarizer.as_mut() else {
                return Err(if asr.is_some() {
                    "The loaded model serves transcription, not diarization".into()
                } else {
                    "no model loaded".into()
                });
            };
            let pcm = read_analysis(&audio, samples)?;
            inject_fault("diarize", diarizer.on_gpu);
            let result = diarizer.session.run(&pcm, &options);
            drop(pcm);
            crate::memory::trim_freed_memory();
            let segments = result.map_err(|error| format!("Diarization failed: {error}"))?;
            Ok(Response::Speakers {
                segments,
                max_speakers: diarizer.max_speakers,
            })
        }
        // Nothing to reset on a diarizer; cleanup must still succeed.
        Request::ResetFlow if asr.is_none() => Ok(Response::Reset),
        request => match asr.as_mut() {
            Some((session, _)) => dispatch_asr(request, session),
            None if slot.diarizer.is_some() => {
                Err("The loaded model serves diarization, not transcription".into())
            }
            None => Err("no model loaded".into()),
        },
    }
}

fn load_diarizer(
    path: &Path,
    backend: Backend,
    selector: DeviceSelector,
) -> Result<(Diarizer, LoadedInfo), String> {
    let device = resolve_device(selector)?;
    inject_fault("diarizer-load", backend != Backend::Cpu);
    let model =
        transcribe_cpp::Model::load_with(path, &transcribe_cpp::ModelOptions { backend, device })
            .map_err(|error| format!("Failed to load diarization model: {error}"))?;
    if !model.roles().contains(transcribe_cpp::Role::Diarize) {
        return Err(format!(
            "Model '{}' does not serve speaker diarization",
            model.arch()
        ));
    }
    let facts = model
        .diarize_info()
        .map_err(|error| format!("Failed to read diarization model facts: {error}"))?;
    if facts.sample_rate != ANALYSIS_SAMPLE_RATE as i32 || facts.max_speakers < 1 {
        return Err(format!(
            "Unsupported diarization model contract ({} Hz, {} speakers)",
            facts.sample_rate, facts.max_speakers
        ));
    }
    let session = model
        .diarize_session()
        .map_err(|error| format!("Failed to create diarization session: {error}"))?;
    let bound = model.device().ok().map(|d| DeviceInfo::from_device(&d));
    let backend_name = model.backend();
    let on_gpu = match &bound {
        Some(device) => device.is_gpu(),
        None => !backend_name.is_empty() && !backend_name.eq_ignore_ascii_case("cpu"),
    };
    let info = LoadedInfo {
        arch: model.arch(),
        variant: model.variant(),
        backend: backend_name,
        device: bound
            .as_ref()
            .map(|d| d.label().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        on_gpu,
        // A diarizer produces no text: report no ASR capability at all.
        capabilities: transcribe_cpp::Capabilities {
            native_sample_rate: facts.sample_rate,
            languages: Vec::new(),
            translate_target_languages: Vec::new(),
            max_timestamp_kind: transcribe_cpp::TimestampKind::Segment,
            supports_language_detect: false,
            supports_translate: false,
            supports_streaming: false,
            supports_spec_decode: false,
            max_audio_ms: 0,
        },
        supports_initial_prompt: false,
        supports_long_form: false,
        supports_vocabulary: false,
    };
    Ok((
        Diarizer {
            session,
            max_speakers: facts.max_speakers,
            on_gpu,
        },
        info,
    ))
}

/// Resolve a device selector exactly as Handy's ASR load does.
fn resolve_device(selector: DeviceSelector) -> Result<Option<transcribe_cpp::Device>, String> {
    Ok(match selector {
        DeviceSelector::Auto => None,
        DeviceSelector::Key(key) => {
            let found = transcribe_cpp::devices().into_iter().find(|d| {
                let info = DeviceInfo::from_device(d);
                info.is_gpu() && info.key == key
            });
            if found.is_none() {
                log::warn!(
                    "Stored transcribe GPU device '{key}' is no longer available; using automatic device selection"
                );
            }
            found
        }
        DeviceSelector::Index(index) => {
            let device = transcribe_cpp::devices()
                .into_iter()
                .find(|d| d.index == Some(index))
                .ok_or_else(|| {
                    format!("No compute device with index {index} (see --list-devices)")
                })?;
            if matches!(
                device.device_type,
                transcribe_cpp::DeviceType::Accel | transcribe_cpp::DeviceType::Unknown
            ) {
                return Err(format!(
                    "Device index {index} ({}) cannot host a model",
                    device.kind
                ));
            }
            Some(device)
        }
    })
}

/// Debug-build fault injection for Grain worker stages, using Handy's
/// `HANDY_WORKER_FAULT=<abort|hang>@<diarizer-load|diarize>` convention with
/// `HANDY_WORKER_FAULT_GPU_ONLY` and `HANDY_WORKER_FAULT_ONCE`.
#[cfg(debug_assertions)]
fn inject_fault(stage: &str, on_gpu: bool) {
    let Ok(spec) = std::env::var("HANDY_WORKER_FAULT") else {
        return;
    };
    let Some((kind, at)) = spec.split_once('@') else {
        return;
    };
    if at != stage || (!on_gpu && std::env::var_os("HANDY_WORKER_FAULT_GPU_ONLY").is_some()) {
        return;
    }
    if let Some(marker) = std::env::var_os("HANDY_WORKER_FAULT_ONCE") {
        if std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .is_err()
        {
            return;
        }
    }
    eprintln!("injected fault: {kind} at {stage}");
    match kind {
        "abort" => std::process::abort(),
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        },
        _ => {}
    }
}

#[cfg(not(debug_assertions))]
fn inject_fault(_stage: &str, _on_gpu: bool) {}

fn dispatch_asr(
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
        Request::LoadDiarizer { .. } | Request::Diarize { .. } => {
            Err("Diarization requests are not ASR queries".into())
        }
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
                grain_role: Role::Asr,
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

    fn write_raw(path: &Path, samples: &[f32]) {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(path, bytes).unwrap();
    }

    fn spec(role: Role) -> LoadSpec {
        LoadSpec {
            path: PathBuf::from("model.gguf"),
            backend: Backend::Vulkan,
            device: DeviceSelector::Key("gpu".into()),
            grain_role: role,
        }
    }

    #[test]
    fn asr_specs_keep_handy_load_and_diarize_specs_carry_their_device_policy() {
        assert!(load_request(&spec(Role::Asr)).is_none());
        match load_request(&spec(Role::Diarize)) {
            Some(Request::LoadDiarizer {
                path,
                backend,
                device,
            }) => {
                assert_eq!(path, PathBuf::from("model.gguf"));
                assert_eq!(backend, Backend::Vulkan);
                assert_eq!(device, DeviceSelector::Key("gpu".into()));
            }
            other => panic!("expected a diarizer load, got {other:?}"),
        }
        assert_eq!(Role::default(), Role::Asr);
    }

    #[test]
    fn only_diarization_requests_scale_their_worker_deadline() {
        assert_eq!(Request::FlowModel.audio_samples(), None);
        assert_eq!(Request::ResetFlow.audio_samples(), None);
        let diarize = Request::Diarize {
            audio: PathBuf::from("a.f32le"),
            samples: 48_000,
            options: DiarizeOptions::default(),
        };
        assert_eq!(diarize.audio_samples(), Some(48_000));
    }

    #[test]
    fn analysis_audio_accepts_only_absolute_whole_float32_files_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.f32le");
        write_raw(&good, &[0.0, 0.25, -0.5]);
        let request = diarize_request(&good, DiarizeOptions::default()).unwrap();
        assert_eq!(request.audio_samples(), Some(3));
        assert_eq!(read_analysis(&good, 3).unwrap(), vec![0.0, 0.25, -0.5]);

        let relative = diarize_request(Path::new("good.f32le"), DiarizeOptions::default());
        assert!(relative.unwrap_err().contains("absolute"));
        let missing = diarize_request(&dir.path().join("none"), DiarizeOptions::default());
        assert!(missing.unwrap_err().contains("unavailable"));
        assert!(diarize_request(dir.path(), DiarizeOptions::default())
            .unwrap_err()
            .contains("regular file"));
        let torn = dir.path().join("torn.f32le");
        std::fs::write(&torn, [0u8; 6]).unwrap();
        assert!(diarize_request(&torn, DiarizeOptions::default())
            .unwrap_err()
            .contains("whole float32"));
        let empty = dir.path().join("empty.f32le");
        write_raw(&empty, &[]);
        assert!(diarize_request(&empty, DiarizeOptions::default())
            .unwrap_err()
            .contains("empty"));
        // The 90-minute cap, checked from the size alone.
        let long = dir.path().join("long.f32le");
        std::fs::File::create(&long)
            .unwrap()
            .set_len((MAX_ANALYSIS_SAMPLES + 1) * 4)
            .unwrap();
        let error = diarize_request(&long, DiarizeOptions::default()).unwrap_err();
        assert!(error.contains("at most 90 minutes"), "{error}");
    }

    #[test]
    fn worker_rejects_changed_or_non_finite_analysis_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.f32le");
        write_raw(&path, &[0.1, 0.2]);
        assert!(read_analysis(&path, 3).unwrap_err().contains("changed"));
        write_raw(&path, &[0.1, f32::NAN]);
        assert!(read_analysis(&path, 2).unwrap_err().contains("non-finite"));
    }

    #[test]
    fn worker_dispatch_without_a_model_reports_the_missing_role() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.f32le");
        write_raw(&path, &[0.0; 16]);
        let mut asr = None;
        let mut slot = WorkerSlot::default();
        let diarize = diarize_request(&path, DiarizeOptions::default()).unwrap();
        assert_eq!(
            dispatch(diarize, &mut asr, &mut slot).unwrap_err(),
            "no model loaded"
        );
        assert_eq!(
            dispatch(Request::FlowModel, &mut asr, &mut slot).unwrap_err(),
            "no model loaded"
        );
        // Cleanup never fails for lack of a model.
        assert!(matches!(
            dispatch(Request::ResetFlow, &mut asr, &mut slot),
            Ok(Response::Reset)
        ));
    }

    #[test]
    fn diarization_messages_round_trip_including_unknown_confidence() {
        let request = Request::Diarize {
            audio: PathBuf::from("C:/meet/track.wav"),
            samples: 16_000,
            options: DiarizeOptions {
                family: Some(transcribe_cpp::DiarizeExtension::Sortformer(
                    transcribe_cpp::SortformerDiarizeOptions {
                        preset: Some(transcribe_cpp::SortformerPreset::VeryHighLatency),
                    },
                )),
            },
        };
        let json = serde_json::to_vec(&request).unwrap();
        match serde_json::from_slice::<Request>(&json).unwrap() {
            Request::Diarize {
                samples, options, ..
            } => {
                assert_eq!(samples, 16_000);
                assert_eq!(options, request_options(&request));
            }
            other => panic!("unexpected {other:?}"),
        }
        let response = Response::Speakers {
            segments: vec![SpeakerSegment {
                t0_ms: 10,
                t1_ms: 900,
                speaker_id: 2,
                p: f32::NAN,
            }],
            max_speakers: 4,
        };
        let json = serde_json::to_vec(&response).unwrap();
        match serde_json::from_slice::<Response>(&json).unwrap() {
            Response::Speakers {
                segments,
                max_speakers,
            } => {
                assert_eq!(max_speakers, 4);
                assert_eq!((segments[0].t0_ms, segments[0].speaker_id), (10, 2));
                assert!(segments[0].p.is_nan());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    fn request_options(request: &Request) -> DiarizeOptions {
        match request {
            Request::Diarize { options, .. } => options.clone(),
            _ => unreachable!(),
        }
    }
}
