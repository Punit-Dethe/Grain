//! [GRAIN] Prompt Record — split one recording into spoken CONTENT + a spoken AI
//! INSTRUCTION at the pill-click mark, transcribing each independently.
//!
//! This is an inline helper, NOT an engine: it owns no state, spawns no threads,
//! and does nothing when no mark was set. The per-session split mark lives on the
//! `AudioRecordingManager` (set when the user clicks the pill); this consumes it
//! once at stop.
//!
//! Why slice the audio rather than the transcript? The mark is a sample index, so
//! the two halves transcribe as fully independent utterances — no dependence on
//! word-level timestamps (which cloud STT providers give unreliably or not at
//! all) and no ambiguity about which side a boundary word belongs to. It costs a
//! second STT pass, but only when Prompt Record was actually used (a deliberate,
//! occasional click) — the no-mark path is byte-for-byte today's single pass.

use tauri::AppHandle;

/// Transcribe `samples`, optionally splitting it at `mark` into content (before)
/// and a spoken AI instruction (after).
///
/// - No usable mark → a single pass over the whole buffer (today's behavior);
///   the instruction is `None`.
/// - Usable mark (`0 < mark < len`) → the content half is transcribed as the
///   primary result; the instruction half is transcribed best-effort (a failure
///   or empty result simply yields `None`, so the session degrades to a normal
///   dictation instead of erroring).
///
/// Both passes route through [`crate::stt_router::transcribe`], so they honor the
/// same local/cloud routing and final-text cleanup as any other transcription.
pub async fn transcribe_split(
    app: &AppHandle,
    samples: Vec<f32>,
    mark: Option<usize>,
) -> (Result<String, String>, Option<String>) {
    match mark {
        Some(m) if m > 0 && m < samples.len() => {
            let content = samples[..m].to_vec();
            let instruction = samples[m..].to_vec();

            // Content first (this is what gets pasted / post-processed).
            let content_res = crate::stt_router::transcribe(app, content).await;

            // Instruction is best-effort: an error or blank result just means "no
            // spoken prompt", and the caller falls back to a normal paste.
            let spoken = crate::stt_router::transcribe(app, instruction)
                .await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            (content_res, spoken)
        }
        _ => (crate::stt_router::transcribe(app, samples).await, None),
    }
}
