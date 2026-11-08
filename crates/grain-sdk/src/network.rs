/// A canonical host for connection and capability admission.
/// Accepts a lowercase DNS name or an unbracketed IP address.
pub fn is_canonical_host(host: &str) -> bool {
    if host.is_empty()
        || host.len() > 253
        || host != host.to_ascii_lowercase()
        || host.contains('*')
        || host.ends_with('.')
    {
        return false;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
    })
}
