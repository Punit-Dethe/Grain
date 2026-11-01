//! Grain batch callers use the shared local model and final-text cleanup.
use crate::managers::transcription::TranscriptionManager;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Use {
    Load,
    Batch,
    Stream,
    Flow(u64),
    /// One meeting processing attempt; owns every model role it loads.
    Meeting(u64),
}

#[derive(Default)]
pub(crate) struct State {
    owner: std::sync::Mutex<(u64, Option<Use>)>,
}

pub(crate) struct Lease {
    state: Arc<State>,
    ticket: u64,
}

impl State {
    pub(crate) fn claim(self: &Arc<Self>, usage: Use) -> Result<Lease, String> {
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.1.is_some() {
            return Err("The transcription model is already in use".into());
        }
        owner.0 = owner
            .0
            .checked_add(1)
            .ok_or("Inference ownership counter exhausted")?;
        owner.1 = Some(usage);
        Ok(Lease {
            state: Arc::clone(self),
            ticket: owner.0,
        })
    }

    pub(crate) fn cancel_flow(&self, session_id: u64, cancel: impl FnOnce()) {
        self.cancel_owner(Use::Flow(session_id), cancel);
    }

    /// Run `cancel` only while `usage` still owns inference, so a stale cancel
    /// can never stop a replacement owner.
    pub(crate) fn cancel_owner(&self, usage: Use, cancel: impl FnOnce()) {
        let owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.1 == Some(usage) {
            cancel();
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut owner = self
            .state
            .owner
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if owner.0 == self.ticket {
            owner.1 = None;
        }
    }
}

pub(crate) struct FlowSession<'a> {
    pub engine: &'a crate::engine_supervisor::EngineSupervisor,
    pub mark: crate::engine_supervisor::CancelMark,
    pub cancelled: &'a std::sync::atomic::AtomicBool,
}

impl FlowSession<'_> {
    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err("Flow session cancelled".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for FlowSession<'_> {
    fn drop(&mut self) {
        // Still inside the exclusive inference lease. The epoch gate rejects
        // stale cleanup after cancellation, so it cannot reset a new owner.
        let _ = self
            .engine
            .grain_request(self.mark, crate::grain_engine::Request::ResetFlow);
    }
}

impl crate::tdt_flow::Inference for FlowSession<'_> {
    fn flow_model(&self) -> Result<crate::tdt_flow::FlowModel, String> {
        self.check_cancelled()?;
        match self
            .engine
            .grain_request(self.mark, crate::grain_engine::Request::FlowModel)
            .map_err(|error| error.to_string())?
        {
            crate::grain_engine::Response::FlowModel {
                arch,
                variant,
                supported,
            } => Ok(crate::tdt_flow::FlowModel {
                arch,
                variant,
                supported,
            }),
            _ => Err("Worker returned an unexpected Flow model response".into()),
        }
    }

    fn run(
        &mut self,
        audio: &[f32],
        options: &transcribe_cpp::RunOptions,
    ) -> Result<transcribe_cpp::Transcript, String> {
        self.check_cancelled()?;
        // Each copy is bounded by the production window ceiling, independent of
        // recording length. The worker transport owns queued audio.
        self.engine
            .transcribe_since(self.mark, audio.to_vec(), options.clone())
            .map_err(|error| error.to_string())
    }

    fn detokenize(&self, tokens: &[i32]) -> Result<String, String> {
        self.check_cancelled()?;
        match self
            .engine
            .grain_request(
                self.mark,
                crate::grain_engine::Request::Detokenize {
                    tokens: tokens.to_vec(),
                },
            )
            .map_err(|error| error.to_string())?
        {
            crate::grain_engine::Response::Text(text) => Ok(text),
            _ => Err("Worker returned an unexpected detokenization response".into()),
        }
    }
}

/// Validated native diarization evidence for one analysis file.
#[derive(Debug, Clone)]
#[cfg_attr(not(test), allow(dead_code))] // Wired by the saved-audio pipeline.
pub(crate) struct Diarization {
    /// Arrival-order speaker activity; ids are local to this analysis run.
    pub segments: Vec<transcribe_cpp::SpeakerSegment>,
    pub max_speakers: i32,
    pub samples: u64,
}

/// Native segment times may round past the last sample by at most one
/// diarizer frame (80 ms); anything later is invalid evidence.
const DIARIZATION_END_SLACK_MS: i64 = 80;

/// Check native speaker evidence before anything is persisted or assigned.
/// Overshoot within the frame slack is clamped to the real audio end: no audio
/// exists beyond it. Malformed evidence is an error, never silently repaired.
pub(crate) fn validate_diarization(
    mut segments: Vec<transcribe_cpp::SpeakerSegment>,
    max_speakers: i32,
    samples: u64,
) -> Result<Vec<transcribe_cpp::SpeakerSegment>, String> {
    let rate = u64::from(crate::grain_engine::ANALYSIS_SAMPLE_RATE);
    let duration_ms = i64::try_from(samples.saturating_mul(1_000) / rate).unwrap_or(i64::MAX);
    if max_speakers < 1 {
        return Err("Diarizer reported no speaker slots".into());
    }
    for segment in &mut segments {
        if segment.speaker_id < 1 || segment.speaker_id > max_speakers {
            return Err(format!(
                "Diarizer returned speaker {} outside 1..={max_speakers}",
                segment.speaker_id
            ));
        }
        if segment.t0_ms < 0
            || segment.t1_ms < segment.t0_ms
            || segment.t1_ms > duration_ms.saturating_add(DIARIZATION_END_SLACK_MS)
        {
            return Err(format!(
                "Diarizer returned an invalid interval {}..{} ms for {duration_ms} ms of audio",
                segment.t0_ms, segment.t1_ms
            ));
        }
        segment.t1_ms = segment.t1_ms.min(duration_ms);
        segment.t0_ms = segment.t0_ms.min(segment.t1_ms);
    }
    Ok(segments)
}

/// One meeting processing attempt's view of the shared engine. It exists only
/// inside the attempt's exclusive [`Use::Meeting`] lease. Each phase loads one
/// model role; loading the next role releases the previous one in the worker,
/// and the lease owner unloads the last one.
#[cfg_attr(not(test), allow(dead_code))] // Wired by the saved-audio pipeline.
pub(crate) struct MeetingEngine<'a> {
    pub engine: &'a crate::engine_supervisor::EngineSupervisor,
    pub mark: crate::engine_supervisor::CancelMark,
    pub cancelled: &'a std::sync::atomic::AtomicBool,
    /// Device policy frozen when the attempt started.
    pub backend: transcribe_cpp::Backend,
    pub device: crate::engine_supervisor::DeviceSelector,
    pub role: std::cell::Cell<Option<crate::grain_engine::Role>>,
}

#[cfg_attr(not(test), allow(dead_code))] // Wired by the saved-audio pipeline.
impl<'a> MeetingEngine<'a> {
    /// Whether the attempt's owner asked it to stop.
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }

    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err("Meeting processing cancelled".into())
        } else {
            Ok(())
        }
    }

    /// Load `path` in `role`, replacing whatever this attempt loaded before.
    /// Model loads are not interruptible; a cancel that arrives during one is
    /// honoured as soon as it finishes, before any inference starts.
    pub(crate) fn load(
        &self,
        path: &std::path::Path,
        role: crate::grain_engine::Role,
    ) -> Result<crate::engine_supervisor::LoadedInfo, String> {
        self.check_cancelled()?;
        self.role.set(None);
        let info = self
            .engine
            .load(crate::engine_supervisor::LoadSpec {
                path: path.to_path_buf(),
                backend: self.backend,
                device: self.device.clone(),
                grain_role: role,
            })
            .map_err(|error| error.to_string())?;
        self.check_cancelled()?;
        self.role.set(Some(role));
        Ok(info)
    }

    /// Diarize one validated analysis WAV with the loaded diarizer.
    pub(crate) fn diarize(
        &self,
        audio: &std::path::Path,
        options: transcribe_cpp::DiarizeOptions,
    ) -> Result<Diarization, String> {
        self.check_cancelled()?;
        if self.role.get() != Some(crate::grain_engine::Role::Diarize) {
            return Err("Load a diarization model before diarizing".into());
        }
        let request = crate::grain_engine::diarize_request(audio, options)?;
        let samples = request.audio_samples().unwrap_or(0) as u64;
        match self
            .engine
            .grain_request(self.mark, request)
            .map_err(|error| error.to_string())?
        {
            crate::grain_engine::Response::Speakers {
                segments,
                max_speakers,
            } => Ok(Diarization {
                segments: validate_diarization(segments, max_speakers, samples)?,
                max_speakers,
                samples,
            }),
            _ => Err("Worker returned an unexpected diarization response".into()),
        }
    }

    /// Bounded Parakeet decoding through the existing Flow adapter. Dropping
    /// the session resets the native decoder under this attempt's epoch.
    pub(crate) fn asr(&self) -> Result<FlowSession<'a>, String> {
        self.check_cancelled()?;
        if self.role.get() != Some(crate::grain_engine::Role::Asr) {
            return Err("Load a transcription model before transcribing".into());
        }
        Ok(FlowSession {
            engine: self.engine,
            mark: self.mark,
            cancelled: self.cancelled,
        })
    }
}

/// Run local inference off the async runtime. Loading is idempotent; the shared
/// manager waits for an in-flight load and owns engine checkout and cleanup.
pub async fn transcribe(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    if !tm.is_model_loaded() {
        tm.initiate_model_load();
    }
    tokio::task::spawn_blocking(move || tm.transcribe(samples))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn exclusive_lease_releases_and_stale_flow_cancel_cannot_kill_new_owner() {
        let state = Arc::new(State::default());
        let lease = state.claim(Use::Flow(41)).unwrap();
        assert!(state.claim(Use::Load).is_err());
        assert!(state.claim(Use::Batch).is_err());
        assert!(state.claim(Use::Stream).is_err());
        let cancelled = std::cell::Cell::new(false);
        state.cancel_flow(40, || cancelled.set(true));
        assert!(!cancelled.get());
        state.cancel_flow(41, || cancelled.set(true));
        assert!(cancelled.get());
        drop(lease);
        let _new = state.claim(Use::Flow(42)).unwrap();
        state.cancel_flow(41, || panic!("stale cancellation reached the replacement"));
    }

    #[test]
    fn meeting_lease_excludes_dictation_and_stale_meeting_cancel_is_ignored() {
        let state = Arc::new(State::default());
        let lease = state.claim(Use::Meeting(7)).unwrap();
        for other in [
            Use::Load,
            Use::Batch,
            Use::Stream,
            Use::Flow(1),
            Use::Meeting(8),
        ] {
            assert!(state.claim(other).is_err(), "{other:?} must wait");
        }
        state.cancel_owner(Use::Meeting(6), || panic!("stale attempt cancelled"));
        state.cancel_owner(Use::Flow(7), || panic!("wrong owner kind cancelled"));
        let cancelled = std::cell::Cell::new(false);
        state.cancel_owner(Use::Meeting(7), || cancelled.set(true));
        assert!(cancelled.get());
        drop(lease);
        let _load = state.claim(Use::Load).unwrap();
        state.cancel_owner(Use::Meeting(7), || {
            panic!("finished attempt cancelled new owner")
        });
    }

    fn segment(t0_ms: i64, t1_ms: i64, speaker_id: i32) -> transcribe_cpp::SpeakerSegment {
        transcribe_cpp::SpeakerSegment {
            t0_ms,
            t1_ms,
            speaker_id,
            p: f32::NAN,
        }
    }

    #[test]
    fn diarization_evidence_is_validated_and_only_frame_overshoot_is_clamped() {
        // 12 s of 16 kHz audio.
        let samples = 192_000;
        let valid = validate_diarization(
            vec![
                segment(0, 2_400, 1),
                segment(4_240, 12_060, 4),
                segment(500, 500, 2),
            ],
            4,
            samples,
        )
        .unwrap();
        assert_eq!((valid[1].t0_ms, valid[1].t1_ms), (4_240, 12_000));
        assert_eq!((valid[2].t0_ms, valid[2].t1_ms), (500, 500));

        for (bad, reason) in [
            (segment(0, 100, 0), "speaker"),
            (segment(0, 100, 5), "speaker"),
            (segment(-1, 100, 1), "interval"),
            (segment(200, 100, 1), "interval"),
            (segment(0, 12_081, 1), "interval"),
        ] {
            let error = validate_diarization(vec![bad], 4, samples).unwrap_err();
            assert!(error.contains(reason), "{error}");
        }
        assert!(validate_diarization(Vec::new(), 0, samples).is_err());
        assert!(validate_diarization(Vec::new(), 4, samples)
            .unwrap()
            .is_empty());
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_chinese_script_setting(
    app: AppHandle,
    script: grain_core::settings::ChineseScript,
) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.chinese_script = script)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_vad_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.vad_enabled = enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn change_long_form_chunking_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    app.state::<Arc<grain_core::AppContext>>()
        .update_settings(|settings| settings.long_form_chunking = enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn change_vad_backend_setting(
    app: AppHandle,
    backend: grain_core::settings::VadBackend,
) -> Result<(), String> {
    let manager = Arc::clone(&app.state::<Arc<crate::managers::audio::AudioRecordingManager>>());
    tauri::async_runtime::spawn_blocking(move || {
        let context = app.state::<Arc<grain_core::AppContext>>();
        let previous = context.settings().vad_backend;
        manager
            .update_vad_backend(backend)
            .map_err(|error| error.to_string())?;
        if let Err(error) = context.update_settings(|settings| settings.vad_backend = backend) {
            let _ = context.update_settings(|settings| settings.vad_backend = previous);
            let _ = manager.update_vad_backend(previous);
            return Err(error.to_string());
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Real-worker role bridge checks: the built Grain executable, a Sortformer
/// v2.1 diarizer, a reviewed Parakeet TDT model and a 16 kHz mono WAV. Run
/// with `--ignored --test-threads=1`; the fault test sets process environment.
#[cfg(test)]
mod meeting_worker_tests {
    use super::*;
    use crate::engine_supervisor::{DeviceSelector, EngineSupervisor};
    use crate::grain_engine::Role;
    use crate::tdt_flow::Inference;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    fn env_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("set {name}"))
            .into()
    }

    fn backend() -> transcribe_cpp::Backend {
        match std::env::var("GRAIN_MEET_TEST_BACKEND")
            .as_deref()
            .unwrap_or("cpu")
        {
            "cpu" => transcribe_cpp::Backend::Cpu,
            "vulkan" => transcribe_cpp::Backend::Vulkan,
            other => panic!("expected cpu or vulkan, got {other}"),
        }
    }

    /// Convert the fixture to the analysis contract, repeated `copies` times.
    fn analysis_wav(dir: &Path, name: &str, copies: usize) -> (PathBuf, usize) {
        let mut reader = hound::WavReader::open(env_path("GRAIN_MEET_TEST_AUDIO")).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels), (16_000, 1));
        let pcm: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i16>()
                .map(|s| f32::from(s.unwrap()) / 32768.0)
                .collect(),
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        };
        let path = dir.join(name);
        let mut out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        for _ in 0..copies {
            for &sample in &pcm {
                std::io::Write::write_all(&mut out, &sample.to_le_bytes()).unwrap();
            }
        }
        std::io::Write::flush(&mut out).unwrap();
        (path, pcm.len() * copies)
    }

    fn meeting<'a>(engine: &'a EngineSupervisor, cancelled: &'a AtomicBool) -> MeetingEngine<'a> {
        MeetingEngine {
            engine,
            mark: engine.cancel_mark(),
            cancelled,
            backend: backend(),
            device: DeviceSelector::Auto,
            role: std::cell::Cell::new(None),
        }
    }

    fn offline() -> transcribe_cpp::DiarizeOptions {
        transcribe_cpp::DiarizeOptions {
            family: Some(transcribe_cpp::DiarizeExtension::Sortformer(
                transcribe_cpp::SortformerDiarizeOptions {
                    preset: Some(transcribe_cpp::SortformerPreset::VeryHighLatency),
                },
            )),
        }
    }

    fn speakers(result: &Diarization) -> std::collections::BTreeSet<i32> {
        result.segments.iter().map(|s| s.speaker_id).collect()
    }

    #[test]
    #[ignore = "requires Grain worker executable, Sortformer, Parakeet and WAV fixtures"]
    fn worker_switches_roles_rejects_wrong_roles_and_recovers_after_cancel() {
        let diarizer = env_path("GRAIN_MEET_TEST_DIARIZER");
        let asr_model = env_path("GRAIN_MEET_TEST_ASR");
        let dir = tempfile::tempdir().unwrap();
        let (fixture, fixture_samples) = analysis_wav(dir.path(), "fixture.wav", 1);
        let engine = EngineSupervisor::new(true);
        let cancelled = AtomicBool::new(false);
        let attempt = meeting(&engine, &cancelled);

        // ASR first: the dictation role and its Flow contract.
        let info = attempt.load(&asr_model, Role::Asr).unwrap();
        assert_eq!(info.arch, "parakeet");
        assert!(attempt.asr().unwrap().flow_model().unwrap().supported);
        assert!(attempt
            .diarize(&fixture, offline())
            .unwrap_err()
            .contains("Load a diarization"));

        // Wrong roles fail explicitly and leave nothing loaded.
        let error = attempt.load(&asr_model, Role::Diarize).unwrap_err();
        assert!(
            error.contains("does not serve speaker diarization"),
            "{error}"
        );
        assert!(engine.loaded().is_none());
        assert!(attempt.load(&diarizer, Role::Asr).is_err());
        assert!(engine.loaded().is_none());

        // Diarizer: the ASR session is gone, ASR work is refused, not retried.
        let info = attempt.load(&diarizer, Role::Diarize).unwrap();
        assert_eq!(info.arch, "sortformer");
        assert_eq!(info.on_gpu, backend() != transcribe_cpp::Backend::Cpu);
        assert!(attempt.asr().is_err());
        let query = engine
            .grain_request(attempt.mark, crate::grain_engine::Request::FlowModel)
            .unwrap_err()
            .to_string();
        assert!(query.contains("serves diarization"), "{query}");
        assert!(engine
            .transcribe_since(attempt.mark, vec![0.0; 16_000], Default::default())
            .is_err());
        assert_eq!(
            engine.loaded().unwrap().arch,
            "sortformer",
            "a refusal is not a crash"
        );

        let started = Instant::now();
        let result = attempt.diarize(&fixture, offline()).unwrap();
        eprintln!(
            "fixture: {} samples, {} segments, speakers {:?}, {:?}",
            fixture_samples,
            result.segments.len(),
            speakers(&result),
            started.elapsed()
        );
        assert_eq!(result.max_speakers, 4);
        assert_eq!(result.samples, fixture_samples as u64);
        assert_eq!(speakers(&result).len(), 2, "two-speaker fixture");

        // Cancel a long run in flight: the worker is killed, the epoch rejects
        // it, and a fresh attempt respawns the diarizer role, not a stale ASR.
        let (long, long_samples) = analysis_wav(dir.path(), "long.wav", 50);
        let long_run = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                // The attempt lives on its processing thread; cancellation
                // arrives from another thread through the engine, as in the app.
                let running = MeetingEngine {
                    mark: attempt.mark,
                    ..meeting(&engine, &cancelled)
                };
                running.role.set(Some(Role::Diarize));
                running.diarize(&long, offline())
            });
            std::thread::sleep(Duration::from_millis(700));
            engine.cancel();
            worker.join().unwrap()
        });
        let cancelled_error = long_run.unwrap_err();
        assert!(cancelled_error.contains("cancelled"), "{cancelled_error}");
        assert!(
            attempt.diarize(&fixture, offline()).is_err(),
            "old epoch stays cancelled"
        );
        let fresh = meeting(&engine, &cancelled);
        fresh.role.set(Some(Role::Diarize));
        let started = Instant::now();
        let again = fresh.diarize(&long, offline()).unwrap();
        eprintln!(
            "long: {} samples ({:.0} s), {} segments, {:?} after respawn",
            long_samples,
            long_samples as f64 / 16_000.0,
            again.segments.len(),
            started.elapsed()
        );
        assert_eq!(engine.loaded().unwrap().arch, "sortformer");

        // Back to ASR: ASR -> diarization -> ASR with one resident role.
        fresh.load(&asr_model, Role::Asr).unwrap();
        assert!(fresh.asr().unwrap().flow_model().unwrap().supported);
        engine.unload().wait();
        assert!(!engine.grain_has_resident_model());
    }

    #[test]
    #[ignore = "requires a GPU, the debug Grain worker and Sortformer fixtures"]
    fn gpu_loss_during_diarization_retries_the_diarizer_role_on_cpu() {
        if backend() != transcribe_cpp::Backend::Vulkan {
            eprintln!("skip: set GRAIN_MEET_TEST_BACKEND=vulkan");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = analysis_wav(dir.path(), "fixture.wav", 1);
        std::env::set_var("HANDY_WORKER_FAULT", "abort@diarize");
        std::env::set_var("HANDY_WORKER_FAULT_GPU_ONLY", "1");
        let engine = EngineSupervisor::new(true);
        let cancelled = AtomicBool::new(false);
        let attempt = meeting(&engine, &cancelled);
        let loaded = attempt.load(&env_path("GRAIN_MEET_TEST_DIARIZER"), Role::Diarize);
        let result = loaded.and_then(|info| {
            assert!(info.on_gpu);
            attempt.diarize(&fixture, offline())
        });
        std::env::remove_var("HANDY_WORKER_FAULT");
        std::env::remove_var("HANDY_WORKER_FAULT_GPU_ONLY");
        let result = result.unwrap();
        assert_eq!(speakers(&result).len(), 2);
        let info = engine.loaded().unwrap();
        assert_eq!(
            info.arch, "sortformer",
            "recovery reloads the diarizer role"
        );
        assert!(!info.on_gpu, "recovery runs on CPU");
        engine.unload().wait();
    }

    /// Resource probe: diarize `GRAIN_MEET_TEST_MINUTES` of repeated fixture
    /// audio (default 10). Repetition stresses memory, not recognition quality.
    /// Measure the worker's peak memory externally while this runs.
    #[test]
    #[ignore = "requires Grain worker executable, Sortformer and WAV fixtures"]
    fn worker_diarizes_long_analysis_audio() {
        let minutes: f64 = std::env::var("GRAIN_MEET_TEST_MINUTES")
            .map(|m| m.parse().unwrap())
            .unwrap_or(10.0);
        let dir = tempfile::tempdir().unwrap();
        let (_, fixture_samples) = analysis_wav(dir.path(), "probe.wav", 1);
        let copies = ((minutes * 60.0 * 16_000.0) / fixture_samples as f64).ceil() as usize;
        let (long, samples) = analysis_wav(dir.path(), "long.wav", copies);
        let engine = EngineSupervisor::new(true);
        let cancelled = AtomicBool::new(false);
        let attempt = meeting(&engine, &cancelled);
        let info = attempt
            .load(&env_path("GRAIN_MEET_TEST_DIARIZER"), Role::Diarize)
            .unwrap();
        let started = Instant::now();
        let result = attempt.diarize(&long, offline()).unwrap();
        let elapsed = started.elapsed();
        let audio_s = samples as f64 / 16_000.0;
        eprintln!(
            "probe: {audio_s:.0} s on {} ({}): {} segments, speakers {:?}, {elapsed:?} ({:.0}x real time)",
            info.backend,
            info.device,
            result.segments.len(),
            speakers(&result),
            audio_s / elapsed.as_secs_f64()
        );
        assert!(!result.segments.is_empty());
        engine.unload().wait();
    }
}
