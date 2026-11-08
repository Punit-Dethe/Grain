//! [GRAIN] Bounded Parakeet TDT window adapter for Flow.
//!
//! Grain owns the journal and geometry. Both reviewed models carry native TDT
//! decoder state across owned frame ranges and detokenize one token sequence.

use grain_tdt::{
    BufferedCursor, Token, Window, BUFFERED_MAX_SAMPLES, BUFFERED_OWNED_FRAMES, FRAME_SAMPLES,
    SAMPLE_RATE,
};
use transcribe_cpp::{ParakeetTdtBufferedOptions, RunExtension, RunOptions, TimestampKind};

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
                ParakeetTdtBufferedOptions::KIND,
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

/// Native evidence for one owned decoding window. Native word/token indices
/// remain local to `transcript`; timestamps are absolute analysis milliseconds.
/// A word at a window edge may be incomplete: these rows are evidence, not a
/// canonical meeting transcript. Never concatenate window text to build words.
#[derive(Debug)]
pub(crate) struct WindowEvidence {
    pub window: Window,
    pub transcript: transcribe_cpp::Transcript,
}

/// The reviewed full-sequence detokenization plus optional native evidence.
#[derive(Debug)]
pub(crate) struct StructuredResult {
    pub text: String,
    pub windows: Vec<WindowEvidence>,
}

/// One recording's bounded-audio token accumulator. Transcript tokens and, when
/// explicitly requested, native evidence grow with output; audio stays on disk.
pub(crate) struct TdtAccumulator {
    cursor: BufferedCursor,
    stable_tokens: Vec<Token>,
    decoded_windows: usize,
    evidence: Option<Vec<WindowEvidence>>,
    finished: bool,
}

impl TdtAccumulator {
    pub(crate) fn new(session: &dyn Inference, config: &TdtRunConfig) -> Result<Self, String> {
        Self::create(session, config, false)
    }

    /// Opt in before decoding any audio. Normal dictation retains only compact
    /// tokens, avoiding the extra native strings and word rows.
    pub(crate) fn new_structured(
        session: &dyn Inference,
        config: &TdtRunConfig,
    ) -> Result<Self, String> {
        Self::create(session, config, true)
    }

    fn create(
        session: &dyn Inference,
        config: &TdtRunConfig,
        retain_evidence: bool,
    ) -> Result<Self, String> {
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
            cursor: BufferedCursor::default(),
            stable_tokens: Vec::new(),
            decoded_windows: 0,
            evidence: retain_evidence.then(Vec::new),
            finished: false,
        })
    }

    pub(crate) fn decoded_windows(&self) -> usize {
        self.decoded_windows
    }

    pub(crate) fn max_input_samples(&self) -> usize {
        BUFFERED_MAX_SAMPLES as usize
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
        if self.finished {
            return Err("Flow accumulator is finalized or failed; create a fresh replay".into());
        }
        while let Some(window) = self.cursor.next_stable(total_samples) {
            // Any failed call may have advanced the native decoder. Poison this
            // accumulator until the caller creates a fresh, reset replay.
            self.finished = true;
            read_window(journal, reader, audio, window)?;
            let transcript = decode_buffered_window(session, config, audio, window)?;
            self.append_window(window, transcript)?;
            if !self.cursor.commit(window) {
                return Err("Flow window cursor rejected its own stable window".into());
            }
            self.decoded_windows += 1;
            self.finished = false;
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
        self.finish(session, config, journal, reader, audio, total_samples)
            .map(|result| result.text)
    }

    pub(crate) fn render_structured(
        &mut self,
        session: &mut dyn Inference,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<StructuredResult, String> {
        if self.evidence.is_none() {
            return Err("Structured output requires evidence collection before decoding".into());
        }
        self.finish(session, config, journal, reader, audio, total_samples)
    }

    fn append_window(
        &mut self,
        window: Window,
        transcript: transcribe_cpp::Transcript,
    ) -> Result<(), String> {
        // Validate the entire batch before committing any of it.
        let tokens = transcript
            .tokens
            .iter()
            .map(convert_token)
            .collect::<Result<Vec<_>, _>>()?;
        self.stable_tokens.extend(tokens);
        if let Some(evidence) = &mut self.evidence {
            evidence.push(WindowEvidence { window, transcript });
        }
        Ok(())
    }

    fn finish(
        &mut self,
        session: &mut dyn Inference,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<StructuredResult, String> {
        self.process_stable(session, config, journal, reader, audio, total_samples)?;
        // A tail advances native decoder state. Success or failure, finalization
        // is single-use; retry with a fresh accumulator and reset native state.
        self.finished = true;
        if let Some(window) = self.cursor.tail(total_samples) {
            read_window(journal, reader, audio, window)?;
            let transcript = decode_buffered_window(session, config, audio, window)?;
            self.append_window(window, transcript)?;
            self.decoded_windows += 1;
        }
        let ids: Vec<i32> = self.stable_tokens.iter().map(|token| token.id).collect();
        let text = session
            .detokenize(&ids)
            .map_err(|error| format!("Flow token detokenization failed: {error}"))?
            .trim()
            .to_string();
        self.stable_tokens = Vec::new();
        Ok(StructuredResult {
            text,
            windows: self.evidence.take().unwrap_or_default(),
        })
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
) -> Result<transcribe_cpp::Transcript, String> {
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
    Ok(transcript)
}

#[cfg(test)]
fn convert_tokens(tokens: Vec<transcribe_cpp::Token>) -> Result<Vec<Token>, String> {
    tokens.iter().map(convert_token).collect()
}

fn convert_token(token: &transcribe_cpp::Token) -> Result<Token, String> {
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
        let expected_capacity = BUFFERED_MAX_SAMPLES as usize;
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
        variant: Option<&'static str>,
    }
    impl Inference for RecordingInference {
        fn flow_model(&self) -> Result<FlowModel, String> {
            Ok(FlowModel {
                arch: "parakeet".into(),
                variant: self.variant.unwrap_or("tdt-0.6b-v2").into(),
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
                panic!("expected buffered TDT")
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
    fn structured_output_preserves_window_evidence_and_full_sequence_text() {
        for incremental in [false, true] {
            let journal = PcmJournal::create().unwrap();
            let mut reader = journal.reader().unwrap();
            let mut session = RecordingInference::default();
            let config = TdtRunConfig {
                model_id: "fixture".into(),
                language: None,
            };
            let mut accumulator = TdtAccumulator::new_structured(&session, &config).unwrap();
            let mut audio = Vec::with_capacity(accumulator.max_input_samples());
            for block in vec![0.0; 400_001].chunks(8001) {
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
            let result = accumulator
                .render_structured(
                    &mut session,
                    &config,
                    &journal,
                    &mut reader,
                    &mut audio,
                    journal.frame_count(),
                )
                .unwrap();
            assert_eq!(result.text, "42,42,42");
            assert_eq!(result.windows.len(), 3);
            for (i, evidence) in result.windows.iter().enumerate() {
                assert_eq!(evidence.window.chunk_start, i as u64 * 160_000);
                assert_eq!(evidence.transcript.tokens[0].t0_ms, i as i64 * 10_000);
                assert_eq!(evidence.transcript.tokens[0].p, 0.8);
            }
            assert!(audio.capacity() <= BUFFERED_MAX_SAMPLES as usize);
            let calls = session.calls.len();
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
            assert_eq!(session.calls.len(), calls);
        }
    }

    #[test]
    fn evidence_retains_point_tokens_unicode_and_local_word_membership() {
        let session = RecordingInference::default();
        let config = TdtRunConfig {
            model_id: "fixture".into(),
            language: None,
        };
        let mut accumulator = TdtAccumulator::new_structured(&session, &config).unwrap();
        let window = BufferedCursor::default().tail(1280).unwrap();
        let transcript = transcribe_cpp::Transcript {
            text: "é".into(),
            raw_text: " é".into(),
            tokens: vec![transcribe_cpp::Token {
                id: 42,
                p: f32::NAN,
                t0_ms: 0,
                t1_ms: 0,
                word_index: 0,
                seg_index: 0,
                text: "é".into(),
            }],
            words: vec![transcribe_cpp::Word {
                text: "é".into(),
                first_token: 0,
                n_tokens: 1,
                ..Default::default()
            }],
            ..Default::default()
        };
        accumulator.append_window(window, transcript).unwrap();
        let evidence = &accumulator.evidence.as_ref().unwrap()[0].transcript;
        assert_eq!(evidence.raw_text, " é");
        assert_eq!(evidence.tokens[0].text, "é");
        assert_eq!(evidence.tokens[0].word_index, 0);
        assert!(evidence.tokens[0].p.is_nan());
        assert_eq!(evidence.words[0].n_tokens, 1);
        assert_eq!(accumulator.stable_tokens[0].duration, 0);
    }

    #[test]
    fn invalid_batch_commits_no_partial_tokens_or_evidence() {
        let session = RecordingInference::default();
        let config = TdtRunConfig {
            model_id: "fixture".into(),
            language: None,
        };
        let mut accumulator = TdtAccumulator::new_structured(&session, &config).unwrap();
        let window = BufferedCursor::default().tail(1280).unwrap();
        let transcript = transcribe_cpp::Transcript {
            tokens: vec![
                transcribe_cpp::Token::default(),
                transcribe_cpp::Token {
                    t0_ms: 1,
                    t1_ms: 81,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(accumulator.append_window(window, transcript).is_err());
        assert!(accumulator.stable_tokens.is_empty());
        assert!(accumulator.evidence.as_ref().unwrap().is_empty());
    }

    #[test]
    fn failed_decode_requires_fresh_replay_without_reentering_native_state() {
        for total in [1280, 400_001] {
            let journal = PcmJournal::create().unwrap();
            journal.append(&vec![0.0; total]).unwrap();
            journal.close().unwrap();
            let mut reader = journal.reader().unwrap();
            let mut session = RecordingInference {
                fail_call: Some(1),
                ..Default::default()
            };
            let config = TdtRunConfig {
                model_id: "fixture".into(),
                language: None,
            };
            let mut accumulator = TdtAccumulator::new_structured(&session, &config).unwrap();
            let mut audio = Vec::new();
            assert!(accumulator
                .render_structured(
                    &mut session,
                    &config,
                    &journal,
                    &mut reader,
                    &mut audio,
                    journal.frame_count()
                )
                .is_err());
            let calls = session.calls.len();
            assert!(accumulator
                .render_structured(
                    &mut session,
                    &config,
                    &journal,
                    &mut reader,
                    &mut audio,
                    journal.frame_count()
                )
                .is_err());
            assert_eq!(session.calls.len(), calls);
        }
    }

    #[test]
    fn empty_structured_recording_has_no_native_runs_or_fabricated_evidence() {
        let journal = PcmJournal::create().unwrap();
        journal.close().unwrap();
        let mut reader = journal.reader().unwrap();
        let mut session = RecordingInference::default();
        let config = TdtRunConfig {
            model_id: "fixture".into(),
            language: None,
        };
        let mut accumulator = TdtAccumulator::new_structured(&session, &config).unwrap();
        let result = accumulator
            .render_structured(
                &mut session,
                &config,
                &journal,
                &mut reader,
                &mut Vec::new(),
                0,
            )
            .unwrap();
        assert!(result.text.is_empty());
        assert!(result.windows.is_empty());
        assert!(session.calls.is_empty());
    }

    #[test]
    fn compact_flow_cannot_claim_structured_evidence() {
        let journal = PcmJournal::create().unwrap();
        let mut reader = journal.reader().unwrap();
        let mut session = RecordingInference::default();
        let config = TdtRunConfig {
            model_id: "fixture".into(),
            language: None,
        };
        let mut accumulator = TdtAccumulator::new(&session, &config).unwrap();
        assert!(accumulator
            .render_structured(
                &mut session,
                &config,
                &journal,
                &mut reader,
                &mut Vec::new(),
                0
            )
            .is_err());
        assert!(!accumulator.finished);
        assert!(session.calls.is_empty());
    }

    #[test]
    fn buffered_journal_owns_frames_once_and_replay_resets_native_state() {
        for variant in ["tdt-0.6b-v2", "tdt-0.6b-v3"] {
            check_buffered_journal_replay(variant);
        }
    }

    fn check_buffered_journal_replay(variant: &'static str) {
        let journal = PcmJournal::create().unwrap();
        journal.append(&vec![0.0; 400_001]).unwrap();
        journal.close().unwrap();
        let mut reader = journal.reader().unwrap();
        let mut session = RecordingInference {
            fail_call: Some(2),
            variant: Some(variant),
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
