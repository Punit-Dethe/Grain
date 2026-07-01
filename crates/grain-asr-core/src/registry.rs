//! Native ASR model registry: the catalog of known ASR models and their on-disk
//! Sherpa bundle layout.
//!
//! Deliberately separate from Handy's Batch/Rolling registry (`selected_model`):
//! ASR models are multi-file transducer bundles with their own topology,
//! capabilities, and lifecycle. This module is pure — it builds paths and
//! validates layout but performs NO I/O. The host (`src-tauri`'s
//! `AsrModelManager`) does the directory listing, download, and extraction.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{
    AsrBackendKind, AsrCapabilities, AsrModelFiles, AsrModelSpec, AsrTuning, MemoryProfile,
};

/// The relative filenames of a Sherpa streaming-transducer bundle, inside its
/// installed model directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SherpaTransducerLayout {
    pub encoder: String,
    pub decoder: String,
    pub joiner: String,
    pub tokens: String,
    /// Optional extra config file (some bundles ship one).
    pub config: Option<String>,
}

impl SherpaTransducerLayout {
    /// The files that MUST be present for the bundle to load (config is optional).
    pub fn required(&self) -> [&str; 4] {
        [&self.encoder, &self.decoder, &self.joiner, &self.tokens]
    }

    /// Resolve to absolute [`AsrModelFiles`] rooted at `dir`. Pure path joins.
    pub fn resolve(&self, dir: &Path) -> AsrModelFiles {
        AsrModelFiles::SherpaTransducer {
            encoder: dir.join(&self.encoder),
            decoder: dir.join(&self.decoder),
            joiner: dir.join(&self.joiner),
            tokens: dir.join(&self.tokens),
            config: self.config.as_ref().map(|c| dir.join(c)),
        }
    }

    /// Required files absent from `present` (a directory listing of filenames).
    /// An empty result means the bundle is complete.
    pub fn missing<'a>(&'a self, present: &[String]) -> Vec<&'a str> {
        self.required()
            .into_iter()
            .filter(|f| !present.iter().any(|p| p == f))
            .collect()
    }

    /// Whether every required file is present.
    pub fn is_complete(&self, present: &[String]) -> bool {
        self.missing(present).is_empty()
    }
}

/// How a model bundle is distributed: one archive to download and extract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsrDownload {
    /// Archive URL (`.tar.bz2` / `.tar.gz` / `.zip` — host picks the extractor).
    pub url: String,
    /// SHA-256 of the archive for verification, when known.
    pub sha256: Option<String>,
    /// Approximate download size (MB), for the UI.
    pub size_mb: u64,
    /// Top-level directory the archive extracts into (Sherpa archives wrap their
    /// files in a folder named after the model). The host strips/joins this to
    /// find the bundle root. `None` if files extract flat.
    pub archive_root: Option<String>,
}

/// One catalog entry: identity, capabilities, how to fetch, and on-disk layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AsrModelCatalogEntry {
    pub id: String,
    pub name: String,
    pub backend: AsrBackendKind,
    pub languages: Vec<String>,
    pub sample_rate_hz: u32,
    pub capabilities: AsrCapabilities,
    pub memory: MemoryProfile,
    /// The model's runtime profile (decoding/endpoint/threading). Every entry
    /// declares one; use [`AsrTuning::default`] when the stock settings are fine.
    pub tuning: AsrTuning,
    pub download: AsrDownload,
    pub layout: SherpaTransducerLayout,
}

impl AsrModelCatalogEntry {
    /// The resolved [`AsrModelSpec`] for this model installed at `dir`.
    pub fn to_spec(&self, dir: &Path) -> AsrModelSpec {
        AsrModelSpec {
            id: self.id.clone(),
            name: self.name.clone(),
            backend: self.backend,
            files: self.layout.resolve(dir),
            sample_rate_hz: self.sample_rate_hz,
            languages: self.languages.clone(),
            capabilities: self.capabilities,
            memory: self.memory,
            tuning: self.tuning,
        }
    }

    /// Where this model's bundle root lives under `models_root` (the host's ASR
    /// models directory), accounting for the archive's wrapping folder.
    pub fn bundle_dir(&self, models_root: &Path) -> PathBuf {
        let base = models_root.join(&self.id);
        match &self.download.archive_root {
            Some(root) => base.join(root),
            None => base,
        }
    }
}

/// The built-in catalog of Native ASR models. Every entry is a Sherpa streaming
/// (online) transducer — encoder/decoder/joiner/tokens — so it drops straight
/// into `OnlineRecognizer` with true word-by-word partials. Filenames/URLs
/// verified against the sherpa-onnx `asr-models` GitHub releases + the models'
/// HuggingFace file trees.
///
/// Note on Parakeet: NVIDIA's headline "Parakeet TDT 0.6b" is an OFFLINE
/// transducer in sherpa-onnx (it only does VAD-chunked *simulated* streaming),
/// so it does not fit this online architecture. The entry below is the
/// STREAMING member of the same NeMo fast-conformer/Parakeet family — a true
/// online transducer — which is the right fit for the live Studio Window.
pub fn builtin_catalog() -> Vec<AsrModelCatalogEntry> {
    vec![
        // NVIDIA Nemotron streaming 0.6B (int8) — the most accurate streaming
        // model here. Big encoder (~622 MB), 560 ms lookahead → accuracy-first
        // profile: beam search + more threads to stay real-time, and a snappier
        // endpoint so segments finalize (and thus fully commit) sooner.
        AsrModelCatalogEntry {
            id: "sherpa-onnx-nemotron-speech-streaming-en-0.6b-560ms-int8-2026-04-25".into(),
            name: "NVIDIA Nemotron Streaming (English, best accuracy)".into(),
            backend: AsrBackendKind::SherpaOnnx,
            languages: vec!["en".into()],
            sample_rate_hz: 16_000,
            capabilities: AsrCapabilities {
                partials: true,
                immutable_final: true,
                endpointing: true,
                word_timestamps: false,
            },
            memory: MemoryProfile { approx_mb: 720 },
            // NeMo transducers only implement greedy search in sherpa-onnx
            // (`modified_beam_search` makes the NeMo impl *abort the process*),
            // and greedy transducer decoding is already essentially as accurate —
            // beam search barely moves transducer WER. Accuracy here comes from
            // the big encoder + enough threads to run it real-time.
            tuning: AsrTuning {
                num_threads: 4,
                decoding: crate::model::DecodingMethod::Greedy,
                endpoint_trailing_silence_secs: 0.8,
                endpoint_max_utterance_secs: 20.0,
            },
            download: AsrDownload {
                url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemotron-speech-streaming-en-0.6b-560ms-int8-2026-04-25.tar.bz2".into(),
                sha256: None,
                size_mb: 464,
                archive_root: Some("sherpa-onnx-nemotron-speech-streaming-en-0.6b-560ms-int8-2026-04-25".into()),
            },
            layout: SherpaTransducerLayout {
                encoder: "encoder.int8.onnx".into(),
                decoder: "decoder.int8.onnx".into(),
                joiner: "joiner.int8.onnx".into(),
                tokens: "tokens.txt".into(),
                config: None,
            },
        },
        // NeMo fast-conformer (Parakeet family), 80 ms lookahead — low-latency,
        // lighter encoder. Beam search is still cheap here; two threads keep it
        // real-time on typical hardware.
        AsrModelCatalogEntry {
            id: "sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms".into(),
            name: "Parakeet Streaming (English, low-latency)".into(),
            backend: AsrBackendKind::SherpaOnnx,
            languages: vec!["en".into()],
            sample_rate_hz: 16_000,
            capabilities: AsrCapabilities {
                partials: true,
                immutable_final: true,
                endpointing: true,
                word_timestamps: false,
            },
            memory: MemoryProfile { approx_mb: 520 },
            // NeMo transducer → greedy only (see the Nemotron note above).
            tuning: AsrTuning {
                num_threads: 2,
                decoding: crate::model::DecodingMethod::Greedy,
                endpoint_trailing_silence_secs: 1.0,
                endpoint_max_utterance_secs: 20.0,
            },
            download: AsrDownload {
                url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms.tar.bz2".into(),
                sha256: None,
                size_mb: 429,
                archive_root: Some("sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms".into()),
            },
            layout: SherpaTransducerLayout {
                encoder: "encoder.onnx".into(),
                decoder: "decoder.onnx".into(),
                joiner: "joiner.onnx".into(),
                tokens: "tokens.txt".into(),
                config: None,
            },
        },
        // Streaming Zipformer — small, fast, low-RAM. Greedy on one thread is
        // plenty; the stock endpoint timing is fine for a compact model.
        AsrModelCatalogEntry {
            id: "sherpa-onnx-streaming-zipformer-en-2023-06-26".into(),
            name: "Streaming Zipformer (English, compact)".into(),
            backend: AsrBackendKind::SherpaOnnx,
            languages: vec!["en".into()],
            sample_rate_hz: 16_000,
            capabilities: AsrCapabilities {
                partials: true,
                immutable_final: true,
                endpointing: true,
                word_timestamps: true,
            },
            memory: MemoryProfile { approx_mb: 350 },
            tuning: AsrTuning::default(),
            download: AsrDownload {
                url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-en-2023-06-26.tar.bz2".into(),
                sha256: None,
                size_mb: 350,
                archive_root: Some("sherpa-onnx-streaming-zipformer-en-2023-06-26".into()),
            },
            layout: SherpaTransducerLayout {
                encoder: "encoder-epoch-99-avg-1-chunk-16-left-128.onnx".into(),
                decoder: "decoder-epoch-99-avg-1-chunk-16-left-128.onnx".into(),
                joiner: "joiner-epoch-99-avg-1-chunk-16-left-128.onnx".into(),
                tokens: "tokens.txt".into(),
                config: None,
            },
        },
    ]
}

/// Look up a catalog entry by id.
pub fn catalog_entry(id: &str) -> Option<AsrModelCatalogEntry> {
    builtin_catalog().into_iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> SherpaTransducerLayout {
        SherpaTransducerLayout {
            encoder: "encoder.onnx".into(),
            decoder: "decoder.onnx".into(),
            joiner: "joiner.onnx".into(),
            tokens: "tokens.txt".into(),
            config: None,
        }
    }

    #[test]
    fn missing_reports_absent_required_files() {
        let l = layout();
        let present = vec!["encoder.onnx".to_string(), "tokens.txt".to_string()];
        let missing = l.missing(&present);
        assert_eq!(missing, vec!["decoder.onnx", "joiner.onnx"]);
        assert!(!l.is_complete(&present));
    }

    #[test]
    fn complete_bundle_has_no_missing() {
        let l = layout();
        let present = vec![
            "encoder.onnx".to_string(),
            "decoder.onnx".to_string(),
            "joiner.onnx".to_string(),
            "tokens.txt".to_string(),
            "extra-unrelated.txt".to_string(),
        ];
        assert!(l.is_complete(&present));
        assert!(l.missing(&present).is_empty());
    }

    #[test]
    fn resolve_joins_under_dir() {
        let l = layout();
        let dir = Path::new("/models/asr/x");
        match l.resolve(dir) {
            AsrModelFiles::SherpaTransducer {
                encoder, tokens, config, ..
            } => {
                assert_eq!(encoder, dir.join("encoder.onnx"));
                assert_eq!(tokens, dir.join("tokens.txt"));
                assert!(config.is_none());
            }
        }
    }

    #[test]
    fn bundle_dir_accounts_for_archive_root() {
        let entry = &builtin_catalog()[0];
        let root = Path::new("/data/models/asr");
        // Archive wraps files in a folder named after the model.
        assert_eq!(
            entry.bundle_dir(root),
            root.join(&entry.id).join(&entry.id)
        );
    }

    #[test]
    fn catalog_entry_builds_resolved_spec() {
        let entry = catalog_entry("sherpa-onnx-streaming-zipformer-en-2023-06-26").unwrap();
        let dir = Path::new("/m");
        let spec = entry.to_spec(dir);
        assert_eq!(spec.id, entry.id);
        assert_eq!(spec.sample_rate_hz, 16_000);
        assert!(spec.capabilities.immutable_final);
        match spec.files {
            AsrModelFiles::SherpaTransducer { joiner, .. } => {
                assert_eq!(joiner, dir.join("joiner-epoch-99-avg-1-chunk-16-left-128.onnx"));
            }
        }
    }

    #[test]
    fn unknown_catalog_id_is_none() {
        assert!(catalog_entry("does-not-exist").is_none());
    }
}
