//! [GRAIN] Shared reqwest transport-error diagnostics.
//!
//! Ported from upstream Handy #1823 ("preserve HTTP transport error causes")
//! and shared across Grain's multi-provider cloud clients — the LLM client
//! ([`crate::grain_llm_client`]) and the cloud STT client
//! ([`crate::stt_client`]). Both previously formatted only the top-level
//! `reqwest::Error`, whose `Display` intentionally omits the nested cause, so a
//! certificate failure / connection reset / proxy error surfaced as a useless
//! "request failed". Upstream's fix lives in its single-provider `llm_client`;
//! Grain's STT client is a Grain-only parallel path the merge can never reach,
//! so the same helper is applied to both here.
//!
//! Every function is written to never leak payload data: URLs are sanitized of
//! credentials/query tokens, and decode errors keep only their classification
//! because serde's error text can quote a malformed response body — which may
//! contain transcription content.

use log::error;
use std::error::Error as StdError;

/// Walk a bounded error source chain. The nested causes carry the useful
/// transport detail; the cap guards against a third-party cyclic chain.
fn error_source_chain(error: &(dyn StdError + 'static)) -> Vec<String> {
    let mut causes = Vec::new();
    let mut source = error.source();

    for _ in 0..16 {
        let Some(cause) = source else {
            break;
        };
        causes.push(cause.to_string());
        source = cause.source();
    }

    causes
}

/// Classify a `reqwest::Error` into the reqwest error-kind flags that are set.
fn reqwest_error_kinds(error: &reqwest::Error) -> String {
    let mut kinds = Vec::new();
    if error.is_builder() {
        kinds.push("builder");
    }
    if error.is_connect() {
        kinds.push("connect");
    }
    if error.is_request() {
        kinds.push("request");
    }
    if error.is_redirect() {
        kinds.push("redirect");
    }
    if error.is_timeout() {
        kinds.push("timeout");
    }
    if error.is_status() {
        kinds.push("status");
    }
    if error.is_body() {
        kinds.push("body");
    }
    if error.is_decode() {
        kinds.push("decode");
    }
    if error.is_upgrade() {
        kinds.push("upgrade");
    }

    if kinds.is_empty() {
        "unknown".to_string()
    } else {
        kinds.join(", ")
    }
}

/// Strip credentials and query/fragment from a URL before it reaches logs or UI.
/// Custom endpoints should not carry secrets, but omit them in case one does.
fn sanitized_url(url: &reqwest::Url) -> String {
    let mut url = url.clone();
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

/// Build a diagnostic string that surfaces the transport cause while never
/// leaking payload data, and log it at error level. Returns the same string so
/// callers can thread it into their own error type.
pub(crate) fn report_reqwest_error(context: &str, error: &reqwest::Error) -> String {
    let kinds = reqwest_error_kinds(error);
    let url = error
        .url()
        .map(sanitized_url)
        .map(|url| format!(", url: {url}"))
        .unwrap_or_default();

    // serde_json's error text can quote values from a malformed response, which
    // may contain transcription content: keep the decode classification but
    // never put its nested source in logs or UI errors.
    let causes = if error.is_decode() {
        Vec::new()
    } else {
        error_source_chain(error)
    };
    let cause_details = if !causes.is_empty() {
        format!(": caused by: {}", causes.join(" -> "))
    } else if error.url().is_none() {
        // Reqwest's short Display text is safe when it cannot append a raw URL.
        format!(": {error}")
    } else {
        // The sanitized URL is already included above; the raw error's Display
        // would re-introduce the unsanitized URL.
        String::new()
    };

    let details = format!("{context} (kind: {kinds}{url}){cause_details}");
    error!("{details}");
    details
}
