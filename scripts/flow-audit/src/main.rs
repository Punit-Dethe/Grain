//! Offline comparison using historical and production Flow layouts/merger.
//! Audio stays local; JSON transcripts go only to the explicitly requested file.
use std::{error::Error, path::Path, time::Instant};

use grain_tdt::{merge_tokens, Token, Window, WindowCursor, SAMPLE_RATE};
use serde_json::{json, Value};
use transcribe_cpp::{
    Backend, ExtSlot, Model, ModelOptions, ParakeetTdtWindowOptions, RunExtension, RunOptions,
    Session, SessionOptions, TimestampKind,
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

fn windows(total: u64) -> Vec<Window> {
    let mut cursor = WindowCursor::default();
    let mut output = Vec::new();
    while let Some(window) = cursor.next_stable(total) {
        assert!(cursor.commit(window));
        output.push(window);
    }
    if let Some(tail) = cursor.tail(total) {
        output.push(tail);
    }
    output
}

fn flow(
    session: &mut Session,
    pcm: &[f32],
    native_decoder: bool,
    right_context_samples: usize,
    independent_samples: Option<u64>,
    prefer_pause: bool,
) -> Result<(String, Value)> {
    let mut merged = Vec::new();
    let mut measurements = Vec::new();
    let mut audio = Vec::with_capacity(
        independent_samples.unwrap_or(grain_tdt::MAX_MODEL_SAMPLES) as usize
            + right_context_samples,
    );
    let work = independent_samples.map_or_else(
        || windows(pcm.len() as u64),
        |limit| independent_windows(pcm, limit, prefer_pause),
    );
    for window in work {
        audio.clear();
        let read_end = if native_decoder {
            (window.read_end as usize)
                .saturating_add(right_context_samples)
                .min(pcm.len())
        } else {
            window.read_end as usize
        };
        audio.extend_from_slice(&pcm[window.read_start as usize..read_end]);
        let mut opts = options();
        if !native_decoder {
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
        let mut tokens = Vec::new();
        for token in result.tokens {
            if token.t0_ms < 0 || token.t1_ms < token.t0_ms || token.t0_ms % 80 != 0 {
                return Err("unexpected native token timing".into());
            }
            let mut frame = (token.t0_ms / 80) as u64;
            if native_decoder {
                // Only the historical-window diagnostic filters context after
                // decoding; it cannot reproduce a decoder starting there.
                // Independent native windows retain the complete Batch output.
                if independent_samples.is_none()
                    && (frame < window.context_frames() as u64
                        || frame >= (window.context_frames() + window.content_frames()) as u64)
                {
                    continue;
                }
                frame += window.global_frame_offset();
            }
            tokens.push(Token {
                id: token.id,
                frame,
                confidence: token.p,
                duration: ((token.t1_ms - token.t0_ms) / 80).try_into()?,
            });
        }
        measurements.push(json!({
            "read_start": window.read_start, "read_end": window.read_end,
            "chunk_start": window.chunk_start, "final": window.is_last,
            "encoder_read_end": read_end,
            "raw_decoder_text": result.text,
            "tokens": tokens.len(), "decode_seconds": decode_seconds,
            "last_token_frame": tokens.last().map(|t| t.frame),
            "token_sequence": tokens.iter().map(|t| json!({
                "id": t.id, "frame": t.frame, "confidence": t.confidence,
                "duration_frames": t.duration,
            })).collect::<Vec<_>>(),
        }));
        merge_tokens(&mut merged, &tokens);
    }
    merged.sort_by_key(|token| token.frame);
    let ids: Vec<_> = merged.iter().map(|token| token.id).collect();
    Ok((
        session.model().detokenize(&ids)?.trim().to_owned(),
        json!(measurements),
    ))
}

fn independent_windows(pcm: &[f32], limit: u64, prefer_pause: bool) -> Vec<Window> {
    let total = pcm.len() as u64;
    if prefer_pause {
        let mut cursor = grain_tdt::NativeWindowCursor::with_max_samples(limit).unwrap();
        let mut work = Vec::new();
        while let Some(input) = cursor.next_stable(total) {
            let window = cursor
                .prefer_pause(
                    input,
                    &pcm[input.read_start as usize..input.read_end as usize],
                )
                .unwrap();
            assert!(cursor.commit(window));
            work.push(window);
        }
        if let Some(window) = cursor.tail(total) {
            work.push(window);
        }
        return work;
    }
    let mut work = Vec::new();
    let stride = limit - grain_tdt::OVERLAP_SAMPLES;
    let mut start = 0;
    while start < total {
        let end = (start + limit).min(total);
        work.push(Window {
            chunk_start: start,
            read_start: start,
            read_end: end,
            is_last: end == total,
        });
        if end == total {
            break;
        }
        start += stride;
    }
    work
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
        return Err("usage: grain-flow-audit <model.gguf> <native-bin-dir> <report.json> <audio.wav>...\noptional: FLOW_AUDIT_REFERENCE=<reference.txt> for a single WAV".into());
    }
    let reference = std::env::var_os("FLOW_AUDIT_REFERENCE")
        .map(std::fs::read_to_string)
        .transpose()?;
    if reference.is_some() && args.len() != 4 {
        return Err("a reference transcript requires exactly one WAV".into());
    }
    let right_context_ms: u64 = std::env::var("FLOW_AUDIT_RIGHT_CONTEXT_MS")
        .unwrap_or_else(|_| "0".to_owned())
        .parse()?;
    if right_context_ms > 4_000 {
        return Err("diagnostic right context must be between 0 and 4000 ms".into());
    }
    let independent_ms: u64 = std::env::var("FLOW_AUDIT_INDEPENDENT_MS")
        .unwrap_or_else(|_| (grain_tdt::NATIVE_MAX_SAMPLES * 1_000 / SAMPLE_RATE).to_string())
        .parse()?;
    if !(15_040..=30_000).contains(&independent_ms) || !independent_ms.is_multiple_of(80) {
        return Err(
            "independent window must be 15040..30000 ms and frame-aligned (e.g. 24960)".into(),
        );
    }
    let prefer_pause = std::env::var("FLOW_AUDIT_PREFER_PAUSE").as_deref() == Ok("1");
    let resource_mode = std::env::var("FLOW_AUDIT_RESOURCE_MODE").ok();
    if resource_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "legacy" | "candidate" | "batch"))
    {
        return Err("resource mode must be legacy, candidate or batch".into());
    }
    let repeats: usize = std::env::var("FLOW_AUDIT_REPEATS")
        .unwrap_or_else(|_| "1".to_owned())
        .parse()?;
    if !(1..=3).contains(&repeats) || (repeats > 1 && resource_mode.is_none()) {
        return Err("repeats must be 1..3 and require resource mode for repeated passes".into());
    }
    let threads: i32 = std::env::var("FLOW_AUDIT_THREADS")
        .unwrap_or_else(|_| "4".to_owned())
        .parse()?;
    if !(1..=8).contains(&threads) {
        return Err("thread count must be 1..8".into());
    }
    let backend = match std::env::var("FLOW_AUDIT_BACKEND")
        .as_deref()
        .unwrap_or("cpu")
    {
        "cpu" => Backend::Cpu,
        "vulkan" => Backend::Vulkan,
        "auto" => Backend::Auto,
        _ => return Err("backend must be cpu, vulkan or auto".into()),
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
    if model.arch() != "parakeet"
        || !matches!(model.variant().as_str(), "tdt-0.6b-v2" | "tdt-0.6b-v3")
        || !model.accepts_ext(ExtSlot::Run, ParakeetTdtWindowOptions::KIND)
    {
        return Err("model does not support Grain's reviewed TDT window extension".into());
    }
    let mut session = model.session_with(&SessionOptions {
        n_threads: threads,
        ..Default::default()
    })?;
    let mut rows = Vec::new();
    for path in args[3..].iter().cycle().take((args.len() - 3) * repeats) {
        let pass = rows.len() / (args.len() - 3) + 1;
        let audio = load_audio(Path::new(path))?;
        if audio.len() < SAMPLE_RATE as usize {
            return Err("Flow requires at least one second of audio".into());
        }
        if let Some(mode) = &resource_mode {
            let candidate = mode == "candidate";
            let start = Instant::now();
            if mode == "batch" {
                let result = session.run(&audio, &options())?;
                println!("batch: {:.3}s compute", start.elapsed().as_secs_f64());
                rows.push(json!({"file": Path::new(path).file_name().unwrap_or_default().to_string_lossy(),
                    "samples": audio.len(), "text": result.text, "pass": pass, "compute_seconds": start.elapsed().as_secs_f64()}));
                std::fs::write(
                    &args[2],
                    serde_json::to_vec_pretty(&json!({
                        "resource_mode": mode, "recordings": rows,
                        "backend": model.backend(), "threads": threads,
                        "note": "Ordinary complete-audio Batch only."
                    }))?,
                )?;
                continue;
            }
            let (text, work) = flow(
                &mut session,
                &audio,
                candidate,
                0,
                candidate.then_some(independent_ms * SAMPLE_RATE / 1_000),
                candidate && prefer_pause,
            )?;
            println!(
                "{}: {} windows, {:.3}s compute",
                mode,
                work.as_array().unwrap().len(),
                start.elapsed().as_secs_f64()
            );
            rows.push(
                json!({"file": Path::new(path).file_name().unwrap_or_default().to_string_lossy(),
                "samples": audio.len(), "text": text, "pass": pass, "windows": work}),
            );
            std::fs::write(
                &args[2],
                serde_json::to_vec_pretty(&json!({
                    "resource_mode": mode, "recordings": rows,
                    "backend": model.backend(), "threads": threads,
                    "independent_window_ms": independent_ms, "independent_prefer_pause": prefer_pause,
                    "note": "One pipeline only; no Batch inference or accuracy comparison."
                }))?,
            )?;
            continue;
        }
        let start = Instant::now();
        let batch = session.run(&audio, &options())?;
        let batch_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let (current, current_windows) = flow(&mut session, &audio, false, 0, None, false)?;
        let flow_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let (native, native_windows) = flow(
            &mut session,
            &audio,
            true,
            (right_context_ms * SAMPLE_RATE / 1_000) as usize,
            None,
            false,
        )?;
        let native_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let (independent, independent_work) = flow(
            &mut session,
            &audio,
            true,
            0,
            Some(independent_ms * SAMPLE_RATE / 1_000),
            prefer_pause,
        )?;
        let independent_seconds = start.elapsed().as_secs_f64();
        let baseline = words(&batch.text);
        let current_edits = word_edits(&baseline, &words(&current));
        let native_edits = word_edits(&baseline, &words(&native));
        let independent_edits = word_edits(&baseline, &words(&independent));
        let mut row = json!({
            "file": Path::new(path).file_name().unwrap_or_default().to_string_lossy(),
            "samples": audio.len(), "batch": batch.text, "flow": current,
            "batch_token_sequence": batch.tokens.iter().map(|t| json!({
                "id": t.id, "start_ms": t.t0_ms, "end_ms": t.t1_ms,
                "confidence": t.p,
            })).collect::<Vec<_>>(),
            "native_windows_diagnostic": native,
            "batch_seconds": batch_seconds, "flow_total_compute_seconds": flow_seconds,
            "native_windows_total_compute_seconds": native_seconds,
            "batch_words": baseline.len(), "flow_word_edits_to_batch": current_edits,
            "native_windows_word_edits_to_batch": native_edits,
            "flow_windows": current_windows, "native_windows": native_windows,
            "independent_native": independent, "independent_windows": independent_work,
            "independent_word_edits_to_batch": independent_edits,
            "independent_total_compute_seconds": independent_seconds,
        });
        if let Some(reference) = &reference {
            let expected = words(reference);
            if expected.is_empty() {
                return Err("reference transcript has no words".into());
            }
            row["reference_words"] = json!(expected.len());
            row["batch_word_error_rate"] =
                json!(word_edits(&expected, &baseline) as f64 / expected.len() as f64);
            row["flow_word_error_rate"] =
                json!(word_edits(&expected, &words(&current)) as f64 / expected.len() as f64);
            row["native_windows_word_error_rate"] =
                json!(word_edits(&expected, &words(&native)) as f64 / expected.len() as f64);
            row["independent_word_error_rate"] =
                json!(word_edits(&expected, &words(&independent)) as f64 / expected.len() as f64);
        }
        println!(
            "{}: {:.2}s, {} windows, Flow/Batch word edits={}, diagnostic native windows/Batch={}, independent/Batch={}",
            row["file"].as_str().unwrap_or_default(),
            audio.len() as f64 / SAMPLE_RATE as f64,
            row["flow_windows"].as_array().unwrap().len(),
            current_edits,
            native_edits, independent_edits
        );
        rows.push(row);
        // Keep progress if a later native decode fails.
        std::fs::write(
            &args[2],
            serde_json::to_vec_pretty(&json!({
                "model_variant": model.variant(), "backend": model.backend(),
                "threads": threads,
                "native_version": transcribe_cpp::version(), "recordings": rows,
                "diagnostic_right_context_ms": right_context_ms,
                "independent_window_ms": independent_ms,
                "independent_prefer_pause": prefer_pause,
                "note": "Identical WAV samples; no capture, VAD, normalization, dictionary or LLM pass. Batch disagreement is not WER. Timings measure total compute, not stop latency. Historical-window filtering is diagnostic; independent quiet windows share production v2 policy.",
            }))?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_distance_measures_missing_and_changed_words_without_formatting() {
        assert_eq!(
            word_edits(&words("Accuracy, matters."), &words("accuracy matters")),
            0
        );
        assert_eq!(
            word_edits(&words("accuracy matters"), &words("accur matters")),
            1
        );
        assert_eq!(word_edits(&words("one two three"), &words("one three")), 1);
        assert_eq!(word_edits(&[], &words("one two")), 2);
    }

    #[test]
    fn batch_planning_keeps_short_audio_whole_and_covers_long_tail() {
        assert_eq!(windows(240_000).len(), 1);
        let work = windows(240_001);
        assert_eq!(work.len(), 2);
        assert_eq!(work[0].read_start, 0);
        assert_eq!(work[1].read_end, 240_001);
        assert_eq!(
            work[1].chunk_start - work[1].read_start,
            grain_tdt::FRAME_SAMPLES
        );
    }
}
