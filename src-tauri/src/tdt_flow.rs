//! [GRAIN] Bounded Parakeet TDT window adapter for Flow.
//!
//! Grain owns the journal and geometry. V2 carries native TDT decoder state
//! across owned frame ranges; v3 retains the reviewed Fluid window/merge policy.

use grain_tdt::{
    merge_tokens, BufferedCursor, Token, Window, WindowCursor, BUFFERED_MAX_SAMPLES,
    BUFFERED_OWNED_FRAMES, FRAME_SAMPLES, SAMPLE_RATE,
};
use transcribe_cpp::{
    ParakeetTdtBufferedOptions, ParakeetTdtWindowOptions, RunExtension, RunOptions, TimestampKind,
};

use crate::grain_audio_journal::{PcmJournal, PcmJournalReader};

const ENCODER_FRAME_MS: i64 = FRAME_SAMPLES as i64 * 1_000 / SAMPLE_RATE as i64;

pub(crate) struct FlowModel {
    pub arch: String,
    pub variant: String,
    pub supported: bool,
}

/// The production adapter sends bounded runs to the existing model worker.
pub(crate) trait Inference {
    fn flow_model(&self) -> Result<FlowModel, String>;
    fn run(
        &mut self,
        audio: &[f32],
        options: &RunOptions,
    ) -> Result<transcribe_cpp::Transcript, String>;
    fn detokenize(&self, tokens: &[i32]) -> Result<String, String>;
}

#[cfg(test)]
impl Inference for transcribe_cpp::Session {
    fn flow_model(&self) -> Result<FlowModel, String> {
        let model = self.model();
        Ok(FlowModel {
            arch: model.arch(),
            variant: model.variant(),
            supported: model.accepts_ext(
                transcribe_cpp::ExtSlot::Run,
                if model.variant() == "tdt-0.6b-v2" {
                    ParakeetTdtBufferedOptions::KIND
                } else {
                    ParakeetTdtWindowOptions::KIND
                },
            ),
        })
    }
    fn run(
        &mut self,
        audio: &[f32],
        options: &RunOptions,
    ) -> Result<transcribe_cpp::Transcript, String> {
        transcribe_cpp::Session::run(self, audio, options).map_err(|error| error.to_string())
    }
    fn detokenize(&self, tokens: &[i32]) -> Result<String, String> {
        self.model()
            .detokenize(tokens)
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn validate_model(model_id: &str, translate_to_english: bool) -> Result<(), String> {
    if translate_to_english {
        return Err("Flow does not support translation; select transcription mode".into());
    }
    if !grain_core::capture::is_reviewed_flow_model(model_id) {
        return Err("Flow requires a reviewed Parakeet TDT 0.6B v2 or v3 GGUF model".into());
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
    buffered_cursor: Option<BufferedCursor>,
    stable_tokens: Vec<Token>,
    decoded_windows: usize,
}

impl TdtAccumulator {
    pub(crate) fn new(session: &dyn Inference, config: &TdtRunConfig) -> Result<Self, String> {
        let model = session.flow_model()?;
        let variant = model.variant;
        if model.arch != "parakeet"
            || !matches!(variant.as_str(), "tdt-0.6b-v2" | "tdt-0.6b-v3")
            || !model.supported
        {
            return Err(format!(
                "loaded model '{}' does not expose the reviewed Parakeet TDT Flow contract",
                config.model_id
            ));
        }
        Ok(Self {
            cursor: WindowCursor::default(),
            buffered_cursor: (variant == "tdt-0.6b-v2").then(BufferedCursor::default),
            stable_tokens: Vec::new(),
            decoded_windows: 0,
        })
    }

    pub(crate) fn decoded_windows(&self) -> usize {
        self.decoded_windows
    }

    pub(crate) fn max_input_samples(&self) -> usize {
        if self.buffered_cursor.is_some() {
            BUFFERED_MAX_SAMPLES as usize
        } else {
            grain_tdt::MAX_MODEL_SAMPLES as usize
        }
    }

    /// Finalize every window that now has real lookahead. Successful accumulation
    /// precedes cursor commit, so native/journal failures never skip audio.
    pub(crate) fn process_stable(
        &mut self,
        session: &mut dyn Inference,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<(), String> {
        if let Some(cursor) = &mut self.buffered_cursor {
            while let Some(window) = cursor.next_stable(total_samples) {
                read_window(journal, reader, audio, window)?;
                let tokens = decode_buffered_window(session, config, audio, window)?;
                self.stable_tokens.extend(tokens);
                if !cursor.commit(window) {
                    return Err("Flow buffered cursor rejected its stable window".into());
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
        session: &mut dyn Inference,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<String, String> {
        self.process_stable(session, config, journal, reader, audio, total_samples)?;
        let mut merged = if self.buffered_cursor.is_some() {
            std::mem::take(&mut self.stable_tokens)
        } else {
            self.stable_tokens.clone()
        };
        let tail_window = match &self.buffered_cursor {
            Some(cursor) => cursor.tail(total_samples),
            None => self.cursor.tail(total_samples),
        };
        if let Some(window) = tail_window {
            let tail = if self.buffered_cursor.is_some() {
                read_window(journal, reader, audio, window)?;
                decode_buffered_window(session, config, audio, window)?
            } else {
                decode_window(session, config, journal, reader, audio, window)?
            };
            if self.buffered_cursor.is_some() {
                merged.extend(tail);
            } else {
                merge_tokens(&mut merged, &tail);
            }
            self.decoded_windows += 1;
        }
        if self.buffered_cursor.is_none() && merged.len() > 1 {
            merged.sort_by_key(|token| token.frame);
        }
        let ids: Vec<i32> = merged.iter().map(|token| token.id).collect();
        Ok(session
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

fn decode_buffered_window(
    session: &mut dyn Inference,
    config: &TdtRunConfig,
    audio: &[f32],
    window: Window,
) -> Result<Vec<Token>, String> {
    i32::try_from(window.read_end.div_ceil(FRAME_SAMPLES))
        .map_err(|_| "Flow recording is too long for native timestamps".to_string())?;
    let start =
        i32::try_from(window.context_frames()).map_err(|_| "Flow context exceeds native range")?;
    let offset = i32::try_from(window.chunk_start / FRAME_SAMPLES)
        .map_err(|_| "Flow offset exceeds native range")?;
    let options = RunOptions {
        timestamps: TimestampKind::Token,
        language: config.language.clone(),
        family: Some(RunExtension::ParakeetTdtBuffered(
            ParakeetTdtBufferedOptions {
                decode_start_frame: start,
                decode_end_frame: if window.is_last {
                    -1
                } else {
                    start + BUFFERED_OWNED_FRAMES as i32
                },
                timestamp_offset_frames: offset,
                reset: window.chunk_start == 0,
                finalize: window.is_last,
            },
        )),
        ..Default::default()
    };
    let transcript = session.run(audio, &options).map_err(|error| {
        format!(
            "Flow buffered decode failed for samples [{}..{}]: {error}",
            window.read_start, window.read_end
        )
    })?;
    convert_tokens(transcript.tokens)
}

fn decode_window(
    session: &mut dyn Inference,
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

    convert_tokens(transcript.tokens)
}

fn convert_tokens(tokens: Vec<transcribe_cpp::Token>) -> Result<Vec<Token>, String> {
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
                frame: (token.t0_ms / ENCODER_FRAME_MS) as u64,
                confidence: token.p,
                duration: (duration_ms / ENCODER_FRAME_MS) as u32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local fixtures may contain private dictation. Supply model/module paths and
    /// a JSON array of {path, expected} cases; nothing is downloaded or printed.
    #[test]
    #[ignore = "requires local Parakeet v2/v3 model and explicitly supplied WAV fixtures"]
    fn real_model_journal_partition_and_fresh_replay_match_offline_policy() {
        use transcribe_cpp::{Backend, Model, ModelOptions, SessionOptions};
        let model_path = std::env::var("GRAIN_FLOW_TEST_MODEL").unwrap();
        transcribe_cpp::disable_logging();
        transcribe_cpp::init_backends(std::env::var("GRAIN_FLOW_TEST_NATIVE_DIR").unwrap())
            .unwrap();
        let backend = match std::env::var("GRAIN_FLOW_TEST_BACKEND")
            .as_deref()
            .unwrap_or("cpu")
        {
            "cpu" => Backend::Cpu,
            "vulkan" => Backend::Vulkan,
            _ => panic!("expected cpu or vulkan"),
        };
        let model = Model::load_with(
            model_path,
            &ModelOptions {
                backend,
                ..Default::default()
            },
        )
        .unwrap();
        let variant = model.variant();
        assert!(matches!(variant.as_str(), "tdt-0.6b-v2" | "tdt-0.6b-v3"));
        let expected_capacity = if variant == "tdt-0.6b-v2" {
            BUFFERED_MAX_SAMPLES as usize
        } else {
            grain_tdt::MAX_MODEL_SAMPLES as usize
        };
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
            model_id: format!("fixture-{variant}"),
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
                    assert_eq!(accumulator.max_input_samples(), expected_capacity);
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
                    assert!(audio.capacity() <= expected_capacity);
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
    fn encoder_frame_duration_is_exactly_eighty_milliseconds() {
        assert_eq!(ENCODER_FRAME_MS, 80);
    }

    #[test]
    fn native_timestamps_preserve_metadata() {
        let token = transcribe_cpp::Token {
            id: 42,
            p: 0.75,
            t0_ms: 160,
            t1_ms: 480,
            ..Default::default()
        };
        let result = convert_tokens(vec![token]).unwrap();
        assert_eq!(result[0].frame, 2);
        assert_eq!(result[0].id, 42);
        assert_eq!(result[0].duration, 4);
        assert_eq!(result[0].confidence, 0.75);
    }

    #[test]
    fn invalid_native_timing_is_rejected() {
        for (start, end) in [(-80, 0), (160, 80), (1, 81), (0, 1), (0, 400)] {
            assert!(convert_tokens(vec![transcribe_cpp::Token {
                t0_ms: start,
                t1_ms: end,
                ..Default::default()
            }])
            .is_err());
        }
    }

    #[derive(Default)]
    struct RecordingInference {
        calls: Vec<(usize, RunOptions)>,
        fail_call: Option<usize>,
    }
    impl Inference for RecordingInference {
        fn flow_model(&self) -> Result<FlowModel, String> {
            Ok(FlowModel {
                arch: "parakeet".into(),
                variant: "tdt-0.6b-v2".into(),
                supported: true,
            })
        }
        fn run(
            &mut self,
            audio: &[f32],
            options: &RunOptions,
        ) -> Result<transcribe_cpp::Transcript, String> {
            self.calls.push((audio.len(), options.clone()));
            if self.fail_call == Some(self.calls.len()) {
                return Err("injected decode failure".into());
            }
            let Some(RunExtension::ParakeetTdtBuffered(ext)) = &options.family else {
                panic!("expected buffered v2")
            };
            let mut result = transcribe_cpp::Transcript::default();
            // Identical token IDs must survive across adjacent owned windows.
            result.tokens.push(transcribe_cpp::Token {
                id: 42,
                p: 0.8,
                t0_ms: i64::from(ext.timestamp_offset_frames) * 80,
                t1_ms: i64::from(ext.timestamp_offset_frames) * 80 + 80,
                ..Default::default()
            });
            Ok(result)
        }
        fn detokenize(&self, tokens: &[i32]) -> Result<String, String> {
            Ok(tokens
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(","))
        }
    }

    #[test]
    fn buffered_journal_owns_frames_once_and_replay_resets_native_state() {
        let journal = PcmJournal::create().unwrap();
        journal.append(&vec![0.0; 400_001]).unwrap();
        journal.close().unwrap();
        let mut reader = journal.reader().unwrap();
        let mut session = RecordingInference {
            fail_call: Some(2),
            ..Default::default()
        };
        let config = TdtRunConfig {
            model_id: "fixture-v2".into(),
            language: None,
        };
        let mut accumulator = TdtAccumulator::new(&session, &config).unwrap();
        let mut audio = Vec::with_capacity(accumulator.max_input_samples());
        assert!(accumulator
            .render(
                &mut session,
                &config,
                &journal,
                &mut reader,
                &mut audio,
                journal.frame_count()
            )
            .is_err());
        assert_eq!(accumulator.decoded_windows(), 1);
        // Production discards the failed accumulator and makes one clean replay.
        let mut replay = TdtAccumulator::new(&session, &config).unwrap();
        assert_eq!(
            replay
                .render(
                    &mut session,
                    &config,
                    &journal,
                    &mut reader,
                    &mut audio,
                    journal.frame_count()
                )
                .unwrap(),
            "42,42,42"
        );
        let replay_calls = &session.calls[2..];
        assert_eq!(
            replay_calls.iter().map(|(len, _)| *len).collect::<Vec<_>>(),
            [239_360, 399_360, 240_001]
        );
        for (i, (_, opts)) in replay_calls.iter().enumerate() {
            let Some(RunExtension::ParakeetTdtBuffered(ext)) = &opts.family else {
                panic!("wrong decoder")
            };
            assert_eq!(ext.timestamp_offset_frames, i as i32 * 125);
            assert_eq!(ext.decode_start_frame, if i == 0 { 0 } else { 125 });
            assert_eq!(
                ext.decode_end_frame,
                if i == 2 {
                    -1
                } else {
                    ext.decode_start_frame + 125
                }
            );
            assert_eq!(ext.reset, i == 0);
            assert_eq!(ext.finalize, i == 2);
        }
        assert!(audio.capacity() <= BUFFERED_MAX_SAMPLES as usize);
    }
}
