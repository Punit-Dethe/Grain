//! Google sign-in for the local Google Workspace server (workspace-mcp).
//!
//! Grain owns the sign-in: the user's own Google "Desktop app" OAuth client,
//! a loopback redirect with PKCE, and the grant kept in the OS vault. The
//! server never runs its own sign-in. It gets a token file only while it runs
//! (see [`write_session`]); the file is deleted when the server stops.
use super::session::Ticket;
use super::{await_callback, bind_callback, credential_entry, delete_vault_entry, CALLBACK_PATH};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

pub(super) const PROVIDER_ID: &str = "google-workspace";
const VAULT: &str = "com.grain.mcp.google";
const ISSUER: &str = "https://accounts.google.com";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SAVED_BYTES: usize = 16 * 1024;

/// Scopes for the services Grain launches the server with (gmail, calendar,
/// drive, docs, sheets). Each broad scope covers the narrower ones the
/// server checks per tool (workspace-mcp 2.2.1 `SCOPE_HIERARCHY`).
pub(super) const SCOPES: &[&str] = &[
    "openid",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.settings.basic",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/drive",
    "https://www.googleapis.com/auth/documents",
    "https://www.googleapis.com/auth/spreadsheets",
];

#[derive(Serialize, Deserialize)]
struct Saved {
    schema: u8,
    client_id: String,
    client_secret: String,
    #[serde(default)]
    account: Option<Account>,
}

#[derive(Serialize, Deserialize)]
struct Account {
    email: String,
    refresh_token: String,
    scopes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Vault

async fn load() -> Result<Option<Saved>, String> {
    tokio::task::spawn_blocking(|| {
        let entry = credential_entry(VAULT, PROVIDER_ID)
            .map_err(|_| "OS credential vault unavailable".to_string())?;
        match entry.get_password() {
            Ok(value) if value.len() <= MAX_SAVED_BYTES => serde_json::from_str(&value)
                .map(Some)
                .map_err(|_| "Saved Google settings are unreadable. Add your client again.".into()),
            Ok(_) => Err("Saved Google settings are too large.".into()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("OS credential vault read failed".into()),
        }
    })
    .await
    .map_err(|_| "OS credential vault read failed".to_string())?
}

async fn store(saved: Option<Saved>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || match saved {
        None => delete_vault_entry(VAULT, PROVIDER_ID)
            .map_err(|_| "OS credential vault delete failed".to_string()),
        Some(saved) => {
            let value = zeroize::Zeroizing::new(
                serde_json::to_string(&saved).map_err(|_| "Could not encode Google settings")?,
            );
            credential_entry(VAULT, PROVIDER_ID)
                .and_then(|entry| entry.set_password(&value))
                .map_err(|_| "OS credential vault write failed".to_string())
        }
    })
    .await
    .map_err(|_| "OS credential vault write failed".to_string())?
}

pub(super) async fn client_configured() -> bool {
    matches!(load().await, Ok(Some(_)))
}

pub(super) async fn connected() -> Result<bool, String> {
    Ok(load().await?.is_some_and(|saved| saved.account.is_some()))
}

// ---------------------------------------------------------------------------
// Client and account

fn printable(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(|c| c.is_control() || c == ' ')
}

/// Save the user's Desktop OAuth client. A new client ends any old sign-in:
/// its grant belongs to the old client.
pub(super) async fn save_client(client_id: &str, client_secret: &str) -> Result<(), String> {
    let client_id = client_id.trim();
    let client_secret = client_secret.trim();
    if !printable(client_id, 512) || !client_id.ends_with(".apps.googleusercontent.com") {
        return Err("Enter the client ID of a Google OAuth client (it ends in .apps.googleusercontent.com).".into());
    }
    if !printable(client_secret, 512) {
        return Err("Enter the client secret of the same Google OAuth client.".into());
    }
    revoke_saved().await;
    store(Some(Saved {
        schema: 1,
        client_id: client_id.into(),
        client_secret: client_secret.into(),
        account: None,
    }))
    .await
}

/// Sign out: revoke the grant with Google (best effort), keep the client.
pub(super) async fn disconnect() -> Result<(), String> {
    revoke_saved().await;
    let Some(mut saved) = load().await? else {
        return Ok(());
    };
    saved.account = None;
    store(Some(saved)).await
}

async fn revoke_saved() {
    if let Ok(Some(Saved {
        account: Some(account),
        ..
    })) = load().await
    {
        revoke(&account.refresh_token).await;
    }
}

async fn revoke(token: &str) {
    let Ok(http) = http() else {
        return;
    };
    let result = http.post(REVOKE_URL).form(&[("token", token)]).send().await;
    if !result.is_ok_and(|response| response.status().is_success()) {
        log::warn!("[GRAIN] mcp-google could not revoke the old grant");
    }
}

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not prepare the Google sign-in.".into())
}

fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

#[derive(Deserialize)]
struct TokenResponse {
    refresh_token: Option<String>,
    scope: Option<String>,
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct IdClaims {
    email: Option<String>,
    email_verified: Option<bool>,
    iss: Option<String>,
    aud: Option<String>,
}

/// The ID token comes straight from Google's token endpoint over TLS, so its
/// claims are read without a signature check (OpenID Connect Core 3.1.3.7).
fn id_claims(id_token: &str) -> Option<IdClaims> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(super) fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    SCOPES
        .iter()
        .copied()
        .filter(|scope| {
            // Google may report the identity scopes by their short names.
            let short = match *scope {
                "https://www.googleapis.com/auth/userinfo.email" => "email",
                "https://www.googleapis.com/auth/userinfo.profile" => "profile",
                other => other,
            };
            !granted
                .iter()
                .any(|granted| granted == scope || granted == short)
        })
        .collect()
}

/// Run the browser sign-in and save the grant. The caller (connect_flow)
/// owns cancellation, the timeout and the ticket.
pub(super) async fn connect(app: &AppHandle, ticket: &Ticket) -> Result<(), String> {
    let _operation = ticket.acquire().await?;
    let saved = load()
        .await?
        .ok_or("Add your Google OAuth client ID and secret first.")?;
    let listener = bind_callback(false).await?;
    let redirect_uri = format!(
        "http://{}{CALLBACK_PATH}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    let state = random();
    let verifier = random();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    let scope = SCOPES.join(" ");
    let url = reqwest::Url::parse_with_params(
        AUTH_URL,
        [
            ("client_id", saved.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", scope.as_str()),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            // A refresh token is issued only with offline access and consent.
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|_| "Could not build the Google sign-in link.")?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|_| "Could not open the sign-in page.")?;
    let callback = await_callback(listener, &state, Some(ISSUER), false).await?;

    let response = http()?
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", callback.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", saved.client_id.as_str()),
            ("client_secret", saved.client_secret.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|_| "Could not reach Google to finish signing in.")?;
    if !response.status().is_success() {
        #[derive(Deserialize)]
        struct Failure {
            error: Option<String>,
        }
        let error = response
            .json::<Failure>()
            .await
            .ok()
            .and_then(|failure| failure.error)
            .unwrap_or_default();
        log::warn!("[GRAIN] mcp-google token exchange failed: {error}");
        return Err(match error.as_str() {
            "invalid_client" | "unauthorized_client" => {
                "Google rejected the OAuth client. Check the client ID and secret, and that the client type is Desktop app."
            }
            _ => "Google sign-in failed. Try again.",
        }
        .into());
    }
    let tokens: TokenResponse = response
        .json()
        .await
        .map_err(|_| "Google returned an unreadable sign-in response.")?;
    let refresh_token = tokens
        .refresh_token
        .ok_or("Google did not grant offline access. Try connecting again.")?;
    let granted: Vec<String> = tokens
        .scope
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let missing = missing_scopes(&granted);
    if !missing.is_empty() {
        revoke(&refresh_token).await;
        log::warn!("[GRAIN] mcp-google missing scopes: {missing:?}");
        return Err("Google Workspace needs every permission on the Google consent screen. Connect again and leave all boxes ticked.".into());
    }
    let claims = tokens
        .id_token
        .as_deref()
        .and_then(id_claims)
        .ok_or("Google did not return the account email.")?;
    let email = match claims {
        IdClaims {
            email: Some(email),
            email_verified: Some(true),
            iss: Some(iss),
            aud: Some(aud),
        } if (iss == ISSUER || iss == "accounts.google.com")
            && aud == saved.client_id
            && printable(&email, 320)
            && email.contains('@') =>
        {
            email
        }
        _ => return Err("Google did not return a verified account email.".into()),
    };
    // Every scope was granted; save the canonical names the server checks.
    let account = Account {
        email,
        refresh_token,
        scopes: SCOPES.iter().map(|scope| scope.to_string()).collect(),
    };
    // Only a current sign-in may replace the saved grant.
    ticket.commit(|| ())?;
    store(Some(Saved {
        account: Some(account),
        ..saved
    }))
    .await
}

// ---------------------------------------------------------------------------
// Server session files

/// Write the server's credential file into `dir` (replacing anything there)
/// and return the environment that points the server at it.
pub(super) async fn write_session(dir: &Path) -> Result<Vec<(&'static str, String)>, String> {
    let saved = load().await?.ok_or("Connect Google Workspace first.")?;
    let account = saved
        .account
        .as_ref()
        .ok_or("Connect Google Workspace first.")?;
    let file = serde_json::json!({
        "token": null,
        "refresh_token": account.refresh_token,
        "token_uri": TOKEN_URL,
        "client_id": saved.client_id,
        "client_secret": saved.client_secret,
        "scopes": account.scopes,
        "expiry": null,
    });
    let file = zeroize::Zeroizing::new(
        serde_json::to_vec_pretty(&file).map_err(|_| "Could not prepare the Google session")?,
    );
    let _ = tokio::fs::remove_dir_all(dir).await;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("Could not prepare the Google session: {error}"))?;
    tokio::fs::write(dir.join(file_name(&account.email)), file.as_slice())
        .await
        .map_err(|error| format!("Could not prepare the Google session: {error}"))?;
    Ok(vec![
        (
            "WORKSPACE_MCP_CREDENTIALS_DIR",
            dir.to_string_lossy().into_owned(),
        ),
        ("GOOGLE_OAUTH_CLIENT_ID", saved.client_id.clone()),
        ("GOOGLE_OAUTH_CLIENT_SECRET", saved.client_secret.clone()),
        ("USER_GOOGLE_EMAIL", account.email.clone()),
    ])
}

/// workspace-mcp's file name: the email, URL-encoded except `@._-`.
fn file_name(email: &str) -> String {
    let mut name = String::with_capacity(email.len() + 5);
    for byte in email.bytes() {
        if byte.is_ascii_alphanumeric() || b"@._-".contains(&byte) {
            name.push(byte as char);
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    name.push_str(".json");
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_file_names_match_workspace_mcp() {
        assert_eq!(file_name("me@gmail.com"), "me@gmail.com.json");
        assert_eq!(file_name("a+b@x.io"), "a%2Bb@x.io.json");
    }

    #[test]
    fn every_scope_must_be_granted() {
        let all: Vec<String> = SCOPES.iter().map(|s| s.to_string()).collect();
        assert!(missing_scopes(&all).is_empty());
        assert_eq!(
            missing_scopes(&all[1..]),
            vec!["openid"],
            "a narrowed grant is refused"
        );
    }

    #[test]
    fn id_token_claims_are_read() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"iss":"https://accounts.google.com","aud":"x.apps.googleusercontent.com","email":"me@gmail.com","email_verified":true}"#,
        );
        let claims = id_claims(&format!("h.{payload}.s")).unwrap();
        assert_eq!(claims.email.as_deref(), Some("me@gmail.com"));
        assert_eq!(claims.email_verified, Some(true));
    }

    #[test]
    fn only_desktop_client_ids_are_accepted() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert!(runtime
            .block_on(save_client("not-a-google-id", "secret"))
            .is_err());
        assert!(runtime
            .block_on(save_client("x.apps.googleusercontent.com", ""))
            .is_err());
    }
}
