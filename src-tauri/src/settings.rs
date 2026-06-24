//! [GRAIN] Settings glue for the Tauri shell.
//!
//! The schema + the production store now live in `grain-core` (Tauri-free). This
//! module re-exports those types so the ~170 `crate::settings::*` call sites
//! compile unchanged, and bridges the `&AppHandle`-keyed accessors to the owned
//! [`grain_core::AppContext`] held in Tauri managed state. `tauri-plugin-store`
//! is no longer used for settings — persistence is owned JSON (+ a separate
//! secrets file) inside `AppContext`.

use std::collections::HashMap;
use std::sync::Arc;

use grain_core::AppContext;
use tauri::{AppHandle, Manager};

// The full schema, enums, defaults, and helpers — single source of truth.
pub use grain_core::settings::*;

/// Convert Grain's `LogLevel` to the tauri-plugin-log level. A free function
/// rather than a `From` impl because both types are foreign here (orphan rule).
pub fn to_tauri_log_level(level: LogLevel) -> tauri_plugin_log::LogLevel {
    match level {
        LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
        LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
        LogLevel::Info => tauri_plugin_log::LogLevel::Info,
        LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
        LogLevel::Error => tauri_plugin_log::LogLevel::Error,
    }
}

/// The headless context owned as Tauri managed state. Panics only if called
/// before [`initialize_core_logic`](crate::initialize_core_logic) stages it —
/// which the setup flow guarantees before any settings access.
fn context(app: &AppHandle) -> Arc<AppContext> {
    app.state::<Arc<AppContext>>().inner().clone()
}

/// Read the current settings (headless: clones from the owned `RwLock`).
pub fn get_settings(app: &AppHandle) -> AppSettings {
    context(app).settings()
}

/// Persist a full settings replacement.
pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    if let Err(e) = context(app).replace_settings(settings) {
        log::error!("Failed to persist settings: {e:#}");
    }
}

/// Settings are loaded (and migrated) when `AppContext` is constructed, so this
/// is now just a read. Name kept for call-site compatibility.
pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    context(app).settings()
}

// -- field accessors (call-site compatibility with Handy's settings.rs) -------

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    get_settings(app).bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    get_bindings(app).get(id).unwrap().clone()
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    get_settings(app).history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    get_settings(app).recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }

    #[test]
    fn debug_output_redacts_api_keys() {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "sk-proj-secret-key-12345".to_string());
        let debug_output = format!("{:?}", settings);
        assert!(!debug_output.contains("sk-proj-secret-key-12345"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn secret_map_debug_redacts_values() {
        let map = SecretMap(HashMap::from([("key".into(), "secret".into())]));
        let out = format!("{:?}", map);
        assert!(!out.contains("secret"));
        assert!(out.contains("[REDACTED]"));
    }
}
