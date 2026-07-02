//! [GRAIN] Gated streaming smoke test against a real GGUF, mirroring exactly
//! what `TranscriptionManager::run_stream_worker` does (Model::load → session →
//! stream → feed → finalize). Run with:
//!
//! ```text
//! GRAIN_TC_GGUF=<path.gguf> GRAIN_TC_WAV=<16k mono s16 wav> \
//!   cargo test --test streaming_smoke -- --nocapture
//! ```
//!
//! Requires the transcribe-cpp runtime DLLs next to the test binary
//! (copy C:\gt\debug\transcribe.dll + ggml*.dll into C:\gt\debug\deps\).
//!
//! Besides asserting a transcript is produced, it reports the committed vs
//! tentative update cadence — the evidence base for rendering the tentative
//! tail in the pill (auto-commit goes long stretches without committing).

use std::time::Instant;

use transcribe_cpp::{Model, RunOptions, StreamOptions, Task, TimestampKind};

fn read_wav_f32(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("open wav");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "test wav must be 16 kHz");
    assert_eq!(spec.channels, 1, "test wav must be mono");
    reader
        .samples::<i16>()
        .map(|s| s.expect("wav sample") as f32 / 32768.0)
        .collect()
}

#[test]
fn streams_a_real_gguf_when_present() {
    let (Ok(gguf), Ok(wav)) = (
        std::env::var("GRAIN_TC_GGUF"),
        std::env::var("GRAIN_TC_WAV"),
    ) else {
        eprintln!("GRAIN_TC_GGUF / GRAIN_TC_WAV not set — skipping streaming smoke test");
        return;
    };

    transcribe_cpp::init_logging();
    transcribe_cpp::init_backends_default().expect("init transcribe-cpp backends");
    let devices = transcribe_cpp::devices();
    eprintln!(
        "devices: {:?}",
        devices.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    assert!(
        !devices.is_empty(),
        "no compute devices registered — DLLs missing next to test exe?"
    );

    let samples = read_wav_f32(&wav);
    let model = Model::load(&gguf).expect("load gguf");
    let mut session = model.session().expect("create session");
    let mut stream = session
        .stream(
            &RunOptions {
                task: Task::Transcribe,
                ..Default::default()
            },
            &StreamOptions::default(),
        )
        .expect("begin stream");

    // Feed in ~100 ms chunks like the mic path.
    let chunk = 1600usize;
    let mut committed_updates = 0u32;
    let mut tentative_updates = 0u32;
    let mut last_commit_at_ms = 0f64;
    let mut max_commit_gap_ms = 0f64;
    let started = Instant::now();
    for (i, c) in samples.chunks(chunk).enumerate() {
        let update = stream.feed(c).expect("feed");
        let pos_ms = (i * chunk) as f64 / 16.0;
        if update.committed_changed {
            committed_updates += 1;
            max_commit_gap_ms = max_commit_gap_ms.max(pos_ms - last_commit_at_ms);
            last_commit_at_ms = pos_ms;
        }
        if update.tentative_changed {
            tentative_updates += 1;
        }
    }
    let tail_gap_ms = (samples.len() as f64 / 16.0) - last_commit_at_ms;
    stream.finalize().expect("finalize");
    let text = stream.text().committed;

    eprintln!(
        "streamed {:.1}s of audio in {:.1}s: {} committed updates, {} tentative updates, \
         max commit gap {:.1}s, tail without commit {:.1}s",
        samples.len() as f64 / 16_000.0,
        started.elapsed().as_secs_f64(),
        committed_updates,
        tentative_updates,
        max_commit_gap_ms / 1000.0,
        tail_gap_ms / 1000.0,
    );
    eprintln!("final committed: {text}");

    assert!(!text.trim().is_empty(), "streaming produced no text");
    assert!(
        tentative_updates > 0,
        "no tentative updates — the pill's moving tail would never move"
    );
}

/// [GRAIN] Validates the rolling path's core assumption: a batch run with
/// `TimestampKind::Word` returns populated, monotonic word rows (which
/// `rolling::map_word_timings` maps into the timeline assembler). Gated on the
/// same env vars; skips when unset. Works with any catalog model (parakeet,
/// nemotron, whisper…) — the point is that SOME timing granularity comes back.
#[test]
fn batch_run_returns_word_timings() {
    let (Ok(gguf), Ok(wav)) = (
        std::env::var("GRAIN_TC_GGUF"),
        std::env::var("GRAIN_TC_WAV"),
    ) else {
        eprintln!("GRAIN_TC_GGUF / GRAIN_TC_WAV not set — skipping word-timing test");
        return;
    };

    transcribe_cpp::init_logging();
    transcribe_cpp::init_backends_default().expect("init transcribe-cpp backends");

    let samples = read_wav_f32(&wav);
    let model = Model::load(&gguf).expect("load gguf");
    let mut session = model.session().expect("create session");
    let transcript = session
        .run(
            &samples,
            &RunOptions {
                task: Task::Transcribe,
                timestamps: TimestampKind::Word,
                ..Default::default()
            },
        )
        .expect("batch run");

    eprintln!(
        "words={} timestamp_kind={:?} text={:?}",
        transcript.words.len(),
        transcript.timestamp_kind,
        transcript.text
    );
    assert!(!transcript.text.trim().is_empty(), "no text");
    assert!(
        !transcript.words.is_empty(),
        "no word rows — rolling would fall back to synthesized timings"
    );
    // Word times must be non-decreasing (they drive positional dedup).
    let mut prev = i64::MIN;
    for w in &transcript.words {
        assert!(w.t0_ms >= prev, "word times not monotonic at {:?}", w.text);
        prev = w.t0_ms;
    }
}
