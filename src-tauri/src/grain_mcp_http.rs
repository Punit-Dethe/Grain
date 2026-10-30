//! Cancellation around the official SDK's HTTP backend, preserving its auth
//! error types and SSE limits. Closing/dropping an operation cancels pending
//! HTTP work; a timed-out response cannot leave a transport worker waiting on it.

use futures_util::stream::BoxStream;
use reqwest_mcp::header::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};
use sse_stream::{Error as SseError, Sse};
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::watch;

#[derive(Clone)]
pub(super) struct CancellableClient<C> {
    client: C,
    cancel: watch::Receiver<bool>,
    cleanup: Option<super::bounded_http::BoundedClient>,
    ticket: Option<super::session::Ticket>,
}

impl<C> CancellableClient<C> {
    pub(super) fn new(client: C, cancel: watch::Receiver<bool>) -> Self {
        Self {
            client,
            cancel,
            cleanup: None,
            ticket: None,
        }
    }

    pub(super) fn with_authenticated_cleanup(
        mut self,
        cleanup: Option<super::bounded_http::BoundedClient>,
    ) -> Self {
        self.cleanup = cleanup;
        self
    }

    pub(super) fn with_auth_observer(mut self, ticket: Option<super::session::Ticket>) -> Self {
        self.ticket = ticket;
        self
    }

    fn observe_error(&self, error: &StreamableHttpError<C::Error>)
    where
        C: StreamableHttpClient,
    {
        let recovery = match error {
            StreamableHttpError::Auth(error) => Some(super::Recovery::from_auth(error)),
            // The SDK intentionally sends an unauthenticated request when a
            // grant cannot be renewed; its final challenge is also typed.
            StreamableHttpError::AuthRequired(_) => Some(super::Recovery::Reconnect),
            StreamableHttpError::InsufficientScope(_) => Some(super::Recovery::Configuration),
            _ => None,
        };
        if let (Some(ticket), Some(recovery)) = (&self.ticket, recovery) {
            ticket.observe_recovery(Some(recovery));
        }
    }
}

async fn interrupted(mut cancel: watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        // Losing the operation owner also cancels its in-flight requests.
        if cancel.changed().await.is_err() {
            return;
        }
    }
}

async fn cancellable<T, E: std::error::Error + Send + Sync + 'static>(
    cancel: watch::Receiver<bool>,
    operation: impl Future<Output = Result<T, StreamableHttpError<E>>>,
) -> Result<T, StreamableHttpError<E>> {
    tokio::select! {
        biased;
        _ = interrupted(cancel) => Err(StreamableHttpError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted, "Grain MCP operation cancelled",
        ))),
        result = operation => result,
    }
}

impl<C: StreamableHttpClient + Sync> StreamableHttpClient for CancellableClient<C> {
    type Error = C::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        #[cfg(feature = "agent-harness")]
        crate::grain_agent_harness_mcp::validate_live_post(&uri, &message, auth_header.is_some())
            .map_err(|error| {
            StreamableHttpError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                error,
            ))
        })?;
        cancellable(
            self.cancel.clone(),
            self.client
                .post_message(uri, message, session_id, auth_header, headers),
        )
        .await
        .inspect_err(|error| self.observe_error(error))
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max_bytes: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        #[cfg(feature = "agent-harness")]
        crate::grain_agent_harness_mcp::validate_live_post(&uri, &message, auth_header.is_some())
            .map_err(|error| {
            StreamableHttpError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                error,
            ))
        })?;
        cancellable(
            self.cancel.clone(),
            self.client.post_message_with_max_sse_event_size(
                uri,
                message,
                session_id,
                auth_header,
                headers,
                max_bytes,
            ),
        )
        .await
        .inspect_err(|error| self.observe_error(error))
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        // Legacy server cleanup is allowed after local cancellation, but cannot
        // hold the operation alive indefinitely. It never repeats tools/call.
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            if let Some(cleanup) = &self.cleanup {
                cleanup
                    .delete_owned_session(uri, session_id, headers)
                    .await
                    .map_err(|error| {
                        StreamableHttpError::Io(std::io::Error::other(match error {
                            StreamableHttpError::Io(_) => "Grain MCP cleanup ownership refused",
                            _ => "Grain MCP authenticated cleanup failed",
                        }))
                    })
            } else {
                self.client
                    .delete_session(uri, session_id, auth_header, headers)
                    .await
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(StreamableHttpError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Grain MCP session cleanup timed out",
            )))
        })
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        cancellable(
            self.cancel.clone(),
            self.client
                .get_stream(uri, session_id, last_event_id, auth_header, headers),
        )
        .await
        .inspect_err(|error| self.observe_error(error))
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max_bytes: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        cancellable(
            self.cancel.clone(),
            self.client.get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_header,
                headers,
                max_bytes,
            ),
        )
        .await
        .inspect_err(|error| self.observe_error(error))
    }
}
