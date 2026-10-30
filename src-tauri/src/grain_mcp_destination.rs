//! Destination policy for the shared MCP HTTP and official SDK OAuth adapters.
//! Validate destinations before forwarding credentials and return checked DNS addresses
//! directly to reqwest's connector, never a check-then-resolve permission.

use std::{io, net::IpAddr, sync::Arc, time::Duration};

use reqwest_mcp::{
    dns::{Addrs, Name, Resolve, Resolving},
    Client, Url,
};
use rmcp::transport::auth::{
    OAuthHttpClient, OAuthHttpClientError, OAuthHttpClientFuture, OAuthHttpRequest,
};

const MAX_DNS_ADDRESSES: usize = 64;
const MAX_OAUTH_BYTES: usize = 1024 * 1024;
const REFUSAL: &str = "MCP destination is not an allowed public HTTPS service";

fn refusal() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, REFUSAL)
}

#[derive(Clone)]
pub(super) enum Policy {
    Public,
    // Only explicitly owned acceptance peers, absent from release builds.
    // The caller must first validate the harness marker and scoped TLS client.
    #[cfg(any(test, feature = "agent-harness"))]
    OwnedOrigins(Vec<Url>),
}

impl Policy {
    pub(super) fn validate(&self, url: &Url) -> Result<(), io::Error> {
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.port() == Some(0)
        {
            return Err(refusal());
        }
        #[cfg(any(test, feature = "agent-harness"))]
        if let Self::OwnedOrigins(origins) = self {
            return if origins.iter().any(|origin| url.origin() == origin.origin()) {
                Ok(())
            } else {
                Err(refusal())
            };
        }
        let host = url.host_str().ok_or_else(refusal)?;
        if host.starts_with('[') || host.parse::<IpAddr>().is_ok() || !public_name(host) {
            return Err(refusal());
        }
        Ok(())
    }

    pub(super) fn validate_uri(&self, uri: &str) -> Result<(), io::Error> {
        self.validate(&Url::parse(uri).map_err(|_| refusal())?)
    }
}

fn public_name(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    host.contains('.')
        && !host.split('.').any(str::is_empty)
        && !["localhost", "local", "internal", "lan", "home", "home.arpa"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

// Conservative Internet-service policy, based on IANA special-purpose ranges.
// Some special anycast exceptions are intentionally excluded. Do not broaden
// this to accept LAN, NAT64, mapped IPv4, Teredo or 6to4 for remote descriptors.
fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(matches!(a, 0 | 10 | 127 | 224..=255)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && ((b == 0 && matches!(c, 0 | 2)) || (b == 88 && c == 99) || b == 168))
                || (a == 198 && (matches!(b, 18 | 19) || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

struct SystemDns;
impl Resolve for SystemDns {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .take(MAX_DNS_ADDRESSES + 1)
                .collect();
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

struct PublicDns<R>(R);
impl<R: Resolve> Resolve for PublicDns<R> {
    fn resolve(&self, name: Name) -> Resolving {
        if !public_name(name.as_str()) {
            return Box::pin(async { Err(refusal().into()) });
        }
        let resolution = self.0.resolve(name);
        Box::pin(async move {
            let addresses = tokio::time::timeout(Duration::from_secs(10), resolution)
                .await
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "MCP DNS lookup timed out")
                })??;
            let addresses: Vec<_> = addresses.take(MAX_DNS_ADDRESSES + 1).collect();
            if addresses.is_empty()
                || addresses.len() > MAX_DNS_ADDRESSES
                || addresses
                    .iter()
                    .any(|address| !public_address(address.ip()))
            {
                return Err(refusal().into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

pub(super) fn public_builder(builder: reqwest_mcp::ClientBuilder) -> reqwest_mcp::ClientBuilder {
    builder
        .redirect(reqwest_mcp::redirect::Policy::none())
        .https_only(true)
        .no_proxy()
        .dns_resolver(Arc::new(PublicDns(SystemDns)))
}

// The SDK owns discovery, registration, exchange and refresh. This is only its
// documented HTTP seam; every manual discovery redirect passes here again.
pub(super) struct OAuthClient {
    http: Client,
    policy: Policy,
}

impl OAuthClient {
    pub(super) fn new(http: Client, policy: Policy) -> Self {
        Self { http, policy }
    }

    async fn execute_request(
        &self,
        request: oauth2::HttpRequest,
    ) -> Result<oauth2::HttpResponse, OAuthHttpClientError> {
        let request = reqwest_mcp::Request::try_from(request)?;
        self.policy.validate(request.url())?;
        // The supplied host client never follows redirects. SDK metadata
        // redirects are handled by the SDK and rechecked, tokens never follow.
        let mut response = self.http.execute(request).await?;
        let mut builder = oauth2::http::Response::builder()
            .status(response.status())
            .version(response.version());
        for (name, value) in response.headers() {
            builder = builder.header(name, value);
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_OAUTH_BYTES as u64)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OAuth response exceeds the byte limit",
            )
            .into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > MAX_OAUTH_BYTES - body.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "OAuth response exceeds the byte limit",
                )
                .into());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(builder.body(body)?)
    }
}

impl OAuthHttpClient for OAuthClient {
    fn execute(&self, request: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
        Box::pin(self.execute_request(request.request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::SocketAddr, str::FromStr};

    #[test]
    fn url_policy_blocks_literals_credentials_local_names_and_downgrades() {
        for uri in [
            "http://example.com/mcp",
            "https://127.0.0.1/mcp",
            "https://[::1]/mcp",
            "https://2130706433/mcp",
            "https://0x7f000001/mcp",
            "https://user:secret@example.com/mcp",
            "https://example.com/#secret",
            "https://example.com:0/mcp",
            "https://localhost/mcp",
            "https://one.LOCALHOST./mcp",
            "https://metadata.google.internal/mcp",
            "https://printer.local/mcp",
            "https://one.home.arpa/mcp",
            "https://one.lan/mcp",
            "https://one.home/mcp",
            "https://singlelabel/mcp",
        ] {
            assert!(Policy::Public.validate_uri(uri).is_err(), "{uri}");
        }
        for uri in [
            "https://mcp.linear.app/mcp",
            "https://accounts.example.com/authorize?scope=read",
            "https://example.com:8443/mcp",
            "https://example.com./mcp",
        ] {
            Policy::Public.validate_uri(uri).unwrap();
        }
    }

    #[test]
    fn special_addresses_are_refused_but_public_ipv4_ipv6_work() {
        for ip in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "127.1.2.3",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.9",
            "192.0.2.1",
            "192.88.99.2",
            "192.168.1.1",
            "198.18.1.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:8.8.8.8",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
            "100::1",
            "2001::1",
            "2001:2::1",
            "2001:db8::1",
            "2002:0808:0808::1",
            "3fff::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
        ] {
            assert!(!public_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.31.196.1",
            "2001:4860:4860::8888",
            "2606:4700:4700::1111",
        ] {
            assert!(public_address(ip.parse().unwrap()), "{ip}");
        }
    }

    struct FixedDns(Vec<SocketAddr>);
    impl Resolve for FixedDns {
        fn resolve(&self, _: Name) -> Resolving {
            let addresses = self.0.clone();
            Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
        }
    }

    #[tokio::test]
    async fn checked_dns_addresses_are_returned_exactly_and_mixed_answers_fail() {
        let good = SocketAddr::from_str("8.8.8.8:0").unwrap();
        let other = SocketAddr::from_str("[2606:4700:4700::1111]:0").unwrap();
        let name = || Name::from_str("service.example.com").unwrap();
        let actual: Vec<_> = PublicDns(FixedDns(vec![good, other]))
            .resolve(name())
            .await
            .unwrap()
            .collect();
        assert_eq!(actual, vec![good, other]);
        assert_eq!(
            PublicDns(FixedDns(vec![good; MAX_DNS_ADDRESSES]))
                .resolve(name())
                .await
                .unwrap()
                .count(),
            MAX_DNS_ADDRESSES
        );
        for addresses in [
            vec![],
            vec![good; MAX_DNS_ADDRESSES + 1],
            vec![good, "127.0.0.1:0".parse().unwrap()],
            vec!["[::ffff:127.0.0.1]:0".parse().unwrap()],
        ] {
            assert!(PublicDns(FixedDns(addresses))
                .resolve(name())
                .await
                .is_err());
        }
        assert!(PublicDns(FixedDns(vec![good]))
            .resolve(Name::from_str("metadata.google.internal").unwrap())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn stalled_dns_wait_is_bounded_and_drops_its_owned_future() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct PendingDns(Arc<AtomicBool>);
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        impl Resolve for PendingDns {
            fn resolve(&self, _: Name) -> Resolving {
                let guard = Dropped(self.0.clone());
                Box::pin(async move {
                    let _guard = guard;
                    std::future::pending().await
                })
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let started = std::time::Instant::now();
        let error = PublicDns(PendingDns(dropped.clone()))
            .resolve("service.example.com".parse().unwrap())
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(started.elapsed() >= Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(15));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn dns_rebinding_is_rechecked_on_each_resolution() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct ChangingDns(AtomicUsize);
        impl Resolve for ChangingDns {
            fn resolve(&self, _: Name) -> Resolving {
                let address = if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    "8.8.8.8:0"
                } else {
                    "127.0.0.1:0"
                }
                .parse()
                .unwrap();
                Box::pin(async move { Ok(Box::new(std::iter::once(address)) as Addrs) })
            }
        }
        let dns = PublicDns(ChangingDns(AtomicUsize::new(0)));
        assert!(dns
            .resolve("service.example.com".parse().unwrap())
            .await
            .is_ok());
        assert!(dns
            .resolve("service.example.com".parse().unwrap())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn denied_dns_never_reaches_an_actual_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = public_builder(Client::builder())
            .dns_resolver(Arc::new(PublicDns(FixedDns(vec![address]))))
            .build()
            .unwrap();
        for method in [
            reqwest_mcp::Method::GET,
            reqwest_mcp::Method::POST,
            reqwest_mcp::Method::DELETE,
        ] {
            assert!(client
                .request(
                    method,
                    format!("https://service.example.com:{}", address.port())
                )
                .bearer_auth("must-not-leak")
                .send()
                .await
                .is_err());
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn oauth_literal_refusal_precedes_sending_any_credentials() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = OAuthClient::new(
            public_builder(Client::builder()).build().unwrap(),
            Policy::Public,
        );
        let request = oauth2::http::Request::builder()
            .method("POST")
            .uri(format!(
                "https://127.0.0.1:{}/token",
                listener.local_addr().unwrap().port()
            ))
            .header("authorization", "Bearer must-not-leak")
            .body(b"secret=must-not-leak".to_vec())
            .unwrap();
        assert_eq!(
            client
                .execute_request(request)
                .await
                .unwrap_err()
                .to_string(),
            REFUSAL
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn owned_fixture_policy_is_exact_origin_and_still_requires_https() {
        let policy = Policy::OwnedOrigins(vec![Url::parse("https://127.0.0.1:1234/mcp").unwrap()]);
        policy
            .validate_uri("https://127.0.0.1:1234/token?scope=read")
            .unwrap();
        for uri in [
            "https://127.0.0.1:1235/token",
            "https://localhost:1234/token",
            "http://127.0.0.1:1234/token",
            "https://example.com/token",
            "https://user:secret@127.0.0.1:1234/token",
        ] {
            assert!(policy.validate_uri(uri).is_err(), "{uri}");
        }
    }
}
