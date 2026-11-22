//! [GRAIN] Agent talk-back: reads Agent chat replies aloud, on-device.
//!
//! The model lives behind [`model`] (the only code that knows which TTS model
//! Grain uses); [`text`] turns markdown replies into speakable sentences.
//!
//! Nothing is resident while idle. Every speech is one job on its own
//! short-lived thread: it loads the model, streams PCM into the default output
//! device, then drops the model and the device. One job speaks at a time. The
//! only reuse: when a job ends while the next one is already waiting (a
//! first-step line followed by its reply), the loaded model is handed straight
//! to it instead of being dropped and loaded again.
//!
//! A newer reply, a new dictation, a summon or closing the notch bumps
//! [`GENERATION`], which stops speech within one audio chunk.

mod model;
mod text;

pub(crate) use model::{init_threads, VoicePreset, VOICES};
pub(crate) use text::step_line;

use log::{info, warn};
use model::{ModelFile, Voice, FILES, VOICE};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use text::{speakable, MAX_SPOKEN_CHARS};

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

// ── Speech jobs ─────────────────────────────────────────────────────────────

/// Bumped by every new speech and every stop; a job speaks only while its
/// generation is current (see [`Policy`]).
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Bumped by [`stop`] only. A first-step line keeps talking through a newer
/// reply (the reply waits for it) but not through a stop.
static STOPS: AtomicU64 = AtomicU64::new(0);
/// Id of the newest speech whose job is still alive (0 = none).
static LIVE: AtomicU64 = AtomicU64::new(0);
/// One voice at a time. A job holds this for its whole life, so two models are
/// never loaded together. It holds a model only between a job that ended and
/// a job that was already [`WAITING`] for it; otherwise it is empty.
static SPEAKING: Mutex<Option<Voice>> = Mutex::new(None);
/// Jobs spawned but not yet holding [`SPEAKING`].
static WAITING: AtomicUsize = AtomicUsize::new(0);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// What a job reads.
enum Source {
    /// Already speakable text.
    Text(String),
    /// A streamed reply, read sentence by sentence as it arrives.
    Feed(Arc<SpeechFeed>),
}

/// When a job keeps talking.
#[derive(Clone, Copy)]
enum Policy {
    /// Until any newer speech or a stop.
    Reply,
    /// A first-step line: until a stop, or a newer speech arriving before any
    /// of it was heard. Once audible it finishes and the newer speech waits.
    Step { stops: u64 },
}

impl Policy {
    fn keep(self, generation: u64, audible: bool) -> bool {
        let current = GENERATION.load(Ordering::SeqCst) == generation;
        match self {
            Policy::Reply => current,
            Policy::Step { stops } => STOPS.load(Ordering::SeqCst) == stops && (audible || current),
        }
    }
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("models");
    dir.extend(model::FOLDER);
    Ok(dir)
}

fn installed(dir: &Path) -> bool {
    FILES.iter().all(|file| {
        std::fs::metadata(dir.join(file.name)).is_ok_and(|meta| meta.len() == file.bytes)
    })
}

/// Stop any speech now (a first-step line too). Cheap and safe to call from
/// any thread.
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
/// that id — once, on the job's thread, however the speech ends (finished,
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
    // Any reply, even an unspeakable one, silences the previous speech.
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
    start(
        dir,
        voice,
        Source::Text(text),
        Policy::Reply,
        generation,
        on_end,
    )
}

/// Like [`speak`], but the text keeps arriving through `feed` until it is
/// finished or cancelled.
pub(crate) fn speak_feed(
    app: &AppHandle,
    feed: Arc<SpeechFeed>,
    on_end: impl FnOnce(u64) + Send + 'static,
) -> Option<u64> {
    if !crate::settings::get_settings(app).agent_talk_back {
        return None;
    }
    let dir = model_dir(app).ok()?;
    if !installed(&dir) {
        ensure_installed(app);
        return None;
    }
    let voice = chosen_voice(app, &dir);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    info!("[GRAIN] talk-back: speaking a streamed reply as {voice}");
    start(
        dir,
        voice,
        Source::Feed(feed),
        Policy::Reply,
        generation,
        on_end,
    )
}

/// Read a first-step line aloud, when talk-back is on and the voice is free.
/// Like [`speak`], returns the speech's id when it started, and only then
/// calls `on_end` once on the job's thread.
///
/// The AI's first tool step carries a few words confirming what it is doing
/// ("Checking your calendar."). A reply that comes back while the line is
/// still loading drops it; once the line is audible it finishes and the reply
/// follows, reusing the loaded model.
pub(crate) fn speak_step(
    app: &AppHandle,
    line: &str,
    on_end: impl FnOnce(u64) + Send + 'static,
) -> Option<u64> {
    if !crate::settings::get_settings(app).agent_talk_back || current_speech().is_some() {
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
        "[GRAIN] talk-back: first-step line, {} chars",
        text.chars().count()
    );
    start(
        dir,
        voice,
        Source::Text(text),
        Policy::Step { stops },
        generation,
        on_end,
    )
}

/// Spawn one speech job. Returns its id when the thread started; only then is
/// `on_end` called, once, however the job ends.
fn start(
    dir: PathBuf,
    voice: &'static str,
    source: Source,
    policy: Policy,
    generation: u64,
    on_end: impl FnOnce(u64) + Send + 'static,
) -> Option<u64> {
    LIVE.store(generation, Ordering::SeqCst);
    WAITING.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("grain-tts".into())
        .spawn(move || {
            run(&dir, voice, &source, policy, generation);
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            on_end(generation);
        });
    match spawned {
        Ok(_) => Some(generation),
        Err(e) => {
            WAITING.fetch_sub(1, Ordering::SeqCst);
            let _ = LIVE.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
            warn!("[GRAIN] talk-back: could not start: {e}");
            None
        }
    }
}

/// The body of one job: wait for the voice, speak, then hand the model on to a
/// job already waiting, or drop it.
fn run(dir: &Path, voice: &str, source: &Source, policy: Policy, generation: u64) {
    let mut slot = SPEAKING.lock().unwrap_or_else(|e| e.into_inner());
    WAITING.fetch_sub(1, Ordering::SeqCst);
    let mut model = slot.take();
    let keep = |audible: bool| {
        policy.keep(generation, audible)
            && !matches!(source, Source::Feed(feed) if feed.cancelled())
    };
    if !keep(false) {
        info!("[GRAIN] talk-back: replaced before it started");
    } else {
        match say(dir, voice, model.take(), source, &keep) {
            Ok(loaded) => model = Some(loaded),
            Err(e) => warn!("[GRAIN] talk-back failed: {e}"),
        }
    }
    if WAITING.load(Ordering::SeqCst) > 0 {
        *slot = model;
    } else if model.take().is_some() {
        crate::memory::trim_freed_memory();
    }
}

/// Speak `source` while `keep(audible)` holds; `audible` says whether any of it
/// has been heard yet. `parked` is a model handed on by the previous job.
/// Returns the loaded model so the caller decides whether to keep it.
fn say(
    dir: &Path,
    voice: &str,
    parked: Option<Voice>,
    source: &Source,
    keep: &dyn Fn(bool) -> bool,
) -> Result<Voice, String> {
    use rodio::buffer::SamplesBuffer;
    use rodio::{ChannelCount, DeviceSinkBuilder, Player, SampleRate};

    let started = Instant::now();
    let tts = match parked {
        Some(tts) if tts.name() == voice => tts,
        _ => Voice::load(dir, voice)?,
    };
    let loaded = started.elapsed();
    if !keep(false) {
        info!("[GRAIN] talk-back: stopped while loading");
        return Ok(tts);
    }

    let rate = SampleRate::new(tts.sample_rate()).ok_or("the voice model has no sample rate")?;
    let mono = ChannelCount::new(1).expect("1 is non-zero");
    let mut device = DeviceSinkBuilder::from_default_device()
        .and_then(DeviceSinkBuilder::open_stream)
        .map_err(|e| format!("no audio output: {e}"))?;
    device.log_on_drop(false);
    let player = Player::connect_new(device.mixer());

    let first_audio = std::cell::Cell::new(None::<Duration>);
    let seconds = std::cell::Cell::new(0.0f32);
    let audible = || first_audio.get().is_some();
    // Synthesize one piece into the player; Ok(false) = stop now.
    let play = |text: &str| -> Result<bool, String> {
        // Dropping the stream on an early return stops and joins its workers.
        for chunk in tts.stream(text)? {
            if !keep(audible()) {
                return Ok(false);
            }
            let pcm = chunk?;
            if first_audio.get().is_none() {
                first_audio.set(Some(started.elapsed()));
            }
            seconds.set(seconds.get() + pcm.len() as f32 / rate.get() as f32);
            player.append(SamplesBuffer::new(mono, rate, pcm));
        }
        Ok(true)
    };
    let finished = match source {
        Source::Text(text) => play(text)?,
        Source::Feed(feed) => {
            let mut spoken = 0usize;
            let mut finished = true;
            while let Some(piece) = feed.next() {
                if !keep(audible()) {
                    finished = false;
                    break;
                }
                let text = speakable(&piece);
                if text.is_empty() || spoken >= MAX_SPOKEN_CHARS {
                    if piece.is_empty() {
                        // Nothing new has arrived yet.
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    continue;
                }
                spoken += text.len();
                if !play(&text)? {
                    finished = false;
                    break;
                }
            }
            finished
        }
    };
    if !finished {
        player.stop();
        info!(
            "[GRAIN] talk-back: stopped after {} ms ({:.1} s spoken)",
            started.elapsed().as_millis(),
            seconds.get()
        );
        return Ok(tts);
    }
    info!(
        "[GRAIN] talk-back: load {} ms, first audio {} ms, {:.1} s of speech synthesized in {} ms",
        loaded.as_millis(),
        first_audio.get().unwrap_or_default().as_millis(),
        seconds.get(),
        started.elapsed().as_millis()
    );
    while !player.empty() {
        if !keep(true) {
            player.stop();
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(tts)
}

// ── Streamed reply (EXPERIMENT, revertable) ─────────────────────────────────
//
// A streamed reply starts talking once enough of it has arrived, and the rest
// is read as it lands: the worker speaks whole sentences from a growing text.

/// Text arriving for one streamed speech.
#[derive(Default)]
pub(crate) struct SpeechFeed(Mutex<FeedState>);

#[derive(Default)]
struct FeedState {
    text: String,
    /// Bytes of `text` already handed to the voice.
    consumed: usize,
    done: bool,
    cancelled: bool,
}

impl SpeechFeed {
    /// The reply so far. Ignored if it no longer starts with what was spoken.
    pub(crate) fn update(&self, text: &str) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !state.done && text.get(..state.consumed) == state.text.get(..state.consumed) {
            state.text = text.to_string();
        }
    }

    /// The whole reply is in: speak the rest, then end.
    pub(crate) fn finish(&self, text: &str) {
        self.update(text);
        self.0.lock().unwrap_or_else(|e| e.into_inner()).done = true;
    }

    /// Stop this speech (only this one).
    pub(crate) fn cancel(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.done = true;
        state.cancelled = true;
    }

    fn cancelled(&self) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).cancelled
    }

    /// The next piece worth speaking: whole sentences (outside a code fence),
    /// or everything once the reply is done. `None` = finished.
    fn next(&self) -> Option<String> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let rest = &state.text[state.consumed..];
        let cut = if state.done {
            rest.len()
        } else {
            rest.char_indices()
                .filter(|&(i, c)| {
                    c == '\n'
                        || (matches!(c, '.' | '!' | '?')
                            && rest[i + 1..].starts_with(char::is_whitespace))
                })
                .map(|(i, c)| i + c.len_utf8())
                .filter(|&end| rest[..end].matches("```").count() % 2 == 0)
                .last()
                .unwrap_or(0)
        };
        if cut == 0 && state.done {
            return None;
        }
        let piece = rest[..cut].to_string();
        state.consumed += cut;
        Some(piece)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Real synthesis benchmark. Needs the model files in GRAIN_TTS_MODEL_DIR:
    /// `GRAIN_TTS_MODEL_DIR=… cargo test --lib grain_tts::tests::bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench() {
        let dir = PathBuf::from(std::env::var("GRAIN_TTS_MODEL_DIR").expect("model dir"));
        let started = Instant::now();
        let tts = Voice::load(&dir, VOICE).unwrap();
        let loaded = started.elapsed();
        let text = speakable(
            "Sure! Your meeting with **Priya** moved to 3:30 PM on Thursday. \
             I've also added the agenda link to the invite, and sent her a short note \
             so she knows the room changed to the second floor.",
        );
        let synth_start = Instant::now();
        let mut first = None;
        let mut pcm = Vec::new();
        for chunk in tts.stream(&text).unwrap() {
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
        assert!(seconds > 1.0, "no speech produced");
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
