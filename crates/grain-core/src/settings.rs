//! Grain's settings schema — the single source of truth.
//!
//! Ported faithfully from Handy's `src-tauri/src/settings.rs` (same serde field
//! names + defaults, so existing `settings_store.json` migrates cleanly), with
//! the Tauri couplings removed: no `tauri-plugin-store`, no `tauri_plugin_log`
//! conversion (that glue stays in the Tauri shell), and the OS-locale default
//! sourced from `sys-locale` instead of `tauri_plugin_os`.
//!
//! Persistence lives in [`crate::context`] (owned JSON + a separate secrets file).
//!
//! Explicit `impl Default`s are kept (rather than `#[derive(Default)]`) to mirror
//! Handy 1:1 — some are `cfg`-conditional and can't be derived anyway.
#![allow(clippy::derivable_impls)]

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";
pub const APPLE_INTELLIGENCE_DEFAULT_MODEL_ID: &str = "Apple Intelligence";

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both the old numeric format (1-5) and the new
// string format ("trace", "debug", ...).
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl Visitor<'_> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
    /// [GRAIN] Included in smart rotation when true. Defaults true so existing
    /// configs (and the manual single-provider path) behave exactly as before.
    #[serde(default = "default_pp_enabled")]
    pub enabled: bool,
    /// [GRAIN] Daily request cap for rotation; `None` = unlimited.
    #[serde(default)]
    pub quota_limit: Option<i64>,
    #[serde(default)]
    pub quota_used_today: i64,
}

fn default_pp_enabled() -> bool {
    true
}

/// [GRAIN] Which transcription backend an STT pool entry talks to. `Local` is the
/// in-process transcribe-rs model; the rest are HTTP adapters (see `stt_client`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum SttProviderKind {
    /// The in-process Parakeet/Whisper model (no network). Exactly one is implicit.
    Local,
    /// Generic OpenAI-compatible `/v1/audio/transcriptions`.
    Openai,
    Deepgram,
    Assemblyai,
}

/// [GRAIN] One entry in the STT routing pool. Each entry carries its OWN key
/// (stored separately in `stt_api_keys` by `id`), so two entries with the same
/// `base_url` = two keys for one provider. Mirrors `provider_router::ProviderConfig`
/// plus the fields the HTTP client needs (`kind`, `model`).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SttProvider {
    pub id: String,
    pub name: String,
    pub kind: SttProviderKind,
    /// Ignored for `Local`.
    #[serde(default)]
    pub base_url: String,
    /// Model/engine name sent to the provider (ignored for `Local`).
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_stt_enabled")]
    pub enabled: bool,
    /// Daily request cap; `None` = unlimited.
    #[serde(default)]
    pub quota_limit: Option<i64>,
    #[serde(default)]
    pub quota_used_today: i64,
}

fn default_stt_enabled() -> bool {
    true
}

/// The implicit, always-present local provider's pool id.
pub const STT_LOCAL_PROVIDER_ID: &str = "local";

/// Default STT pool: just the in-process local model. Remote entries are added
/// by the user. Local is always first so single-provider behavior == today.
pub fn default_stt_providers() -> Vec<SttProvider> {
    vec![
        SttProvider {
            id: STT_LOCAL_PROVIDER_ID.to_string(),
            name: "Local (on-device)".to_string(),
            kind: SttProviderKind::Local,
            base_url: String::new(),
            model: String::new(),
            enabled: true,
            quota_limit: None,
            quota_used_today: 0,
        },
        SttProvider {
            id: "groq".to_string(),
            name: "Groq STT".to_string(),
            kind: SttProviderKind::Openai,
            base_url: "https://api.groq.com/openai/v1".to_string(),
            model: "whisper-large-v3".to_string(),
            enabled: false,
            quota_limit: None,
            quota_used_today: 0,
        },
        SttProvider {
            id: "openai".to_string(),
            name: "OpenAI Whisper".to_string(),
            kind: SttProviderKind::Openai,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "whisper-1".to_string(),
            enabled: false,
            quota_limit: None,
            quota_used_today: 0,
        },
        SttProvider {
            id: "deepgram".to_string(),
            name: "Deepgram".to_string(),
            kind: SttProviderKind::Deepgram,
            base_url: "https://api.deepgram.com".to_string(),
            model: "nova-2".to_string(),
            enabled: false,
            quota_limit: None,
            quota_used_today: 0,
        },
        SttProvider {
            id: "assemblyai".to_string(),
            name: "AssemblyAI".to_string(),
            kind: SttProviderKind::Assemblyai,
            base_url: "https://api.assemblyai.com".to_string(),
            model: String::new(),
            enabled: false,
            quota_limit: None,
            quota_used_today: 0,
        },
    ]
}

fn default_stt_api_keys() -> SecretMap {
    SecretMap::default()
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum DefaultPanel {
    Settings,
    QuickPanel,
}

impl Default for DefaultPanel {
    fn default() -> Self {
        DefaultPanel::Settings
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::HandyKeys;
    }
}

impl Default for ModelUnloadTimeout {
    fn default() -> Self {
        ModelUnloadTimeout::Min5
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl Default for ClipboardHandling {
    fn default() -> Self {
        ClipboardHandling::DontModify
    }
}

impl Default for AutoSubmitKey {
    fn default() -> Self {
        AutoSubmitKey::Enter
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0),
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0),
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0),
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(&self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(&self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

impl Default for TypingTool {
    fn default() -> Self {
        TypingTool::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum WhisperAcceleratorSetting {
    Auto,
    Cpu,
    Gpu,
}

impl Default for WhisperAcceleratorSetting {
    fn default() -> Self {
        WhisperAcceleratorSetting::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

impl Default for OrtAcceleratorSetting {
    fn default() -> Self {
        OrtAcceleratorSetting::Auto
    }
}

/// Map of provider id → API key. Persisted to a SEPARATE credential file by
/// [`crate::context`], never inline in the main settings JSON. `Debug` redacts.
#[derive(Clone, Default, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct SecretMap(pub HashMap<String, String>);

impl fmt::Debug for SecretMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: HashMap<&String, &str> = self
            .0
            .iter()
            .map(|(k, v)| (k, if v.is_empty() { "" } else { "[REDACTED]" }))
            .collect();
        redacted.fmt(f)
    }
}

impl std::ops::Deref for SecretMap {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SecretMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    /// [GRAIN] Which panel is visible when the main window opens.
    #[serde(default)]
    pub default_panel: DefaultPanel,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default = "default_custom_words")]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default = "default_post_process_api_keys")]
    pub post_process_api_keys: SecretMap,
    /// [GRAIN] When true, post-processing routes among ENABLED post-process
    /// providers (round-robin + per-provider daily quota + failover). When false
    /// (default), the single `post_process_provider_id` is used — today's behavior.
    /// Independent of STT rotation: each side has its OWN provider list.
    #[serde(default)]
    pub post_process_smart_rotation: bool,
    /// [GRAIN] Local date (YYYY-MM-DD) the post-process daily quotas last reset on.
    #[serde(default)]
    pub post_process_quota_reset_date: String,
    /// [GRAIN] STT routing pool (local + remote OpenAI-compatible providers).
    #[serde(default = "default_stt_providers")]
    pub stt_providers: Vec<SttProvider>,
    /// [GRAIN] When true, transcription routes among enabled CLOUD providers
    /// (round-robin + quota + failover); the LOCAL model is excluded. When false
    /// (default), the local in-process model is used — never a surprise spike.
    #[serde(default)]
    pub stt_smart_rotation: bool,
    /// [GRAIN] STT provider API keys, by pool-entry id. Split into grain.secrets.json.
    #[serde(default = "default_stt_api_keys")]
    pub stt_api_keys: SecretMap,
    /// [GRAIN] Local date (YYYY-MM-DD) the STT daily quotas were last reset on.
    /// When today differs, quotas roll back to 0 (checked lazily at routing time).
    #[serde(default)]
    pub stt_quota_reset_date: String,
    #[serde(default = "default_post_process_models")]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub whisper_accelerator: WhisperAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_whisper_gpu_device")]
    pub whisper_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    /// [GRAIN] Voice conditioning before VAD + STT: 85 Hz high-pass (de-rumble)
    /// + boost-only noise-gated AGC for quiet/laptop mics. On by default; helps
    /// accuracy on low-volume input without touching already-loud audio.
    #[serde(default = "default_audio_conditioning")]
    pub audio_conditioning: bool,
}

fn default_model() -> String {
    "".to_string()
}
fn default_always_on_microphone() -> bool {
    false
}
fn default_audio_conditioning() -> bool {
    true
}
fn default_translate_to_english() -> bool {
    false
}
fn default_start_hidden() -> bool {
    false
}
fn default_autostart_enabled() -> bool {
    false
}
fn default_update_checks_enabled() -> bool {
    true
}
fn default_selected_language() -> String {
    "auto".to_string()
}
fn default_overlay_position() -> OverlayPosition {
    #[cfg(target_os = "linux")]
    return OverlayPosition::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayPosition::Bottom;
}
fn default_debug_mode() -> bool {
    false
}
fn default_log_level() -> LogLevel {
    LogLevel::Debug
}
fn default_word_correction_threshold() -> f64 {
    0.18
}

/// [GRAIN] Seed the dictionary with a few broadly-useful, commonly mis-split
/// proper nouns so the custom-word correction shows value out of the box (mirrors
/// how the default post-process prompts ship). The conservative 0.18 threshold
/// means these only fix near-exact splits ("you tube" → YouTube). Users can
/// remove them. Only applies to fresh settings that lack the field.
fn default_custom_words() -> Vec<String> {
    vec![
        "YouTube".to_string(),
        "iPhone".to_string(),
        "PayPal".to_string(),
        "Bluetooth".to_string(),
    ]
}
fn default_paste_delay_ms() -> u64 {
    60
}
fn default_auto_submit() -> bool {
    false
}
fn default_history_limit() -> usize {
    5
}
fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}
fn default_audio_feedback_volume() -> f32 {
    1.0
}
fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}
fn default_post_process_enabled() -> bool {
    false
}
fn default_app_language() -> String {
    sys_locale::get_locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}
fn default_show_tray_icon() -> bool {
    true
}
fn default_post_process_provider_id() -> String {
    "openai".to_string()
}

pub fn default_post_process_providers() -> Vec<PostProcessProvider> {
    // Local constructor so the rotation fields (enabled/quota) stay in one place
    // instead of being repeated across every built-in entry.
    fn p(
        id: &str,
        label: &str,
        base_url: &str,
        allow_base_url_edit: bool,
        models_endpoint: Option<&str>,
        supports_structured_output: bool,
    ) -> PostProcessProvider {
        PostProcessProvider {
            id: id.to_string(),
            label: label.to_string(),
            base_url: base_url.to_string(),
            allow_base_url_edit,
            models_endpoint: models_endpoint.map(|s| s.to_string()),
            supports_structured_output,
            enabled: true,
            quota_limit: None,
            quota_used_today: 0,
        }
    }

    let mut providers = vec![
        p("openai", "OpenAI", "https://api.openai.com/v1", false, Some("/models"), true),
        p("zai", "Z.AI", "https://api.z.ai/api/paas/v4", false, Some("/models"), true),
        p("openrouter", "OpenRouter", "https://openrouter.ai/api/v1", false, Some("/models"), true),
        p("anthropic", "Anthropic", "https://api.anthropic.com/v1", false, Some("/models"), false),
        p("groq", "Groq", "https://api.groq.com/openai/v1", false, Some("/models"), false),
        p("cerebras", "Cerebras", "https://api.cerebras.ai/v1", false, Some("/models"), true),
        p("gemini", "Gemini", "https://generativelanguage.googleapis.com/v1beta/openai", false, Some("/models"), true),
    ];

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        providers.push(p(
            APPLE_INTELLIGENCE_PROVIDER_ID,
            "Apple Intelligence",
            "apple-intelligence://local",
            false,
            None,
            true,
        ));
    }

    providers.push(p(
        "bedrock_mantle",
        "AWS Bedrock (Mantle)",
        "https://bedrock-mantle.us-east-1.api.aws/v1",
        false,
        Some("/models"),
        true,
    ));

    providers.push(p(
        "custom",
        "Custom",
        "http://localhost:11434/v1",
        true,
        Some("/models"),
        false,
    ));

    providers
}

fn default_post_process_api_keys() -> SecretMap {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id, String::new());
    }
    SecretMap(map)
}

fn default_model_for_provider(provider_id: &str) -> String {
    if provider_id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string();
    }
    String::new()
}

fn default_post_process_models() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id.clone(), default_model_for_provider(&provider.id));
    }
    map
}

/// [GRAIN] Three built-in prompts for the prompt switcher (General / Email /
/// Coding). The switcher cycles `post_process_selected_prompt_id` through the
/// full `post_process_prompts` list (these + any user-added), showing the
/// `name` (title) in the pill; the `prompt` (body, with `${output}` standing in
/// for the transcript) is what reaches the LLM.
fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![
        LLMPrompt {
            id: "general".to_string(),
            name: "General".to_string(),
            prompt: "Clean this transcript:\n1. Fix spelling, capitalization, and punctuation errors\n2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)\n3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)\n4. Remove filler words (um, uh, like as filler)\n5. Keep the language in the original version (if it was french, keep it in french for example)\n\nPreserve exact meaning and word order. Do not paraphrase or reorder content.\n\nReturn only the cleaned transcript.\n\nTranscript:\n${output}".to_string(),
        },
        LLMPrompt {
            id: "email".to_string(),
            name: "Email".to_string(),
            prompt: "Rewrite this dictated transcript as a clear, professional email body.\n1. Fix spelling, grammar, capitalization, and punctuation\n2. Organize the content into natural paragraphs\n3. Use a polite, professional tone while preserving the original meaning and intent\n4. Remove filler words and false starts\n5. Do NOT invent a greeting, sign-off, subject line, or any facts that were not said\n6. Keep the original language\n\nReturn only the email body text.\n\nTranscript:\n${output}".to_string(),
        },
        LLMPrompt {
            id: "coding".to_string(),
            name: "Coding".to_string(),
            prompt: "This is a dictated transcript describing code or a technical request.\n1. Fix spelling, grammar, and punctuation\n2. Correct spoken programming terms to their proper form (e.g. \"snake case\" → snake_case, \"dunder init\" → __init__, \"useeffect\" → useEffect)\n3. Format inline code, identifiers, and symbols with backticks where appropriate\n4. Remove filler words while preserving the exact technical meaning and order\n5. Do not add explanations or implement anything that was not asked for\n6. Keep the original language\n\nReturn only the cleaned text.\n\nTranscript:\n${output}".to_string(),
        },
    ]
}

fn default_whisper_gpu_device() -> i32 {
    -1 // auto
}
fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

/// Ensure every default post-process provider exists in `settings` and that key
/// maps stay in sync (migration). Returns true if anything changed.
pub fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    for provider in default_post_process_providers() {
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                if existing.supports_structured_output != provider.supports_structured_output {
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
            }
            None => {
                settings.post_process_providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.post_process_api_keys.contains_key(&provider.id) {
            settings
                .post_process_api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.post_process_models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .post_process_models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    // [GRAIN] Seed the built-in prompt-switcher prompts (General/Email/Coding) by
    // id without clobbering user-edited or user-added prompts, so existing installs
    // gain the defaults too.
    for prompt in default_post_process_prompts() {
        if !settings
            .post_process_prompts
            .iter()
            .any(|p| p.id == prompt.id)
        {
            settings.post_process_prompts.push(prompt);
            changed = true;
        }
    }
    // If nothing valid is selected, point at the first available prompt.
    let selected_valid = settings
        .post_process_selected_prompt_id
        .as_deref()
        .is_some_and(|id| settings.post_process_prompts.iter().any(|p| p.id == id));
    if !selected_valid {
        if let Some(first) = settings.post_process_prompts.first() {
            settings.post_process_selected_prompt_id = Some(first.id.clone());
            changed = true;
        }
    }

    // [GRAIN] Seed the prompt-switcher + agent bindings for installs that predate them.
    let defaults = get_default_settings();
    for id in ["prompt_next", "prompt_prev", "summon_agent", "transcribe_send_to_ai"] {
        if !settings.bindings.contains_key(id) {
            if let Some(binding) = defaults.bindings.get(id) {
                settings.bindings.insert(id.to_string(), binding.clone());
                changed = true;
            }
        }
    }

    // [GRAIN] Seed the default custom words for existing installs. The
    // `#[serde(default = "default_custom_words")]` only fires when the field is
    // absent from the JSON file; users whose settings were saved with
    // `custom_words: []` never hit it. Seed when empty so the 4 built-in words
    // (YouTube, iPhone, PayPal, Bluetooth) are visible out of the box.
    if settings.custom_words.is_empty() {
        settings.custom_words = default_custom_words();
        changed = true;
    }

    changed
}

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    // [GRAIN] dedicated real-time (rolling-window) transcribe shortcut.
    #[cfg(target_os = "macos")]
    let default_realtime_shortcut = "option+ctrl+space";
    #[cfg(not(target_os = "macos"))]
    let default_realtime_shortcut = "ctrl+alt+space";
    bindings.insert(
        "transcribe_realtime".to_string(),
        ShortcutBinding {
            id: "transcribe_realtime".to_string(),
            name: "Real-Time Transcribe".to_string(),
            description: "Rolling-window transcription that processes as you speak.".to_string(),
            default_binding: default_realtime_shortcut.to_string(),
            current_binding: default_realtime_shortcut.to_string(),
        },
    );

    // [GRAIN] Prompt switcher: cycle the active post-processing prompt; the new
    // title shows in the pill. Tap shortcuts (not push-to-talk). Defaults use the
    // arrow keys per the "control + arrows" idea — rebindable if the platform
    // key parser names them differently.
    bindings.insert(
        "prompt_next".to_string(),
        ShortcutBinding {
            id: "prompt_next".to_string(),
            name: "Next Prompt".to_string(),
            description: "Switch to the next post-processing prompt.".to_string(),
            default_binding: "ctrl+alt+right".to_string(),
            current_binding: "ctrl+alt+right".to_string(),
        },
    );
    bindings.insert(
        "prompt_prev".to_string(),
        ShortcutBinding {
            id: "prompt_prev".to_string(),
            name: "Previous Prompt".to_string(),
            description: "Switch to the previous post-processing prompt.".to_string(),
            default_binding: "ctrl+alt+left".to_string(),
            current_binding: "ctrl+alt+left".to_string(),
        },
    );

    // [GRAIN] Summon the Agent: a voice-first AI scratchpad in its own destroyable
    // window. Tap shortcut (fires on press). Captures the current selection, then
    // dictate/type an instruction; uses the configured post-process provider.
    #[cfg(target_os = "macos")]
    let default_agent_shortcut = "option+shift+a";
    #[cfg(not(target_os = "macos"))]
    let default_agent_shortcut = "ctrl+shift+a";
    bindings.insert(
        "summon_agent".to_string(),
        ShortcutBinding {
            id: "summon_agent".to_string(),
            name: "Summon Agent".to_string(),
            description: "Open the AI agent on your selected text — dictate or type an instruction."
                .to_string(),
            default_binding: default_agent_shortcut.to_string(),
            current_binding: default_agent_shortcut.to_string(),
        },
    );

    #[cfg(target_os = "macos")]
    let default_send_to_ai_shortcut = "option+shift+enter";
    #[cfg(not(target_os = "macos"))]
    let default_send_to_ai_shortcut = "ctrl+shift+enter";
    bindings.insert(
        "transcribe_send_to_ai".to_string(),
        ShortcutBinding {
            id: "transcribe_send_to_ai".to_string(),
            name: "Send to AI (End)".to_string(),
            description: "End an in-progress dictation or real-time session and send the transcript to AI. Only used when push-to-talk is off."
                .to_string(),
            default_binding: default_send_to_ai_shortcut.to_string(),
            current_binding: default_send_to_ai_shortcut.to_string(),
        },
    );

    AppSettings {
        bindings,
        push_to_talk: true,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        default_panel: DefaultPanel::default(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        selected_model: "".to_string(),
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        post_process_provider_id: default_post_process_provider_id(),
        post_process_providers: default_post_process_providers(),
        post_process_api_keys: default_post_process_api_keys(),
        post_process_smart_rotation: false,
        post_process_quota_reset_date: String::new(),
        stt_providers: default_stt_providers(),
        stt_smart_rotation: false,
        stt_api_keys: default_stt_api_keys(),
        stt_quota_reset_date: String::new(),
        post_process_models: default_post_process_models(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: Some("general".to_string()),
        mute_while_recording: false,
        append_trailing_space: false,
        app_language: default_app_language(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        whisper_accelerator: WhisperAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        whisper_gpu_device: default_whisper_gpu_device(),
        extra_recording_buffer_ms: 0,
        audio_conditioning: default_audio_conditioning(),
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        get_default_settings()
    }
}

impl AppSettings {
    pub fn active_post_process_provider(&self) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == self.post_process_provider_id)
    }

    pub fn post_process_provider(&self, provider_id: &str) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn post_process_provider_mut(
        &mut self,
        provider_id: &str,
    ) -> Option<&mut PostProcessProvider> {
        self.post_process_providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
    }
}
