//! [GRAIN] Voice actions: a spoken trigger opens one or more apps / websites.
//!
//! This is the zero-overhead sibling of voice snippets. Where a snippet swaps a
//! trigger phrase for verbatim text, an *action* fires a local side effect —
//! launching apps/files/folders (OS default handler) and opening URLs (default
//! browser) — then strips the trigger from whatever gets pasted. One action can
//! open several targets at once, so "start coding" can bring up the editor, a
//! terminal, and two docs in a single breath.
//!
//! No AI, no network, no background engine: matching is a single linear scan
//! (reusing the snippet matcher for case/punctuation tolerance) that runs ONCE
//! per dictation on the finalized transcript, and only when the user has actions
//! configured. When the actions list is empty the common path is a single
//! `is_empty()` check and the transcript is returned untouched.

use crate::audio_toolkit::snippets::{match_at, normalize, tokenize};
use crate::settings::{self, ActionTarget};
use log::{info, warn};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// A trigger pre-flattened for matching, tagged with its index into the settings
/// `actions` list so a hit can look the action back up to fire it.
struct CompiledAction {
    flat: String,
    idx: usize,
}

/// Turn a user-typed website value into something the browser accepts. A bare
/// host ("x.com") gets an `https://` scheme; anything already carrying a scheme
/// (or a `mailto:` / `file:` etc.) is passed through untouched.
fn normalize_url(value: &str) -> String {
    let v = value.trim();
    if v.contains("://") || v.starts_with("mailto:") {
        v.to_string()
    } else {
        format!("https://{v}")
    }
}

/// Open every target of an action. Best-effort and fire-and-forget: a launch
/// failure is logged, never surfaced into the paste pipeline (the user still
/// gets their transcript). Shared by the live intercept and the Settings
/// "Test"/"Run" button so both behave identically.
pub fn open_targets(app: &AppHandle, targets: &[ActionTarget]) {
    for target in targets {
        match target {
            ActionTarget::Url(url) => {
                let url = normalize_url(url);
                if url.is_empty() {
                    continue;
                }
                if let Err(e) = app.opener().open_url(url.clone(), None::<String>) {
                    warn!("[GRAIN] action: failed to open url '{url}': {e}");
                }
            }
            ActionTarget::App(path) => {
                let path = path.trim();
                if path.is_empty() {
                    continue;
                }
                if let Err(e) = app.opener().open_path(path.to_string(), None::<String>) {
                    warn!("[GRAIN] action: failed to open app/path '{path}': {e}");
                }
            }
        }
    }
}

/// Fire any voice actions whose trigger appears in `text`, then return the text
/// with those triggers removed (whitespace re-collapsed). When no action matches
/// — the overwhelmingly common case — the original text is returned unchanged
/// and nothing is launched.
///
/// Runs on the finalized transcript BEFORE post-processing, so a pure command
/// ("start coding") is consumed here and never costs an LLM call or a paste.
pub fn intercept(app: &AppHandle, text: &str) -> String {
    let settings = settings::get_settings(app);
    if settings.actions.is_empty() {
        return text.to_string();
    }

    // Compile enabled actions that actually have a trigger and a target. Sorted
    // longest-first so a more specific trigger wins over a shorter prefix.
    let mut compiled: Vec<CompiledAction> = settings
        .actions
        .iter()
        .enumerate()
        .filter(|(_, a)| a.enabled && !a.targets.is_empty())
        .filter_map(|(idx, a)| {
            let flat = normalize(&a.trigger);
            if flat.is_empty() {
                None
            } else {
                Some(CompiledAction { flat, idx })
            }
        })
        .collect();
    if compiled.is_empty() {
        return text.to_string();
    }
    compiled.sort_by(|a, b| b.flat.len().cmp(&a.flat.len()));

    let tokens = tokenize(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize; // byte position in `text` copied so far
    let mut i = 0usize;
    // Preserve first-seen order, dedup so an action repeated in one utterance
    // ("code, code") fires once.
    let mut fired: Vec<usize> = Vec::new();
    while i < tokens.len() {
        let hit = compiled
            .iter()
            .find_map(|c| match_at(&tokens, i, &c.flat).map(|end| (c.idx, end)));
        match hit {
            Some((idx, end)) => {
                let first = &tokens[i];
                out.push_str(&text[cursor..first.start]);
                // Drop the whole matched span — the command word carries no text.
                cursor = tokens[end - 1].end;
                i = end;
                if !fired.contains(&idx) {
                    fired.push(idx);
                }
            }
            None => i += 1,
        }
    }

    if fired.is_empty() {
        return text.to_string();
    }
    out.push_str(&text[cursor..]);

    for idx in &fired {
        let action = &settings.actions[*idx];
        info!(
            "[GRAIN] action fired: '{}' ({} target(s))",
            action.trigger,
            action.targets.len()
        );
        open_targets(app, &action.targets);
    }

    // A trigger removed from mid-sentence leaves a double space; collapse it.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// [GRAIN] Open a set of targets on demand — powers the Settings "Test" button so
/// a user can confirm an action before saving it.
#[tauri::command]
#[specta::specta]
pub fn run_action(app: AppHandle, targets: Vec<ActionTarget>) -> Result<(), String> {
    open_targets(&app, &targets);
    Ok(())
}

/// [GRAIN] Native file picker for choosing an application/executable to launch,
/// so users never have to type a path. Returns the absolute path, or `None` if
/// the dialog was cancelled. Runs the modal off the async executor.
#[tauri::command]
#[specta::specta]
pub async fn pick_action_app(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_file())
            .await
            .map_err(|e| e.to_string())?;

    Ok(picked
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().to_string()))
}
