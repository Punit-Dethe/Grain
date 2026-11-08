//! Meeting audio sources: the microphone and system audio, each with its own
//! on/off choice and device. These are Recall's own settings, stored beside the
//! meetings; they never read or change the dictation microphone.
//!
//! Each enabled source is captured by its own stream for the length of one
//! recording and closed when it stops. The microphone uses `handy-recorder`
//! (its own instance, not the dictation recorder). System audio is a WASAPI
//! loopback of an output device through cpal, Windows only for now.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Every source delivers this rate to the writer: the recorder resamples the
/// microphone, and loopback is converted to it.
pub const CAPTURE_RATE: u32 = 48_000;
const SETTINGS_FILE: &str = "sources.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MicrophoneChoice {
    pub enabled: bool,
    /// A `handy-recorder` device id; `None` follows the system default.
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SystemAudioChoice {
    pub enabled: bool,
    /// A cpal output device id; `None` follows the default output.
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceSettings {
    pub microphone: MicrophoneChoice,
    pub system_audio: SystemAudioChoice,
}

impl Default for SourceSettings {
    fn default() -> Self {
        Self {
            microphone: MicrophoneChoice {
                enabled: true,
                device_id: None,
            },
            system_audio: SystemAudioChoice {
                enabled: system_audio_supported(),
                device_id: None,
            },
        }
    }
}

impl SourceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.microphone.enabled && !self.system_audio.enabled {
            return Err("Turn on the microphone, system audio, or both".into());
        }
        if self.system_audio.enabled && !system_audio_supported() {
            return Err("System audio recording is not available on this platform yet".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn system_audio_supported() -> bool {
    cfg!(windows)
}

fn settings_path(root: &Path) -> PathBuf {
    root.join(SETTINGS_FILE)
}

/// Recall's source settings; defaults when none were saved or the file is unreadable.
pub fn load_settings(root: &Path) -> SourceSettings {
    std::fs::read(settings_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(root: &Path, settings: &SourceSettings) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("Could not save audio sources: {e}"))?;
    let tmp = root.join(format!("{SETTINGS_FILE}.tmp"));
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, settings_path(root)))
        .map_err(|e| format!("Could not save audio sources: {e}"))
}

pub fn list_microphones() -> Result<Vec<SourceDevice>, String> {
    Ok(handy_recorder::list_input_devices()
        .map_err(|e| format!("Could not list microphones: {e}"))?
        .into_iter()
        .filter(|device| !device.is_monitor)
        .map(|device| SourceDevice {
            id: device.id,
            name: device.name,
            is_default: device.is_default,
        })
        .collect())
}

/// The display name of a microphone choice, for the track label.
pub fn microphone_name(choice: &MicrophoneChoice) -> String {
    let devices = list_microphones().unwrap_or_default();
    match &choice.device_id {
        Some(id) => devices
            .iter()
            .find(|d| &d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "Microphone".into()),
        None => devices
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "Default microphone".into()),
    }
}

/// A source's frames, as delivered to the writer.
pub type Deliver = Arc<dyn Fn(&[f32], u32, u16) + Send + Sync>;

// --- Microphone -------------------------------------------------------------

struct MicSink {
    deliver: Deliver,
}

impl handy_recorder::Sink for MicSink {
    fn process_chunk(&mut self, chunk: handy_recorder::AudioChunk<'_>) {
        let samples = &chunk.samples[..chunk.valid_frames * usize::from(chunk.channels)];
        (self.deliver)(samples, chunk.sample_rate, chunk.channels);
    }
}

pub struct MicCapture {
    recorder: Option<handy_recorder::Recorder<MicSink>>,
}

impl MicCapture {
    /// Open the chosen microphone and start delivering 48 kHz mono frames.
    /// `failed` is set if the device later fails (unplugged, revoked).
    pub fn start(
        choice: &MicrophoneChoice,
        deliver: Deliver,
        failed: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let config = handy_recorder::RecorderConfig {
            device: choice.device_id.clone(),
            sample_rate: Some(CAPTURE_RATE),
            channels: handy_recorder::Channels::MixToMono,
            frames_per_chunk: Some(CAPTURE_RATE as usize / 100),
            ..Default::default()
        };
        let recorder = handy_recorder::Recorder::open_with_failure_handler(config, move |_| {
            failed.store(true, Ordering::Release)
        })
        .map_err(|e| format!("Could not open the microphone: {e}"))?;
        recorder
            .start(MicSink { deliver })
            .map_err(|e| format!("Could not start the microphone: {}", e.error))?;
        Ok(Self {
            recorder: Some(recorder),
        })
    }
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        if let Some(recorder) = self.recorder.take() {
            let _ = recorder.stop();
            let _ = recorder.close();
        }
    }
}

// --- System audio (WASAPI loopback) ----------------------------------------

#[cfg(windows)]
mod loopback {
    use super::*;
    use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use rodio::cpal::{self, SampleFormat};
    use std::sync::mpsc;

    pub fn list_outputs() -> Result<Vec<SourceDevice>, String> {
        let host = cpal::default_host();
        let default_id = host.default_output_device().and_then(|d| d.id().ok());
        let devices = host
            .output_devices()
            .map_err(|e| format!("Could not list audio outputs: {e}"))?;
        Ok(devices
            .filter_map(|device| {
                let id = device.id().ok()?;
                let name = device.description().ok()?.name().to_owned();
                Some(SourceDevice {
                    is_default: default_id.as_ref() == Some(&id),
                    id: id.to_string(),
                    name,
                })
            })
            .collect())
    }

    fn device(device_id: Option<&str>) -> Result<cpal::Device, String> {
        let host = cpal::default_host();
        match device_id {
            Some(id) => {
                let id: cpal::DeviceId =
                    id.parse().map_err(|_| "Unknown audio output".to_string())?;
                host.device_by_id(&id)
                    .ok_or_else(|| "The chosen audio output is not available".to_string())
            }
            None => host
                .default_output_device()
                .ok_or_else(|| "There is no audio output to record".to_string()),
        }
    }

    pub fn output_name(device_id: Option<&str>) -> String {
        device(device_id)
            .ok()
            .and_then(|d| d.description().ok().map(|desc| desc.name().to_owned()))
            .unwrap_or_else(|| "System audio".into())
    }

    /// The loopback stream lives on its own thread (cpal streams are not Send
    /// on every backend) and closes when `stop` is dropped.
    pub struct SystemCapture {
        stop: Option<mpsc::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl SystemCapture {
        pub fn start(
            choice: &SystemAudioChoice,
            deliver: Deliver,
            failed: Arc<AtomicBool>,
        ) -> Result<Self, String> {
            let (ready_tx, ready) = mpsc::channel::<Result<(), String>>();
            let (stop, stopped) = mpsc::channel::<()>();
            let device_id = choice.device_id.clone();
            let thread = std::thread::Builder::new()
                .name("grain-meet-loopback".into())
                .spawn(move || {
                    let opened = (|| -> Result<cpal::Stream, String> {
                        let device = device(device_id.as_deref())?;
                        let supported = device
                            .default_output_config()
                            .map_err(|e| format!("Could not read the audio output format: {e}"))?;
                        let format = supported.sample_format();
                        let config: cpal::StreamConfig = supported.into();
                        let rate = config.sample_rate;
                        let channels = config.channels;
                        let on_error = {
                            let failed = Arc::clone(&failed);
                            move |error: cpal::Error| {
                                use cpal::ErrorKind;
                                // Glitches and reroutes leave the stream running.
                                if matches!(
                                    error.kind(),
                                    ErrorKind::Xrun
                                        | ErrorKind::RealtimeDenied
                                        | ErrorKind::DeviceChanged
                                ) {
                                    log::debug!("System audio capture: {error}");
                                    return;
                                }
                                log::warn!("System audio capture stopped: {error}");
                                failed.store(true, Ordering::Release);
                            }
                        };
                        let stream = match format {
                            SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
                                config,
                                move |data: &[f32], _| deliver(data, rate, channels),
                                on_error,
                                None,
                            ),
                            SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
                                config,
                                move |data: &[i16], _| {
                                    let samples: Vec<f32> =
                                        data.iter().map(|&s| f32::from(s) / 32768.0).collect();
                                    deliver(&samples, rate, channels);
                                },
                                on_error,
                                None,
                            ),
                            other => {
                                return Err(format!("Unsupported audio output format {other:?}"))
                            }
                        }
                        .map_err(|e| format!("Could not record system audio: {e}"))?;
                        stream
                            .play()
                            .map_err(|e| format!("Could not record system audio: {e}"))?;
                        Ok(stream)
                    })();
                    match opened {
                        Ok(stream) => {
                            let _ = ready_tx.send(Ok(()));
                            let _ = stopped.recv();
                            drop(stream);
                        }
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                        }
                    }
                })
                .map_err(|e| format!("Could not start system audio capture: {e}"))?;
            match ready.recv() {
                Ok(Ok(())) => Ok(Self {
                    stop: Some(stop),
                    thread: Some(thread),
                }),
                Ok(Err(error)) => {
                    let _ = thread.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = thread.join();
                    Err("System audio capture stopped unexpectedly".into())
                }
            }
        }
    }

    impl Drop for SystemCapture {
        fn drop(&mut self) {
            drop(self.stop.take());
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(windows)]
pub use loopback::{list_outputs, output_name, SystemCapture};

#[cfg(not(windows))]
pub fn list_outputs() -> Result<Vec<SourceDevice>, String> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn output_name(_device_id: Option<&str>) -> String {
    "System audio".into()
}

#[cfg(not(windows))]
pub struct SystemCapture;

#[cfg(not(windows))]
impl SystemCapture {
    pub fn start(_: &SystemAudioChoice, _: Deliver, _: Arc<AtomicBool>) -> Result<Self, String> {
        Err("System audio recording is not available on this platform yet".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_require_a_source() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_settings(dir.path()), SourceSettings::default());
        let mut settings = SourceSettings::default();
        settings.microphone.device_id = Some("mic-2".into());
        save_settings(dir.path(), &settings).unwrap();
        assert_eq!(load_settings(dir.path()), settings);
        let none = SourceSettings {
            microphone: MicrophoneChoice {
                enabled: false,
                device_id: None,
            },
            system_audio: SystemAudioChoice {
                enabled: false,
                device_id: None,
            },
        };
        assert!(none.validate().is_err());
        std::fs::write(dir.path().join(SETTINGS_FILE), b"{").unwrap();
        assert_eq!(
            load_settings(dir.path()),
            SourceSettings::default(),
            "corrupt falls back"
        );
    }

    /// Opens this machine's real devices: run by hand with `--ignored`.
    #[test]
    #[ignore = "uses real audio devices"]
    fn real_devices_open_and_deliver() {
        use std::sync::atomic::AtomicU64;
        let outputs = list_outputs().unwrap();
        println!("outputs: {outputs:?}");
        println!("microphones: {:?}", list_microphones().unwrap());
        let counted = |frames: &Arc<AtomicU64>, rate: &Arc<AtomicU64>| -> Deliver {
            let (frames, rate) = (Arc::clone(frames), Arc::clone(rate));
            Arc::new(move |samples, sample_rate, channels| {
                frames.fetch_add(
                    (samples.len() / usize::from(channels)) as u64,
                    Ordering::Relaxed,
                );
                rate.store(u64::from(sample_rate), Ordering::Relaxed);
            })
        };
        let failed = Arc::new(AtomicBool::new(false));
        if system_audio_supported() {
            let output = outputs
                .iter()
                .find(|o| o.is_default)
                .expect("a default output device");
            let choice = SystemAudioChoice {
                enabled: true,
                device_id: Some(output.id.clone()),
            };
            assert_eq!(output_name(Some(&output.id)), output.name, "ids round-trip");
            let (frames, rate) = (Arc::default(), Arc::default());
            let capture =
                SystemCapture::start(&choice, counted(&frames, &rate), Arc::clone(&failed))
                    .unwrap();
            // Play a quiet tone so loopback has something to deliver.
            let tone = {
                use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
                let device = rodio::cpal::default_host().default_output_device().unwrap();
                let config: rodio::cpal::StreamConfig =
                    device.default_output_config().unwrap().into();
                let mut phase = 0f32;
                let step = 440.0 * std::f32::consts::TAU / config.sample_rate as f32;
                let stream = device
                    .build_output_stream::<f32, _, _>(
                        config,
                        move |data: &mut [f32], _| {
                            for s in data.iter_mut() {
                                *s = phase.sin() * 0.02;
                                phase = (phase + step) % std::f32::consts::TAU;
                            }
                        },
                        |_| {},
                        None,
                    )
                    .unwrap();
                stream.play().unwrap();
                stream
            };
            std::thread::sleep(std::time::Duration::from_secs(1));
            assert!(
                !failed.load(Ordering::Relaxed),
                "loopback failed while running"
            );
            drop(tone);
            drop(capture);
            println!(
                "loopback: {} frames at {} Hz",
                frames.load(Ordering::Relaxed),
                rate.load(Ordering::Relaxed)
            );
            assert!(
                frames.load(Ordering::Relaxed) > 0,
                "loopback delivered audio"
            );
        }
        let (frames, rate) = (Arc::default(), Arc::default());
        let mic = MicCapture::start(
            &MicrophoneChoice {
                enabled: true,
                device_id: None,
            },
            counted(&frames, &rate),
            Arc::clone(&failed),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        drop(mic);
        let mic_frames = frames.load(Ordering::Relaxed);
        println!("microphone: {mic_frames} frames");
        assert_eq!(rate.load(Ordering::Relaxed), u64::from(CAPTURE_RATE));
        assert!(mic_frames > u64::from(CAPTURE_RATE) / 2);
        assert!(!failed.load(Ordering::Relaxed));
    }
}
