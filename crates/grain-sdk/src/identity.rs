/// Validate the identifier before it is used in a registry key, prompt
/// attribution, shortcut namespace, log line, or filesystem path.
pub fn validate_extension_id(id: &str) -> Result<(), String> {
    if id.len() > 253 || id != id.trim() {
        return Err("manifest.id must be a canonical reverse-dns identifier".into());
    }
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 63
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || !part
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || !part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    {
        return Err(
            "manifest.id must be lowercase reverse-dns segments using letters, digits, or '-'"
                .into(),
        );
    }
    Ok(())
}

/// Versions are used as directory components by store installs. Grain accepts
/// simple semver-like versions (including prerelease/build separators) without
/// requiring a semver dependency in the SDK leaf.
pub fn validate_extension_version(version: &str) -> Result<(), String> {
    if version.is_empty()
        || version.len() > 64
        || version != version.trim()
        || version.contains("..")
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        || !version
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !version
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err("manifest.version must be a safe semver-like value".into());
    }
    Ok(())
}
