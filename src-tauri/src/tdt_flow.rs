//! [GRAIN] Bounded Parakeet TDT window adapter for Flow.
//!
//! Grain owns the append-only journal and window cursor. Every native call is
//! an isolated utterance. V2 uses Batch's ordinary decoder with bounded native
//! windows and quiet cuts near their ceiling. V3 retains its reviewed Fluid
//! window contract. Only native token IDs/timestamps cross window boundaries.

use grain_tdt::{
    merge_tokens, NativeWindowCursor, Token, Window, WindowCursor, FRAME_SAMPLES,
    NATIVE_MAX_SAMPLES, SAMPLE_RATE,
};
use transcribe_cpp::{
    ExtSlot, ParakeetTdtWindowOptions, RunExtension, RunOptions, Session, TimestampKind,
};

use crate::grain_audio_journal::{PcmJournal, PcmJournalReader};

const ENCODER_FRAME_MS: i64 = FRAME_SAMPLES as i64 * 1_000 / SAMPLE_RATE as i64;

pub(crate) fn validate_model(model_id: &str, translate_to_english: bool) -> Result<(), String> {
    if translate_to_english {
        return Err("Flow does not support translation; select transcription mode".into());
    }
    if !grain_core::capture::is_reviewed_flow_model(model_id) {
        return Err("Flow requires a reviewed Parakeet TDT 0.6B v2 or v3 GGUF model".into());
    }
    Ok(())
}

pub(crate) fn validate_model_installation(is_downloaded: bool) -> Result<(), String> {
    if !is_downloaded {
        return Err("Flow requires its selected Parakeet TDT model to be downloaded".into());
    }
    Ok(())
}

pub(crate) struct TdtRunConfig {
    pub(crate) model_id: String,
    pub(crate) language: Option<String>,
}

/// One recording's bounded token accumulator. The only growing allocation is
/// the transcript token sequence; audio remains in the file-backed journal.
pub(crate) struct TdtAccumulator {
    cursor: WindowCursor,
    native_cursor: Option<NativeWindowCursor>,
    stable_tokens: Vec<Token>,
    decoded_windows: usize,
}

impl TdtAccumulator {
    pub(crate) fn new(session: &Session, config: &TdtRunConfig) -> Result<Self, String> {
        let model = session.model();
        let variant = model.variant();
        if model.arch() != "parakeet"
            || !matches!(variant.as_str(), "tdt-0.6b-v2" | "tdt-0.6b-v3")
            || !model.accepts_ext(ExtSlot::Run, ParakeetTdtWindowOptions::KIND)
        {
            return Err(format!(
                "loaded model '{}' does not expose the reviewed Parakeet TDT Flow contract",
                config.model_id
            ));
        }
        Ok(Self {
            cursor: WindowCursor::default(),
            native_cursor: (variant == "tdt-0.6b-v2").then(NativeWindowCursor::default),
            stable_tokens: Vec::new(),
            decoded_windows: 0,
        })
    }

    pub(crate) fn decoded_windows(&self) -> usize {
        self.decoded_windows
    }

    pub(crate) fn max_input_samples(&self) -> usize {
        if self.native_cursor.is_some() {
            NATIVE_MAX_SAMPLES as usize
        } else {
            grain_tdt::MAX_MODEL_SAMPLES as usize
        }
    }

    /// Finalize every window that now has real lookahead. A successful merge
    /// precedes cursor commit, so native/journal failures never skip audio.
    pub(crate) fn process_stable(
        &mut self,
        session: &mut Session,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<(), String> {
        if let Some(cursor) = &mut self.native_cursor {
            while let Some(input) = cursor.next_stable(total_samples) {
                read_window(journal, reader, audio, input)?;
                let window = cursor
                    .prefer_pause(input, audio)
                    .ok_or_else(|| "Flow native cursor rejected its input buffer".to_string())?;
                audio.truncate(window.input_samples());
                let tokens = decode_native_window(session, config, audio, window)?;
                merge_tokens(&mut self.stable_tokens, &tokens);
                if !cursor.commit(window) {
                    return Err("Flow native cursor rejected its stable window".into());
                }
                self.decoded_windows += 1;
            }
            return Ok(());
        }
        while let Some(window) = self.cursor.next_stable(total_samples) {
            let tokens = decode_window(session, config, journal, reader, audio, window)?;
            merge_tokens(&mut self.stable_tokens, &tokens);
            if !self.cursor.commit(window) {
                return Err("Flow window cursor rejected its own stable window".into());
            }
            self.decoded_windows += 1;
        }
        Ok(())
    }

    /// Render the final transcript from stable windows and the remaining tail.
    pub(crate) fn render(
        &mut self,
        session: &mut Session,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<String, String> {
        self.process_stable(session, config, journal, reader, audio, total_samples)?;
        let mut merged = self.stable_tokens.clone();
        let tail_window = match &self.native_cursor {
            Some(cursor) => cursor.tail(total_samples),
            None => self.cursor.tail(total_samples),
        };
        if let Some(window) = tail_window {
            let tail = if self.native_cursor.is_some() {
                read_window(journal, reader, audio, window)?;
                decode_native_window(session, config, audio, window)?
            } else {
                decode_window(session, config, journal, reader, audio, window)?
            };
            merge_tokens(&mut merged, &tail);
            self.decoded_windows += 1;
        }
        if merged.len() > 1 {
            merged.sort_by_key(|token| token.frame);
        }
        let ids: Vec<i32> = merged.iter().map(|token| token.id).collect();
        Ok(session
            .model()
            .detokenize(&ids)
            .map_err(|error| format!("Flow token detokenization failed: {error}"))?
            .trim()
            .to_string())
    }
}

fn read_window(
    journal: &PcmJournal,
    reader: &mut PcmJournalReader,
    audio: &mut Vec<f32>,
    window: Window,
) -> Result<(), String> {
    journal
        .flush()
        .map_err(|error| format!("Flow journal flush failed: {error}"))?;
    reader
        .read_f32_range(window.read_start, window.read_end, audio)
        .map_err(|error| format!("Flow journal read failed: {error}"))?;
    Ok(())
}

fn decode_native_window(
    session: &mut Session,
    config: &TdtRunConfig,
    audio: &[f32],
    window: Window,
) -> Result<Vec<Token>, String> {
    // Ordinary Batch policy: no extra padding, context filtering, tail loop,
    // custom SOS or fixed token budget. Every window starts a fresh utterance.
    let options = RunOptions {
        timestamps: TimestampKind::Token,
        language: config.language.clone(),
        ..Default::default()
    };
    let frame_offset = window.read_start / FRAME_SAMPLES;
    // Validate the global bound before expensive inference.
    i32::try_from(window.read_end.div_ceil(FRAME_SAMPLES))
        .map_err(|_| "Flow recording is too long for native timestamps".to_string())?;
    let transcript = session.run(audio, &options).map_err(|error| {
        format!(
            "Flow native decode failed for samples [{}..{}]: {error}",
            window.read_start, window.read_end
        )
    })?;
    convert_tokens(transcript.tokens, frame_offset)
}

fn decode_window(
    session: &mut Session,
    config: &TdtRunConfig,
    journal: &PcmJournal,
    reader: &mut PcmJournalReader,
    audio: &mut Vec<f32>,
    window: Window,
) -> Result<Vec<Token>, String> {
    read_window(journal, reader, audio, window)?;
    audio.resize(window.encoder_input_samples(), 0.0);

    let decode_start_frame = i32::try_from(window.context_frames())
        .map_err(|_| "Flow context frame exceeds native range".to_string())?;
    let decode_end_frame = i32::try_from(window.content_frames())
        .map_err(|_| "Flow content frame exceeds native range".to_string())?;
    let timestamp_offset_frames = i32::try_from(window.global_frame_offset())
        .map_err(|_| "Flow recording is too long for native timestamps".to_string())?;
    let options = RunOptions {
        timestamps: TimestampKind::Token,
        language: config.language.clone(),
        family: Some(RunExtension::ParakeetTdtWindow(ParakeetTdtWindowOptions {
            decode_start_frame,
            decode_end_frame,
            timestamp_offset_frames,
            finalize_tail: window.finalize_decoder(),
        })),
        ..Default::default()
    };
    let transcript = session.run(audio, &options).map_err(|error| {
        format!(
            "Flow decode failed for samples [{}..{}] (content starts {}, final={}): {error}",
            window.read_start, window.read_end, window.chunk_start, window.is_last
        )
    })?;

    convert_tokens(transcript.tokens, 0)
}

fn convert_tokens(
    tokens: Vec<transcribe_cpp::Token>,
    frame_offset: u64,
) -> Result<Vec<Token>, String> {
    tokens
        .into_iter()
        .map(|token| {
            let duration_ms = token.t1_ms.saturating_sub(token.t0_ms);
            if token.t0_ms < 0
                || token.t1_ms < token.t0_ms
                || token.t0_ms % ENCODER_FRAME_MS != 0
                || duration_ms % ENCODER_FRAME_MS != 0
                || duration_ms / ENCODER_FRAME_MS > 4
            {
                return Err("Flow native token carried non-encoder-aligned timing".to_string());
            }
            Ok(Token {
                id: token.id,
                frame: frame_offset
                    .checked_add((token.t0_ms / ENCODER_FRAME_MS) as u64)
                    .ok_or_else(|| "Flow absolute token timestamp overflowed".to_string())?,
                confidence: token.p,
                duration: (duration_ms / ENCODER_FRAME_MS) as u32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local fixtures contain private dictation. Supply model/module paths and
    /// a JSON array of {path, expected} cases; nothing is downloaded or printed.
    #[test]
    #[ignore = "requires local Parakeet v2 model and explicitly supplied WAV fixtures"]
    fn real_model_journal_partition_and_fresh_replay_match_offline_policy() {
        use transcribe_cpp::{Backend, Model, ModelOptions, SessionOptions};
        let model_path = std::env::var("GRAIN_FLOW_TEST_MODEL").unwrap();
        transcribe_cpp::disable_logging();
        transcribe_cpp::init_backends(std::env::var("GRAIN_FLOW_TEST_NATIVE_DIR").unwrap())
            .unwrap();
        let model = Model::load_with(
            model_path,
            &ModelOptions {
                backend: Backend::Cpu,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(model.variant(), "tdt-0.6b-v2");
        let mut session = model
            .session_with(&SessionOptions {
                n_threads: 4,
                ..Default::default()
            })
            .unwrap();
        let cases: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(std::env::var("GRAIN_FLOW_TEST_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        assert!(!cases.is_empty());
        let config = TdtRunConfig {
            model_id: "fixture-v2".into(),
            language: None,
        };
        for case in cases {
            let mut wav = hound::WavReader::open(case["path"].as_str().unwrap()).unwrap();
            assert_eq!(wav.spec().sample_rate, 16_000);
            assert_eq!(wav.spec().channels, 1);
            assert_eq!(wav.spec().bits_per_sample, 16);
            let pcm: Vec<f32> = wav
                .samples::<i16>()
                .map(|s| s.unwrap() as f32 / 32768.0)
                .collect();
            for incremental in [true, false] {
                let journal = PcmJournal::create().unwrap();
                {
                    let mut reader = journal.reader().unwrap();
                    let mut accumulator = TdtAccumulator::new(&session, &config).unwrap();
                    assert_eq!(accumulator.max_input_samples(), NATIVE_MAX_SAMPLES as usize);
                    let mut audio = Vec::with_capacity(accumulator.max_input_samples());
                    for block in pcm.chunks(8_001) {
                        journal.append(block).unwrap();
                        if incremental {
                            accumulator
                                .process_stable(
                                    &mut session,
                                    &config,
                                    &journal,
                                    &mut reader,
                                    &mut audio,
                                    journal.frame_count(),
                                )
                                .unwrap();
                        }
                    }
                    journal.close().unwrap();
                    let text = accumulator
                        .render(
                            &mut session,
                            &config,
                            &journal,
                            &mut reader,
                            &mut audio,
                            journal.frame_count(),
                        )
                        .unwrap();
                    assert!(
                        text == case["expected"].as_str().unwrap(),
                        "journal result differs from offline policy"
                    );
                    assert!(audio.capacity() <= NATIVE_MAX_SAMPLES as usize);
                }
                drop(journal);
            }
        }
    }

    #[test]
    fn routing_is_exact_and_translation_is_rejected() {
        const REVIEWED_QUANTIZATIONS: &[&str] = &["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0", "F16", "F32"];
        for version in ["v2", "v3"] {
            for quant in REVIEWED_QUANTIZATIONS {
                let model = format!("handy-computer/parakeet-tdt-0.6b-{version}-gguf/parakeet-tdt-0.6b-{version}-{quant}.gguf");
                assert!(validate_model(&model, false).is_ok(), "rejected {model}");
                assert!(validate_model(&model, true).is_err());
            }
        }
        for model in [
            "handy-computer/parakeet-tdt-0.6b-v1-gguf/parakeet-tdt-0.6b-v1-Q8_0.gguf",
            "local/parakeet-tdt-0.6b-v3-Q8_0.gguf",
            "handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q8_0.bin",
            "handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q2_K.gguf",
        ] {
            assert!(validate_model(model, false).is_err(), "accepted {model}");
        }
    }

    #[test]
    fn model_must_be_installed_before_capture() {
        assert!(validate_model_installation(true).is_ok());
        assert!(validate_model_installation(false).is_err());
    }

    #[test]
    fn encoder_frame_duration_is_exactly_eighty_milliseconds() {
        assert_eq!(ENCODER_FRAME_MS, 80);
    }

    #[test]
    fn native_window_timestamps_preserve_absolute_phase_and_metadata() {
        let token = transcribe_cpp::Token {
            id: 42,
            p: 0.75,
            t0_ms: 160,
            t1_ms: 480,
            ..Default::default()
        };
        let result = convert_tokens(vec![token.clone()], 350).unwrap();
        assert_eq!(result[0].frame, 352);
        assert_eq!(result[0].id, 42);
        assert_eq!(result[0].duration, 4);
        assert_eq!(result[0].confidence, 0.75);
        assert_eq!(convert_tokens(vec![token], 0).unwrap()[0].frame, 2);
    }

    #[test]
    fn invalid_native_timing_and_absolute_overflow_are_rejected() {
        for (start, end) in [(-80, 0), (160, 80), (1, 81), (0, 1), (0, 400)] {
            assert!(convert_tokens(
                vec![transcribe_cpp::Token {
                    t0_ms: start,
                    t1_ms: end,
                    ..Default::default()
                }],
                0
            )
            .is_err());
        }
        assert!(convert_tokens(
            vec![transcribe_cpp::Token {
                t0_ms: 80,
                t1_ms: 80,
                ..Default::default()
            }],
            u64::MAX
        )
        .is_err());
    }
}
