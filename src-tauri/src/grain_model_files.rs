//! [GRAIN] Pinned files for Grain's own models: the talk-back voice and Meet's
//! speaker detection. Handy's speech-to-text catalog stays 1:1 with Handy;
//! these models never enter it.
//!
//! A file is pinned to an exact URL revision, size and SHA-256. It is streamed
//! into `<name>.partial`, verified, and only then renamed into place, so a
//! present file is always a complete, verified one.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) struct PinnedFile {
    /// Path inside the model's folder.
    pub(crate) name: &'static str,
    pub(crate) url: &'static str,
    pub(crate) bytes: u64,
    pub(crate) sha256: &'static str,
}

impl PinnedFile {
    /// Whether the file is installed in `dir`: present with its pinned size.
    /// Its content was verified when it was downloaded.
    pub(crate) fn present_in(&self, dir: &Path) -> bool {
        std::fs::metadata(dir.join(self.name)).is_ok_and(|meta| meta.len() == self.bytes)
    }

    /// Pinned to a revision (not a moving branch) with a full SHA-256.
    #[cfg(test)]
    pub(crate) fn is_pinned(&self) -> bool {
        self.url.contains("/resolve/")
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    path.with_file_name(name)
}

/// Download `file` into `dir`. `progress` is called with the bytes received
/// so far after every chunk. A failed or unverifiable download leaves nothing
/// behind.
pub(crate) async fn download(
    client: &reqwest::Client,
    file: &PinnedFile,
    dir: &Path,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let path = dir.join(file.name);
    let folder = path.parent().unwrap_or(dir);
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("could not create {}: {e}", folder.display()))?;
    let partial = partial_path(&path);
    let response = client
        .get(file.url)
        .timeout(Duration::from_secs(30 * 60))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("{}: {e}", file.name))?;
    let mut out = std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", file.name))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    let result: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("{}: {e}", file.name))?;
            downloaded += chunk.len() as u64;
            if downloaded > file.bytes {
                return Err(format!("{} is larger than expected", file.name));
            }
            hasher.update(&chunk);
            out.write_all(&chunk)
                .map_err(|e| format!("{}: {e}", file.name))?;
            progress(downloaded);
        }
        out.sync_all().map_err(|e| format!("{}: {e}", file.name))?;
        if downloaded != file.bytes || format!("{:x}", hasher.finalize()) != file.sha256 {
            return Err(format!("{} failed verification", file.name));
        }
        Ok(())
    }
    .await;
    drop(out);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", file.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_sits_beside_the_file() {
        assert_eq!(
            partial_path(Path::new("m/voices/Zoey.safetensors")),
            Path::new("m/voices/Zoey.safetensors.partial")
        );
        assert_eq!(
            partial_path(Path::new("m/model.q8.gguf")),
            Path::new("m/model.q8.gguf.partial")
        );
    }

    #[test]
    fn present_means_the_pinned_size() {
        let dir = std::env::temp_dir().join(format!("grain-model-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = PinnedFile {
            name: "f.bin",
            url: "https://example.com/resolve/abc/f.bin",
            bytes: 3,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        assert!(!file.present_in(&dir));
        std::fs::write(dir.join("f.bin"), b"ab").unwrap();
        assert!(!file.present_in(&dir));
        std::fs::write(dir.join("f.bin"), b"abc").unwrap();
        assert!(file.present_in(&dir));
        assert!(file.is_pinned());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
