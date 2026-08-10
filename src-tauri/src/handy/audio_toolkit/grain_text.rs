//! [GRAIN] Grain's final-text stage, layered on upstream's `text.rs`
//! (Handy Isolation phase 6). Upstream's file keeps only its own
//! `apply_custom_words` / `filter_transcription_output`; the Grain-specific
//! composition — "scrap that" reset, then custom words, then filler filtering,
//! then snippet expansion — lives here so the upstream file stays clean.

use super::text::{
    apply_custom_words, normalize_transcription_output, remove_filler_words, OutputLanguageEvidence,
};

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
/// * `output_language` - the transcription output/intent language code (NOT the
///   UI language). A concrete code is treated as `UserSelected` evidence for
///   #1738 filler removal; `"auto"`/empty is `Unknown` (universal-tier only).
/// * `custom_filler_words` - optional filler-word override (see
///   [`remove_filler_words`]).
/// * `filler_word_removal_enabled` - master toggle for built-in/custom filler
///   removal (upstream #1738; defaults on).
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
#[allow(clippy::too_many_arguments)]
pub fn finalize_transcript(
    text: &str,
    custom_words: &[String],
    word_correction_threshold: f64,
    output_language: &str,
    custom_filler_words: &Option<Vec<String>>,
    filler_word_removal_enabled: bool,
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
    // [GRAIN] #1738: filler removal is language-evidence-gated. These paths
    // (rolling, cloud, Agent) carry the transcription language intent, not audio
    // LID, so a concrete selection is UserSelected evidence and "auto"/empty is
    // Unknown (universal-tier fillers only — never a gated real word).
    let evidence = if output_language.is_empty() || output_language == "auto" {
        OutputLanguageEvidence::Unknown
    } else {
        OutputLanguageEvidence::UserSelected(output_language.to_string())
    };
    let without_fillers = remove_filler_words(
        &corrected,
        &evidence,
        custom_filler_words,
        filler_word_removal_enabled,
    );
    let filtered = normalize_transcription_output(&without_fillers);
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
            true,
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
            finalize_transcript("um hello world", &[], 0.5, "en", &None, true, false, &[], false);
        assert_eq!(result, "hello world");
    }
}

/// [GRAIN] Grain's bracketing stages around upstream's batch text pipeline
/// ([`crate::managers::transcription::post_process_transcription_text`]):
/// "scrap that" runs FIRST on the raw transcript, so everything spoken before
/// the last reset phrase is discarded and the rest of the pipeline (custom
/// words, fillers) only sees the kept remainder; snippet expansion runs LAST on
/// the corrected/filtered text. Covers the local batch and stream-finalize
/// paths — rolling and cloud STT expand via [`finalize_transcript`] instead.
pub fn finalize_batch_text(
    raw: String,
    settings: &crate::settings::AppSettings,
    custom_words_already_prompted: bool,
    output_language: &OutputLanguageEvidence,
    supported_languages: &[String],
) -> String {
    let raw = if settings.scrap_that_enabled {
        super::strip_scrapped(&raw)
    } else {
        raw
    };

    let filtered = crate::managers::transcription::post_process_transcription_text(
        raw,
        settings,
        custom_words_already_prompted,
        output_language,
        supported_languages,
    );

    // [GRAIN] Snippets built-in extension gate (SPEC 10.1).
    if settings.snippets_enabled {
        super::apply_snippets(&filtered, &settings.snippets)
    } else {
        filtered
    }
}
