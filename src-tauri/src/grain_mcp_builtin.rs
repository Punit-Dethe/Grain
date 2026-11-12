//! Build-supplied public desktop OAuth identities for bundled MCP providers.
//! This credential is extractable from the binary, not a confidential secret or
//! an account token. Never use it as signing authority or for arbitrary servers.

pub(super) struct Client<'a> {
    pub id: &'a str,
    pub secret: &'a str,
}

/// A provider whose server does not register clients itself. Grain is
/// registered once on the provider's developer page; the build carries that
/// registration. Each identity is pinned to one resource and one sign-in server.
struct Registration {
    provider_id: &'static str,
    endpoint: &'static str,
    issuer: &'static str,
    authorization_endpoint: &'static str,
    token_endpoint: &'static str,
    client_id: Option<&'static str>,
    client_secret: Option<&'static str>,
}

const REGISTRATIONS: &[Registration] = &[
    Registration {
        provider_id: "github",
        endpoint: "https://api.githubcopilot.com/mcp/",
        issuer: "https://github.com/login/oauth",
        authorization_endpoint: "https://github.com/login/oauth/authorize",
        token_endpoint: "https://github.com/login/oauth/access_token",
        client_id: option_env!("GRAIN_GITHUB_OAUTH_CLIENT_ID"),
        client_secret: option_env!("GRAIN_GITHUB_OAUTH_CLIENT_SECRET"),
    },
    Registration {
        provider_id: "asana",
        endpoint: "https://mcp.asana.com/v2/mcp",
        issuer: "https://app.asana.com",
        authorization_endpoint: "https://app.asana.com/-/oauth_authorize",
        token_endpoint: "https://app.asana.com/-/oauth_token",
        client_id: option_env!("GRAIN_ASANA_OAUTH_CLIENT_ID"),
        client_secret: option_env!("GRAIN_ASANA_OAUTH_CLIENT_SECRET"),
    },
];

fn registration(provider_id: &str, endpoint: &str) -> Option<&'static Registration> {
    REGISTRATIONS
        .iter()
        .find(|item| item.provider_id == provider_id && item.endpoint == endpoint)
}

pub(super) fn client(provider_id: &str, endpoint: &str) -> Option<Client<'static>> {
    let item = registration(provider_id, endpoint)?;
    desktop_client(item.client_id, item.client_secret)
}

#[cfg(test)]
fn resolve<'a>(
    provider_id: &str,
    endpoint: &str,
    client_id: Option<&'a str>,
    client_secret: Option<&'a str>,
) -> Option<Client<'a>> {
    registration(provider_id, endpoint)?;
    desktop_client(client_id, client_secret)
}

fn desktop_client<'a>(
    client_id: Option<&'a str>,
    client_secret: Option<&'a str>,
) -> Option<Client<'a>> {
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

/// The build credential is only ever exchanged with its own sign-in server.
pub(super) fn metadata_matches(
    provider_id: &str,
    endpoint: &str,
    issuer: Option<&str>,
    authorization_endpoint: &str,
    token_endpoint: &str,
) -> bool {
    registration(provider_id, endpoint).is_some_and(|item| {
        issuer == Some(item.issuer)
            && authorization_endpoint == item.authorization_endpoint
            && token_endpoint == item.token_endpoint
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB: &str = "https://api.githubcopilot.com/mcp/";
    const ASANA: &str = "https://mcp.asana.com/v2/mcp";

    #[test]
    fn credentials_are_restricted_to_the_supported_extension_and_exact_resource() {
        let resolve = |id, endpoint| resolve(id, endpoint, Some("Iv1.test"), Some("test-secret"));
        assert!(resolve("github", GITHUB).is_some());
        assert!(resolve("asana", ASANA).is_some());
        assert!(resolve("asana", GITHUB).is_none());
        assert!(resolve("github", ASANA).is_none());
        for id in ["", "com.example.github", "github.extra", "linear"] {
            assert!(resolve(id, GITHUB).is_none());
        }
        for endpoint in [
            "https://example.com/mcp/",
            "https://api.githubcopilot.com/mcp",
            "https://api.githubcopilot.com/mcp/?redirect=example.com",
            "https://api.githubcopilot.com.evil.example/mcp/",
            "http://api.githubcopilot.com/mcp/",
        ] {
            assert!(resolve("github", endpoint).is_none());
        }
        for endpoint in [
            "https://mcp.asana.com/sse",
            "https://mcp.asana.com/v2/mcp/",
            "http://mcp.asana.com/v2/mcp",
            "https://mcp.asana.com.evil.example/v2/mcp",
        ] {
            assert!(resolve("asana", endpoint).is_none());
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
            assert!(resolve("github", GITHUB, id, secret).is_none());
        }
        assert!(resolve("github", GITHUB, Some(&"x".repeat(513)), Some("secret")).is_none());
        assert!(resolve("github", GITHUB, Some("client"), Some(&"x".repeat(4097))).is_none());
    }

    #[test]
    fn matching_issuer_cannot_redirect_the_credential_exchange() {
        let check =
            |issuer, authorize, token| metadata_matches("github", GITHUB, issuer, authorize, token);
        let issuer = Some("https://github.com/login/oauth");
        let authorize = "https://github.com/login/oauth/authorize";
        let token = "https://github.com/login/oauth/access_token";
        assert!(check(issuer, authorize, token));
        assert!(!check(None, authorize, token));
        assert!(!check(Some("https://example.com"), authorize, token));
        assert!(!check(issuer, "https://example.com/authorize", token));
        assert!(!check(issuer, authorize, "https://example.com/token"));
        assert!(!check(issuer, authorize, "https://github.com/other-token"));
    }

    #[test]
    fn each_build_credential_only_trusts_its_own_sign_in_server() {
        let asana = (
            Some("https://app.asana.com"),
            "https://app.asana.com/-/oauth_authorize",
            "https://app.asana.com/-/oauth_token",
        );
        assert!(metadata_matches("asana", ASANA, asana.0, asana.1, asana.2));
        assert!(!metadata_matches(
            "github", GITHUB, asana.0, asana.1, asana.2
        ));
        assert!(!metadata_matches(
            "asana",
            ASANA,
            Some("https://github.com/login/oauth"),
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token"
        ));
        assert!(!metadata_matches(
            "asana",
            "https://mcp.asana.com/sse",
            asana.0,
            asana.1,
            asana.2
        ));
    }
}
