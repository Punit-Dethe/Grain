//! [GRAIN] Agent talk-back: reads Agent chat replies aloud, on-device.
//!
//! Model: Gradium Phonon 31m English (Apache-2.0), run by Gradium's own `ptts`
//! crate on the pure-Rust `xn` tensor runtime — no ONNX Runtime, no ggml, no
//! phonemizer. Voice: Zoey, the first US-English female voice in Gradium's
//! catalog and the one its 31m demos use.
//!
//! Nothing is resident while idle. Each reply (or progress line, see
//! [`speak_progress`]) loads the model on one worker thread, streams PCM into
//! the default output device, then drops the model and the device. A newer reply, a new dictation, a summon or closing the
//! notch bumps [`GENERATION`], which stops speech within one audio chunk.

use log::{info, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// The default voice (installed with the model): the file stem under the
/// model's `voices/` folder.
const VOICE: &str = "Zoey";
/// Hard cap on what is read aloud; longer replies stop at a sentence end.
const MAX_SPOKEN_CHARS: usize = 3000;

struct ModelFile {
    /// Path inside the model folder.
    name: &'static str,
    url: &'static str,
    bytes: u64,
    sha256: &'static str,
}

/// Pinned to exact Hugging Face commits and verified by size + SHA-256.
const FILES: [ModelFile; 4] = [
    ModelFile {
        name: "config.json",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/config.json",
        bytes: 4_634,
        sha256: "b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85",
    },
    ModelFile {
        name: "tokenizer.json",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/tokenizer.json",
        bytes: 277_967,
        sha256: "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936",
    },
    ModelFile {
        name: "voices/Zoey.safetensors",
        url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/NbpkqMVS3CJeq2j8.safetensors",
        bytes: 256_136,
        sha256: "97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23",
    },
    // The big one last, so the small files fail fast on a bad connection.
    ModelFile {
        name: "model.q8.gguf",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/model.q8.gguf",
        bytes: 64_512_352,
        sha256: "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae",
    },
];

/// A talk-back voice the user can pick. Each is one small embedding file,
/// downloaded (pinned and verified) the first time it is picked.
pub(crate) struct VoicePreset {
    pub(crate) name: &'static str,
    /// "us" or "gb".
    pub(crate) accent: &'static str,
    /// "female" or "male".
    pub(crate) gender: &'static str,
    file: ModelFile,
}

const VOICE_BYTES: u64 = 256_136;

/// Seven female and seven male English presets (American and British), from
/// gradium/phonon-voices at a pinned commit. The first is the default.
pub(crate) const VOICES: [VoicePreset; 14] = [
    VoicePreset {
        name: "Zoey",
        accent: "us",
        gender: "female",
        file: ModelFile {
            name: "voices/Zoey.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/NbpkqMVS3CJeq2j8.safetensors",
            bytes: VOICE_BYTES,
            sha256: "97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23",
        },
    },
    VoicePreset {
        name: "Harper",
        accent: "us",
        gender: "female",
        file: ModelFile {
            name: "voices/Harper.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/4SZHfMpw-p46Ywgs.safetensors",
            bytes: VOICE_BYTES,
            sha256: "6b3da6e2d796a20c51af892c482bd6509fb88267a221aff83c870843dee2a123",
        },
    },
    VoicePreset {
        name: "Hadley",
        accent: "us",
        gender: "female",
        file: ModelFile {
            name: "voices/Hadley.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/bXwrkHhURL1VFJdG.safetensors",
            bytes: VOICE_BYTES,
            sha256: "a879938aa9a2df0a8220bdadef88137929f71e887bd91e7db205bd5fa11661d2",
        },
    },
    VoicePreset {
        name: "Delia",
        accent: "us",
        gender: "female",
        file: ModelFile {
            name: "voices/Delia.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/nGzMHMPAWGaJFHOo.safetensors",
            bytes: VOICE_BYTES,
            sha256: "a0c3dcb91b2247eca56536be9ae8e8becc1c807b1136c5fe101552d6a155b24a",
        },
    },
    VoicePreset {
        name: "Maeve",
        accent: "gb",
        gender: "female",
        file: ModelFile {
            name: "voices/Maeve.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/6PWnV0Nq4wu7RVBT.safetensors",
            bytes: VOICE_BYTES,
            sha256: "56494ee67d533ad227ff0dfd0b1c13ff5aa91f1ad6e596e66c2986ba3005e844",
        },
    },
    VoicePreset {
        name: "Pippa",
        accent: "gb",
        gender: "female",
        file: ModelFile {
            name: "voices/Pippa.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/uem82D50GRv2Dwma.safetensors",
            bytes: VOICE_BYTES,
            sha256: "d41a8f01d6e33450f1ce062d332914da10beee99e585dab531750706f187c48a",
        },
    },
    VoicePreset {
        name: "Bryony",
        accent: "gb",
        gender: "female",
        file: ModelFile {
            name: "voices/Bryony.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/gfP44RlHwr2kEwk7.safetensors",
            bytes: VOICE_BYTES,
            sha256: "bcc207a73b7885e45b49ab633d3377a10158292e3a7cef9cd6764ad107f1cb9b",
        },
    },
    VoicePreset {
        name: "Micah",
        accent: "us",
        gender: "male",
        file: ModelFile {
            name: "voices/Micah.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/40QrMVgAxFiUnP9f.safetensors",
            bytes: VOICE_BYTES,
            sha256: "0de1098d1051686a138d068e83ff09deb5053328bc74ec9541a1fdb90bfc24da",
        },
    },
    VoicePreset {
        name: "Miles",
        accent: "us",
        gender: "male",
        file: ModelFile {
            name: "voices/Miles.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/d9XCyhjo4JhkHF1d.safetensors",
            bytes: VOICE_BYTES,
            sha256: "eab20e4f159b48cfbc01b1885804e24f681f80e7a64f909182763a4e3361bee8",
        },
    },
    VoicePreset {
        name: "Preston",
        accent: "us",
        gender: "male",
        file: ModelFile {
            name: "voices/Preston.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/qIxlZ7Ei8KvWlyQH.safetensors",
            bytes: VOICE_BYTES,
            sha256: "6e04b84d3ae8751b22bca0a4f63936f9cb648f4f2182b2d0788071ea1c0b68a1",
        },
    },
    VoicePreset {
        name: "Trevor",
        accent: "us",
        gender: "male",
        file: ModelFile {
            name: "voices/Trevor.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/0zLApsZfVLLNaEGl.safetensors",
            bytes: VOICE_BYTES,
            sha256: "dc1341c8937af5534e6bbcc5c0fc5e7c10312229b59061228074f93683fe2261",
        },
    },
    VoicePreset {
        name: "Hugh",
        accent: "gb",
        gender: "male",
        file: ModelFile {
            name: "voices/Hugh.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/YyKcxPG7iIgS3jsK.safetensors",
            bytes: VOICE_BYTES,
            sha256: "3169d74b47a54d1eb15e4c243f56393abe9fe90c7e1c308ecf96ee67618b152a",
        },
    },
    VoicePreset {
        name: "Edwin",
        accent: "gb",
        gender: "male",
        file: ModelFile {
            name: "voices/Edwin.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/zUCxzrOrugTSis03.safetensors",
            bytes: VOICE_BYTES,
            sha256: "ff1d0735b8b45710462b1d1f846da040f970c6eb62c7c2f36dbfb8e5bde74f1e",
        },
    },
    VoicePreset {
        name: "Teddy",
        accent: "gb",
        gender: "male",
        file: ModelFile {
            name: "voices/Teddy.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/Zn7g6NxLqXlJb7Qi.safetensors",
            bytes: VOICE_BYTES,
            sha256: "baea0ea9506f3237a5ebfc61afb7c72dbd189c5603b199ef42fbb00dbda379e0",
        },
    },
];

pub(crate) fn preset(name: &str) -> Option<&'static VoicePreset> {
    VOICES.iter().find(|voice| voice.name == name)
}

fn voice_ready(dir: &Path, voice: &VoicePreset) -> bool {
    std::fs::metadata(dir.join(voice.file.name)).is_ok_and(|meta| meta.len() == voice.file.bytes)
}

/// The voice to speak with: the chosen preset once its file is here, else the
/// default (starting the chosen one's download).
fn chosen_voice(app: &AppHandle, dir: &Path) -> &'static str {
    let chosen = crate::settings::get_settings(app).agent_voice;
    match preset(&chosen) {
        Some(voice) if voice_ready(dir, voice) => voice.name,
        Some(voice) => {
            fetch_voice(app, voice, false);
            VOICE
        }
        None => VOICE,
    }
}

static FETCHING_VOICE: AtomicBool = AtomicBool::new(false);

/// Download a preset's file in the background when it is missing; with
/// `preview`, then say a line in it (only while talk-back is on).
pub(crate) fn fetch_voice(app: &AppHandle, voice: &'static VoicePreset, preview: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(dir) = model_dir(&app) else {
            return;
        };
        if !installed(&dir) {
            // The model download fetches the default voice; a preset waits
            // for it (the reply speaks in the default until then).
            ensure_installed(&app);
            return;
        }
        if !voice_ready(&dir, voice) {
            if FETCHING_VOICE.swap(true, Ordering::AcqRel) {
                return;
            }
            let client = app.state::<reqwest::Client>().inner().clone();
            let path = dir.join(voice.file.name);
            let result = download_verified(&client, &voice.file, &path).await;
            FETCHING_VOICE.store(false, Ordering::Release);
            if let Err(e) = result {
                warn!(
                    "[GRAIN] talk-back: could not download voice {}: {e}",
                    voice.name
                );
                return;
            }
            info!("[GRAIN] talk-back: voice {} downloaded", voice.name);
        }
        if preview && crate::settings::get_settings(&app).agent_voice == voice.name {
            speak(
                &app,
                &format!(
                    "Hi, I'm {}. This is how I'll read your replies.",
                    voice.name
                ),
                |_| {},
            );
        }
    });
}

/// Bumped by every new reply and every stop; a worker speaks only while its
/// generation is current.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// One voice at a time: a new reply waits for the previous worker to exit, so
/// two models are never loaded together.
static SPEAKING: Mutex<()> = Mutex::new(());
/// Id of the newest speech whose worker is still alive (0 = none).
static LIVE: AtomicU64 = AtomicU64::new(0);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("models")
        .join("grain-tts")
        .join("phonon-31m-en"))
}

fn installed(dir: &Path) -> bool {
    FILES.iter().all(|file| {
        std::fs::metadata(dir.join(file.name)).is_ok_and(|meta| meta.len() == file.bytes)
    })
}

/// Size `xn`'s CPU pool once, at startup. Its default is every core, which on a
/// 20-thread desktop measured 2× SLOWER than 4 threads (the flow LM and Mimi
/// decoder already run on two threads of their own). Called before any other
/// thread exists, because `xn` applies it with `std::env::set_var`.
pub(crate) fn init_threads() {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    xn::set_num_threads((cores / 2).clamp(1, 4));
}

/// Stop any speech now (a progress line too). Cheap and safe to call from any
/// thread.
pub(crate) fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    STOPS.fetch_add(1, Ordering::SeqCst);
}

/// The id of the speech playing right now, if any — for a notch opened while
/// the voice is still talking (it missed the start event).
pub(crate) fn current_speech() -> Option<u64> {
    let live = LIVE.load(Ordering::SeqCst);
    (live != 0 && live == GENERATION.load(Ordering::SeqCst)).then_some(live)
}

/// Read an Agent reply aloud when talk-back is on. Returns immediately; the
/// work runs on its own short-lived thread. Before the voice is installed it
/// starts the download and stays silent for this reply.
///
/// Returns the speech's id when it started. Only then is `on_end` called with
/// that id — once, on the worker thread, however the speech ends (finished,
/// stopped or failed).
pub(crate) fn speak(
    app: &AppHandle,
    reply: &str,
    on_end: impl FnOnce(u64) + Send + 'static,
) -> Option<u64> {
    if !crate::settings::get_settings(app).agent_talk_back {
        return None;
    }
    let text = speakable(reply);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if text.is_empty() {
        return None;
    }
    let dir = match model_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            warn!("[GRAIN] talk-back: no model folder: {e}");
            return None;
        }
    };
    if !installed(&dir) {
        info!("[GRAIN] talk-back: voice not installed yet; downloading");
        ensure_installed(app);
        return None;
    }
    let voice = chosen_voice(app, &dir);
    info!(
        "[GRAIN] talk-back: speaking {} chars as {voice}",
        text.chars().count()
    );
    LIVE.store(generation, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("grain-tts".into())
        .spawn(move || {
            {
                let _one = SPEAKING.lock().unwrap_or_else(|e| e.into_inner());
                if GENERATION.load(Ordering::SeqCst) != generation {
                    info!("[GRAIN] talk-back: replaced before it started");
                } else if let Err(e) = say(&dir, voice, &text, move |_| {
                    GENERATION.load(Ordering::SeqCst) == generation
                }) {
                    warn!("[GRAIN] talk-back failed: {e}");
                }
            }
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            on_end(generation);
        });
    match spawned {
        Ok(_) => Some(generation),
        Err(e) => {
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            warn!("[GRAIN] talk-back: could not start: {e}");
            None
        }
    }
}

/// Load the voice model (CPU, Q8) with only `voice` registered (the other
/// downloaded presets stay on disk).
fn load_voice(dir: &Path, voice: &str) -> Result<ptts::synth::Synth, String> {
    use ptts::checkpoint::{Checkpoint, ResolveOptions};
    use ptts::preprocess::Lang;
    use ptts::synth::{DeviceKind, Quant};

    let mut checkpoint = Checkpoint::resolve(
        dir,
        ResolveOptions {
            quant: Quant::Q80,
            weights: None,
        },
    )
    .map_err(|e| format!("could not read the voice model: {e}"))?;
    checkpoint.voices.retain(|(name, _)| name == voice);
    let mut tts = checkpoint
        .builder(Lang::En)
        .device(DeviceKind::Cpu)
        .build()
        .map_err(|e| format!("could not load the voice model: {e}"))?;
    checkpoint.register_voices(&mut tts);
    Ok(tts)
}

/// Speak `text` while `keep(audible)` holds; `audible` says whether any of it
/// has been heard yet.
fn say(dir: &Path, voice: &str, text: &str, keep: impl Fn(bool) -> bool) -> Result<(), String> {
    use ptts::synth::SpeechOptions;
    use rodio::buffer::SamplesBuffer;
    use rodio::{ChannelCount, DeviceSinkBuilder, Player, SampleRate};

    let started = Instant::now();
    let tts = load_voice(dir, voice)?;
    let loaded = started.elapsed();
    if !keep(false) {
        info!("[GRAIN] talk-back: stopped while loading");
        return Ok(());
    }

    let rate = SampleRate::new(tts.sample_rate()).ok_or("the voice model has no sample rate")?;
    let mono = ChannelCount::new(1).expect("1 is non-zero");
    let mut device = DeviceSinkBuilder::from_default_device()
        .and_then(DeviceSinkBuilder::open_stream)
        .map_err(|e| format!("no audio output: {e}"))?;
    device.log_on_drop(false);
    let player = Player::connect_new(device.mixer());

    let options = SpeechOptions::default().voice(voice);
    let stream = tts
        .stream_with(text, &options)
        .map_err(|e| format!("could not start speech: {e}"))?;
    let mut first_audio = None;
    let mut seconds = 0.0f32;
    // Dropping `stream` on an early return stops and joins its workers.
    for chunk in stream {
        if !keep(first_audio.is_some()) {
            player.stop();
            info!(
                "[GRAIN] talk-back: stopped after {} ms ({:.1} s spoken)",
                started.elapsed().as_millis(),
                seconds
            );
            return Ok(());
        }
        let pcm = chunk.map_err(|e| format!("speech failed: {e}"))?;
        first_audio.get_or_insert_with(|| started.elapsed());
        seconds += pcm.len() as f32 / rate.get() as f32;
        player.append(SamplesBuffer::new(mono, rate, pcm));
    }
    info!(
        "[GRAIN] talk-back: load {} ms, first audio {} ms, {:.1} s of speech synthesized in {} ms",
        loaded.as_millis(),
        first_audio.unwrap_or_default().as_millis(),
        seconds,
        started.elapsed().as_millis()
    );
    while !player.empty() {
        if !keep(true) {
            player.stop();
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}

/// Download the pinned voice files in the background when they are missing.
pub(crate) fn ensure_installed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = install(&app).await {
            warn!("[GRAIN] talk-back: voice download failed: {e}");
        }
    });
}

async fn install(app: &AppHandle) -> Result<(), String> {
    let dir = model_dir(app)?;
    if installed(&dir) || DOWNLOADING.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            DOWNLOADING.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    let client = app.state::<reqwest::Client>().inner().clone();
    for file in &FILES {
        let path = dir.join(file.name);
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() == file.bytes) {
            continue;
        }
        download_verified(&client, file, &path).await?;
    }
    info!("[GRAIN] talk-back: voice installed at {}", dir.display());
    Ok(())
}

// ── Progress lines ──────────────────────────────────────────────────────────
//
// With progress speech on, the AI's first tool step carries one short line
// confirming what it is doing ("Checking your calendar."), and that line is
// read aloud. Later steps say nothing, and a reply that needs no tool has no
// line. The Agent loop picks the step; this only speaks.

/// A line longer than this is not a short confirmation; it is not read.
const MAX_PROGRESS_CHARS: usize = 80;

/// Bumped by [`stop`] only. A progress line keeps talking through a newer
/// reply (the reply waits for it) but not through a stop.
static STOPS: AtomicU64 = AtomicU64::new(0);

/// The line to read from a tool round's text, if it is one: its first line,
/// when short. (The caller leaves out a same-round final reply.)
pub(crate) fn progress_line(text: &str) -> Option<String> {
    let line = text.trim().lines().next()?.trim();
    let count = line.chars().count();
    (count > 0 && count <= MAX_PROGRESS_CHARS).then(|| line.to_string())
}

/// Read a progress line aloud, when talk-back and progress speech are on and
/// the voice is free. Like [`speak`], returns the speech's id when it started,
/// and only then calls `on_end` once on the worker thread.
///
/// A reply that comes back while the line is still loading drops it; once the
/// line is audible it finishes and the reply follows.
pub(crate) fn speak_progress(
    app: &AppHandle,
    line: &str,
    on_end: impl FnOnce(u64) + Send + 'static,
) -> Option<u64> {
    let settings = crate::settings::get_settings(app);
    if !settings.agent_talk_back || !settings.agent_spoken_progress || current_speech().is_some() {
        return None;
    }
    let text = speakable(line);
    let dir = model_dir(app).ok()?;
    if text.is_empty() || !installed(&dir) {
        return None;
    }
    let voice = chosen_voice(app, &dir);
    let stops = STOPS.load(Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    info!(
        "[GRAIN] talk-back: progress line, {} chars",
        text.chars().count()
    );
    LIVE.store(generation, Ordering::SeqCst);
    let keep = move |audible: bool| {
        STOPS.load(Ordering::SeqCst) == stops
            && (audible || GENERATION.load(Ordering::SeqCst) == generation)
    };
    let spawned = std::thread::Builder::new()
        .name("grain-tts-progress".into())
        .spawn(move || {
            {
                let _one = SPEAKING.lock().unwrap_or_else(|e| e.into_inner());
                if !keep(false) {
                    info!("[GRAIN] talk-back: progress line dropped");
                } else if let Err(e) = say(&dir, voice, &text, keep) {
                    warn!("[GRAIN] talk-back: progress line failed: {e}");
                }
            }
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            on_end(generation);
        });
    match spawned {
        Ok(_) => Some(generation),
        Err(e) => {
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            warn!("[GRAIN] talk-back: could not start a progress line: {e}");
            None
        }
    }
}

async fn download_verified(
    client: &reqwest::Client,
    file: &ModelFile,
    path: &Path,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let folder = path.parent().expect("model file has a folder");
    std::fs::create_dir_all(folder).map_err(|e| format!("could not create {folder:?}: {e}"))?;
    let partial = path.with_extension("partial");
    let response = client
        .get(file.url)
        .timeout(Duration::from_secs(30 * 60))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("{}: {e}", file.name))?;
    let mut out = std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", file.name))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    let result: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("{}: {e}", file.name))?;
            downloaded += chunk.len() as u64;
            if downloaded > file.bytes {
                return Err(format!("{} is larger than expected", file.name));
            }
            hasher.update(&chunk);
            out.write_all(&chunk)
                .map_err(|e| format!("{}: {e}", file.name))?;
        }
        out.sync_all().map_err(|e| format!("{}: {e}", file.name))?;
        if downloaded != file.bytes || format!("{:x}", hasher.finalize()) != file.sha256 {
            return Err(format!("{} failed verification", file.name));
        }
        Ok(())
    }
    .await;
    drop(out);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    std::fs::rename(&partial, path).map_err(|e| format!("{}: {e}", file.name))
}

/// Turn a markdown chat reply into text worth hearing: code blocks, link
/// targets, URLs and markup are dropped; list items and lines get a pause.
fn speakable(markdown: &str) -> String {
    let mut sentences: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut skipped_code = false;
    for raw in markdown.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            skipped_code = true;
            continue;
        }
        if in_code || trimmed.is_empty() || is_rule(trimmed) {
            continue;
        }
        let line = strip_line_marker(trimmed);
        let line = plain_inline(line);
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        let ends_sentence = line.ends_with(['.', '!', '?', ':', ';']);
        sentences.push(if ends_sentence {
            line
        } else {
            format!("{line}.")
        });
    }
    if sentences.is_empty() && skipped_code {
        return "The code is in the reply.".into();
    }
    truncate_at_sentence(sentences.join(" "), MAX_SPOKEN_CHARS)
}

fn is_rule(line: &str) -> bool {
    line.len() >= 3
        && line
            .chars()
            .all(|c| matches!(c, '-' | '*' | '_' | '=' | ' '))
        || line.starts_with('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// Drop heading, quote and bullet markers. Numbered lists keep their number.
fn strip_line_marker(line: &str) -> &str {
    let line = line
        .trim_start_matches('#')
        .trim_start_matches('>')
        .trim_start();
    for bullet in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest;
        }
    }
    line
}

/// Links keep their label, bare URLs become "link", markup characters go.
fn plain_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.split(' ')
        .map(|word| {
            if word.starts_with("http://") || word.starts_with("https://") {
                "link".to_string()
            } else {
                let chars: Vec<char> = word
                    .chars()
                    .filter(|c| !matches!(c, '*' | '`' | '|' | '~'))
                    .collect();
                // `_emphasis_` goes; `snake_case` keeps its underscore.
                (0..chars.len())
                    .filter(|&i| {
                        chars[i] != '_'
                            || i > 0
                                && i + 1 < chars.len()
                                && chars[i - 1].is_alphanumeric()
                                && chars[i + 1].is_alphanumeric()
                    })
                    .map(|i| chars[i])
                    .collect()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_at_sentence(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let cut: String = text.chars().take(max).collect();
    match cut.rfind(['.', '!', '?']) {
        Some(end) => cut[..=end].to_string(),
        None => cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_plain_sentences() {
        let reply = "## Summary\n\n- **First** point\n- See [the docs](https://x.y/z)\n\nVisit https://example.com now";
        assert_eq!(
            speakable(reply),
            "Summary. First point. See the docs. Visit link now."
        );
    }

    #[test]
    fn code_blocks_are_not_read() {
        assert_eq!(
            speakable("Run this:\n```sh\nrm -rf x\n```\nDone!"),
            "Run this: Done!"
        );
        assert_eq!(
            speakable("```rust\nfn main() {}\n```"),
            "The code is in the reply."
        );
    }

    #[test]
    fn inline_markup_and_rules_go() {
        assert_eq!(
            speakable("Use `cargo` and _care_.\n---\n> quoted"),
            "Use cargo and care. quoted."
        );
        assert_eq!(speakable("| a | b |\n|---|---|\n| 1 | 2 |"), "a b. 1 2.");
    }

    #[test]
    fn long_replies_stop_at_a_sentence() {
        let text = "One two three. Four five six.".to_string();
        assert_eq!(truncate_at_sentence(text, 20), "One two three.");
    }

    #[test]
    fn only_a_short_first_line_is_a_progress_line() {
        assert_eq!(
            progress_line("  Checking your calendar.\nThen more.").as_deref(),
            Some("Checking your calendar.")
        );
        assert_eq!(progress_line("   \n"), None);
        assert_eq!(progress_line(&"word ".repeat(40)), None);
    }

    /// Real synthesis benchmark. Needs the model files in GRAIN_TTS_MODEL_DIR:
    /// `GRAIN_TTS_MODEL_DIR=… cargo test --lib grain_tts::tests::bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench() {
        use ptts::checkpoint::{Checkpoint, ResolveOptions};
        use ptts::preprocess::Lang;
        use ptts::synth::{DeviceKind, Quant, SpeechOptions};
        let dir = PathBuf::from(std::env::var("GRAIN_TTS_MODEL_DIR").expect("model dir"));
        let started = Instant::now();
        let checkpoint = Checkpoint::resolve(
            &dir,
            ResolveOptions {
                quant: Quant::Q80,
                weights: None,
            },
        )
        .unwrap();
        let mut tts = checkpoint
            .builder(Lang::En)
            .device(DeviceKind::Cpu)
            .build()
            .unwrap();
        checkpoint.register_voices(&mut tts);
        let loaded = started.elapsed();
        assert!(
            tts.voices().iter().any(|v| v == VOICE),
            "{:?}",
            tts.voices()
        );
        let text = speakable(
            "Sure! Your meeting with **Priya** moved to 3:30 PM on Thursday. \
             I've also added the agenda link to the invite, and sent her a short note \
             so she knows the room changed to the second floor.",
        );
        let synth_start = Instant::now();
        let mut first = None;
        let mut pcm = Vec::new();
        for chunk in tts
            .stream_with(&text, &SpeechOptions::default().voice(VOICE))
            .unwrap()
        {
            first.get_or_insert_with(|| synth_start.elapsed());
            pcm.extend(chunk.unwrap());
        }
        let took = synth_start.elapsed();
        let seconds = pcm.len() as f32 / tts.sample_rate() as f32;
        println!(
            "load {} ms | first audio {} ms | {:.1} s audio in {} ms | {:.1}x real time",
            loaded.as_millis(),
            first.unwrap().as_millis(),
            seconds,
            took.as_millis(),
            seconds / took.as_secs_f32()
        );
        ptts::wav::write_wav_file(dir.join("zoey-sample.wav"), &pcm, tts.sample_rate()).unwrap();
    }

    #[test]
    fn files_are_pinned() {
        assert!(FILES
            .iter()
            .all(|f| f.url.contains("/resolve/") && f.sha256.len() == 64));
        assert_eq!(FILES.last().unwrap().name, "model.q8.gguf");
    }

    #[test]
    fn voice_presets_are_pinned_and_balanced() {
        // The default comes with the model download, byte for byte.
        assert_eq!(VOICES[0].name, VOICE);
        let bundled = FILES.iter().find(|f| f.name == VOICES[0].file.name);
        assert_eq!(bundled.map(|f| f.sha256), Some(VOICES[0].file.sha256));
        for voice in &VOICES {
            assert_eq!(
                voice.file.name,
                format!("voices/{}.safetensors", voice.name)
            );
            assert!(voice.file.url.contains("/resolve/") && voice.file.sha256.len() == 64);
            assert!(matches!(voice.accent, "us" | "gb"));
        }
        let female = VOICES.iter().filter(|v| v.gender == "female").count();
        assert_eq!((female, VOICES.len() - female), (7, 7));
        assert!(preset("Harper").is_some() && preset("Nobody").is_none());
    }
}
