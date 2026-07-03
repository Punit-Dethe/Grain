#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error};
use crate::llm_client::LlmError;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::rotation_state::CallOutcome;
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use grain_core::PostProcessProvider;
use grain_core::{DaemonEvent, SessionMode}; // [GRAIN] pill lifecycle events
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// [GRAIN] Monotonic id for the current recording session (pill events).
static SESSION_ID: AtomicU64 = AtomicU64::new(0);

/// [GRAIN] The current pill session id, for emitters outside this module (the
/// unified TranscriptionManager mirrors live stream text to the pill).
pub(crate) fn current_session_id() -> u64 {
    SESSION_ID.load(Ordering::Relaxed)
}
use tauri::Manager;
use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn set_post_process_override(&self, _override: bool) {}
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
    post_process_override: AtomicBool,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

/// [GRAIN] Token-efficient seam-repair layer appended to the post-process
/// prompt ONLY for rolling-window transcripts. Rolling text is assembled from
/// sequential audio segments, so its residual defects are seam-shaped (casing,
/// stray/missing sentence punctuation, doubled boundary words) — this line aims
/// the LLM at exactly those, and at nothing else. ~40 tokens; invisible to the
/// user.
const ROLLING_SEAM_PROMPT: &str = "\n[Live dictation]\nThe text was assembled \
from sequential speech segments. Repair segment-join artifacts: wrong \
capitalization, stray or missing periods/commas, doubled words, extra spaces. \
Never reword, reorder, or drop content.";

async fn post_process_transcription(
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
    if transcription.trim().is_empty() {
        debug!("Post-processing skipped because transcription is empty");
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
        prompt.push_str(ROLLING_SEAM_PROMPT);
    }

    // [GRAIN] Smart rotation: fan out across ENABLED post-process providers
    // (round-robin + per-provider daily quota + failover). Independent of STT —
    // post-processing keeps its own provider list.
    if settings.post_process_smart_rotation {
        return post_process_rotated(app, &prompt, transcription).await;
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
    match run_one_provider_with_timeout(
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

/// [GRAIN] Rotation path: try ENABLED post-process providers best-first by live
/// health (recent 429s cool down, headroom leads — via `select_order`) until one
/// returns a result, recording quota usage on success. Returns None if none are
/// eligible or all fail (the caller then pastes the raw transcript).
async fn post_process_rotated(
    app: &AppHandle,
    prompt: &str,
    transcription: &str,
) -> Option<String> {
    // [GRAIN] Roll daily quotas first; then read settings ONCE so the pool
    // reflects any newly-zeroed counters.
    crate::post_process_router::reset_quota_if_new_day(app);
    let settings = get_settings(app);
    // Eligible = enabled + under daily quota (the hard gate) AND has a model
    // configured (no point routing to a provider that can't run). The quota gate
    // is ours; the tracker only orders what survives it.
    let eligible: Vec<PostProcessProvider> = crate::post_process_router::rotation_pool(&settings)
        .into_iter()
        .filter(|p| {
            settings
                .post_process_models
                .get(&p.id)
                .map(|m| !m.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    if eligible.is_empty() {
        debug!(
            "Post-process smart rotation is on, but no eligible providers have a model configured"
        );
        return None;
    }

    let Some(trackers) = app.try_state::<Arc<crate::rotation_state::RotationTrackers>>() else {
        warn!("Post-process rotation: RotationTrackers unavailable");
        return None;
    };

    let Some(http_client) = app.try_state::<reqwest::Client>() else {
        warn!("Post-process rotation: shared HTTP client unavailable");
        return None;
    };
    let http_client = http_client.inner().clone();

    // Order best-first by live health (recent 429s cool down, headroom leads).
    let est_tokens = provider_router::estimate_tokens(transcription);
    let candidates: Vec<(String, String)> = eligible
        .iter()
        .map(|p| (p.id.clone(), p.base_url.clone()))
        .collect();

    // Failover walk lives in the shared driver; we supply only how to run one
    // provider and how to record quota on success.
    let result = crate::rotation_state::run_with_rotation(
        &trackers.llm,
        &candidates,
        est_tokens,
        |id| {
            let http_client = http_client.clone();
            let eligible = &eligible;
            let settings = &settings;
            async move {
                let Some(provider) = eligible.iter().find(|p| p.id == id) else {
                    return CallOutcome::Failed;
                };
                let model = settings
                    .post_process_models
                    .get(&provider.id)
                    .cloned()
                    .unwrap_or_default();
                let api_key = settings
                    .post_process_api_keys
                    .get(&provider.id)
                    .cloned()
                    .unwrap_or_default();
                debug!(
                    "Rotation: trying provider '{}' (model: {})",
                    provider.id, model
                );
                run_one_provider_with_timeout(
                    &http_client,
                    provider,
                    model,
                    api_key,
                    prompt,
                    transcription,
                )
                .await
            }
        },
        |id| {
            crate::post_process_router::record_usage(app, id);
            log::info!("[GRAIN] post-process routed to '{id}'");
        },
    )
    .await;

    match result {
        Ok(text) => Some(text),
        Err(e) => {
            warn!("Post-process rotation: no provider produced output ({e})");
            None
        }
    }
}

/// Hard ceiling on a single post-process provider call. A provider that accepts
/// the connection but never responds must not hang the transcribe→paste pipeline
/// (and in rotation mode must yield so the next provider is tried). Matches the
/// Agent's `AGENT_LLM_TIMEOUT` so all LLM paths behave the same.
const LLM_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// [GRAIN] `run_one_provider` bounded by [`LLM_REQUEST_TIMEOUT`]. A timeout is
/// surfaced as [`CallOutcome::Failed`] — identical to any other failure — so the
/// single-provider path returns `None` and the rotation path fails over to the
/// next candidate instead of stalling. Both post-process call sites go through
/// here so neither can forget the deadline.
async fn run_one_provider_with_timeout(
    client: &reqwest::Client,
    provider: &PostProcessProvider,
    model: String,
    api_key: String,
    prompt: &str,
    transcription: &str,
) -> CallOutcome {
    match tokio::time::timeout(
        LLM_REQUEST_TIMEOUT,
        run_one_provider(client, provider, model, api_key, prompt, transcription),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            warn!(
                "post-process provider '{}' timed out after {}s",
                provider.id,
                LLM_REQUEST_TIMEOUT.as_secs()
            );
            CallOutcome::Failed
        }
    }
}

/// Run ONE post-process provider with already-resolved model/key/prompt. Returns
/// the processed text, or None on any failure/empty result (so callers can fail
/// over to the next provider or fall back to the raw transcript).
async fn run_one_provider(
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

async fn maybe_convert_chinese_variant(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    // Check if language is set to Simplified or Traditional Chinese
    let is_simplified = settings.selected_language == "zh-Hans";
    let is_traditional = settings.selected_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("selected_language is not Simplified or Traditional Chinese; skipping translation");
        return None;
    }

    debug!(
        "Starting Chinese translation using OpenCC for language: {}",
        settings.selected_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
    // [GRAIN] Prompt Record: the spoken AI instruction for this transcript (audio
    // after the pill-click mark), already transcribed. `None` for a normal
    // dictation. When present, the caller also forces `post_process = true` so the
    // instruction is actually applied regardless of which shortcut stopped the
    // session.
    spoken_prompt: Option<String>,
    // [GRAIN] True when `transcription` came from the rolling-window assembler —
    // enables the token-efficient seam-repair prompt layer.
    rolling: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);

    // [GRAIN] Voice actions: fire any spoken trigger (open apps/sites) and strip
    // it from what we paste. Runs on the finalized transcript BEFORE
    // post-processing so a pure command ("start coding") never costs an LLM call
    // — if the whole utterance was the command, `final_text` is now empty and the
    // paste path below already skips empty output. Zero-cost when no actions
    // are configured (a single `is_empty()` check inside `intercept`).
    let mut final_text = crate::voice_actions::intercept(app, transcription);
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    if let Some(converted_text) = maybe_convert_chinese_variant(&settings, &final_text).await {
        final_text = converted_text;
    }

    if post_process {
        if let Some(processed_text) = post_process_transcription(
            app,
            &settings,
            &final_text,
            spoken_prompt.as_deref(),
            rolling,
        )
        .await
        {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    } else if final_text != transcription {
        post_processed_text = Some(final_text.clone());
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // [GRAIN] Only warm the local ASR model when this recording will be
        // transcribed locally. When STT smart rotation routes batch to a cloud
        // provider, loading the on-device model here is wasted work that sits
        // resident in RAM until the idle/immediate unload fires. The cloud route
        // never touches it; if rotation later finds no eligible provider,
        // stt_router::local() loads the model on demand. VAD pre-load stays
        // unconditional below — recording needs it for either backend.
        let kickoff_started = Instant::now();
        if !crate::stt_router::will_route_to_cloud(app) {
            tm.initiate_model_load();
        } else {
            debug!("[GRAIN] batch routes to cloud STT — skipping local model warm-up");
        }
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        change_tray_icon(app, TrayIconState::Recording);
        let tray_elapsed = tray_started.elapsed();
        // [GRAIN] The winit pill is the single overlay surface for BOTH batch and
        // rolling — driven by the DaemonEvents below (emitted on successful start,
        // same pattern as the rolling path). No Handy webview overlay.

        // Get the microphone mode to determine audio feedback timing
        let plan_started = Instant::now();
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        let plan_elapsed = plan_started.elapsed();
        // Everything above runs before capture can begin, so each span here is
        // added keypress->capture latency. [GRAIN] No overlay step: the pill is
        // shown by DaemonEvents after the recording actually starts.
        debug!(
            "start-path pre-recording steps: model_kickoff={:?} tray={:?} settings={:?}",
            kickoff_elapsed, tray_elapsed, plan_elapsed
        );
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    // Small delay to ensure microphone stream is active
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        // Helper handles disabled audio feedback by returning early, so we reuse it
                        // to keep mute sequencing consistent in every mode.
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // [GRAIN] tell the single pill recording has started (Batch mode). The
            // pill is mode-agnostic, so this drives the same show+animate as rolling.
            // OverlayConfig first so the pill anchors (or stays hidden if None).
            let sid = SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
            crate::bridge::emit(
                app,
                DaemonEvent::OverlayConfig {
                    position: get_settings(app).overlay_position,
                },
            );
            crate::bridge::emit(
                app,
                DaemonEvent::RecordingStarted {
                    session_id: sid,
                    mode: SessionMode::Batch,
                },
            );
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
            if !get_settings(app).push_to_talk {
                shortcut::register_send_to_ai_shortcut(app);
            }
        } else {
            // Starting failed (e.g. blocked mic permissions). The pill was never
            // shown (we only emit on success), so nothing to tear down here.
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn set_post_process_override(&self, override_val: bool) {
        self.post_process_override
            .store(override_val, Ordering::Relaxed);
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);
        shortcut::unregister_send_to_ai_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        // [GRAIN] stop pressed → the single pill enters "processing" while we
        // transcribe (mirrors the rolling path). Carried into the async tail so
        // ProcessingComplete (pill hide) reuses the same session id.
        let session_id = SESSION_ID.load(Ordering::Relaxed);
        crate::bridge::emit(app, DaemonEvent::RecordingStopped { session_id });

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process || self.post_process_override.load(Ordering::Relaxed);

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::ProcessingComplete {
                            session_id,
                            text: String::new(),
                        },
                    );
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save. [GRAIN] S4: route
                    // through the STT dispatcher — local in-process by default,
                    // or cloud rotation when smart rotation is on. The WAV task
                    // above runs concurrently while this awaits.
                    //
                    // [GRAIN] Prompt Record: if the user clicked the pill mid-
                    // recording, the buffer is split at that mark into content +
                    // a spoken AI instruction, each transcribed independently. A
                    // recorded instruction forces the AI path regardless of which
                    // shortcut stopped the session. No mark → a single pass, as before.
                    let prompt_mark = rm.take_prompt_mark();
                    let transcription_time = Instant::now();
                    let (transcription_result, spoken_prompt) =
                        crate::prompt_record::transcribe_split(&ah, samples, prompt_mark).await;
                    let post_process = post_process || spoken_prompt.is_some();

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            // [GRAIN] pill is already in "processing" from the
                            // RecordingStopped above — no extra overlay call needed.
                            let processed = process_transcription_output(
                                &ah,
                                &transcription,
                                post_process,
                                spoken_prompt,
                                false,
                            )
                            .await;

                            // Save to history if WAV was saved
                            if wav_saved {
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    post_process,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            if processed.final_text.is_empty() {
                                crate::bridge::emit(
                                    &ah,
                                    DaemonEvent::ProcessingComplete {
                                        session_id,
                                        text: String::new(),
                                    },
                                );
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                ah.run_on_main_thread(move || {
                                    match utils::paste(final_text, ah_clone.clone()) {
                                        Ok(()) => debug!(
                                            "Text pasted successfully in {:?}",
                                            paste_time.elapsed()
                                        ),
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                    }
                                    crate::bridge::emit(
                                        &ah_clone,
                                        DaemonEvent::ProcessingComplete {
                                            session_id,
                                            text: String::new(),
                                        },
                                    );
                                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    crate::bridge::emit(
                                        &ah,
                                        DaemonEvent::ProcessingComplete {
                                            session_id,
                                            text: String::new(),
                                        },
                                    );
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                });
                            }
                        }
                        Err(err) => {
                            debug!("Global Shortcut Transcription error: {}", err);
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            crate::bridge::emit(
                                &ah,
                                DaemonEvent::ProcessingComplete {
                                    session_id,
                                    text: String::new(),
                                },
                            );
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                crate::bridge::emit(
                    &ah,
                    DaemonEvent::ProcessingComplete {
                        session_id,
                        text: String::new(),
                    },
                );
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
        // [GRAIN] tear down any rolling session + tell the pill to hide.
        if let Some(rt) = app.try_state::<Arc<crate::rolling::RollingTranscriber>>() {
            rt.cancel_session();
        }
        // [GRAIN] Native ASR: cancel must tear down the live stream worker too,
        // or its command channel stays open and blocks the next start_stream.
        // The discarded transcript is intentionally dropped.
        if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
            tm.cancel_stream();
        }
        crate::bridge::emit(
            app,
            DaemonEvent::SessionCancelled {
                session_id: SESSION_ID.load(Ordering::Relaxed),
            },
        );
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// [GRAIN] Prompt switcher — cycles the active post-processing prompt and shows
// the new title in the pill. A tap shortcut: the switch happens on press.
struct PromptSwitchAction {
    delta: i32,
}

impl ShortcutAction for PromptSwitchAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let mut settings = get_settings(app);
        let n = settings.post_process_prompts.len() as i32;
        if n == 0 {
            return;
        }
        let cur_idx = settings
            .post_process_selected_prompt_id
            .as_deref()
            .and_then(|id| {
                settings
                    .post_process_prompts
                    .iter()
                    .position(|p| p.id == id)
            })
            .unwrap_or(0) as i32;
        // Wrapping modulo that stays correct for negative deltas.
        let new_idx = (((cur_idx + self.delta) % n) + n) % n;
        let chosen = &settings.post_process_prompts[new_idx as usize];
        let chosen_id = chosen.id.clone();
        let chosen_name = chosen.name.clone();

        settings.post_process_selected_prompt_id = Some(chosen_id);
        crate::settings::write_settings(app, settings);

        // Show the new title in the pill.
        crate::bridge::emit(app, DaemonEvent::PromptChanged { name: chosen_name });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Summon the Agent — a voice-first AI scratchpad on the current selection.
// A tap shortcut: it fires on press and hands off to `agent::summon`, which does
// the selection capture + window creation off the input thread.
struct SummonAgentAction;

impl ShortcutAction for SummonAgentAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

struct AgentSubmitAction;

impl ShortcutAction for AgentSubmitAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::global_submit(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

struct AgentCloseAction;

impl ShortcutAction for AgentCloseAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::global_close(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Ask a follow-up on the Agent's latest reply. Registered transiently by
// agent.rs while an Agent surface (panel / pill offer) is live — never global.
struct AgentFollowupAction;

impl ShortcutAction for AgentFollowupAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::open_followup(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Grain Space quick add (Input C) — a tap shortcut that silently saves
// the current selection as a raw note. All work happens off the input thread
// inside `grain_space::capture::quick_add` (selection grab polls the clipboard).
struct GrainSpaceQuickAddAction;

impl ShortcutAction for GrainSpaceQuickAddAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::grain_space::capture::quick_add(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Grain Space overlay toggle (Phase 3) — tap creates the notes window,
// tap again destroys it. All window work hops to the async runtime inside
// `window::toggle` (tauri#3990), so this returns instantly.
struct GrainSpaceOpenAction;

impl ShortcutAction for GrainSpaceOpenAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::grain_space::window::toggle(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Grain Recall (RECALL-PLAN R1) — summons the Agent surfaces in memory
// mode (ask your notes, get an answer). Its OWN binding, distinct from
// summon_agent: the mode is fixed by which key fired, never guessed.
struct GrainSpaceRecallAction;

impl ShortcutAction for GrainSpaceRecallAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon_memory(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// [GRAIN] Grain Space note capture — summons the Agent surfaces in Capture mode:
// speak OR type a note (and any selected text comes along as the body), then it
// is structured and saved. Replaces the old transcribe-pipeline capture so the
// user gets the pill's text input for free. Its OWN binding; mode fixed here.
struct GrainSpaceCaptureAction;

impl ShortcutAction for GrainSpaceCaptureAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::agent::summon_capture(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
// [GRAIN] Real-time rolling-window transcribe action. Streams audio through the
// rolling engine in the background (no partial display); pastes the assembled
// transcript on stop, with a batch fallback if rolling yields nothing.
struct RealtimeTranscribeAction {
    post_process_override: AtomicBool,
}

impl ShortcutAction for RealtimeTranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rt = Arc::clone(&app.state::<Arc<crate::rolling::RollingTranscriber>>());
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

        // Load the rolling model in the background — ready before the first chunk;
        // a failed/slow load is covered by the batch fallback on stop.
        {
            let app = app.clone();
            let rt = rt.clone();
            std::thread::spawn(move || {
                if let Err(e) = rt.ensure_loaded(&app) {
                    warn!("[GRAIN] rolling model load failed: {e}");
                }
            });
        }
        // [GRAIN] Session id up front so the (optional) live-preview worker can
        // tag its AsrStreamText events with the same id the RecordingStarted
        // event below carries. Preview is opt-in; when off the worker takes the
        // zero-overhead path and no preview events fire.
        let preview = get_settings(app).rolling_live_preview;
        let sid = SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        rt.start_session(app.clone(), sid, preview);
        {
            let rm = Arc::clone(&rm);
            std::thread::spawn(move || {
                let _ = rm.preload_vad();
            });
        }

        change_tray_icon(app, TrayIconState::Recording);
        // [GRAIN] C1: no Handy webview overlay on the real-time path — the winit
        // pill is the only surface, driven by the DaemonEvents below.

        let binding_id = binding_id.to_string();
        let is_always_on = get_settings(app).always_on_microphone;
        let mut recording_error: Option<String> = None;
        if is_always_on {
            let rm_mute = Arc::clone(&rm);
            let app2 = app.clone();
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app2, SoundType::Start);
                rm_mute.apply_mute();
            });
            if let Err(e) = rm.try_start_recording(&binding_id) {
                recording_error = Some(e);
            }
        } else {
            match rm.try_start_recording(&binding_id) {
                Ok(()) => {
                    let app2 = app.clone();
                    let rm = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        play_feedback_sound_blocking(&app2, SoundType::Start);
                        rm.apply_mute();
                    });
                }
                Err(e) => recording_error = Some(e),
            }
        }

        if recording_error.is_none() {
            // [GRAIN] B2: tell the pill recording has started (it shows + animates).
            // OverlayConfig first so the pill anchors (or stays hidden if None).
            // With the live preview on, use the Studio Window (NativeAsr) so the
            // growing caption has room; otherwise the compact dictation pill.
            crate::bridge::emit(
                app,
                DaemonEvent::OverlayConfig {
                    position: get_settings(app).overlay_position,
                },
            );
            crate::bridge::emit(
                app,
                DaemonEvent::RecordingStarted {
                    session_id: sid,
                    mode: if preview {
                        SessionMode::NativeAsr
                    } else {
                        SessionMode::Dictation
                    },
                },
            );
            shortcut::register_cancel_shortcut(app);
            if !get_settings(app).push_to_talk {
                shortcut::register_send_to_ai_shortcut(app);
            }
        } else {
            rt.cancel_session();
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }
    }

    fn set_post_process_override(&self, override_val: bool) {
        self.post_process_override
            .store(override_val, Ordering::Relaxed);
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        shortcut::unregister_cancel_shortcut(app);
        shortcut::unregister_send_to_ai_shortcut(app);
        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let rt = Arc::clone(&app.state::<Arc<crate::rolling::RollingTranscriber>>());

        // [GRAIN] B2: stop pressed → pill enters "processing" while the remaining
        // chunks finalize (recording overrode processing until now).
        let session_id = SESSION_ID.load(Ordering::Relaxed);
        crate::bridge::emit(app, DaemonEvent::RecordingStopped { session_id });

        change_tray_icon(app, TrayIconState::Transcribing);
        // [GRAIN] C1: pill already showed "processing" from RecordingStopped above.
        rm.remove_mute();
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string();
        let post_process = self.post_process_override.load(Ordering::Relaxed);
        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());

            // Full audio (for WAV/history + the batch fallback).
            let samples = rm.stop_recording(&binding_id).unwrap_or_default();
            // [GRAIN] Prompt Record mark (the pill-click split point), taken before
            // draining the worker.
            let prompt_mark = rm.take_prompt_mark();
            // Drain the rolling worker → final assembled transcript. Always done,
            // even under Prompt Record, so the worker never leaks — its text is
            // just unused in that case (it mixed content + instruction).
            let rolling_text = rt.finish_session().unwrap_or_default();

            // [GRAIN] Prompt Record: the rolling-assembled text covers the WHOLE
            // utterance (content + spoken instruction mixed), so it can't be split.
            // Re-transcribe the two audio slices batch-style instead. This extra
            // pass only happens when the user actually clicked the pill.
            let (final_text, spoken_prompt, post_process, was_rolling) =
                if let Some(m) = prompt_mark.filter(|&m| m > 0 && m < samples.len()) {
                    let (content_res, spoken) =
                        crate::prompt_record::transcribe_split(&ah, samples.clone(), Some(m)).await;
                    // `transcribe_split` routes through the STT dispatcher, which
                    // finalizes internally — don't finalize again. Batch-style
                    // re-transcription has no rolling seams.
                    (
                        content_res.unwrap_or_default(),
                        spoken.clone(),
                        post_process || spoken.is_some(),
                        false,
                    )
                } else {
                    let assembled = !rolling_text.trim().is_empty();
                    let ft = if assembled {
                        // [GRAIN] Apply the shared final-text stage (custom-word dictionary
                        // + filler/stutter filtering) ONCE on the assembled transcript.
                        // The rolling engine never biases via Whisper `initial_prompt`, so
                        // the fuzzy custom-word pass must run here. Done once per dictation,
                        // NOT per 15-20s chunk.
                        let settings = get_settings(&ah);
                        crate::audio_toolkit::finalize_transcript(
                            &rolling_text,
                            &settings.custom_words,
                            settings.word_correction_threshold,
                            &settings.app_language,
                            &settings.custom_filler_words,
                            false,
                            &settings.snippets,
                            settings.scrap_that_enabled,
                        )
                    } else if !samples.is_empty() {
                        warn!("[GRAIN] rolling produced no text — falling back to batch");
                        // `tm.transcribe` already runs finalize_transcript internally, so
                        // the fallback text is finalized; don't finalize it again.
                        tm.transcribe(samples.clone()).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    (ft, None, post_process, assembled)
                };

            let processed = process_transcription_output(
                &ah,
                &final_text,
                post_process,
                spoken_prompt,
                was_rolling,
            )
            .await;
            let final_text = processed.final_text;

            if !samples.is_empty() {
                let file_name = format!("grain-{}.wav", chrono::Utc::now().timestamp());
                let wav_path = hm.recordings_dir().join(&file_name);
                let samples_for_wav = samples.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                })
                .await;
                if let Err(e) = hm.save_entry(
                    file_name,
                    final_text.clone(),
                    post_process,
                    processed.post_processed_text.clone(),
                    processed.post_process_prompt.clone(),
                ) {
                    error!("Failed to save history entry: {e}");
                }
            }

            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste(final_text, ah_clone.clone()) {
                        error!("Failed to paste real-time transcription: {e}");
                        let _ = ah_clone.emit("paste-error", ());
                    }
                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                })
                .unwrap_or_else(|e| {
                    error!("Failed to run paste on main thread: {e:?}");
                    change_tray_icon(&ah, TrayIconState::Idle);
                });
            }

            // [GRAIN] B2: processing finished → pill hides.
            crate::bridge::emit(
                &ah,
                DaemonEvent::ProcessingComplete {
                    session_id,
                    text: String::new(),
                },
            );
        });
    }
}

// [GRAIN] Native ASR — push-to-talk live streaming, on the SAME unified
// TranscriptionManager engine as Batch/Rolling: the shortcut loads the selected
// streaming model into the shared slot, opens the mic (frames fan out to the
// manager's StreamRouter), and the manager's stream worker emits live committed
// text to the Studio Window (`AsrStreamText` `DaemonEvent`s — this action only
// owns the recording lifecycle, not the live text). `stop` finalizes the
// stream, pastes the transcript, and saves history.
struct NativeAsrAction;

impl ShortcutAction for NativeAsrAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Require a selected + installed + streaming-capable model. Without one,
        // surface a clear, actionable error to the pill and don't open the mic.
        let selected = get_settings(app).selected_asr_model;
        let mm = app.state::<Arc<ModelManager>>();
        let ok = mm
            .get_model_info(&selected)
            .is_some_and(|m| m.is_downloaded && m.supports_streaming);
        if !ok {
            warn!("Native ASR: no streaming model selected/installed");
            crate::bridge::emit(
                app,
                DaemonEvent::ModelError {
                    error: "Install and select a streaming model in Settings → Speech to Text"
                        .into(),
                },
            );
            return;
        }

        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());

        // Load the streaming model into the shared engine slot (swaps out a
        // resident Batch model if needed), then open the stream worker: it waits
        // for the load, and frames queued on the router are never lost.
        tm.initiate_model_load_for(selected);
        tm.start_stream();

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);

        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        let mut recording_error: Option<String> = None;
        if is_always_on {
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });
            if let Err(e) = rm.try_start_recording(&binding_id) {
                recording_error = Some(e);
            }
        } else {
            match rm.try_start_recording(&binding_id) {
                Ok(()) => {
                    let app2 = app.clone();
                    let rm2 = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        play_feedback_sound_blocking(&app2, SoundType::Start);
                        rm2.apply_mute();
                    });
                }
                Err(e) => recording_error = Some(e),
            }
        }

        if recording_error.is_none() {
            let sid = SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
            crate::bridge::emit(
                app,
                DaemonEvent::OverlayConfig {
                    position: get_settings(app).overlay_position,
                },
            );
            crate::bridge::emit(
                app,
                DaemonEvent::RecordingStarted {
                    session_id: sid,
                    mode: SessionMode::NativeAsr,
                },
            );

            shortcut::register_cancel_shortcut(app);
        } else {
            // Tear down the pending stream worker so its channel doesn't leak
            // and block the next start_stream.
            tm.cancel_stream();
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        shortcut::unregister_cancel_shortcut(app);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let binding_id = binding_id.to_string();

        let session_id = SESSION_ID.load(Ordering::Relaxed);
        crate::bridge::emit(app, DaemonEvent::RecordingStopped { session_id });

        change_tray_icon(app, TrayIconState::Transcribing);
        rm.remove_mute();
        play_feedback_sound(app, SoundType::Stop);

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());

            // The mic frames already reached the stream worker live; keep the
            // captured samples only as the batch-fallback input (mirrors Handy:
            // a model that turned out not to stream still yields a transcript).
            let samples = rm.stop_recording(&binding_id).unwrap_or_default();
            // [GRAIN] Prompt Record split mark (a pill click on the Studio waveform).
            let prompt_mark = rm.take_prompt_mark();

            // `finalize_stream` blocks up to its internal timeout while the worker
            // flushes, so keep the wait off the async executor. Always run it (even
            // under Prompt Record) so the stream worker never leaks — its text is
            // just unused when we re-transcribe the sliced audio below.
            let tm_finalize = Arc::clone(&tm);
            let samples_for_fallback = samples.clone();
            let finalized = tauri::async_runtime::spawn_blocking(move || {
                match tm_finalize.finalize_stream() {
                    // A finalized stream with usable text wins (already
                    // custom-word/filler processed by finalize_stream).
                    Ok(Some(text)) if !text.trim().is_empty() => text,
                    // No usable stream → batch-transcribe the captured audio.
                    Ok(_) if !samples_for_fallback.is_empty() => {
                        warn!("Native ASR: stream produced no text — batch fallback");
                        tm_finalize
                            .transcribe(samples_for_fallback)
                            .unwrap_or_default()
                    }
                    Ok(_) => String::new(),
                    Err(e) => {
                        error!("Native ASR: stream finalize failed: {e}");
                        String::new()
                    }
                }
            })
            .await
            .unwrap_or_default();

            let final_text = if let Some(m) = prompt_mark.filter(|&m| m > 0 && m < samples.len()) {
                // [GRAIN] Prompt Record on the streaming path: the live transcript
                // covered content + the spoken instruction together, so it can't be
                // split. Re-transcribe the two audio slices and post-process the
                // content with the spoken instruction (AI forced on, regardless of
                // which shortcut stopped the session). `process_transcription_output`
                // also runs voice actions on the content.
                let (content_res, spoken) =
                    crate::prompt_record::transcribe_split(&ah, samples.clone(), Some(m)).await;
                let content = content_res.unwrap_or_default();
                let processed =
                    process_transcription_output(&ah, &content, true, spoken, false).await;
                let ft = processed.final_text;
                if !ft.trim().is_empty() {
                    if let Err(e) = hm.save_entry(
                        String::new(),
                        content.clone(),
                        true,
                        processed.post_processed_text.clone(),
                        processed.post_process_prompt.clone(),
                    ) {
                        error!("Failed to save Native ASR history entry: {e}");
                    }
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::AsrSessionFinal {
                            session_id,
                            text: ft.clone(),
                        },
                    );
                }
                ft
            } else {
                // [GRAIN] Voice actions also apply to the Live streaming path: fire
                // any spoken trigger and strip it before paste (no-op when unused).
                let finalized = crate::voice_actions::intercept(&ah, &finalized);
                if finalized.trim().is_empty() {
                    String::new()
                } else {
                    if let Err(e) =
                        hm.save_entry(String::new(), finalized.clone(), false, None, None)
                    {
                        error!("Failed to save Native ASR history entry: {e}");
                    }
                    // Protocol parity with the old worker: announce the session's
                    // final transcript on the event bus.
                    crate::bridge::emit(
                        &ah,
                        DaemonEvent::AsrSessionFinal {
                            session_id,
                            text: finalized.clone(),
                        },
                    );
                    finalized
                }
            };

            if final_text.trim().is_empty() {
                change_tray_icon(&ah, TrayIconState::Idle);
            } else {
                let ah_clone = ah.clone();
                ah.run_on_main_thread(move || {
                    if let Err(e) = utils::paste(final_text, ah_clone.clone()) {
                        error!("Failed to paste Native ASR transcription: {e}");
                        let _ = ah_clone.emit("paste-error", ());
                    }
                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                })
                .unwrap_or_else(|e| {
                    error!("Failed to run paste on main thread: {e:?}");
                    change_tray_icon(&ah, TrayIconState::Idle);
                });
            }

            // [GRAIN] processing finished → pill/Studio Window hides.
            crate::bridge::emit(
                &ah,
                DaemonEvent::ProcessingComplete {
                    session_id,
                    text: String::new(),
                },
            );
        });
    }
}

pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] real-time rolling-window transcription.
    map.insert(
        "transcribe_realtime".to_string(),
        Arc::new(RealtimeTranscribeAction {
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: true,
            post_process_override: AtomicBool::new(false),
        }) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] Native ASR — streaming dictation in the Studio Window.
    map.insert(
        "transcribe_native_asr".to_string(),
        Arc::new(NativeAsrAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] prompt switcher (cycles the active post-processing prompt).
    map.insert(
        "prompt_next".to_string(),
        Arc::new(PromptSwitchAction { delta: 1 }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "prompt_prev".to_string(),
        Arc::new(PromptSwitchAction { delta: -1 }) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] summon the Agent window.
    map.insert(
        "summon_agent".to_string(),
        Arc::new(SummonAgentAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "agent_submit".to_string(),
        Arc::new(AgentSubmitAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "agent_close".to_string(),
        Arc::new(AgentCloseAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "agent_followup".to_string(),
        Arc::new(AgentFollowupAction) as Arc<dyn ShortcutAction>,
    );
    // [GRAIN] Grain Space: silent selection quick-add (Input C) and note capture
    // (Inputs A/B — summons the Agent pill in Capture mode: speak or type, any
    // selection becomes the body, then it's structured and saved). Both bindings
    // only register while `grain_space_enabled` is on.
    map.insert(
        "grain_space_quick_add".to_string(),
        Arc::new(GrainSpaceQuickAddAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "grain_space_capture".to_string(),
        Arc::new(GrainSpaceCaptureAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "grain_space_open".to_string(),
        Arc::new(GrainSpaceOpenAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "grain_space_recall".to_string(),
        Arc::new(GrainSpaceRecallAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});
