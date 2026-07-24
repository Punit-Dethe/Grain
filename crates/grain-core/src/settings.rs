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

/// [GRAIN] A voice snippet: when the (normalized) trigger phrase appears in a
/// final transcript, it is replaced by the expansion text verbatim. Matching is
/// case/punctuation tolerant so rolling-window chunk artifacts ("Grain, GitHub
/// repo.") still expand.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct Snippet {
    pub id: String,
    pub trigger: String,
    pub replacement: String,
    #[serde(default = "default_snippet_enabled")]
    pub enabled: bool,
}

fn default_snippet_enabled() -> bool {
    true
}

/// [GRAIN] One thing a voice ACTION opens. `App` is launched with the OS default
/// handler (an executable, a document, or a folder — cross-platform via the
/// opener); `Url` is opened in the user's default browser. Kept as a two-variant
/// enum so the UI stays a single App/Website toggle (childishly simple).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ActionTarget {
    App(String),
    Url(String),
}

/// [GRAIN] A voice ACTION (Experimentations tab): when the (normalized) trigger
/// phrase is spoken, every `target` is opened and the trigger is stripped from
/// the pasted text. One action can open several apps + sites at once — a
/// "workflow" (e.g. "start coding" opens the editor, terminal, and two docs).
/// Matching reuses the snippet matcher, so it is case/punctuation tolerant and
/// survives rolling-window chunk artifacts. No AI, no network — a local launch.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct VoiceAction {
    pub id: String,
    pub trigger: String,
    pub targets: Vec<ActionTarget>,
    #[serde(default = "default_snippet_enabled")]
    pub enabled: bool,
}

/// [GRAIN] How an [`AppMode`] is bound to the active target. A mode fires when the
/// foreground app (or, in a browser, the current site) matches. `Process` matches
/// the executable stem case-insensitively (e.g. `"Code"`, `"slack"`); `UrlHost`
/// matches the browser address-bar host by suffix (`"mail.google.com"` also
/// matches `"…mail.google.com"`), so users type a bare host, not a regex.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AppMatch {
    Process(String),
    UrlHost(String),
}

/// [GRAIN] A user-defined "mode": a specific post-processing prompt (HARD
/// formatting) applied ONLY when its `matcher` hits the active app/site. This is
/// the opt-in, per-target layer that rides on top of the always-on base prompt +
/// automatic soft context. The `prompt` is inline (self-contained) rather than a
/// reference into `post_process_prompts`, so a mode can be shared/exported whole.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppMode {
    pub id: String,
    pub name: String,
    #[serde(rename = "match")]
    pub matcher: AppMatch,
    pub prompt: String,
    #[serde(default = "default_snippet_enabled")]
    pub enabled: bool,
}

/// [GRAIN] Agent auto-copy policy: which assistant replies are copied to the
/// clipboard automatically as they arrive. `First` (default) mirrors the
/// original behavior — only the first reply of a session is auto-copied.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentAutocopy {
    Off,
    First,
    All,
}

impl Default for AgentAutocopy {
    fn default() -> Self {
        AgentAutocopy::First
    }
}

/// [GRAIN] Agent context awareness: what (if anything) is read from the focused
/// field at summon and handed to the LLM as background. `Unique` reuses the
/// nearby-terms extractor (high-signal identifiers/names only); `Full` sends the
/// capped raw field text. OFF by default — reading field content is opt-in.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentContextMode {
    Off,
    Unique,
    Full,
}

/// [GRAIN] Where the Agent reply surface appears. `Side` (default) is the
/// original bottom-right card that grows into a right-side conversation.
/// `Center` is the sleek center-top panel that hugs its content and grows
/// downward as the conversation lengthens (up to a max height, then scrolls).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentPanelPosition {
    Side,
    Center,
}

impl Default for AgentPanelPosition {
    fn default() -> Self {
        AgentPanelPosition::Side
    }
}

impl Default for AgentContextMode {
    fn default() -> Self {
        AgentContextMode::Off
    }
}

/// [GRAIN] A learned-word candidate for auto-add-to-dictionary. When the user
/// repeatedly re-spells the same pasted word, `count` climbs; at the threshold it
/// is suggested (pill), and on accept it moves into `custom_words`. Persisted so
/// the count survives across sessions (the user rarely corrects the same term
/// twice in one sitting).
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct DictCandidate {
    /// The corrected spelling as the user typed it (display + what gets added).
    pub word: String,
    /// How many distinct paste-sessions this correction has been observed in.
    pub count: u32,
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

// OverlayPosition moved to grain-sdk (it crosses the wire in
// DaemonEvent::OverlayConfig); re-exported here so `settings::OverlayPosition`
// paths — and the generated bindings — are unchanged.
pub use grain_sdk::OverlayPosition;

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

/// Compute preference for transcribe-cpp (whisper-family GGUF) model loads.
/// Renamed from `WhisperAcceleratorSetting` when the batch path moved from
/// transcribe-rs whisper.cpp onto transcribe-cpp (upstream parity); the stored
/// values (`auto`/`cpu`/`gpu`) are unchanged, so old JSON deserializes via the
/// field-level `alias` on [`AppSettings::transcribe_accelerator`].
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeAcceleratorSetting {
    Auto,
    Cpu,
    Gpu,
}

impl Default for TranscribeAcceleratorSetting {
    fn default() -> Self {
        TranscribeAcceleratorSetting::Auto
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

/// The container-level `serde(default)` (backed by the `Default` impl below)
/// guarantees every field — including ones added in the future — falls back to
/// its `get_default_settings()` value when missing from a stored settings
/// object, so a partial store can never fail the whole load (upstream #1619).
/// Field-level defaults below take precedence where present.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
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
    /// [GRAIN] Native ASR model id (separate registry from `selected_model`).
    /// Empty = none selected. Never overload `selected_model`: Batch/Rolling and
    /// Native ASR have different model topologies and lifecycles.
    #[serde(default)]
    pub selected_asr_model: String,
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
    /// [GRAIN] Voice snippets (Experimentations tab): trigger phrase → expansion.
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    /// [GRAIN] Voice actions (Experimentations tab): trigger phrase → open apps/sites.
    #[serde(default)]
    pub actions: Vec<VoiceAction>,
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
    #[serde(default = "default_paste_delay_after_ms")]
    pub paste_delay_after_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default, alias = "whisper_accelerator")]
    pub transcribe_accelerator: TranscribeAcceleratorSetting,
    /// transcribe-cpp compute-device *registry index* for explicit GPU picks
    /// (`-1` = auto). NOTE: deliberately NOT aliased to the old
    /// `whisper_gpu_device` — that was a transcribe-rs UI ordinal with different
    /// semantics, so legacy values reset to auto instead of pointing at a
    /// possibly different device.
    #[serde(default = "default_transcribe_gpu_device")]
    pub transcribe_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    /// [GRAIN] Voice conditioning before VAD + STT: 85 Hz high-pass (de-rumble)
    ///   + boost-only noise-gated AGC for quiet/laptop mics. On by default; helps
    ///     accuracy on low-volume input without touching already-loud audio.
    #[serde(default = "default_audio_conditioning")]
    pub audio_conditioning: bool,
    /// [GRAIN] Rolling live preview: show growing text in the Studio Window while
    /// dictating in the rolling (real-time) mode. OFF by default and OFF is
    /// truly zero-cost — the rolling worker takes exactly the same path it
    /// always did (no events, no extra decode). ON adds a committed-text preview
    /// after each chunk merge PLUS an efficient inter-chunk tail decode
    /// (LocalAgreement-2) that costs extra compute, so it is strictly opt-in.
    #[serde(default = "default_rolling_live_preview")]
    pub rolling_live_preview: bool,
    /// [GRAIN] Context awareness (post-processing only): when on, the backend
    /// detects the foreground app/site right before LLM post-processing and layers
    /// an automatic SOFT context line (tone/vocab, never restructuring) plus any
    /// matching user [`AppMode`] (HARD formatting) on top of the selected base
    /// prompt. OFF by default — zero behavior change until opted in, and it only
    /// affects installs that also run post-processing.
    #[serde(default)]
    pub context_awareness_enabled: bool,
    /// [GRAIN] Extension platform (SPEC §10.1): the Snippets built-in extension's
    /// switch. OFF by default for NEW installs; the one-time import in
    /// `load_settings` turns it on for existing users who already have snippets
    /// (the upgrade rule — a working feature must not vanish on update).
    #[serde(default)]
    pub snippets_enabled: bool,
    /// [GRAIN] Extension platform (SPEC §10.1): the Agent built-in extension's
    /// switch — the on/off the Agent never had. Gates summoning and the
    /// summon-agent binding. OFF by default for NEW installs; the one-time
    /// import turns it on for existing users (the Agent was previously always
    /// available).
    #[serde(default)]
    pub agent_enabled: bool,
    /// [GRAIN] Extension platform: the Voice Actions built-in extension's switch.
    /// Unlike snippets/agent it defaults ON — voice actions never had an
    /// off-switch and an empty action list already costs nothing (the matcher
    /// early-returns), so ON preserves the exact prior behavior for every user,
    /// new and existing, without a migration. The toggle simply adds the
    /// off-switch the feature never had.
    #[serde(default = "default_true")]
    pub actions_enabled: bool,
    /// [GRAIN] One-time marker for the extension-platform settings import above
    /// (SPEC §10.1 upgrade rule). False in files written before the platform;
    /// `load_settings` performs the import exactly once and sets it.
    #[serde(default)]
    pub extensions_imported_v1: bool,
    /// [GRAIN] Explicit human-controlled authoring mode (Phase 3.5). This is
    /// separate from diagnostic `debug_mode`: only this switch allows native
    /// folder selection and load-unpacked projects. OFF by default.
    #[serde(default)]
    pub extension_developer_mode: bool,
    /// [GRAIN] User-defined per-app / per-site modes (HARD formatting). Empty by
    /// default; only consulted when `context_awareness_enabled` is true.
    #[serde(default)]
    pub app_modes: Vec<AppMode>,
    /// [GRAIN] Silent nearby-term hints: when on (and context awareness is on),
    /// read UNIQUE non-dictionary tokens (proper nouns, code identifiers, library
    /// names) from the focused field via UI Automation and pass them to the LLM as
    /// an *additive, low-authority* bias — never the raw text, never persisted,
    /// never surfaced in the UI. OFF by default because it reads the focused
    /// field's content; password fields are always skipped.
    #[serde(default)]
    pub context_nearby_terms: bool,
    /// [GRAIN] Auto-add to dictionary: when on, Grain briefly watches the field it
    /// just pasted into (~10s) and, if you re-spell one of the pasted words the
    /// same way across a couple of pastes, offers to add that spelling to your
    /// dictionary (confirm by clicking the pill). OFF by default and **truly
    /// zero-overhead when off** — no watcher is ever spawned. Only proper-noun /
    /// identifier-shaped corrections are considered; common words are ignored.
    #[serde(default)]
    pub auto_dictionary_enabled: bool,
    /// [GRAIN] Persisted learning counters for auto-add-to-dictionary (see
    /// [`DictCandidate`]). Not user-facing; managed by the watcher.
    #[serde(default)]
    pub dictionary_candidates: Vec<DictCandidate>,
    /// [GRAIN] Which Agent replies are auto-copied to the clipboard (off / first
    /// reply only / every reply). Default `first` — the original behavior.
    #[serde(default)]
    pub agent_autocopy: AgentAutocopy,
    /// [GRAIN] Quick Agent: when on, submitting an instruction from the palette
    /// runs the AI headlessly and pastes the reply straight at the cursor instead
    /// of opening the reply panel. The pill then briefly offers "ask follow-up".
    #[serde(default)]
    pub agent_quick_enabled: bool,
    /// [GRAIN] Agent context awareness: read the focused field at summon and pass
    /// it to the AI as background (`unique` = high-signal terms only, `full` =
    /// capped raw text). OFF by default.
    #[serde(default)]
    pub agent_context_mode: AgentContextMode,
    /// [GRAIN] "Scrap that" voice reset: when on, saying the phrase "scrap that"
    /// mid-dictation discards everything spoken before it — the transcript starts
    /// fresh from that point. Reuses the snippet matcher (no new engine), so OFF
    /// is truly zero-overhead. In live-streaming modes the expanded Studio pill
    /// resets and collapses back to the compact capsule until the next word.
    #[serde(default)]
    pub scrap_that_enabled: bool,
    /// [GRAIN] Native agent input: when on (default), typing a printable key
    /// while the input is listening immediately switches it to the expanded
    /// typing card. When off, the input stays in voice mode and typing is
    /// ignored until the user expands it explicitly (Tab / click).
    #[serde(default = "default_true")]
    pub agent_input_type_to_expand: bool,
    /// [GRAIN] Where the Agent reply surface appears: the original bottom-right
    /// `side` card, or the sleek center-top `center` panel that hugs its content
    /// and grows downward. Default `side`; the center panel is in development.
    #[serde(default)]
    pub agent_panel_position: AgentPanelPosition,
    /// [GRAIN] Grain Space master gate. OFF by default and OFF is truly
    /// zero-overhead: no shortcuts register, no directories are created, no
    /// DB opens, no models load. Disabling never deletes on-disk data.
    #[serde(default)]
    pub grain_space_enabled: bool,
    /// [GRAIN] Grain Space semantic search. OFF = fuzzy/FTS matching only and
    /// the Candle embedding model must NEVER load into RAM. Turning it ON is
    /// what triggers the opt-in BGE-small model download (the model is not
    /// shipped with the app).
    #[serde(default)]
    pub grain_space_semantic: bool,
    /// [GRAIN] Load the semantic embedding model in half precision (f16) instead
    /// of f32 — roughly half the resident RAM, near-identical results, CPU speed
    /// about the same. Opt-in side-by-side option (the download is the same file).
    #[serde(default)]
    pub grain_space_embed_f16: bool,
    /// [GRAIN] When ON (default), reminders extracted from a captured note are
    /// armed automatically; when OFF the note pane shows a manual "arm" button.
    #[serde(default = "default_true")]
    pub grain_space_auto_reminders: bool,
    /// [GRAIN] Half-life (days) for time-decayed semantic ranking:
    /// `S_final = S_semantic * exp(-ln2/half_life * age_days)`. Pinned notes
    /// rank as if brand new (age 0).
    #[serde(default = "default_grain_space_decay_half_life_days")]
    pub grain_space_decay_half_life_days: u32,
    /// [GRAIN] Which store backs Grain Space (OBSIDIAN-PLAN.md). A hard switch:
    /// flipping it swaps the corpus every surface sees; nothing is migrated.
    #[serde(default)]
    pub grain_space_backend: GrainSpaceBackend,
    /// [GRAIN] Absolute path of the Obsidian vault (a plain folder of .md
    /// files). Empty = not configured; the vault backend refuses to run.
    #[serde(default)]
    pub grain_space_vault_path: String,
    /// [GRAIN] Subfolder inside the vault where Grain writes its captures.
    /// Grain only ever creates/edits files under this folder; the rest of the
    /// vault is read-only (searchable, never written).
    #[serde(default = "default_grain_space_vault_folder")]
    pub grain_space_vault_folder: String,
    /// [GRAIN] Auto-categorization (AUTO-CATEGORIZATION-PLAN.md). When ON, a
    /// captured note is routed into the best-fitting existing Grain folder via
    /// the structuring call that already runs — no extra model, no idle work.
    /// Off by default; when off, no categorization code path runs.
    #[serde(default)]
    pub grain_space_auto_categorize: bool,
}

/// [GRAIN] Grain Space storage backend (OBSIDIAN-PLAN.md §1).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "snake_case")]
pub enum GrainSpaceBackend {
    /// Flat JSON notes under `{app_data}/grain_space/notes/` (the original store).
    #[default]
    Grain,
    /// Markdown + YAML frontmatter files in a user-chosen Obsidian vault.
    Obsidian,
}

fn default_true() -> bool {
    true
}

fn default_grain_space_decay_half_life_days() -> u32 {
    30
}

fn default_grain_space_vault_folder() -> String {
    "Grain".to_string()
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
/// Rolling live preview defaults OFF — it trades compute for a live caption, so
/// users opt in explicitly (see `rolling_live_preview`).
fn default_rolling_live_preview() -> bool {
    false
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

fn default_paste_delay_after_ms() -> u64 {
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
    // [GRAIN] AI post-processing is exposed ON by default so the feature is
    // discoverable out of the box. This only makes the post-process shortcut +
    // controls available; plain dictation never routes to an LLM on its own, so
    // a fresh install with no API key still transcribes normally.
    true
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
        p(
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            false,
            Some("/models"),
            true,
        ),
        p(
            "zai",
            "Z.AI",
            "https://api.z.ai/api/paas/v4",
            false,
            Some("/models"),
            true,
        ),
        p(
            "openrouter",
            "OpenRouter",
            "https://openrouter.ai/api/v1",
            false,
            Some("/models"),
            true,
        ),
        p(
            "anthropic",
            "Anthropic",
            "https://api.anthropic.com/v1",
            false,
            Some("/models"),
            false,
        ),
        p(
            "groq",
            "Groq",
            "https://api.groq.com/openai/v1",
            false,
            Some("/models"),
            false,
        ),
        p(
            "cerebras",
            "Cerebras",
            "https://api.cerebras.ai/v1",
            false,
            Some("/models"),
            true,
        ),
        p(
            "gemini",
            "Gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai",
            false,
            Some("/models"),
            true,
        ),
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
        map.insert(
            provider.id.clone(),
            default_model_for_provider(&provider.id),
        );
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

fn default_transcribe_gpu_device() -> i32 {
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
    for id in [
        "prompt_next",
        "prompt_prev",
        "summon_agent",
        "agent_followup",
        "transcribe_send_to_ai",
        "transcribe_native_asr",
        "grain_space_quick_add",
        "grain_space_capture",
        "grain_space_open",
        "grain_space_recall",
    ] {
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
            name: "Standard".to_string(),
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
            name: "Flow".to_string(),
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

    // [GRAIN] Native ASR: streaming dictation with live partial/committed text in
    // the Studio Window overlay. Push-to-talk like the other capture modes — the
    // engine loads/unloads automatically around the shortcut, never resident
    // otherwise. Default mirrors the "+shift" relationship between
    // transcribe_realtime and transcribe_with_post_process.
    #[cfg(target_os = "macos")]
    let default_native_asr_shortcut = "option+ctrl+shift+space";
    #[cfg(not(target_os = "macos"))]
    let default_native_asr_shortcut = "ctrl+alt+shift+space";
    bindings.insert(
        "transcribe_native_asr".to_string(),
        ShortcutBinding {
            id: "transcribe_native_asr".to_string(),
            name: "Live".to_string(),
            description: "Native real-time dictation with live streaming text.".to_string(),
            default_binding: default_native_asr_shortcut.to_string(),
            current_binding: default_native_asr_shortcut.to_string(),
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
            description:
                "Open the AI agent on your selected text — dictate or type an instruction."
                    .to_string(),
            default_binding: default_agent_shortcut.to_string(),
            current_binding: default_agent_shortcut.to_string(),
        },
    );

    // [GRAIN] Ask a follow-up on the Agent's latest reply. Only registered as a
    // GLOBAL shortcut while an Agent surface (reply card / pill offer) is live —
    // and in that window it OVERRIDES any other Grain binding using the same keys.
    #[cfg(target_os = "macos")]
    let default_agent_followup_shortcut = "option+shift+f";
    #[cfg(not(target_os = "macos"))]
    let default_agent_followup_shortcut = "ctrl+alt+f";
    bindings.insert(
        "agent_followup".to_string(),
        ShortcutBinding {
            id: "agent_followup".to_string(),
            name: "Agent Follow-up".to_string(),
            description:
                "Ask a follow-up on the Agent's latest reply. Active only while the Agent is open."
                    .to_string(),
            default_binding: default_agent_followup_shortcut.to_string(),
            current_binding: default_agent_followup_shortcut.to_string(),
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

    // [GRAIN] Grain Space bindings. Only registered while `grain_space_enabled`
    // is on (init + toggle both gate on it) — the ids existing in the map costs
    // nothing. Quick Add is ctrl+shift+c per the feature spec (rebindable — it
    // collides with terminal-copy on Linux and DevTools inspect in browsers).
    #[cfg(target_os = "macos")]
    let default_quick_add_shortcut = "cmd+shift+c";
    #[cfg(not(target_os = "macos"))]
    let default_quick_add_shortcut = "ctrl+shift+c";
    bindings.insert(
        "grain_space_quick_add".to_string(),
        ShortcutBinding {
            id: "grain_space_quick_add".to_string(),
            name: "Quick Add to Space".to_string(),
            description: "Silently save the highlighted text as a Grain Space note.".to_string(),
            default_binding: default_quick_add_shortcut.to_string(),
            current_binding: default_quick_add_shortcut.to_string(),
        },
    );

    #[cfg(target_os = "macos")]
    let default_space_capture_shortcut = "option+shift+n";
    #[cfg(not(target_os = "macos"))]
    let default_space_capture_shortcut = "ctrl+alt+n";
    bindings.insert(
        "grain_space_capture".to_string(),
        ShortcutBinding {
            id: "grain_space_capture".to_string(),
            name: "Create Note".to_string(),
            description: "Open the Grain pill to speak or type a note; any selected text becomes the note (AI title/summary when available)."
                .to_string(),
            default_binding: default_space_capture_shortcut.to_string(),
            current_binding: default_space_capture_shortcut.to_string(),
        },
    );

    // Tap toggle for the Grain Space overlay browser (Phase 3): create the
    // window if absent, destroy it if open.
    #[cfg(target_os = "macos")]
    let default_space_open_shortcut = "option+shift+g";
    #[cfg(not(target_os = "macos"))]
    let default_space_open_shortcut = "ctrl+shift+g";
    bindings.insert(
        "grain_space_open".to_string(),
        ShortcutBinding {
            id: "grain_space_open".to_string(),
            name: "Open Space".to_string(),
            description: "Open or close the Grain Space notes window.".to_string(),
            default_binding: default_space_open_shortcut.to_string(),
            current_binding: default_space_open_shortcut.to_string(),
        },
    );

    // [GRAIN] Grain Recall — conversational memory retrieval. Its OWN shortcut,
    // distinct from summon_agent: pressing this summons the Agent surfaces in
    // memory mode (ask your notes, get an answer). The mode is fixed by which
    // key fired — the AI never decides whether a request is assist vs recall.
    #[cfg(target_os = "macos")]
    let default_space_recall_shortcut = "option+shift+m";
    #[cfg(not(target_os = "macos"))]
    let default_space_recall_shortcut = "ctrl+shift+m";
    bindings.insert(
        "grain_space_recall".to_string(),
        ShortcutBinding {
            id: "grain_space_recall".to_string(),
            name: "Recall Memory".to_string(),
            description: "Ask Grain about your saved notes and get a spoken-style answer."
                .to_string(),
            default_binding: default_space_recall_shortcut.to_string(),
            current_binding: default_space_recall_shortcut.to_string(),
        },
    );

    AppSettings {
        bindings,
        // [GRAIN] Push-to-talk defaults OFF — a fresh install uses toggle-style
        // capture (press once to start, again to stop) rather than hold-to-talk.
        push_to_talk: false,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        default_panel: DefaultPanel::default(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        selected_model: "".to_string(),
        selected_asr_model: String::new(),
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
        snippets: Vec::new(),
        actions: Vec::new(),
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
        paste_delay_after_ms: default_paste_delay_after_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        transcribe_accelerator: TranscribeAcceleratorSetting::default(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        extra_recording_buffer_ms: 0,
        audio_conditioning: default_audio_conditioning(),
        rolling_live_preview: default_rolling_live_preview(),
        context_awareness_enabled: false,
        // [GRAIN] Built-in extensions default OFF for new installs (SPEC §10.1);
        // the upgrade import in context.rs turns them on for existing users.
        snippets_enabled: false,
        agent_enabled: false,
        // Voice actions default ON (see the field doc): preserves prior always-on
        // behavior; an empty action list is already a no-op.
        actions_enabled: true,
        extensions_imported_v1: false,
        extension_developer_mode: false,
        app_modes: Vec::new(),
        context_nearby_terms: false,
        auto_dictionary_enabled: false,
        dictionary_candidates: Vec::new(),
        agent_autocopy: AgentAutocopy::default(),
        agent_quick_enabled: false,
        agent_context_mode: AgentContextMode::default(),
        scrap_that_enabled: false,
        agent_input_type_to_expand: true,
        agent_panel_position: AgentPanelPosition::default(),
        grain_space_enabled: false,
        grain_space_semantic: false,
        grain_space_embed_f16: false,
        grain_space_auto_reminders: true,
        grain_space_decay_half_life_days: default_grain_space_decay_half_life_days(),
        grain_space_backend: GrainSpaceBackend::default(),
        grain_space_vault_path: String::new(),
        grain_space_vault_folder: default_grain_space_vault_folder(),
        grain_space_auto_categorize: false,
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
