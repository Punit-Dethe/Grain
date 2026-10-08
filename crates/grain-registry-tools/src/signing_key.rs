//! Explicit operator credentials, scoped to one signing operation.
use anyhow::{bail, Context, Result};
use clap::Args;
use minisign::{SecretKey, SecretKeyBox};
use std::{
    io::{Cursor, IsTerminal},
    path::{Path, PathBuf},
};

#[derive(Args, Default)]
pub(crate) struct Unlock {
    /// Prompt without echo on an attached terminal; never prompt implicitly.
    #[arg(long, global = true, conflicts_with_all = ["key_password_file", "dev_empty_key_password"])]
    pub prompt_key_password: bool,
    /// Operator-owned UTF-8 password file, outside author/build workspaces.
    #[arg(long, global = true, conflicts_with = "dev_empty_key_password")]
    pub key_password_file: Option<PathBuf>,
    /// Explicit disposable-development-key mode; not password protection.
    #[arg(long, global = true)]
    pub dev_empty_key_password: bool,
}

impl Unlock {
    pub(crate) fn outside(&self, key: &Path, inputs: &[&Path]) -> Result<()> {
        for path in std::iter::once(key).chain(self.key_password_file.as_deref()) {
            let credential = path
                .canonicalize()
                .context("Resolve explicit operator credential")?;
            for input in inputs {
                if credential.starts_with(input.canonicalize()?) {
                    bail!("Operator credentials must be outside author and serving inputs");
                }
            }
        }
        Ok(())
    }

    pub(crate) fn requested(&self) -> bool {
        self.prompt_key_password || self.key_password_file.is_some() || self.dev_empty_key_password
    }

    #[cfg(test)]
    pub(crate) fn development() -> Self {
        Self {
            dev_empty_key_password: true,
            ..Self::default()
        }
    }

    pub(crate) fn password(&self, confirm: bool) -> Result<String> {
        let password = if self.dev_empty_key_password {
            return Ok(String::new());
        } else if let Some(path) = &self.key_password_file {
            let raw = private_read(path, 1024)?;
            let mut text = String::from_utf8(raw).context("Password file must be UTF-8")?;
            if text.ends_with("\r\n") {
                text.truncate(text.len() - 2);
            } else if text.ends_with('\n') {
                text.pop();
            }
            text
        } else if self.prompt_key_password {
            if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
                bail!(
                    "Password prompting requires an attached terminal; no implicit stdin fallback"
                );
            }
            let value = rpassword::prompt_password("Signing key password: ")?;
            if confirm && value != rpassword::prompt_password("Confirm signing key password: ")? {
                bail!("Signing key password confirmation differs");
            }
            value
        } else {
            bail!("Explicit key unlock required: --prompt-key-password, --key-password-file or --dev-empty-key-password");
        };
        if password.is_empty() || password.len() > 1024 || password.contains(['\r', '\n', '\0']) {
            bail!("Signing key password must be one nonempty UTF-8 line, at most 1024 bytes");
        }
        Ok(password)
    }
}

fn private_read(path: &Path, max: u64) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::symlink_metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!("Operator credential file must have owner-only permissions");
        }
    }
    crate::review::read(path, max)
}

pub(crate) fn load(path: &Path, unlock: &Unlock) -> Result<SecretKey> {
    // Read the bounded non-linked key before any password access or prompt.
    let raw = if unlock.dev_empty_key_password {
        crate::review::read(path, 8192)?
    } else {
        private_read(path, 8192)?
    };
    let text = String::from_utf8(raw).context("Secret key must be UTF-8")?;
    let key = SecretKeyBox::from_string(&text).context("Parse secret key")?;
    key.into_secret_key(Some(unlock.password(false)?))
        .context("Unlock signing key; check the selected key and password")
}

pub(crate) fn sign(key: &SecretKey, bytes: &[u8]) -> Result<String> {
    Ok(minisign::sign(
        None,
        key,
        Cursor::new(bytes),
        Some("grain registry signed document"),
        Some("grain-registry"),
    )
    .context("Sign document")?
    .into_string())
}

#[cfg(test)]
pub(crate) fn password_file(path: &Path, password: &str) -> Unlock {
    std::fs::write(path, password).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    Unlock {
        key_password_file: Some(path.to_owned()),
        ..Unlock::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn encrypted_key_signs_and_wrong_missing_empty_passwords_refuse() {
        let root = tempfile::tempdir().unwrap();
        let unlock = password_file(&root.path().join("password"), "test protected password\r\n");
        crate::keygen(root.path().join("keys"), "publisher".into(), &unlock).unwrap();
        let key_path = root.path().join("keys/publisher.key");
        let key = load(&key_path, &unlock).unwrap();
        let signature = sign(&key, b"signed bytes").unwrap();
        let public = minisign::PublicKeyBox::from_string(
            &fs::read_to_string(root.path().join("keys/publisher.pub")).unwrap(),
        )
        .unwrap()
        .into_public_key()
        .unwrap()
        .to_base64();
        let roots = grain_sdk::Roots {
            spec: 1,
            version: 1,
            publishing_key: public,
            base_urls: vec![],
            mirrors: vec![],
            expires: None,
        };
        grain_core::trust::verify_publisher_signature(&roots, b"signed bytes", &signature).unwrap();
        assert!(grain_core::trust::verify_publisher_signature(
            &roots,
            b"changed bytes",
            &signature
        )
        .is_err());
        assert!(load(&key_path, &Unlock::default())
            .err()
            .unwrap()
            .to_string()
            .contains("Explicit key unlock"));
        assert!(load(&key_path, &Unlock::development()).is_err());
        let wrong = password_file(&root.path().join("wrong"), "wrong protected password");
        assert!(load(&key_path, &wrong).is_err());
        let empty = password_file(&root.path().join("empty"), "\n");
        assert!(load(&key_path, &empty).is_err());
    }

    #[test]
    fn keygen_never_replaces_existing_pair_or_accepts_path_names() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("keys");
        crate::keygen(out.clone(), "development".into(), &Unlock::development()).unwrap();
        let before = fs::read(out.join("development.key")).unwrap();
        assert!(crate::keygen(out.clone(), "development".into(), &Unlock::default()).is_err());
        assert_eq!(fs::read(out.join("development.key")).unwrap(), before);
        for name in ["../escape", "", "name.pub", "C:\\outside", "/outside"] {
            assert!(crate::keygen(
                root.path().join("absent"),
                name.into(),
                &Unlock::development()
            )
            .is_err());
        }
        assert!(!root.path().join("absent").exists());
        assert!(crate::keygen(
            root.path().join("absent"),
            "protected".into(),
            &Unlock::default()
        )
        .is_err());
        assert!(!root.path().join("absent").exists());
    }

    #[test]
    fn credential_files_are_bounded_and_password_whitespace_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        for password in ["", "\n", "abc\ndef", "abc\0def", &"x".repeat(1025)] {
            assert!(password_file(&root.path().join("password"), password)
                .password(false)
                .is_err());
        }
        assert_eq!(
            password_file(&root.path().join("password"), "  spaced password  \n")
                .password(false)
                .unwrap(),
            "  spaced password  "
        );
        let key = root.path().join("oversized.key");
        fs::write(&key, vec![b'x'; 8193]).unwrap();
        assert!(load(&key, &Unlock::development()).is_err());
    }

    #[test]
    fn credentials_cannot_come_from_author_or_serving_trees() {
        let root = tempfile::tempdir().unwrap();
        let author = root.path().join("author");
        fs::create_dir(&author).unwrap();
        let key = root.path().join("operator.key");
        fs::write(&key, "controlled path only").unwrap();
        let unlock = password_file(&root.path().join("operator-password"), "password");
        unlock.outside(&key, &[&author]).unwrap();
        let author_unlock = password_file(&author.join("password"), "password");
        assert!(author_unlock.outside(&key, &[&author]).is_err());
        let author_key = author.join("key");
        fs::write(&author_key, "controlled path only").unwrap();
        assert!(unlock.outside(&author_key, &[&author]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn protected_credentials_require_owner_only_and_refuse_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("password");
        let unlock = password_file(&path, "private password");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(unlock.password(false).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let linked = root.path().join("linked");
        symlink(&path, &linked).unwrap();
        assert!(Unlock {
            key_password_file: Some(linked),
            ..Unlock::default()
        }
        .password(false)
        .is_err());
        crate::keygen(root.path().join("keys"), "protected".into(), &unlock).unwrap();
        let key = root.path().join("keys/protected.key");
        assert_eq!(
            fs::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&key, &unlock).is_err());
    }
}
