use crate::settings::PostProcessProvider;
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// Per-request read timeout. This previously lived on a throwaway per-call
/// `reqwest::Client`; it is now applied to the request builder so the shared,
/// pooled client is reused (no new connection pool / TLS per call). The connect
/// timeout stays configured on the shared client.
const LLM_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    /// Optional so tool-call assistant turns can serialize `content: null` and
    /// so a `tool` result message is a plain string. For system/user/assistant
    /// text turns this is always `Some(_)` → byte-identical to the old wire
    /// format (the field is only skipped when genuinely `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    /// Present only on an assistant turn that requested tool calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<WireToolCall>>,
    /// Present only on a `tool` result message (echoes the call id).
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl ChatMessage {
    /// A plain text turn (system / user / assistant) — the only shape the
    /// non-tool paths (`send_chat`, post-process) ever build.
    fn text(role: impl Into<String>, content: String) -> Self {
        Self {
            role: role.into(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// OpenAI tool-calling wire types. Kept separate from the public
/// [`ToolCallOut`] so the transport format never leaks into callers.
#[derive(Debug, Serialize)]
struct WireTool {
    #[serde(rename = "type")]
    kind: String,
    function: WireToolSpec,
}

#[derive(Debug, Serialize)]
struct WireToolSpec {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Serialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: WireToolCallFn,
}

#[derive(Debug, Serialize)]
struct WireToolCallFn {
    name: String,
    arguments: String,
}

/// One function the model may call, as handed in by a caller (Grain Recall's
/// `search_memory`). `parameters` is a JSON-Schema object.
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// One tool call the model asked for (or that we echo back on the next turn).
/// `arguments` is a raw JSON string exactly as the model emitted it.
#[derive(Debug, Clone)]
pub struct ToolCallOut {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// One entry in a tool-enabled conversation. Richer than `(role, content)`
/// because assistant turns can carry tool calls and `tool` results reference a
/// call id.
#[derive(Debug, Clone)]
pub enum ChatEntry {
    System(String),
    User(String),
    Assistant(String),
    /// The assistant asked to call one or more tools (content is null).
    AssistantToolCalls(Vec<ToolCallOut>),
    /// A tool's result fed back to the model.
    ToolResult {
        call_id: String,
        content: String,
    },
}

/// A tool-enabled chat completion result: either free-text `content`, or one or
/// more `tool_calls` the caller must execute and feed back — plus the same
/// rate-limit signal the plain path returns.
pub struct LlmChatResult {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCallOut>,
    pub remaining_requests: Option<i64>,
    pub remaining_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
    /// Native tool-calling: present only for the tool-enabled Recall path, so
    /// every existing caller serializes exactly as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<WireTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<RespToolCall>>,
}

#[derive(Debug, Deserialize)]
struct RespToolCall {
    #[serde(default)]
    id: String,
    function: RespToolCallFn,
}

#[derive(Debug, Deserialize)]
struct RespToolCallFn {
    #[serde(default)]
    name: String,
    #[serde(default)]
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    total_tokens: Option<i64>,
}

/// A successful chat completion plus the live rate-limit signal the rotation
/// tracker learns from. `remaining_*` come from response headers when present;
/// `total_tokens` from the response `usage` (both `None` if the provider omits them).
pub struct LlmSuccess {
    pub content: Option<String>,
    pub remaining_requests: Option<i64>,
    pub remaining_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

/// Why a chat completion failed, split so the router can cool a rate-limited
/// provider (honoring Retry-After) versus briefly backing off any other error.
pub enum LlmError {
    /// HTTP 429. `retry_after_s` parsed from Retry-After / reset headers (or `None`).
    RateLimited { retry_after_s: Option<f64> },
    /// Network error, non-429 HTTP status, bad key (401), parse failure, etc.
    Other(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::RateLimited { retry_after_s } => {
                write!(f, "rate limited (retry after {retry_after_s:?}s)")
            }
            LlmError::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Send a chat completion request to an OpenAI-compatible API. Returns an
/// [`LlmSuccess`] (content may be `None` if the response carried none) plus the
/// rate-limit signal, or an [`LlmError`] distinguishing 429 from other failures.
pub async fn send_chat_completion(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: String,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<LlmSuccess, LlmError> {
    send_chat_completion_with_schema(
        client,
        provider,
        api_key,
        model,
        prompt,
        None,
        None,
        reasoning_effort,
        reasoning,
    )
    .await
}

/// Send a chat completion request with structured output support.
/// `reasoning_effort` sets the OpenAI-style top-level field (e.g., "none", "low", "medium", "high")
/// `reasoning` sets the OpenRouter-style nested object (effort + exclude)
pub async fn send_chat_completion_with_schema(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<LlmSuccess, LlmError> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    debug!("Sending chat completion request to: {}", url);

    // Build provider-specific auth + common headers; the shared pooled `client`
    // is reused for the actual request (no per-call connection pool / TLS).
    let headers = build_auth_headers(provider, &api_key).map_err(LlmError::Other)?;

    // Build messages vector
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(system) = system_prompt {
        messages.push(ChatMessage::text("system", system));
    }

    // Add user message
    messages.push(ChatMessage::text("user", user_content));

    // Build response_format if schema is provided
    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format,
        reasoning_effort,
        reasoning,
        tools: None,
        tool_choice: None,
    };

    send_request(client, &url, headers, &request_body).await
}

/// [GRAIN] Send a free-form multi-turn chat completion (used by the Agent).
///
/// `messages` is an ordered list of `(role, content)` — e.g. `("system", …)`,
/// `("user", …)`, `("assistant", …)`. Unlike the post-process path there is no
/// structured-output schema: the model answers freely.
pub async fn send_chat(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    messages: Vec<(String, String)>,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<LlmSuccess, LlmError> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    let headers = build_auth_headers(provider, &api_key).map_err(LlmError::Other)?;

    let messages = messages
        .into_iter()
        .map(|(role, content)| ChatMessage::text(role, content))
        .collect();

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format: None,
        reasoning_effort,
        reasoning,
        tools: None,
        tool_choice: None,
    };

    send_request(client, &url, headers, &request_body).await
}

/// [GRAIN] Send a tool-enabled multi-turn chat completion (Grain Recall's
/// native `search_memory`). Same OpenAI-compatible endpoint as [`send_chat`],
/// but the request advertises `tools` and the response may come back as one or
/// more `tool_calls` instead of prose. The agentic loop (bounded hops) lives in
/// the caller (`recall.rs`); this function is a single stateless round-trip.
#[allow(clippy::too_many_arguments)]
pub async fn send_chat_with_tools(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    entries: Vec<ChatEntry>,
    tools: Vec<ToolSpec>,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<LlmChatResult, LlmError> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    let headers = build_auth_headers(provider, &api_key).map_err(LlmError::Other)?;

    let messages: Vec<ChatMessage> = entries
        .into_iter()
        .map(|e| match e {
            ChatEntry::System(c) => ChatMessage::text("system", c),
            ChatEntry::User(c) => ChatMessage::text("user", c),
            ChatEntry::Assistant(c) => ChatMessage::text("assistant", c),
            ChatEntry::AssistantToolCalls(calls) => ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(
                    calls
                        .into_iter()
                        .map(|tc| WireToolCall {
                            id: tc.id,
                            kind: "function".to_string(),
                            function: WireToolCallFn {
                                name: tc.name,
                                arguments: tc.arguments,
                            },
                        })
                        .collect(),
                ),
                tool_call_id: None,
            },
            ChatEntry::ToolResult { call_id, content } => ChatMessage {
                role: "tool".to_string(),
                content: Some(content),
                tool_calls: None,
                tool_call_id: Some(call_id),
            },
        })
        .collect();

    let wire_tools: Vec<WireTool> = tools
        .into_iter()
        .map(|t| WireTool {
            kind: "function".to_string(),
            function: WireToolSpec {
                name: t.name,
                description: t.description,
                parameters: t.parameters,
            },
        })
        .collect();

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format: None,
        reasoning_effort,
        reasoning,
        tools: (!wire_tools.is_empty()).then_some(wire_tools),
        tool_choice: Some("auto".to_string()),
    };

    send_request_with_tools(client, &url, headers, &request_body).await
}

/// Build the common + provider-specific auth headers for one request.
///
/// [GRAIN] Previously this built a throwaway `reqwest::Client` per call, which
/// created a fresh connection pool + TLS state every request (a TCP/TLS
/// handshake on every post-process and Agent turn). reqwest's pool lives on the
/// `Client`, not on per-request headers, so we now keep the SHARED pooled client
/// and attach these headers (plus timeouts) to each request builder instead
/// (see `send_request` / `fetch_models`). This reuses connections across calls.
fn build_auth_headers(provider: &PostProcessProvider, api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    // [GRAIN] Identify as Grain (not upstream Handy) on outbound requests — the
    // Referer/User-Agent/X-Title surface in provider dashboards (e.g. OpenRouter
    // shows X-Title), so they must reflect this client, not the fork origin.
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://gitlab.com/grain2/grain-stt"),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Grain/1.0 (+https://gitlab.com/grain2/grain-stt)"),
    );
    headers.insert("X-Title", HeaderValue::from_static("Grain"));

    if !api_key.is_empty() {
        // [GRAIN] Phase 2 note: will switch to provider.auth_style enum;
        // keep this narrow id match until that migration lands.
        if provider.id == "anthropic" {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(api_key)
                    .map_err(|e| format!("Invalid API key header value: {e}"))?,
            );
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))
                    .map_err(|e| format!("Invalid authorization header value: {e}"))?,
            );
        }
    }
    Ok(headers)
}

/// POST a built request to `{base}/chat/completions` and decode it into an
/// [`LlmSuccess`] (or [`LlmError`]). Shared by the structured-output post-process
/// path and the Agent's free-form chat so both honor identical 429 / rate-limit
/// header handling.
async fn send_request(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    request_body: &ChatCompletionRequest,
) -> Result<LlmSuccess, LlmError> {
    let (completion, rem_req, rem_tok) = post_chat(client, url, headers, request_body).await?;
    Ok(LlmSuccess {
        content: completion
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone()),
        remaining_requests: rem_req,
        remaining_tokens: rem_tok,
        total_tokens: completion.usage.and_then(|u| u.total_tokens),
    })
}

/// Tool-aware sibling of [`send_request`]: same HTTP/429/header handling, but
/// surfaces any `tool_calls` alongside the content.
async fn send_request_with_tools(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    request_body: &ChatCompletionRequest,
) -> Result<LlmChatResult, LlmError> {
    let (completion, rem_req, rem_tok) = post_chat(client, url, headers, request_body).await?;
    let message = completion.choices.into_iter().next().map(|c| c.message);
    let (content, tool_calls) = match message {
        Some(m) => {
            let calls = m
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|tc| ToolCallOut {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: tc.function.arguments,
                })
                .collect();
            (m.content, calls)
        }
        None => (None, Vec::new()),
    };
    Ok(LlmChatResult {
        content,
        tool_calls,
        remaining_requests: rem_req,
        remaining_tokens: rem_tok,
        total_tokens: completion.usage.and_then(|u| u.total_tokens),
    })
}

/// POST a built request to `{base}/chat/completions`, apply shared 429 /
/// rate-limit-header / error handling, and decode the JSON body. The two
/// `send_request*` wrappers project the parsed response into their result type.
async fn post_chat(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    request_body: &ChatCompletionRequest,
) -> Result<(ChatCompletionResponse, Option<i64>, Option<i64>), LlmError> {
    let response = client
        .post(url)
        .headers(headers)
        .timeout(LLM_REQUEST_TIMEOUT)
        .json(request_body)
        .send()
        .await
        .map_err(|e| LlmError::Other(format!("HTTP request failed: {}", e)))?;

    // Capture rate-limit signal from headers BEFORE consuming the body.
    let status = response.status();
    let hmap = crate::rotation_state::headers_to_map(response.headers());
    let (rem_req, rem_tok) = provider_router::parse_rate_limit_headers(&hmap);

    if status.as_u16() == 429 {
        let retry = provider_router::parse_retry_after(&hmap);
        return Err(LlmError::RateLimited {
            retry_after_s: Some(retry),
        });
    }

    let body = response
        .text()
        .await
        .map_err(|e| LlmError::Other(format!("read body: {e}")))?;
    if !status.is_success() {
        return Err(LlmError::Other(format!(
            "API request failed with status {}: {}",
            status,
            body.chars().take(300).collect::<String>()
        )));
    }

    let completion: ChatCompletionResponse = serde_json::from_str(&body)
        .map_err(|e| LlmError::Other(format!("Failed to parse API response: {}", e)))?;
    Ok((completion, rem_req, rem_tok))
}

/// Fetch available models from an OpenAI-compatible API
/// Returns a list of model IDs
pub async fn fetch_models(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    api_key: String,
) -> Result<Vec<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/models", base_url);

    debug!("Fetching models from: {}", url);

    let headers = build_auth_headers(provider, &api_key)?;

    let response = client
        .get(&url)
        .headers(headers)
        .timeout(LLM_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Model list request failed ({}): {}",
            status, error_text
        ));
    }

    let parsed: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut models = Vec::new();

    // Handle OpenAI format: { data: [ { id: "..." }, ... ] }
    if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                models.push(id.to_string());
            } else if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                models.push(name.to_string());
            }
        }
    }
    // Handle array format: [ "model1", "model2", ... ]
    else if let Some(array) = parsed.as_array() {
        for entry in array {
            if let Some(model) = entry.as_str() {
                models.push(model.to_string());
            }
        }
    }

    Ok(models)
}
