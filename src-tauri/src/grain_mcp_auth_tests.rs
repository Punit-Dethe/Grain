//! Production callback parser/gates and the actual pinned SDK OAuth client.
//! Local HTTP and memory credentials only; no browser, live account or OS vault.
use super::*;
use rmcp::transport::auth::{AuthorizationMetadata, InMemoryCredentialStore};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn hosted_metadata_requires_advertised_pkce_issuer_and_https_endpoints() {
    let mut metadata = AuthorizationMetadata::default();
    metadata.authorization_endpoint = "https://issuer.example/authorize".into();
    metadata.token_endpoint = "https://issuer.example/token".into();
    metadata.issuer = Some("https://issuer.example".into());
    metadata.code_challenge_methods_supported = Some(vec!["S256".into()]);
    assert!(validate_hosted_oauth_metadata(&metadata).is_ok());
    let mut missing_pkce = metadata.clone();
    missing_pkce.code_challenge_methods_supported = None;
    assert!(validate_hosted_oauth_metadata(&missing_pkce).is_err());
    missing_pkce.code_challenge_methods_supported = Some(vec!["plain".into()]);
    assert!(validate_hosted_oauth_metadata(&missing_pkce).is_err());
    let mut missing_issuer = metadata.clone();
    missing_issuer.issuer = None;
    assert!(validate_hosted_oauth_metadata(&missing_issuer).is_err());
    for endpoint in [
        "http://issuer.example/token",
        "https://secret@issuer.example/token",
        "https://issuer.example/token#fragment",
    ] {
        let mut unsafe_token = metadata.clone();
        unsafe_token.token_endpoint = endpoint.into();
        assert!(validate_hosted_oauth_metadata(&unsafe_token).is_err());
    }
    metadata.registration_endpoint = Some("http://issuer.example/register".into());
    assert!(validate_hosted_oauth_metadata(&metadata).is_err());
    metadata.registration_endpoint = None;
    metadata.additional_fields.insert(
        "authorization_response_iss_parameter_supported".into(),
        json!("true"),
    );
    assert!(validate_hosted_oauth_metadata(&metadata).is_err());
}

fn callback(query: &str) -> String {
    format!("GET {CALLBACK_PATH}?{query} HTTP/1.1\r\nHost: {CALLBACK_ADDR}\r\n\r\n")
}

#[test]
fn callback_checks_state_issuer_and_ambiguity_before_denial() {
    let issuer = "https://issuer.example";
    for query in [
        "state=wrong&code=code&iss=https%3A%2F%2Fissuer.example",
        "state=expected&state=expected&code=code&iss=https%3A%2F%2Fissuer.example",
        "state=expected&code=code",
        "state=expected&error=access_denied&iss=https%3A%2F%2Fevil.example",
        "state=expected&code=code&iss=https%3A%2F%2Fissuer.example%2F",
        "state=expected&error=access_denied&code=code&iss=https%3A%2F%2Fissuer.example",
    ] {
        assert!(matches!(
            parse_callback(
                &callback(query),
                CALLBACK_ADDR,
                "expected",
                Some(issuer),
                true
            ),
            CallbackDecision::Invalid
        ));
    }
    assert!(matches!(
        parse_callback(
            &callback("state=expected&error=access_denied&iss=https%3A%2F%2Fissuer.example"),
            CALLBACK_ADDR,
            "expected",
            Some(issuer),
            true
        ),
        CallbackDecision::Denied
    ));
    let oversized = format!("state=expected&code=code&iss={}", "x".repeat(2049));
    assert!(matches!(
        parse_callback(
            &callback(&oversized),
            CALLBACK_ADDR,
            "expected",
            None,
            false
        ),
        CallbackDecision::Invalid
    ));
    let double_host = callback("state=expected&code=code")
        .replace("\r\n\r\n", &format!("\r\nHost: {CALLBACK_ADDR}\r\n\r\n"));
    assert!(matches!(
        parse_callback(&double_host, CALLBACK_ADDR, "expected", None, false),
        CallbackDecision::Invalid
    ));
    assert!(matches!(
        parse_callback(
            &callback("state=expected&code=code"),
            CALLBACK_ADDR,
            "expected",
            None,
            false
        ),
        CallbackDecision::Accepted(_)
    ));
    let truncated = callback("state=expected&code=code").trim_end().to_string();
    assert!(matches!(
        parse_callback(&truncated, CALLBACK_ADDR, "expected", None, false),
        CallbackDecision::Invalid
    ));
    let rewritten =
        callback("state=expected&code=code").replace(CALLBACK_PATH, "/other/../mcp/oauth/callback");
    assert!(matches!(
        parse_callback(&rewritten, CALLBACK_ADDR, "expected", None, false),
        CallbackDecision::Invalid
    ));
}

#[test]
fn dropping_login_future_owner_invalidates_pending_vault_commits() {
    let control = session::Control::new();
    let ticket = control.ticket();
    let owner = ConnectGuard {
        id: "fixture".into(),
        ticket: Some(ticket.clone()),
    };
    drop(owner);
    assert!(ticket.commit(|| "late token write").is_err());
}

#[tokio::test]
async fn callback_cancellation_releases_port_and_rejects_late_login() {
    let control = session::Control::new();
    let ticket = control.ticket();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let running = tokio::spawn(async move {
        ticket
            .run(await_callback(listener, "expected", None, false))
            .await
    });
    tokio::task::yield_now().await;
    control.invalidate();
    assert!(running.await.unwrap().is_err());
    assert!(TcpListener::bind(addr).await.is_ok());
}

struct TokenServer {
    origin: String,
    requests: Arc<Mutex<Vec<BTreeMap<String, String>>>>,
    calls: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl TokenServer {
    async fn start(status: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let captured = requests.clone();
        let counter = calls.clone();
        let status = status.to_string();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let body = tokio::time::timeout(Duration::from_secs(3), read_form(&mut stream))
                    .await
                    .unwrap();
                let params = reqwest_mcp::Url::parse(&format!("http://localhost/?{body}"))
                    .unwrap()
                    .query_pairs()
                    .into_owned()
                    .collect();
                captured.lock().unwrap().push(params);
                counter.fetch_add(1, Ordering::SeqCst);
                let response = if status.starts_with("200") {
                    json!({"access_token":"new-token","refresh_token":"rotated-token","token_type":"Bearer","expires_in":3600})
                } else { json!({"error":"temporarily_unavailable"}) }.to_string();
                let headers = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len());
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self {
            origin,
            requests,
            calls,
            task,
        }
    }

    fn metadata(&self) -> AuthorizationMetadata {
        let mut metadata = AuthorizationMetadata::default();
        metadata.authorization_endpoint = format!("{}/authorize", self.origin);
        metadata.token_endpoint = format!("{}/token", self.origin);
        metadata.issuer = Some(self.origin.clone());
        metadata.code_challenge_methods_supported = Some(vec!["S256".into()]);
        metadata.response_types_supported = Some(vec!["code".into()]);
        metadata.additional_fields.insert(
            "authorization_response_iss_parameter_supported".into(),
            json!(true),
        );
        metadata
    }

    async fn manager(&self, store: InMemoryCredentialStore) -> AuthorizationManager {
        let mut manager = AuthorizationManager::new(&format!("{}/mcp", self.origin))
            .await
            .unwrap();
        manager
            .with_client(McpHttpClient::build().unwrap().0)
            .unwrap();
        manager.set_metadata(self.metadata());
        manager.set_credential_store(store);
        manager
    }
}

impl Drop for TokenServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_form(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0 && bytes.len() + count <= 16 * 1024);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    assert!(headers.starts_with("POST /token "));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    assert!(length < 8192);
    while bytes.len() < header_end + length {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
    }
    String::from_utf8(bytes[header_end..header_end + length].to_vec()).unwrap()
}

#[tokio::test]
async fn public_client_uses_pkce_resource_and_validates_before_token_exchange() {
    let server = TokenServer::start("200 OK").await;
    let store = InMemoryCredentialStore::new();
    let oauth = AuthorizationSession::new(
        server.manager(store.clone()).await,
        AuthorizationRequest::new(&format!("http://{CALLBACK_ADDR}{CALLBACK_PATH}"))
            .with_preregistered_client("public-client")
            .with_application_type("native"),
    )
    .await
    .map_err(|(_, error)| error)
    .unwrap();
    let url = reqwest_mcp::Url::parse(oauth.get_authorization_url()).unwrap();
    let params: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["resource"], format!("{}/mcp", server.origin));
    assert!(!params.contains_key("client_secret"));
    assert!(oauth
        .handle_callback_with_issuer("code", "wrong-state", Some(&server.origin))
        .await
        .is_err());
    assert!(oauth
        .handle_callback_with_issuer("code", &params["state"], Some("https://wrong.example"))
        .await
        .is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    oauth
        .handle_callback_with_issuer("code", &params["state"], Some(&server.origin))
        .await
        .unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0]["code_verifier"].len() >= 43);
    assert_eq!(requests[0]["resource"], format!("{}/mcp", server.origin));
    assert!(!requests[0].contains_key("client_secret"));
    drop(requests);
    assert!(store
        .load()
        .await
        .unwrap()
        .unwrap()
        .token_response
        .is_some());
}

async fn expired_store(server: &TokenServer) -> InMemoryCredentialStore {
    let store = InMemoryCredentialStore::new();
    store.save(StoredCredentials::new("public-client".into(), Some(serde_json::from_value(json!({
        "access_token":"expired-token", "refresh_token":"old-refresh", "token_type":"Bearer", "expires_in":1
    })).unwrap()), vec![], Some(1)).with_issuer(Some(server.origin.clone()))).await.unwrap();
    store
}

async fn fresh_client_token(
    server: &TokenServer,
    store: InMemoryCredentialStore,
    ticket: session::Ticket,
) -> Result<String, String> {
    let _operation = ticket.acquire().await?;
    ticket
        .run(async {
            let mut manager = server.manager(store).await;
            assert!(manager.initialize_from_store().await.unwrap());
            manager.get_access_token().await.map_err(|e| e.to_string())
        })
        .await?
}

#[tokio::test]
async fn short_lived_clients_share_rotated_refresh_through_the_provider_gate() {
    let server = TokenServer::start("200 OK").await;
    let store = expired_store(&server).await;
    let control = session::Control::new();
    let (first, second) = tokio::join!(
        fresh_client_token(&server, store.clone(), control.ticket()),
        fresh_client_token(&server, store.clone(), control.ticket())
    );
    assert_eq!(first.unwrap(), "new-token");
    assert_eq!(second.unwrap(), "new-token");
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        server.requests.lock().unwrap()[0]["refresh_token"],
        "old-refresh"
    );
}

#[tokio::test]
async fn transient_refresh_failure_preserves_stored_grant() {
    let server = TokenServer::start("503 Service Unavailable").await;
    let store = expired_store(&server).await;
    let before = serde_json::to_value(store.load().await.unwrap()).unwrap();
    assert!(
        fresh_client_token(&server, store.clone(), session::Control::new().ticket())
            .await
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(store.load().await.unwrap()).unwrap(),
        before
    );
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
}
