//! Ordered provider fallback. No tracker, scoring, quota or retained state.
use grain_core::PostProcessProvider;
use tauri::{AppHandle, Manager};

pub enum ProviderOutcome {
    Ok { text: String },
    RateLimited,
    Failed,
}

const LLM_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Bound each attempt so a stalled endpoint can yield to the next provider.
pub(crate) async fn run_one_provider_with_timeout(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    prompt: &str,
    transcription: &str,
) -> ProviderOutcome {
    tokio::time::timeout(
        LLM_REQUEST_TIMEOUT,
        crate::grain_post_process::run_one_provider(
            client,
            provider,
            model,
            api_key,
            prompt,
            transcription,
        ),
    )
    .await
    .unwrap_or(ProviderOutcome::Failed)
}

pub(crate) async fn post_process_with_fallback(
    app: &AppHandle,
    settings: &grain_core::AppSettings,
    prompt: &str,
    transcription: &str,
) -> Option<String> {
    let client = app.try_state::<reqwest::Client>()?.inner().clone();
    try_providers(&client, settings, prompt, transcription).await
}

async fn try_providers(
    client: &reqwest::Client,
    settings: &grain_core::AppSettings,
    prompt: &str,
    transcription: &str,
) -> Option<String> {
    for provider in grain_core::providers::fallback_pool(settings) {
        let model = settings.post_process_models[&provider.id].clone();
        let key = settings
            .post_process_api_keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        if let ProviderOutcome::Ok { text } =
            run_one_provider_with_timeout(client, provider, model, key, prompt, transcription).await
        {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Server {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    impl Server {
        async fn start(tool_reply: bool) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let seen = requests.clone();
            let task = tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    let body_start;
                    let length;
                    loop {
                        let mut bytes = [0u8; 4096];
                        let n = stream.read(&mut bytes).await.unwrap();
                        assert!(n > 0 && request.len() + n < 65536);
                        request.extend_from_slice(&bytes[..n]);
                        if let Some(index) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            body_start = index + 4;
                            length = String::from_utf8_lossy(&request[..index])
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|value| value.trim().parse::<usize>().unwrap())
                                })
                                .unwrap();
                            break;
                        }
                    }
                    while request.len() < body_start + length {
                        let mut bytes = [0u8; 4096];
                        let n = stream.read(&mut bytes).await.unwrap();
                        assert!(n > 0 && request.len() + n < 65536);
                        request.extend_from_slice(&bytes[..n]);
                    }
                    let body: serde_json::Value =
                        serde_json::from_slice(&request[body_start..body_start + length]).unwrap();
                    let model = body["model"].as_str().unwrap();
                    seen.lock().unwrap().push(model.to_string());
                    let (status, message) = match model {
                        "rate" => (429, serde_json::json!({"error": "limited"})),
                        "error" => (503, serde_json::json!({"error": "unavailable"})),
                        "empty" => (
                            200,
                            serde_json::json!({"choices": [{"message": {"content": "  "}}]}),
                        ),
                        "success" if tool_reply => (
                            200,
                            serde_json::json!({"choices": [{"message": {"content": null, "tool_calls": [{"id": "call-winner", "type": "function", "function": {"name": "lookup", "arguments": "{}"}}]}}]}),
                        ),
                        _ => (
                            200,
                            serde_json::json!({"choices": [{"message": {"content": "winner"}}]}),
                        ),
                    };
                    let body = message.to_string();
                    let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            });
            Self {
                url,
                requests,
                task,
            }
        }

        fn settings(&self, models: &[&str]) -> grain_core::AppSettings {
            let mut s = grain_core::AppSettings::default();
            s.post_process_providers.clear();
            for (index, model) in models.iter().enumerate() {
                let id = format!("test-{index}");
                s.post_process_providers.push(PostProcessProvider {
                    id: id.clone(),
                    label: id.clone(),
                    base_url: self.url.clone(),
                    enabled: true,
                    allow_base_url_edit: true,
                    models_endpoint: None,
                    supports_structured_output: false,
                });
                s.post_process_models.insert(id.clone(), model.to_string());
                s.post_process_api_keys.insert(id, "test-key".into());
            }
            s
        }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn post_processing_advances_on_rate_error_and_empty_then_stops_at_success() {
        let server = Server::start(false).await;
        let settings = server.settings(&["rate", "error", "empty", "success", "unused"]);
        assert_eq!(
            try_providers(&client(), &settings, "${output}", "hello")
                .await
                .as_deref(),
            Some("winner")
        );
        assert_eq!(
            *server.requests.lock().unwrap(),
            ["rate", "error", "empty", "success"]
        );
        server.requests.lock().unwrap().clear();
        assert_eq!(
            try_providers(&client(), &settings, "${output}", "hello")
                .await
                .as_deref(),
            Some("winner")
        );
        // Each request starts at priority one; no cooldown or round-robin state remains.
        assert_eq!(
            *server.requests.lock().unwrap(),
            ["rate", "error", "empty", "success"]
        );
    }

    #[tokio::test]
    async fn exhausted_pool_returns_none_and_reorder_changes_first_attempt() {
        let server = Server::start(false).await;
        let mut settings = server.settings(&["error", "empty"]);
        assert!(try_providers(&client(), &settings, "${output}", "hello")
            .await
            .is_none());
        assert_eq!(*server.requests.lock().unwrap(), ["error", "empty"]);
        settings = server.settings(&["error", "success"]);
        let ids = settings
            .post_process_providers
            .iter()
            .rev()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        grain_core::providers::reorder(&mut settings, &ids).unwrap();
        server.requests.lock().unwrap().clear();
        assert_eq!(
            try_providers(&client(), &settings, "${output}", "hello")
                .await
                .as_deref(),
            Some("winner")
        );
        assert_eq!(*server.requests.lock().unwrap(), ["success"]);
    }

    #[tokio::test]
    async fn agent_plain_and_tool_calls_use_the_same_priority_and_keep_winning_reply() {
        let server = Server::start(false).await;
        let settings = server.settings(&["rate", "error", "empty", "success", "unused"]);
        let result = crate::agent::agent_run_with_fallback(
            &client(),
            &settings,
            &[("user".into(), "hello".into())],
            None,
        )
        .await
        .unwrap();
        assert_eq!(result, "winner");
        assert_eq!(
            *server.requests.lock().unwrap(),
            ["rate", "error", "empty", "success"]
        );
        let tools_server = Server::start(true).await;
        let settings = tools_server.settings(&["rate", "error", "empty", "success", "unused"]);
        let result = crate::agent::agent_run_with_fallback_tools(
            &client(),
            &settings,
            vec![crate::llm_client::ChatEntry::User("hello".into())],
            vec![crate::llm_client::ToolSpec {
                name: "lookup".into(),
                description: "lookup".into(),
                parameters: serde_json::json!({"type":"object"}),
            }],
            None,
        )
        .await
        .unwrap();
        assert!(result.content.is_empty());
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "call-winner");
        assert_eq!(result.tool_calls[0].name, "lookup");
        assert_eq!(
            *tools_server.requests.lock().unwrap(),
            ["rate", "error", "empty", "success"]
        );
    }
}
