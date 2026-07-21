//! [GRAIN] Grain's post-processing pipeline — the multi-provider rewrite of the
//! single-provider path upstream keeps in `actions.rs` (Handy Isolation phase 6).
//!
//! Grain's version differs structurally, not cosmetically: it layers context
//! awareness / spoken Prompt Record instructions / the rolling seam prompt onto
//! the base prompt, can fan out across providers via
//! [`crate::post_process_router`], takes the shared `reqwest::Client` from Tauri
//! state, and reports rate limits as [`CallOutcome`] so the router can fail over.
//!
//! Upstream's original `post_process_transcription` stays in `actions.rs`,
//! un-called, so upstream changes to it still merge cleanly and can be read
//! against this file. Keep the two in sync deliberately — nothing enforces it.

use crate::llm_client::LlmError;
use crate::rotation_state::CallOutcome;
use crate::settings::{AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::actions::{
    build_system_prompt, is_blank_transcription, strip_invisible_chars, TRANSCRIPTION_FIELD,
};
use grain_core::PostProcessProvider;
use log::{debug, error, warn};
use tauri::{AppHandle, Manager};

pub(crate) async fn post_process_transcription(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    // [GRAIN] Prompt Record: an instruction the user dictated mid-recording (by
    // clicking the pill). Layered as the ABSOLUTE highest-priority stage in
    // `compose_prompt`, above any hard app mode.
    spoken_prompt: Option<&str>,
    // [GRAIN] True when the transcript came from the rolling-window assembler —
    // appends the compact seam-repair layer above.
    rolling: bool,
) -> Option<String> {
    if is_blank_transcription(transcription) {
        debug!("Post-processing skipped because the transcription is empty");
        return None;
    }

    // Resolve the selected prompt body once — shared by both the single-provider
    // and the rotation paths.
    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return None;
        }
    };

    let prompt = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return None;
        }
    };

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }

    // [GRAIN] Context awareness: layer automatic SOFT context (per detected app
    // category) and any matching user MODE (hard formatting) on top of the base
    // prompt. Detection is one cheap OS call made ONCE here — never per rolling
    // chunk — and `compose_prompt` returns the base untouched when the feature is
    // off, nothing is detected, and no mode matches (so the common path is today's).
    // [GRAIN] Detect context only when the feature is on (one cheap OS call). The
    // spoken Prompt Record instruction is independent of that toggle, so
    // `compose_prompt` is always consulted — it returns the base untouched when
    // there is neither a spoken instruction nor any context layer to add.
    let ctx = if settings.context_awareness_enabled {
        crate::context_detect::detect_active_context(settings.context_nearby_terms)
    } else {
        None
    };
    let mut prompt =
        crate::context_detect::compose_prompt(&prompt, settings, ctx.as_ref(), spoken_prompt);
    if rolling {
        prompt.push_str(crate::post_process_router::ROLLING_SEAM_PROMPT);
    }

    // [GRAIN] Smart rotation: fan out across ENABLED post-process providers
    // (round-robin + per-provider daily quota + failover). Independent of STT —
    // post-processing keeps its own provider list.
    if settings.post_process_smart_rotation {
        return crate::post_process_router::post_process_rotated(app, &prompt, transcription).await;
    }

    // Default single-provider path — unchanged behavior.
    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    // Fetch the shared HTTP client from Tauri managed state.
    let Some(http_client) = app.try_state::<reqwest::Client>() else {
        warn!("post-process: shared HTTP client unavailable");
        return None;
    };
    let http_client = http_client.inner().clone();

    // Single-provider path: no rotation, so the tracker isn't consulted/updated.
    match crate::post_process_router::run_one_provider_with_timeout(
        &http_client,
        &provider,
        model,
        api_key,
        &prompt,
        transcription,
    )
    .await
    {
        CallOutcome::Ok { text, .. } => Some(text),
        _ => None,
    }
}

/// Run ONE post-process provider with already-resolved model/key/prompt. Returns
/// the processed text, or None on any failure/empty result (so callers can fail
/// over to the next provider or fall back to the raw transcript).
/// [GRAIN] pub(crate): driven by post_process_router's timeout wrapper.
pub(crate) async fn run_one_provider(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    prompt: &str,
    transcription: &str,
) -> CallOutcome {
    // Disable reasoning for providers where post-processing rarely benefits from it.
    // - custom: top-level reasoning_effort (works for local OpenAI-compat servers)
    // - openrouter: nested reasoning object; exclude:true also keeps reasoning text
    //   out of the response so it can't pollute structured-output JSON parsing
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let system_prompt = build_system_prompt(prompt);
        let user_content = transcription.to_string();

        // Handle Apple Intelligence separately since it uses native Swift APIs.
        // It's a LOCAL backend — no network, so no rate-limit signal (success or
        // Failed only).
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return CallOutcome::Failed;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            CallOutcome::Failed
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            CallOutcome::Ok {
                                text: result,
                                remaining_requests: None,
                                remaining_tokens: None,
                                total_tokens: None,
                            }
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        CallOutcome::Failed
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return CallOutcome::Failed;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            client,
            provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            reasoning_effort.clone(),
            reasoning.clone(),
        )
        .await
        {
            Ok(success) => match success.content {
                Some(content) => {
                    // Extract the transcription field; fall back to raw content.
                    let text = match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(json) => json
                            .get(TRANSCRIPTION_FIELD)
                            .and_then(|t| t.as_str())
                            .map(strip_invisible_chars)
                            .unwrap_or_else(|| {
                                error!("Structured output response missing 'transcription' field");
                                strip_invisible_chars(&content)
                            }),
                        Err(e) => {
                            error!("Failed to parse structured output JSON: {e}. Returning raw content.");
                            strip_invisible_chars(&content)
                        }
                    };
                    debug!(
                        "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                        provider.id,
                        text.len()
                    );
                    return CallOutcome::Ok {
                        text,
                        remaining_requests: success.remaining_requests,
                        remaining_tokens: success.remaining_tokens,
                        total_tokens: success.total_tokens,
                    };
                }
                None => {
                    error!("LLM API response has no content");
                    return CallOutcome::Failed;
                }
            },
            // A 429 means this provider is rate-limited — don't retry it in legacy
            // mode; surface the cooldown so the router moves on.
            Err(LlmError::RateLimited { retry_after_s }) => {
                warn!(
                    "Structured output rate-limited for provider '{}'",
                    provider.id
                );
                return CallOutcome::RateLimited { retry_after_s };
            }
            Err(LlmError::Other(e)) => {
                warn!(
                    "Structured output failed for provider '{}': {e}. Falling back to legacy mode.",
                    provider.id
                );
                // Fall through to legacy mode below.
            }
        }
    }

    // Legacy mode: Replace ${output} variable in the prompt with the actual text
    let processed_prompt = prompt.replace("${output}", transcription);
    debug!("Processed prompt length: {} chars", processed_prompt.len());

    match crate::llm_client::send_chat_completion(
        client,
        provider,
        api_key,
        &model,
        processed_prompt,
        reasoning_effort,
        reasoning,
    )
    .await
    {
        Ok(success) => match success.content {
            Some(content) => {
                let text = strip_invisible_chars(&content);
                debug!(
                    "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                    provider.id,
                    text.len()
                );
                CallOutcome::Ok {
                    text,
                    remaining_requests: success.remaining_requests,
                    remaining_tokens: success.remaining_tokens,
                    total_tokens: success.total_tokens,
                }
            }
            None => {
                error!("LLM API response has no content");
                CallOutcome::Failed
            }
        },
        Err(LlmError::RateLimited { retry_after_s }) => {
            warn!(
                "LLM post-processing rate-limited for provider '{}'",
                provider.id
            );
            CallOutcome::RateLimited { retry_after_s }
        }
        Err(LlmError::Other(e)) => {
            error!(
                "LLM post-processing failed for provider '{}': {e}. Falling back to original transcription.",
                provider.id
            );
            CallOutcome::Failed
        }
    }
}

/// [GRAIN] Extension host `llm.complete` (SPEC §1.3 `llm` capability): run an
/// arbitrary prompt through the user's ACTIVE post-process provider. The
/// extension supplies only the text — the provider id, model, and API key are
/// resolved here and never cross the WS boundary. Reuses the single-provider
/// resolution + timeout wrapper (no rotation; extensions are `background`
/// priority by default, SPEC §3.4).
pub(crate) async fn complete_for_extension(
    app: &tauri::AppHandle,
    prompt: &str,
) -> Result<String, String> {
    use tauri::Manager;
    let settings = crate::settings::get_settings(app);
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or("no post-processing provider is configured")?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    if model.trim().is_empty() {
        return Err("the active provider has no model configured".into());
    }
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let http_client = app
        .try_state::<reqwest::Client>()
        .ok_or("shared HTTP client unavailable")?
        .inner()
        .clone();
    // The extension's text is the USER message; no system prompt.
    match crate::post_process_router::run_one_provider_with_timeout(
        &http_client,
        &provider,
        model,
        api_key,
        "",
        prompt,
    )
    .await
    {
        CallOutcome::Ok { text, .. } => Ok(text),
        _ => Err("the LLM call failed or returned nothing".into()),
    }
}
