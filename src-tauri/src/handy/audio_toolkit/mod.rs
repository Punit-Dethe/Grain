pub mod audio;
pub mod chunking;
pub mod constants;
pub mod grain_text; // [GRAIN] Shared final-text composition.
pub mod lang_id;
pub mod snippets; // [GRAIN] Voice reset, snippet expansion and preview cleanup.
pub mod text;
pub mod utils;
pub mod vad;

pub use audio::{
    list_output_devices, read_wav_samples, save_wav_file, verify_wav_file, CaptureSink, CaptureVad,
    VadPolicy,
};
pub use grain_text::finalize_transcript; // [GRAIN]
pub use lang_id::detect_output_language;
pub use snippets::{apply_snippets, scrub_stream_preview, strip_scrapped}; // [GRAIN]
pub use text::{
    apply_custom_words, normalize_transcription_output, remove_filler_words, OutputLanguageEvidence,
};
pub use utils::get_cpal_host;
pub use vad::{EarshotVad, SileroVad, SpeechScorer, VoiceActivityDetector};
