//! Audio cleanup for history entries, including streaming entries without a WAV.

use std::{fs, io, path::Path};

/// Returns whether an audio file was removed. An empty filename means that the
/// entry contains text only; joining it would target the recordings directory.
pub(crate) fn remove_audio(recordings_dir: &Path, file_name: &str) -> io::Result<bool> {
    if file_name.is_empty() {
        return Ok(false);
    }

    let path = recordings_dir.join(file_name);
    const ATTEMPTS: usize = 5;
    for attempt in 0..ATTEMPTS {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                if attempt + 1 == ATTEMPTS {
                    return Err(error);
                }
                // Preserve the existing retry for briefly locked recording files.
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
        }
    }
    unreachable!("the last attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_only_entry_does_not_delete_or_retry_the_recordings_directory() {
        let dir = tempfile::tempdir().unwrap();
        let other_audio = dir.path().join("other.wav");
        fs::write(&other_audio, b"recording").unwrap();

        assert!(!remove_audio(dir.path(), "").unwrap());
        assert!(dir.path().is_dir());
        assert_eq!(fs::read(other_audio).unwrap(), b"recording");
    }

    #[test]
    fn removes_named_audio_and_accepts_already_missing_audio() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("recording.wav");
        fs::write(&audio, b"recording").unwrap();

        assert!(remove_audio(dir.path(), "recording.wav").unwrap());
        assert!(!audio.exists());
        assert!(!remove_audio(dir.path(), "recording.wav").unwrap());
        assert!(dir.path().is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn retries_a_temporarily_locked_audio_file() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("locked.wav");
        let held = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&audio)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(40));
            drop(held);
        });

        assert!(remove_audio(dir.path(), "locked.wav").unwrap());
        release.join().unwrap();
        assert!(!audio.exists());
    }
}
