//! Model-matrix harness: run a WAV file through the rolling-window engine with a
//! real `transcribe-rs` model, printing the chunk-by-chunk assembled transcript.
//!
//! Usage:
//!   asr-harness <model-dir> <input.wav> [--block-ms N] [--window-s N]
//!
//! `<model-dir>` is a Parakeet directory (encoder/decoder_joint/nemo128/vocab).
//! The WAV must be 16 kHz mono 16-bit (Parakeet's required input).

use std::path::Path;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use grain_transcribe::{block_rms, EngineKind, GrainModel, RollingWindowSession};
use rolling_window::RollingWindowConfig;

fn main() -> ExitCode {
    env_logger_init();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn env_logger_init() {
    // transcribe-rs logs via `log`; surface it if RUST_LOG is set, else stay quiet.
    let _ = std::env::var("RUST_LOG");
}

struct Args {
    model_dir: String,
    wav: String,
    engine: Option<EngineKind>,
    block_ms: u32,
    window_s: Option<f64>,
}

fn parse_args() -> Result<Args> {
    let mut positional = Vec::new();
    let mut engine = None;
    let mut block_ms = 50u32; // ~800 frames @16k, matching the Python BLOCKSIZE
    let mut window_s = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--engine" => {
                let v = it.next().context("--engine needs a value")?;
                engine = Some(
                    EngineKind::parse(&v)
                        .with_context(|| format!("unknown engine '{v}'; one of: parakeet, moonshine, moonshine-streaming, sense-voice, gigaam, canary, cohere, whisper"))?,
                );
            }
            "--block-ms" => {
                block_ms = it.next().context("--block-ms needs a value")?.parse()?;
            }
            "--window-s" => {
                window_s = Some(it.next().context("--window-s needs a value")?.parse()?);
            }
            _ => positional.push(a),
        }
    }
    if positional.len() != 2 {
        bail!("usage: asr-harness <model-dir> <input.wav> [--engine NAME] [--block-ms N] [--window-s N]");
    }
    Ok(Args {
        model_dir: positional[0].clone(),
        wav: positional[1].clone(),
        engine,
        block_ms,
        window_s,
    })
}

/// Read a WAV as mono 16 kHz i16 frames (averaging stereo, warning on rate).
fn read_wav_mono16(path: &str) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("open {path}"))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 {
        eprintln!(
            "warning: WAV sample rate is {} Hz, expected 16000 — Parakeet may degrade",
            spec.sample_rate
        );
    }
    let raw: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => reader.samples::<i16>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map(|f| (f * 32767.0) as i16))
            .collect::<Result<_, _>>()?,
    };
    let mono = if spec.channels == 1 {
        raw
    } else {
        let ch = spec.channels as usize;
        raw.chunks(ch)
            .map(|frame| (frame.iter().map(|&s| s as i32).sum::<i32>() / ch as i32) as i16)
            .collect()
    };
    Ok(mono)
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let model_dir = Path::new(&args.model_dir);

    let kind = args
        .engine
        .or_else(|| EngineKind::guess_from_path(model_dir))
        .with_context(|| {
            format!(
                "could not infer engine from '{}'; pass --engine NAME",
                model_dir.display()
            )
        })?;
    eprintln!(
        "loading {} model from {} ...",
        kind.as_str(),
        model_dir.display()
    );
    let asr = GrainModel::load(kind, model_dir)?;

    let samples = read_wav_mono16(&args.wav)?;
    let cfg = RollingWindowConfig::default();
    let block_frames = (cfg.sample_rate * args.block_ms as usize / 1000).max(1);
    eprintln!(
        "audio: {} frames ({:.1}s) · block {} frames · feeding through rolling window ...",
        samples.len(),
        samples.len() as f64 / cfg.sample_rate as f64,
        block_frames
    );

    let mut session = RollingWindowSession::new(asr, cfg);
    if let Some(w) = args.window_s {
        session.set_rolling_window(w);
    }

    for block in samples.chunks(block_frames) {
        let rms = block_rms(block);
        session.push_block(block, rms)?;
    }
    let transcript = session.finish()?;

    println!("\n=== per-chunk assembly ===");
    for (i, (fresh_start, text)) in session.chunk_log.iter().enumerate() {
        println!("[chunk {i} @ {fresh_start:6.2}s] {text}");
    }
    println!("\n=== final transcript ===\n{transcript}");
    Ok(())
}
