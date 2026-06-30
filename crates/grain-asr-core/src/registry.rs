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

use crate::model::{AsrBackendKind, AsrCapabilities, AsrModelFiles, AsrModelSpec, MemoryProfile};

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

/// The built-in catalog of Native ASR models. Starts with one known-good Sherpa
/// streaming transducer (the MVP target). Filenames/URL verified against the
/// sherpa-onnx pretrained-models docs.
pub fn builtin_catalog() -> Vec<AsrModelCatalogEntry> {
    vec![AsrModelCatalogEntry {
        id: "sherpa-onnx-streaming-zipformer-en-2023-06-26".into(),
        name: "Streaming Zipformer (English)".into(),
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
    }]
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
