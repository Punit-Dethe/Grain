use anyhow::{Context, Result};
use std::{fs, path::Path};

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    atomic_write_with(path, |file| file.write_all(bytes).context("write state"))
}

pub(crate) fn atomic_write_with(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".grain-state-")
        .suffix(".pending")
        .tempfile_in(parent)
        .with_context(|| format!("stage {}", path.display()))?;
    write(file.as_file_mut())?;
    file.as_file()
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    // Keep the same synced staging file across a short Windows sharing lock.
    // This retries only publication, never serialization, mutation or a tool.
    // Every terminal failure drops only our staging file; there is no delete/
    // copy fallback and no operation after successful atomic replacement.
    #[cfg(not(windows))]
    return file
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
        .with_context(|| format!("persist {}", path.display()));

    #[cfg(windows)]
    {
        let mut delays = [10, 20, 40, 80, 100].into_iter(); // 250ms total sleep ceiling
        loop {
            match file.persist(path) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    if matches!(error.error.raw_os_error(), Some(5 | 32 | 33)) {
                        if let Some(delay) = delays.next() {
                            file = error.file;
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                            continue;
                        }
                    }
                    return Err(error.error).with_context(|| format!("persist {}", path.display()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_owned_atomic_writer_preserves_destination_and_unowned_pending_files() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"original").unwrap();
        let orphan = path.with_extension(format!("{}.0.pending", std::process::id()));
        fs::write(&orphan, b"recoverable previous process bytes").unwrap();
        assert!(atomic_write_with(&path, |file| {
            file.write_all(b"partial new state")?;
            anyhow::bail!("injected write failure")
        })
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(
            fs::read(&orphan).unwrap(),
            b"recoverable previous process bytes"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        atomic_write(&path, b"complete new state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete new state");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn atomic_publication_recovers_after_a_short_windows_delete_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"original").unwrap();
        // A real Windows reader that permits reads/writes but denies rename.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            drop(held);
        });
        let writes = std::cell::Cell::new(0);
        let saved = atomic_write_with(&path, |file| {
            use std::io::Write;
            writes.set(writes.get() + 1);
            file.write_all(b"complete new state")?;
            Ok(())
        });
        release.join().unwrap();
        saved.unwrap();
        assert_eq!(writes.get(), 1, "publication must not replay the writer");
        assert_eq!(fs::read(&path).unwrap(), b"complete new state");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn atomic_publication_refuses_a_persistent_windows_lock_and_preserves_state() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"original").unwrap();
        let orphan = dir.path().join("unowned.pending");
        fs::write(&orphan, b"unowned recovery bytes").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        let start = std::time::Instant::now();
        let error = atomic_write(&path, b"new state").unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
        let io = error.downcast_ref::<std::io::Error>().unwrap();
        assert!(matches!(io.raw_os_error(), Some(5 | 32 | 33)));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&orphan).unwrap(), b"unowned recovery bytes");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        drop(held);
        atomic_write(&path, b"fresh state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"fresh state");
    }
}
