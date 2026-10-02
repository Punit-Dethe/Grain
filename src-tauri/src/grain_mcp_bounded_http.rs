//! Byte-bounded HTTP binding for the pinned rmcp transport. Protocol negotiation,
//! auth, request IDs, header projection and session ownership remain in rmcp.
//! Unlike its default reqwest binding, JSON/error bodies are bounded before
//! decoding. SSE has a stricter whole-response wire budget (including comments).

use futures_util::{stream, StreamExt};
use reqwest_mcp::{
    header::{HeaderName, HeaderValue},
    Client, RequestBuilder, Response, StatusCode,
};
use rmcp::{
    model::{ClientJsonRpcMessage, JsonRpcMessage, ServerJsonRpcMessage},
    transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, StreamableHttpClient, StreamableHttpError,
        StreamableHttpPostResponse,
    },
};
use sse_stream::{Error as SseError, Sse, SseStream};
use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

pub(super) const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_OPERATION_BYTES: usize = 8 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 16 * 1024;
const MAX_HEADER_BYTES: usize = 8 * 1024;
type HttpError = StreamableHttpError<reqwest_mcp::Error>;

#[derive(Clone)]
pub(super) struct BoundedClient {
    http: Client,
    received: Arc<AtomicUsize>,
    legacy_probe: Arc<AtomicBool>,
}

fn invalid(message: &'static str) -> HttpError {
    StreamableHttpError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
}

impl BoundedClient {
    pub(super) fn new(http: Client) -> Self {
        Self {
            http,
            received: Arc::new(AtomicUsize::new(0)),
            legacy_probe: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn legacy_probe(&self) -> Arc<AtomicBool> {
        self.legacy_probe.clone()
    }

    fn request(
        &self,
        method: reqwest_mcp::Method,
        uri: &str,
        session: Option<&str>,
        token: Option<&str>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<RequestBuilder, HttpError> {
        let mut request = self
            .http
            .request(method, uri)
            .header("accept", "application/json, text/event-stream");
        for (name, value) in headers {
            // Keep SDK projections; credentials/session/representation stay host-owned.
            if matches!(
                name.as_str(),
                "accept" | "content-type" | "authorization" | "mcp-session-id" | "last-event-id"
            ) {
                return Err(StreamableHttpError::ReservedHeaderConflict(
                    name.to_string(),
                ));
            }
            request = request.header(name, value);
        }
        if let Some(session) = session {
            request = request.header("mcp-session-id", session);
        }
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        Ok(request)
    }

    fn header(response: &Response, name: &str) -> Result<Option<String>, HttpError> {
        response
            .headers()
            .get(name)
            .map(|value| {
                if value.as_bytes().len() > MAX_HEADER_BYTES {
                    return Err(invalid("MCP response header exceeds the limit"));
                }
                value
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|_| invalid("MCP response header is invalid"))
            })
            .transpose()
    }

    fn challenge(response: &Response) -> Result<(), HttpError> {
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            if let Some(challenge) = Self::header(response, "www-authenticate")? {
                return Err(if response.status() == StatusCode::UNAUTHORIZED {
                    StreamableHttpError::AuthRequired(AuthRequiredError::new(challenge))
                } else {
                    let scope = rmcp::transport::auth::WWWAuthenticateParams::parse(
                        &challenge,
                        response.url(),
                    )
                    .scope;
                    StreamableHttpError::InsufficientScope(InsufficientScopeError::new(
                        challenge, scope,
                    ))
                });
            }
        }
        Ok(())
    }

    fn charge(received: &AtomicUsize, bytes: usize) -> Result<(), io::Error> {
        received
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= MAX_OPERATION_BYTES)
            })
            .map(|_| ())
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP operation exceeds the response byte limit",
                )
            })
    }

    async fn body(&self, mut response: Response, limit: usize) -> Result<Vec<u8>, HttpError> {
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(invalid("MCP response exceeds the byte limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(StreamableHttpError::Client)?
        {
            if chunk.len() > limit.saturating_sub(bytes.len()) {
                return Err(invalid("MCP response exceeds the byte limit"));
            }
            Self::charge(&self.received, chunk.len()).map_err(StreamableHttpError::Io)?;
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn sse(
        &self,
        response: Response,
        limit: usize,
    ) -> Result<futures_util::stream::BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        let limit = limit.min(super::MAX_SSE_EVENT_BYTES);
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(invalid("MCP SSE response exceeds the byte limit"));
        }
        let received = self.received.clone();
        let bytes = stream::try_unfold((response, 0usize), move |(mut response, used)| {
            let received = received.clone();
            async move {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if chunk.len() > limit.saturating_sub(used) {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "MCP SSE response exceeds the byte limit",
                            ));
                        }
                        Self::charge(&received, chunk.len())?;
                        let size = chunk.len();
                        Ok(Some((chunk, (response, used + size))))
                    }
                    Ok(None) => Ok(None),
                    Err(_) => Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "MCP SSE body failed",
                    )),
                }
            }
        });
        Ok(SseStream::from_bytes_stream(bytes).boxed())
    }
}

impl StreamableHttpClient for BoundedClient {
    type Error = reqwest_mcp::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        self.post_message_with_max_sse_event_size(
            uri,
            message,
            session_id,
            auth_header,
            headers,
            super::MAX_SSE_EVENT_BYTES,
        )
        .await
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max_bytes: usize,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        let response = self
            .request(
                reqwest_mcp::Method::POST,
                &uri,
                session_id.as_deref(),
                auth_header.as_deref(),
                headers,
            )?
            .json(&message)
            .send()
            .await
            .map_err(StreamableHttpError::Client)?;
        Self::challenge(&response)?;
        let status = response.status();
        if matches!(status, StatusCode::ACCEPTED | StatusCode::NO_CONTENT) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if status == StatusCode::NOT_FOUND && session_id.is_some() {
            return Err(StreamableHttpError::SessionExpired);
        }
        let session = Self::header(&response, "mcp-session-id")?;
        let kind = Self::header(&response, "content-type")?.unwrap_or_default();
        let kind = kind.split(';').next().unwrap_or_default().trim();
        let notification = !matches!(message, ClientJsonRpcMessage::Request(_));
        if status.is_success() && notification && response.content_length() == Some(0) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if !status.is_success() {
            let bytes = self.body(response, MAX_ERROR_BYTES).await?;
            if kind.eq_ignore_ascii_case("application/json") {
                if let Ok(reply @ JsonRpcMessage::Error(_)) =
                    serde_json::from_slice::<ServerJsonRpcMessage>(&bytes)
                {
                    if uncorrelated_legacy_probe(status, &message, &reply) {
                        self.legacy_probe.store(true, Ordering::Relaxed);
                    }
                    return Ok(StreamableHttpPostResponse::Json(reply, session));
                }
            }
            return Err(invalid("MCP server returned an HTTP error"));
        }
        if kind.eq_ignore_ascii_case("text/event-stream") {
            return Ok(StreamableHttpPostResponse::Sse(
                self.sse(response, max_bytes)?,
                session,
            ));
        }
        if !kind.eq_ignore_ascii_case("application/json") {
            return Err(invalid("MCP response content type is unsupported"));
        }
        let bytes = self.body(response, MAX_JSON_BYTES).await?;
        match serde_json::from_slice::<ServerJsonRpcMessage>(&bytes) {
            Ok(message) => Ok(StreamableHttpPostResponse::Json(message, session)),
            Err(_) if notification => Ok(StreamableHttpPostResponse::Accepted),
            Err(_) => Err(invalid("MCP response is not valid JSON-RPC")),
        }
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), HttpError> {
        // SDK deletion never reads the body; the outer adapter bounds its lifetime.
        self.http
            .delete_session(uri, session_id, auth_header, headers)
            .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<futures_util::stream::BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        self.get_stream_with_max_sse_event_size(
            uri,
            session_id,
            last_event_id,
            auth_header,
            headers,
            super::MAX_SSE_EVENT_BYTES,
        )
        .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max_bytes: usize,
    ) -> Result<futures_util::stream::BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        let mut request = self.request(
            reqwest_mcp::Method::GET,
            &uri,
            session_id.as_deref(),
            auth_header.as_deref(),
            headers,
        )?;
        if let Some(id) = last_event_id {
            request = request.header("last-event-id", id);
        }
        let response = request.send().await.map_err(StreamableHttpError::Client)?;
        if response.status() == StatusCode::METHOD_NOT_ALLOWED {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        Self::challenge(&response)?;
        let kind = Self::header(&response, "content-type")?.unwrap_or_default();
        if !response.status().is_success()
            || !kind
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(invalid("MCP SSE stream is unavailable"));
        }
        self.sse(response, max_bytes)
    }
}

// Era detection from an initial HTTP rejection is distinct from accepting a
// JSON-RPC response. Keep the reply unchanged; the SDK still refuses its ID.
// Never arm this for a tool, a success, authorization, or a modern error.
fn uncorrelated_legacy_probe(
    status: StatusCode,
    request: &ClientJsonRpcMessage,
    reply: &ServerJsonRpcMessage,
) -> bool {
    let ClientJsonRpcMessage::Request(request) = request else {
        return false;
    };
    let ServerJsonRpcMessage::Error(reply) = reply else {
        return false;
    };
    status == StatusCode::BAD_REQUEST
        && matches!(
            request.request,
            rmcp::model::ClientRequest::DiscoverRequest(_)
        )
        && reply.id.as_ref() != Some(&request.id)
        && matches!(
            reply.error.code,
            rmcp::model::ErrorCode::INVALID_REQUEST | rmcp::model::ErrorCode::METHOD_NOT_FOUND
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_era_detection_never_retries_actions_successes_auth_or_modern_errors() {
        let probe: ClientJsonRpcMessage = serde_json::from_value(
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}}),
        )
        .unwrap();
        let call: ClientJsonRpcMessage = serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read","arguments":{}}})).unwrap();
        let make = |id: serde_json::Value, code: i32| {
            serde_json::from_value::<ServerJsonRpcMessage>(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":"rejected"}})).unwrap()
        };
        for id in [serde_json::Value::Null, serde_json::json!("server-error")] {
            let error = make(id.clone(), -32600);
            assert!(uncorrelated_legacy_probe(
                StatusCode::BAD_REQUEST,
                &probe,
                &error
            ));
            assert!(!uncorrelated_legacy_probe(
                StatusCode::BAD_REQUEST,
                &call,
                &error
            ));
            for status in [
                StatusCode::OK,
                StatusCode::UNAUTHORIZED,
                StatusCode::FORBIDDEN,
                StatusCode::INTERNAL_SERVER_ERROR,
            ] {
                assert!(!uncorrelated_legacy_probe(status, &probe, &error));
            }
            for code in [-32022, -32021, -32020, -32603] {
                assert!(!uncorrelated_legacy_probe(
                    StatusCode::BAD_REQUEST,
                    &probe,
                    &make(id.clone(), code)
                ));
            }
        }
        assert!(!uncorrelated_legacy_probe(
            StatusCode::BAD_REQUEST,
            &probe,
            &make(serde_json::json!(1), -32600)
        ));
    }
}
