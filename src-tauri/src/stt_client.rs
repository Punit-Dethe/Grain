//! [GRAIN] S2: HTTP speech-to-text client — OpenAI-compatible, Deepgram,
//! AssemblyAI. Mirrors the Python `stt_client.py` adapters: each returns a
//! normalized transcript. Keys are passed per-call (never stored on the
//! provider). Text-only for now — batch only needs text; word timings can be
//! parsed here later if the rolling path is ever routed to a remote provider.

use std::io::Cursor;
use std::time::Duration;

use grain_core::{SttProvider, SttProviderKind};
use serde_json::Value;

/// transcribe-rs / our pipeline standard: 16 kHz mono.
const SAMPLE_RATE: u32 = 16_000;
/// AssemblyAI async polling budget.
const POLL_DEADLINE: Duration = Duration::from_secs(60);

/// Normalized transcription result from any remote provider, plus the live
/// rate-limit signal (when the provider reports it) the rotation tracker learns
/// from. `remaining_*` are `None` for providers that don't send those headers.
pub struct SttResult {
    pub text: String,
    pub remaining_requests: Option<i64>,
    pub remaining_tokens: Option<i64>,
}

/// Why a remote STT call failed, split so the router can cool a rate-limited
/// provider (honoring Retry-After) versus briefly backing off any other error.
pub enum SttError {
    /// HTTP 429. `retry_after_s` parsed from Retry-After / reset headers (or `None`).
    RateLimited { retry_after_s: Option<f64> },
    /// Network error, non-429 HTTP status, bad key (401), parse failure, etc.
    Other(String),
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SttError::RateLimited { retry_after_s } => {
                write!(f, "rate limited (retry after {retry_after_s:?}s)")
            }
            SttError::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Encode 16 kHz mono `f32` samples to in-memory WAV bytes (PCM s16le) — the
/// upload body every adapter sends.
fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).map_err(|e| format!("wav init: {e}"))?;
        for &s in samples {
            let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(v)
                .map_err(|e| format!("wav write: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("wav finalize: {e}"))?;
    }
    Ok(cursor.into_inner())
}

/// Transcribe `samples` via `provider` using `api_key`. The caller must NOT pass
/// a `Local` provider — that is handled in-process by the dispatcher.
pub async fn transcribe(
    client: &reqwest::Client,
    provider: &SttProvider,
    samples: &[f32],
    api_key: &str,
) -> Result<SttResult, SttError> {
    if samples.is_empty() {
        return Ok(SttResult {
            text: String::new(),
            remaining_requests: None,
            remaining_tokens: None,
        });
    }
    let wav = encode_wav(samples).map_err(SttError::Other)?;
    match provider.kind {
        SttProviderKind::Deepgram => deepgram(client, provider, wav, api_key).await,
        SttProviderKind::Assemblyai => assemblyai(client, provider, wav, api_key).await,
        SttProviderKind::Openai => openai(client, provider, wav, api_key).await,
        SttProviderKind::Local => Err(SttError::Other(
            "local provider is handled in-process".into(),
        )),
    }
}

/// Build the OpenAI transcriptions URL, tolerating a base_url with or without a
/// trailing `/v1`.
fn openai_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/audio/transcriptions")
    } else {
        format!("{base}/v1/audio/transcriptions")
    }
}

async fn openai(
    client: &reqwest::Client,
    provider: &SttProvider,
    wav: Vec<u8>,
    api_key: &str,
) -> Result<SttResult, SttError> {
    let url = openai_url(&provider.base_url);
    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| SttError::Other(format!("multipart: {e}")))?;
    let mut form = reqwest::multipart::Form::new().part("file", part);
    if !provider.model.is_empty() {
        form = form.text("model", provider.model.clone());
    }
    let mut req = client
        .post(&url)
        .multipart(form);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| SttError::Other(format!("request: {e}")))?;
    let (body, rr, rt) = read_signal(resp).await?;
    // OpenAI returns {"text": "..."}.
    Ok(SttResult {
        text: body
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        remaining_requests: rr,
        remaining_tokens: rt,
    })
}

async fn deepgram(
    client: &reqwest::Client,
    provider: &SttProvider,
    wav: Vec<u8>,
    api_key: &str,
) -> Result<SttResult, SttError> {
    let base = if provider.base_url.is_empty() {
        "https://api.deepgram.com"
    } else {
        provider.base_url.trim_end_matches('/')
    };
    let model = if provider.model.is_empty() {
        "nova-3"
    } else {
        &provider.model
    };
    let url = format!("{base}/v1/listen?model={model}&smart_format=true");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Token {api_key}"))
        .header("Content-Type", "audio/wav")
        .body(wav)
        .send()
        .await
        .map_err(|e| SttError::Other(format!("request: {e}")))?;
    let (body, rr, rt) = read_signal(resp).await?;
    let text = body
        .pointer("/results/channels/0/alternatives/0/transcript")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(SttResult {
        text,
        remaining_requests: rr,
        remaining_tokens: rt,
    })
}

async fn assemblyai(
    client: &reqwest::Client,
    provider: &SttProvider,
    wav: Vec<u8>,
    api_key: &str,
) -> Result<SttResult, SttError> {
    let base = if provider.base_url.is_empty() {
        "https://api.assemblyai.com"
    } else {
        provider.base_url.trim_end_matches('/')
    };

    // 1) upload (read_signal surfaces 429 → cooldown and other non-2xx → failover)
    let (upload, _, _) = read_signal(
        client
            .post(format!("{base}/v2/upload"))
            .header("Authorization", api_key)
            .header("Content-Type", "application/octet-stream")
            .body(wav)
            .send()
            .await
            .map_err(|e| SttError::Other(format!("upload: {e}")))?,
    )
    .await?;
    let upload_url = upload
        .get("upload_url")
        .and_then(Value::as_str)
        .ok_or_else(|| SttError::Other("assemblyai: no upload_url".into()))?;

    // 2) request transcription
    let (created, _, _) = read_signal(
        client
            .post(format!("{base}/v2/transcript"))
            .header("Authorization", api_key)
            .json(&serde_json::json!({ "audio_url": upload_url }))
            .send()
            .await
            .map_err(|e| SttError::Other(format!("create: {e}")))?,
    )
    .await?;
    let id = created
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| SttError::Other("assemblyai: no id".into()))?;

    // 3) poll until completed / error / deadline
    let poll_url = format!("{base}/v2/transcript/{id}");
    let deadline = std::time::Instant::now() + POLL_DEADLINE;
    loop {
        let (body, rr, rt) = read_signal(
            client
                .get(&poll_url)
                .header("Authorization", api_key)
                .send()
                .await
                .map_err(|e| SttError::Other(format!("poll: {e}")))?,
        )
        .await?;
        match body.get("status").and_then(Value::as_str) {
            Some("completed") => {
                return Ok(SttResult {
                    text: body
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    remaining_requests: rr,
                    remaining_tokens: rt,
                })
            }
            Some("error") => {
                let err = body
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                return Err(SttError::Other(format!("assemblyai error: {err}")));
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            return Err(SttError::Other(
                "assemblyai: transcription timed out".into(),
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Read a response into `(json, remaining_requests, remaining_tokens)`, mapping
/// HTTP 429 → [`SttError::RateLimited`] (with parsed Retry-After) and any other
/// non-2xx → [`SttError::Other`] with the body — the signals the rotation tracker
/// learns from. Rate-limit headers are captured on every response (when present).
async fn read_signal(
    resp: reqwest::Response,
) -> Result<(Value, Option<i64>, Option<i64>), SttError> {
    let status = resp.status();
    let hmap = crate::rotation_state::headers_to_map(resp.headers());
    let (rem_req, rem_tok) = provider_router::parse_rate_limit_headers(&hmap);
    if status.as_u16() == 429 {
        let retry = provider_router::parse_retry_after(&hmap);
        let _ = resp.text().await; // drain the body
        return Err(SttError::RateLimited {
            retry_after_s: Some(retry),
        });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| SttError::Other(format!("read body: {e}")))?;
    if !status.is_success() {
        return Err(SttError::Other(format!(
            "HTTP {status}: {}",
            text.chars().take(300).collect::<String>()
        )));
    }
    let v = serde_json::from_str(&text).map_err(|e| SttError::Other(format!("parse json: {e}")))?;
    Ok((v, rem_req, rem_tok))
}
