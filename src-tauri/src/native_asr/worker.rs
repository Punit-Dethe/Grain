//! [GRAIN] Native ASR streaming worker — transcribe-cpp engine.
//!
//! Loads a GGUF model, opens a streaming `Session`, and drives the
//! feed → committed → finalize loop, emitting the cumulative committed
//! transcript to the pill as [`DaemonEvent::AsrStreamText`]. transcribe-cpp does
//! its own commit stabilization (`CommitPolicy::Auto`), so there is NO separate
//! stabilizer here — `stream.text().committed` is already the flicker-free
//! prefix Handy's UI (and ours) renders.
//!
//! This module is deliberately self-contained (our own glue, not ported upstream
//! line-for-line) so future upstream syncs stay simple.

use std::path::Path;

use grain_asr_core::session::AudioFrame;
use grain_core::DaemonEvent;
use transcribe_cpp::{Model, RunOptions, StreamOptions, Task};

/// What the frame source hands the worker on each pull. transcribe-cpp buffers
/// and commits continuously, so there's no explicit flush — just frames + stop.
pub enum FrameCmd {
    /// One captured frame (16 kHz mono f32) to feed the stream.
    Frame(AudioFrame),
    /// End the session: finalize, emit the final text, and return.
    Stop,
}

/// Drive one streaming session to completion. Returns the final committed text
/// (also emitted as the last [`DaemonEvent::AsrStreamText`] +
/// [`DaemonEvent::AsrSessionFinal`]) for the caller's paste/history step.
pub fn drive_stream(
    gguf_path: &Path,
    language: Option<String>,
    session_id: u64,
    mut next: impl FnMut() -> FrameCmd,
    mut emit: impl FnMut(DaemonEvent),
) -> anyhow::Result<String> {
    let model = Model::load(gguf_path)
        .map_err(|e| anyhow::anyhow!("failed to load GGUF model {}: {e}", gguf_path.display()))?;
    let mut session = model
        .session()
        .map_err(|e| anyhow::anyhow!("failed to create transcribe-cpp session: {e}"))?;

    let run = RunOptions {
        task: Task::Transcribe,
        language,
        ..Default::default()
    };
    let mut stream = session
        .stream(&run, &StreamOptions::default())
        .map_err(|e| anyhow::anyhow!("failed to start transcribe-cpp stream: {e}"))?;

    // Emit only when the committed text actually grows (transcribe-cpp already
    // gates via `committed_changed`; the extra string compare guards against
    // no-op churn so the pill isn't woken for nothing).
    let mut last_committed = String::new();
    loop {
        match next() {
            FrameCmd::Frame(f) => {
                let update = stream
                    .feed(f.samples.as_ref())
                    .map_err(|e| anyhow::anyhow!("stream feed failed: {e}"))?;
                if update.committed_changed {
                    let committed = stream.text().committed;
                    if committed != last_committed {
                        last_committed = committed.clone();
                        emit(DaemonEvent::AsrStreamText {
                            session_id,
                            committed,
                        });
                    }
                }
            }
            FrameCmd::Stop => break,
        }
    }

    // Finalize flushes buffered audio; the committed prefix then holds the full
    // transcript.
    stream
        .finalize()
        .map_err(|e| anyhow::anyhow!("stream finalize failed: {e}"))?;
    let final_text = stream.text().committed;
    emit(DaemonEvent::AsrStreamText {
        session_id,
        committed: final_text.clone(),
    });
    emit(DaemonEvent::AsrSessionFinal {
        session_id,
        text: final_text.clone(),
    });
    Ok(final_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    /// End-to-end streaming smoke test against a REAL GGUF model. Skips unless
    /// both env vars are set (so CI without a model still passes):
    ///   GRAIN_TC_GGUF = path to a streaming .gguf
    ///   GRAIN_TC_WAV  = path to a 16 kHz mono wav of speech
    /// The ggml backend DLLs must sit next to the test binary (copy them from the
    /// build profile dir first — see the sherpa smoke test note).
    #[test]
    fn streams_a_real_gguf_when_present() {
        let (Ok(gguf), Ok(wav)) =
            (std::env::var("GRAIN_TC_GGUF"), std::env::var("GRAIN_TC_WAV"))
        else {
            eprintln!("GRAIN_TC_GGUF/GRAIN_TC_WAV unset — skipping transcribe-cpp smoke test");
            return;
        };
        let _ = transcribe_cpp::init_backends_default();

        let mut reader = hound::WavReader::open(&wav).expect("open wav");
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max)
                    .collect()
            }
        };
        // Feed in 100 ms chunks like the live capture would, then Stop.
        let rate = spec.sample_rate;
        let chunk = (rate as usize / 10).max(1);
        let mut chunks: std::collections::VecDeque<Vec<f32>> =
            samples.chunks(chunk).map(|c| c.to_vec()).collect();
        let next = move || match chunks.pop_front() {
            Some(c) => FrameCmd::Frame(AudioFrame::new(Arc::from(c.into_boxed_slice()), rate)),
            None => FrameCmd::Stop,
        };

        let final_text =
            drive_stream(Path::new(&gguf), None, 1, next, |_ev| {}).expect("drive_stream");
        eprintln!("TRANSCRIBE_CPP_TRANSCRIPT: {final_text}");
        assert!(
            !final_text.trim().is_empty(),
            "real speech must produce a non-empty transcript"
        );
    }
}
