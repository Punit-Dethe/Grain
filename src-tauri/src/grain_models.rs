//! [GRAIN] Speech-to-text models Grain offers beyond Handy's bundled catalog.
//!
//! Handy's `catalog.json` stays a verbatim upstream copy. Grain's extra models
//! are appended to [`crate::catalog::CATALOG`] through one `[GRAIN]` hook, so
//! they download, dedup, rank and render exactly like catalog entries. Each is
//! pinned to an immutable Hugging Face commit.
//!
//! Fluid Speech Pro (FluidVoice, CC BY 4.0) is converted from its original
//! `.nemo` by `scripts/models/convert_fluid_speech_pro.py` and hosted at
//! `Straylight-dev/fluid-speech-pro-gguf`. It is stored in the plain Parakeet
//! TDT 0.6B v3 layout, so batch and Flow treat it as v3. The licence requires
//! the credit line carried in its description.

use crate::catalog::MirrorFile;
use crate::managers::model::{ModelDescriptor, ModelSource, QuantFile};
use crate::managers::model_capabilities::{CapabilityProbe, Compatibility};

pub(crate) const FLUID_SPEECH_PRO_REPO: &str = "Straylight-dev/fluid-speech-pro-gguf";
const FLUID_SPEECH_PRO_REVISION: &str = "fe8969ed6265df26f01a56f932076e4969bb1689";
const FLUID_SPEECH_PRO_FILE: &str = "fluid-speech-pro-Q8_0.gguf";
const FLUID_SPEECH_PRO_BYTES: u64 = 739_508_704;
const FLUID_SPEECH_PRO_SHA256: &str =
    "c9bd4f57cebbaef056bb2bd69568e7897084e6ed70bf46c6834b7b4bbadc25d4";

/// Parakeet TDT 0.6B v3's 25 languages.
const V3_LANGUAGES: [&str; 25] = [
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "ru", "sk", "sl", "es", "sv", "uk",
];

/// Grain's extra transcription models, in catalog descriptor shape.
pub(crate) fn descriptors() -> impl Iterator<Item = ModelDescriptor> {
    std::iter::once(fluid_speech_pro())
}

/// Hash-verified fallback for a Grain model whose hf-hub download failed: the
/// same pinned file fetched as a plain URL and checked against its SHA-256.
/// Empty for any other id.
pub(crate) fn mirror_fallbacks(model_id: &str) -> Vec<MirrorFile> {
    if model_id != format!("{FLUID_SPEECH_PRO_REPO}/{FLUID_SPEECH_PRO_FILE}") {
        return Vec::new();
    }
    vec![MirrorFile {
        url: format!(
            "https://huggingface.co/{FLUID_SPEECH_PRO_REPO}/resolve/{FLUID_SPEECH_PRO_REVISION}/{FLUID_SPEECH_PRO_FILE}"
        ),
        sha256: FLUID_SPEECH_PRO_SHA256.into(),
        size_bytes: FLUID_SPEECH_PRO_BYTES,
    }]
}

fn fluid_speech_pro() -> ModelDescriptor {
    ModelDescriptor {
        id: format!("{FLUID_SPEECH_PRO_REPO}/{FLUID_SPEECH_PRO_FILE}"),
        source: ModelSource::HuggingFace {
            repo_id: FLUID_SPEECH_PRO_REPO.into(),
            revision: FLUID_SPEECH_PRO_REVISION.into(),
        },
        name: "Fluid Speech Pro".into(),
        description: "Accurate 25-language dictation with auto language detection. \
                      Fluid Speech Pro by FluidVoice. Built on NVIDIA Parakeet TDT 0.6B v3 \
                      and Moondream Parakeet Ultra (CC BY 4.0)."
            .into(),
        caps: CapabilityProbe {
            verdict: Compatibility::Compatible,
            display_name: None,
            architecture: Some("parakeet".into()),
            variant: None,
            languages: Some(V3_LANGUAGES.iter().map(|l| (*l).into()).collect()),
            supports_streaming: Some(false),
            supports_translation: Some(false),
            supports_language_detect: Some(true),
        },
        files: vec![QuantFile {
            filename: FLUID_SPEECH_PRO_FILE.into(),
            quant: "Q8_0".into(),
            size_bytes: FLUID_SPEECH_PRO_BYTES,
        }],
        default_quant: Some("Q8_0".into()),
        // Provisional until benchmarked: v3's speed (same runtime layout) and
        // Parakeet Ultra's English accuracy (FluidVoice reports Pro at or above it).
        speed_score: 0.96,
        accuracy_score: 0.75,
        recommended_rank: Some(2),
        recommended: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_speech_pro_is_pinned_and_flow_reviewed() {
        let desc = fluid_speech_pro();
        assert_eq!(FLUID_SPEECH_PRO_REVISION.len(), 40);
        assert!(desc.description.contains("Fluid Speech Pro by FluidVoice"));
        assert!(grain_core::capture::is_reviewed_flow_model(&desc.id));
    }

    #[test]
    fn appears_once_in_the_catalog() {
        let id = fluid_speech_pro().id;
        assert_eq!(
            crate::catalog::CATALOG
                .iter()
                .filter(|d| d.id == id)
                .count(),
            1
        );
        let (found, file) =
            crate::catalog::file_in_catalog(FLUID_SPEECH_PRO_FILE, Some(FLUID_SPEECH_PRO_REPO))
                .expect("file resolves through the catalog");
        assert_eq!(found.id, id);
        assert_eq!(file.size_bytes, FLUID_SPEECH_PRO_BYTES);
        let mirrors = crate::catalog::mirror_fallbacks(&id);
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0].sha256, FLUID_SPEECH_PRO_SHA256);
        assert!(mirrors[0].url.contains(FLUID_SPEECH_PRO_REVISION));
        assert!(crate::catalog::mirror_fallbacks("other/model.gguf").is_empty());
    }
}
