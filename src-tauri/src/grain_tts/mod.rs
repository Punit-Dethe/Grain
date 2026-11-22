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

use crate::grain_model_files::{self, GrainModel, ModelFiles};
use log::{info, warn};
use model::{Voice, FILES, FOLDER, VOICE};
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
    voice.file.present_in(dir)
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

/// Where speech plays: the chosen output, or `None` for the system default.
fn chosen_output(app: &AppHandle) -> Option<String> {
    crate::settings::get_settings(app).agent_voice_output
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
            let never = AtomicBool::new(false);
            let result =
                grain_model_files::download(&client, &voice.file, &dir, |_| {}, &never).await;
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

/// The talk-back voice as one of Grain's own models (Settings shows, downloads
/// and deletes it through [`grain_model_files`]).
pub(crate) const MODEL: ModelFiles = ModelFiles {
    folder: &FOLDER,
    files: &FILES,
    in_use: busy,
};

/// A speech job is alive or waiting, so the model files may be open.
fn busy() -> bool {
    WAITING.load(Ordering::SeqCst) > 0 || SPEAKING.try_lock().is_err()
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    MODEL.dir(app)
}

fn installed(dir: &Path) -> bool {
    MODEL.installed_in(dir)
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
        chosen_output(app),
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
        chosen_output(app),
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
        chosen_output(app),
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
    output: Option<String>,
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
            run(&dir, voice, output.as_deref(), &source, policy, generation);
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
fn run(
    dir: &Path,
    voice: &str,
    output: Option<&str>,
    source: &Source,
    policy: Policy,
    generation: u64,
) {
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
        match say(dir, voice, output, model.take(), source, &keep) {
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
    output: Option<&str>,
    parked: Option<Voice>,
    source: &Source,
    keep: &dyn Fn(bool) -> bool,
) -> Result<Voice, String> {
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

    let rate = tts.sample_rate();
    let speaker = std::cell::RefCell::new(Speaker::open(output, rate)?);

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
            seconds.set(seconds.get() + pcm.len() as f32 / rate as f32);
            let mut speaker = speaker.borrow_mut();
            speaker.recover(output)?;
            speaker.append(pcm);
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
    let mut speaker = speaker.into_inner();
    if !finished {
        speaker.stop();
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
    while !speaker.empty() {
        if !keep(true) {
            speaker.stop();
            break;
        }
        speaker.recover(output)?;
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(tts)
}

/// The output one speech plays on. It keeps a copy of the audio still queued,
/// so when the device is lost (unplugged, or the system default moved while
/// following it) it reopens on the chosen output, or the new default, and
/// replays the unheard rest there. Dropped with the speech.
struct Speaker {
    // Field order: the player detaches before its device closes.
    player: rodio::Player,
    _sink: rodio::MixerDeviceSink,
    lost: Arc<AtomicBool>,
    /// Pieces appended but not finished playing, oldest first.
    queued: std::collections::VecDeque<Vec<f32>>,
    rate: u32,
}

impl Speaker {
    fn open(output: Option<&str>, rate: u32) -> Result<Self, String> {
        use rodio::cpal::ErrorKind;
        let device = match output {
            Some(id) => crate::grain_audio::output_device(Some(id)).or_else(|e| {
                warn!("[GRAIN] talk-back: {e}; using the default output");
                crate::grain_audio::output_device(None)
            }),
            None => crate::grain_audio::output_device(None),
        }?;
        let lost = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&lost);
        let mut sink = rodio::DeviceSinkBuilder::from_device(device)
            .map_err(|e| format!("no audio output: {e}"))?
            .with_error_callback(move |error: rodio::cpal::Error| {
                if matches!(
                    error.kind(),
                    ErrorKind::StreamInvalidated | ErrorKind::DeviceNotAvailable
                ) {
                    info!("[GRAIN] talk-back: output lost ({error}); reopening");
                    flag.store(true, Ordering::Release);
                }
            })
            .open_stream()
            .map_err(|e| format!("no audio output: {e}"))?;
        sink.log_on_drop(false);
        Ok(Self {
            player: rodio::Player::connect_new(sink.mixer()),
            _sink: sink,
            lost,
            queued: Default::default(),
            rate,
        })
    }

    fn buffer(&self, pcm: Vec<f32>) -> rodio::buffer::SamplesBuffer {
        let mono = rodio::ChannelCount::new(1).expect("1 is non-zero");
        let rate = rodio::SampleRate::new(self.rate).expect("the voice has a sample rate");
        rodio::buffer::SamplesBuffer::new(mono, rate, pcm)
    }

    fn trim(&mut self) {
        while self.queued.len() > self.player.len() {
            self.queued.pop_front();
        }
    }

    fn append(&mut self, pcm: Vec<f32>) {
        self.trim();
        self.player.append(self.buffer(pcm.clone()));
        self.queued.push_back(pcm);
    }

    /// Reopen after a lost device; the piece that was playing restarts.
    fn recover(&mut self, output: Option<&str>) -> Result<(), String> {
        if !self.lost.load(Ordering::Acquire) {
            return Ok(());
        }
        self.trim();
        let queued = std::mem::take(&mut self.queued);
        *self = Speaker::open(output, self.rate)?;
        for pcm in queued {
            self.append(pcm);
        }
        Ok(())
    }

    fn empty(&self) -> bool {
        self.player.empty()
    }

    fn stop(&self) {
        self.player.stop();
    }
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

/// Download the pinned voice files in the background when they are missing
/// (progress shows in Settings).
pub(crate) fn ensure_installed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = install(&app).await {
            warn!("[GRAIN] talk-back: voice download failed: {e}");
        }
    });
}

async fn install(app: &AppHandle) -> Result<(), String> {
    grain_model_files::install(app, GrainModel::TalkBackVoice).await?;
    info!("[GRAIN] talk-back: voice ready");
    Ok(())
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
        assert!(FILES.iter().all(grain_model_files::PinnedFile::is_pinned));
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
            assert!(voice.file.is_pinned());
            assert!(matches!(voice.accent, "us" | "gb"));
        }
        let female = VOICES.iter().filter(|v| v.gender == "female").count();
        assert_eq!((female, VOICES.len() - female), (7, 7));
        assert!(preset("Harper").is_some() && preset("Nobody").is_none());
    }
}
