//! Build-supplied public desktop OAuth identity for the supported GitHub MCP.
//! This credential is extractable from the binary, not a confidential secret or
//! an account token. Never use it as signing authority or for arbitrary servers.

pub(super) struct Client<'a> {
    pub id: &'a str,
    pub secret: &'a str,
}

pub(super) fn github<'a>(
    extension_id: Option<&str>,
    endpoint: &str,
    client_id: Option<&'a str>,
    client_secret: Option<&'a str>,
) -> Option<Client<'a>> {
    if extension_id != Some("com.grain.github") || endpoint != "https://api.githubcopilot.com/mcp/"
    {
        return None;
    }
    let id = client_id?;
    let secret = client_secret?;
    if id.is_empty()
        || id.len() > 512
        || secret.is_empty()
        || secret.len() > 4096
        || id.chars().chain(secret.chars()).any(char::is_control)
        || id.trim() != id
        || secret.trim() != secret
    {
        return None;
    }
    Some(Client { id, secret })
}

pub(super) fn github_metadata(
    issuer: Option<&str>,
    authorization_endpoint: &str,
    token_endpoint: &str,
) -> bool {
    issuer == Some("https://github.com/login/oauth")
        && authorization_endpoint == "https://github.com/login/oauth/authorize"
        && token_endpoint == "https://github.com/login/oauth/access_token"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_restricted_to_the_supported_extension_and_exact_resource() {
        let resolve = |id, endpoint| github(id, endpoint, Some("Iv1.test"), Some("test-secret"));
        assert!(resolve(
            Some("com.grain.github"),
            "https://api.githubcopilot.com/mcp/"
        )
        .is_some());
        for id in [
            None,
            Some("com.example.github"),
            Some("com.grain.github.extra"),
        ] {
            assert!(resolve(id, "https://api.githubcopilot.com/mcp/").is_none());
        }
        for endpoint in [
            "https://example.com/mcp/",
            "https://api.githubcopilot.com/mcp",
            "https://api.githubcopilot.com/mcp/?redirect=example.com",
            "https://api.githubcopilot.com.evil.example/mcp/",
            "http://api.githubcopilot.com/mcp/",
        ] {
            assert!(resolve(Some("com.grain.github"), endpoint).is_none());
        }
    }

    #[test]
    fn incomplete_or_malformed_build_credentials_never_enable_the_client() {
        for (id, secret) in [
            (None, None),
            (Some("client"), None),
            (None, Some("secret")),
            (Some(""), Some("secret")),
            (Some("client"), Some("")),
            (Some("client\n"), Some("secret")),
            (Some("client"), Some(" secret")),
        ] {
            assert!(github(
                Some("com.grain.github"),
                "https://api.githubcopilot.com/mcp/",
                id,
                secret
            )
            .is_none());
        }
        assert!(github(
            Some("com.grain.github"),
            "https://api.githubcopilot.com/mcp/",
            Some(&"x".repeat(513)),
            Some("secret")
        )
        .is_none());
        assert!(github(
            Some("com.grain.github"),
            "https://api.githubcopilot.com/mcp/",
            Some("client"),
            Some(&"x".repeat(4097))
        )
        .is_none());
    }

    #[test]
    fn matching_issuer_cannot_redirect_the_credential_exchange() {
        let issuer = Some("https://github.com/login/oauth");
        let authorize = "https://github.com/login/oauth/authorize";
        let token = "https://github.com/login/oauth/access_token";
        assert!(github_metadata(issuer, authorize, token));
        assert!(!github_metadata(None, authorize, token));
        assert!(!github_metadata(
            Some("https://example.com"),
            authorize,
            token
        ));
        assert!(!github_metadata(
            issuer,
            "https://example.com/authorize",
            token
        ));
        assert!(!github_metadata(
            issuer,
            authorize,
            "https://example.com/token"
        ));
        assert!(!github_metadata(
            issuer,
            authorize,
            "https://github.com/other-token"
        ));
    }
}
