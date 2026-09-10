//! [GRAIN] Stateless Parakeet TDT v2/v3 window adapter for Flow.
//!
//! Grain owns the append-only journal and window cursor. Every native call is
//! an isolated utterance; only model-native token ids and absolute encoder
//! timestamps cross window boundaries. This intentionally contains no generic
//! rolling policy, VAD segmentation, text seam repair, or native state carry.

use grain_tdt::{merge_tokens, Token, Window, WindowCursor, FRAME_SAMPLES, SAMPLE_RATE};
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
    stable_tokens: Vec<Token>,
    cached_sample_count: Option<u64>,
    cached_text: String,
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
            stable_tokens: Vec::new(),
            cached_sample_count: None,
            cached_text: String::new(),
            decoded_windows: 0,
        })
    }

    pub(crate) fn decoded_windows(&self) -> usize {
        self.decoded_windows
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
        while let Some(window) = self.cursor.next_stable(total_samples) {
            let tokens = decode_window(session, config, journal, reader, audio, window)?;
            merge_tokens(&mut self.stable_tokens, &tokens);
            if !self.cursor.commit(window) {
                return Err("Flow window cursor rejected its own stable window".into());
            }
            self.decoded_windows += 1;
            self.cached_sample_count = None;
            self.cached_text.clear();
        }
        Ok(())
    }

    /// Render the exact accepted prefix. Reuses an equal-count preview at stop,
    /// avoiding all native work when no samples arrived after the last preview.
    pub(crate) fn render(
        &mut self,
        session: &mut Session,
        config: &TdtRunConfig,
        journal: &PcmJournal,
        reader: &mut PcmJournalReader,
        audio: &mut Vec<f32>,
        total_samples: u64,
    ) -> Result<String, String> {
        if self.cached_sample_count == Some(total_samples) {
            return Ok(self.cached_text.clone());
        }

        self.process_stable(session, config, journal, reader, audio, total_samples)?;
        let mut merged = self.stable_tokens.clone();
        if let Some(window) = self.cursor.tail(total_samples) {
            let tail = decode_window(session, config, journal, reader, audio, window)?;
            merge_tokens(&mut merged, &tail);
            self.decoded_windows += 1;
        }
        if merged.len() > 1 {
            merged.sort_by_key(|token| token.frame);
        }
        let ids: Vec<i32> = merged.iter().map(|token| token.id).collect();
        let text = session
            .model()
            .detokenize(&ids)
            .map_err(|error| format!("Flow token detokenization failed: {error}"))?
            .trim()
            .to_string();
        self.cached_sample_count = Some(total_samples);
        self.cached_text.clone_from(&text);
        Ok(text)
    }
}

fn decode_window(
    session: &mut Session,
    config: &TdtRunConfig,
    journal: &PcmJournal,
    reader: &mut PcmJournalReader,
    audio: &mut Vec<f32>,
    window: Window,
) -> Result<Vec<Token>, String> {
    journal
        .flush()
        .map_err(|error| format!("Flow journal flush failed: {error}"))?;
    reader
        .read_f32_range(window.read_start, window.read_end, audio)
        .map_err(|error| format!("Flow journal read failed: {error}"))?;
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

    transcript
        .tokens
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
}
