//! [GRAIN] Grain's final-text stage, layered on upstream's `text.rs`
//! (Handy Isolation phase 6). Upstream's file keeps only its own
//! `apply_custom_words` / `filter_transcription_output`; the Grain-specific
//! composition — "scrap that" reset, then custom words, then filler filtering,
//! then snippet expansion — lives here so the upstream file stays clean.

use super::text::{apply_custom_words, filter_transcription_output};

/// Apply the full final-text stage to a completed transcript: custom-word
/// correction followed by filler-word / stutter filtering.
///
/// This is the single place every transcription path converges on so the
/// behavior is identical regardless of backend (local batch, rolling window,
/// or cloud STT). Run it ONCE on the finished transcript — never per rolling
/// chunk, which would corrupt words across chunk seams and repeat the work.
///
/// # Arguments
/// * `text` - the completed transcript.
/// * `custom_words` - the user's dictionary (may be empty).
/// * `word_correction_threshold` - fuzzy-match acceptance threshold.
/// * `app_language` - language code selecting the default filler-word set.
/// * `custom_filler_words` - optional filler-word override (see
///   [`filter_transcription_output`]).
/// * `skip_custom_words` - when `true`, skip the fuzzy custom-word correction.
///   The local Whisper batch path sets this because it already biases the model
///   via `initial_prompt`; paths with no such biasing (rolling, cloud, Agent)
///   pass `false` so the dictionary is honored.
/// * `snippets` - the user's voice snippets; expanded LAST so triggers match
///   the corrected/filtered text (may be empty).
/// * `scrap_that` - when `true`, apply the "scrap that" voice reset FIRST: drop
///   everything up to and including the last spoken reset phrase before any other
///   correction runs.
///
/// # Returns
/// The finalized transcript.
pub fn finalize_transcript(
    text: &str,
    custom_words: &[String],
    word_correction_threshold: f64,
    app_language: &str,
    custom_filler_words: &Option<Vec<String>>,
    skip_custom_words: bool,
    snippets: &[crate::settings::Snippet],
    scrap_that: bool,
) -> String {
    // [GRAIN] "Scrap that" runs before every other stage so the rest only sees
    // the kept remainder (mirrors `post_process_transcription_text`).
    let scrapped;
    let text = if scrap_that {
        scrapped = crate::audio_toolkit::strip_scrapped(text);
        scrapped.as_str()
    } else {
        text
    };
    let corrected = if skip_custom_words || custom_words.is_empty() {
        text.to_string()
    } else {
        apply_custom_words(text, custom_words, word_correction_threshold)
    };
    let filtered = filter_transcription_output(&corrected, app_language, custom_filler_words);
    crate::audio_toolkit::apply_snippets(&filtered, snippets)
}

#[cfg(test)]
mod tests {
    use super::finalize_transcript;

    #[test]
    fn test_finalize_applies_custom_words_and_filters() {
        // Non-whisper path: fuzzy correction runs, then fillers are removed.
        let custom = vec!["ChargeBee".to_string()];
        let result = finalize_transcript(
            "um the Charge B um dashboard",
            &custom,
            0.5,
            "en",
            &None,
            false,
            &[],
            false,
        );
        assert!(result.contains("ChargeBee"), "got: {result}");
        assert!(!result.contains("um"), "fillers not removed: {result}");
    }

    #[test]
    fn test_finalize_skip_custom_words_still_filters() {
        // Whisper path: fuzzy correction skipped (model already biased), but
        // filler filtering still applies.
        let custom = vec!["ChargeBee".to_string()];
        let result = finalize_transcript(
            "um the Charge B dashboard",
            &custom,
            0.5,
            "en",
            &None,
            true,
            &[],
            false,
        );
        assert!(
            !result.contains("ChargeBee"),
            "should not fuzzy-correct: {result}"
        );
        assert!(result.contains("Charge B"), "original kept: {result}");
        assert!(!result.contains("um"), "fillers not removed: {result}");
    }

    #[test]
    fn test_finalize_empty_custom_words_is_just_filter() {
        let result =
            finalize_transcript("um hello world", &[], 0.5, "en", &None, false, &[], false);
        assert_eq!(result, "hello world");
    }
}
