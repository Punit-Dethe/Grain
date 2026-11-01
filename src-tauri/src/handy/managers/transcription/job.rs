//! Background transcription, always chunked: chunks are the unit of
//! progress, of yielding to dictation, and of retry.

use anyhow::Result;
use log::{debug, info};
use std::time::Instant;
use transcribe_cpp::Transcript;

use super::{
    post_process_transcription_text, real_time_factor, BatchContext, TranscriptionManager,
};
use crate::audio_toolkit::chunking::Chunker;
use crate::managers::audio::create_detector;

/// Audio handed to the chunker at a time: the first chunk is transcribed
/// without scoring the whole file first. Cuts match the whole-file plan (a
/// rare short blip at a feed boundary can nudge one within its pause).
const FEED_SAMPLES: usize = 60 * 16_000;

/// What a background transcription asks of whoever runs it.
pub trait ChunkControl {
    /// Called before each feed to the chunker and before each chunk is
    /// decoded. Blocks while dictation is using the engine; `false` stops the
    /// transcription.
    fn proceed(&self) -> bool;
    /// `done` of `total` samples are transcribed.
    fn progress(&self, done: usize, total: usize);
}

impl TranscriptionManager {
    /// Transcribe `audio` chunk by chunk with the selected model, one
    /// paragraph per chunk. `None` if `control` stopped it.
    pub fn transcribe_in_chunks(
        &self,
        audio: Vec<f32>,
        control: &dyn ChunkControl,
    ) -> Result<Option<String>> {
        let started = Instant::now();
        let total = audio.len();
        if total == 0 {
            return Ok(Some(String::new()));
        }

        self.touch_activity();
        // [GRAIN] Standard model and shared-engine admission, including Flow/Streaming.
        let Some(batch) = self.grain_background_context(control)? else {
            return Ok(None);
        };
        let BatchContext {
            settings,
            active_model,
            setup,
            ..
        } = batch;

        let backend = settings.vad_backend;
        let app_handle = self.app_handle.clone();
        let mut chunker = Chunker::new(Box::new(move || create_detector(backend, &app_handle)));
        let mut pieces = audio.chunks(FEED_SAMPLES);

        let mut transcripts: Vec<Transcript> = Vec::new();
        let mut done = 0;
        control.progress(done, total);
        loop {
            // Scoring waits out dictation too, and stops on cancel.
            if !control.proceed() {
                return Ok(None);
            }
            let (chunks, last) = match pieces.next() {
                Some(piece) => (chunker.push(piece), false),
                None => (chunker.finish(), true),
            };
            for chunk in chunks {
                let len = chunk.len();
                // [GRAIN] Reload the pinned Standard model after foreground dictation.
                let Some(transcript) =
                    self.grain_background_chunk(control, chunk, &setup.run_options, &active_model)?
                else {
                    return Ok(None);
                };
                transcripts.push(transcript);
                done += len;
                control.progress(done, total);
            }
            if last {
                break;
            }
        }

        debug!("Transcribed {} chunks in the background", transcripts.len());
        let paragraphs: Vec<String> = transcripts
            .into_iter()
            .map(|t| {
                let output_language = setup.output_language(&settings, t.language);
                post_process_transcription_text(
                    t.text,
                    &settings,
                    setup.custom_words_prompted(),
                    &output_language,
                    &setup.languages,
                )
            })
            .filter(|p| !p.trim().is_empty())
            .collect();

        let elapsed_secs = started.elapsed().as_secs_f64();
        let audio_secs = total as f64 / 16_000.0;
        info!(
            "Background transcription of {:.1}s of audio in {} paragraphs took {:.1}s ({:.1}x real time)",
            audio_secs,
            paragraphs.len(),
            elapsed_secs,
            real_time_factor(audio_secs, elapsed_secs)
        );
        Ok(Some(paragraphs.join("\n\n")))
    }
}
