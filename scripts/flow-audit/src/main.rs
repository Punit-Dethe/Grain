//! Explicit offline conformance diagnostics for the production Flow policies.
//! Audio stays local; JSON transcripts go only to the explicitly requested file.
use std::{error::Error, path::Path, time::Instant};

use grain_tdt::{
    merge_tokens, BufferedCursor, Token, WindowCursor, BUFFERED_MAX_SAMPLES, BUFFERED_OWNED_FRAMES,
    FRAME_SAMPLES, SAMPLE_RATE,
};
use serde_json::{json, Value};
use transcribe_cpp::{
    Backend, ExtSlot, Model, ModelOptions, ParakeetTdtBufferedOptions, ParakeetTdtWindowOptions,
    RunExtension, RunOptions, Session, SessionOptions, TimestampKind,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn load_audio(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != SAMPLE_RATE as u32 {
        return Err("expected a 16 kHz mono WAV; resample explicitly before comparing".into());
    }
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => Ok(reader
            .samples::<i16>()
            .map(|sample| sample.map(|v| v as f32 / 32768.0))
            .collect::<std::result::Result<_, _>>()?),
        (hound::SampleFormat::Float, 32) => Ok(reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?),
        _ => Err("expected PCM16 or Float32 WAV".into()),
    }
}

fn options() -> RunOptions {
    RunOptions {
        timestamps: TimestampKind::Token,
        ..Default::default()
    }
}

fn flow(session: &mut Session, pcm: &[f32], v2: bool) -> Result<(String, Value)> {
    let mut tokens = Vec::new();
    let mut measurements = Vec::new();
    let mut buffered = BufferedCursor::default();
    let mut fluid = WindowCursor::default();
    let mut audio = Vec::with_capacity(if v2 {
        BUFFERED_MAX_SAMPLES
    } else {
        grain_tdt::MAX_MODEL_SAMPLES
    } as usize);
    loop {
        let total = pcm.len() as u64;
        let window = if v2 {
            buffered.next_stable(total).or_else(|| buffered.tail(total))
        } else {
            fluid.next_stable(total).or_else(|| fluid.tail(total))
        };
        let Some(window) = window else {
            break;
        };
        audio.clear();
        audio.extend_from_slice(&pcm[window.read_start as usize..window.read_end as usize]);
        let mut opts = options();
        if v2 {
            let start: i32 = window.context_frames().try_into()?;
            opts.family = Some(RunExtension::ParakeetTdtBuffered(
                ParakeetTdtBufferedOptions {
                    decode_start_frame: start,
                    decode_end_frame: if window.is_last {
                        -1
                    } else {
                        start + BUFFERED_OWNED_FRAMES as i32
                    },
                    timestamp_offset_frames: (window.chunk_start / FRAME_SAMPLES).try_into()?,
                    reset: window.chunk_start == 0,
                    finalize: window.is_last,
                },
            ));
        } else {
            audio.resize(window.encoder_input_samples(), 0.0);
            opts.family = Some(RunExtension::ParakeetTdtWindow(ParakeetTdtWindowOptions {
                decode_start_frame: window.context_frames().try_into()?,
                decode_end_frame: window.content_frames().try_into()?,
                timestamp_offset_frames: window.global_frame_offset().try_into()?,
                finalize_tail: window.finalize_decoder(),
            }));
        }
        let start = Instant::now();
        let result = session.run(&audio, &opts)?;
        let decode_seconds = start.elapsed().as_secs_f64();
        let mut owned = Vec::new();
        for t in result.tokens {
            if t.t0_ms < 0
                || t.t1_ms < t.t0_ms
                || t.t0_ms % 80 != 0
                || (t.t1_ms - t.t0_ms) % 80 != 0
            {
                return Err("unexpected native token timing".into());
            }
            owned.push(Token {
                id: t.id,
                frame: (t.t0_ms / 80) as u64,
                confidence: t.p,
                duration: ((t.t1_ms - t.t0_ms) / 80).try_into()?,
            });
        }
        measurements.push(
            json!({"read_start": window.read_start, "read_end": window.read_end,
            "owned_start": window.chunk_start, "final": window.is_last,
            "decode_seconds": decode_seconds,
            "token_sequence": owned.iter().map(|t| json!({"id": t.id, "frame": t.frame,
                "confidence": t.confidence, "duration_frames": t.duration})).collect::<Vec<_>>() }),
        );
        if v2 {
            tokens.extend(owned);
        } else {
            merge_tokens(&mut tokens, &owned);
        }
        if window.is_last {
            break;
        }
        let committed = if v2 {
            buffered.commit(window)
        } else {
            fluid.commit(window)
        };
        if !committed {
            return Err("cursor rejected its own window".into());
        }
    }
    if !v2 {
        tokens.sort_by_key(|t| t.frame);
    }
    let ids: Vec<_> = tokens.iter().map(|t| t.id).collect();
    Ok((
        session.model().detokenize(&ids)?.trim().to_owned(),
        json!(measurements),
    ))
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn word_edits(reference: &[String], actual: &[String]) -> usize {
    let mut row: Vec<_> = (0..=actual.len()).collect();
    for (i, left) in reference.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in actual.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(left != right))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[actual.len()]
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        return Err(
            "usage: grain-flow-audit <model.gguf> <native-bin-dir> <report.json> <audio.wav>..."
                .into(),
        );
    }
    let reference = std::env::var_os("FLOW_AUDIT_REFERENCE")
        .map(std::fs::read_to_string)
        .transpose()?;
    if reference.is_some() && args.len() != 4 {
        return Err("a reference requires exactly one WAV".into());
    }
    let threads: i32 = std::env::var("FLOW_AUDIT_THREADS")
        .unwrap_or_else(|_| "4".into())
        .parse()?;
    if threads <= 0 {
        return Err("thread count must be positive".into());
    }
    let backend = match std::env::var("FLOW_AUDIT_BACKEND")
        .as_deref()
        .unwrap_or("cpu")
    {
        "cpu" => Backend::Cpu,
        "vulkan" => Backend::Vulkan,
        "cuda" => Backend::Cuda,
        "metal" => Backend::Metal,
        "auto" => Backend::Auto,
        _ => return Err("backend must be cpu, vulkan, cuda, metal or auto".into()),
    };
    transcribe_cpp::disable_logging();
    transcribe_cpp::init_backends(&args[1])?;
    let model = Model::load_with(
        &args[0],
        &ModelOptions {
            backend,
            ..Default::default()
        },
    )?;
    let v2 = model.variant() == "tdt-0.6b-v2";
    if model.arch() != "parakeet"
        || !matches!(model.variant().as_str(), "tdt-0.6b-v2" | "tdt-0.6b-v3")
        || !model.accepts_ext(
            ExtSlot::Run,
            if v2 {
                ParakeetTdtBufferedOptions::KIND
            } else {
                ParakeetTdtWindowOptions::KIND
            },
        )
    {
        return Err("model does not support the reviewed Flow contract".into());
    }
    let mut session = model.session_with(&SessionOptions {
        n_threads: threads,
        ..Default::default()
    })?;
    let mut rows = Vec::new();
    for path in &args[3..] {
        let audio = load_audio(Path::new(path))?;
        let start = Instant::now();
        let (text, windows) = flow(&mut session, &audio, v2)?;
        let mut row = json!({"file": Path::new(path).file_name().unwrap_or_default().to_string_lossy(),
            "samples": audio.len(), "text": text, "windows": windows,
            "total_compute_seconds": start.elapsed().as_secs_f64()});
        if let Some(reference) = &reference {
            let expected = words(reference);
            if expected.is_empty() {
                return Err("reference contains no words".into());
            }
            row["reference_word_error_rate"] =
                json!(word_edits(&expected, &words(&text)) as f64 / expected.len() as f64);
        }
        println!(
            "{} windows processed",
            row["windows"].as_array().unwrap().len()
        );
        rows.push(row);
    }
    std::fs::write(
        &args[2],
        serde_json::to_vec_pretty(&json!({
            "model_variant": model.variant(), "backend": model.backend(), "threads": threads,
        "native_version": transcribe_cpp::version(), "native_patch": transcribe_cpp::grain_patch_id(),
        "native_abi_hash": transcribe_cpp::runtime_header_hash(),
        "model_file": Path::new(&args[0]).file_name().unwrap_or_default().to_string_lossy(),
            "policy": if v2 { "stateful-125-125-62" } else { "fluid-v3" }, "recordings": rows,
            "note": "Offline conformance diagnostics. Compute time excludes capture, cold loading and transport; this is not Stop latency or a parameter-tuning benchmark."
        }))?,
    )?;
    Ok(())
}
