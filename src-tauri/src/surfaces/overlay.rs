//! [GRAIN] Extension-facing **overlay** surfaces (SPEC §1.2, §7.1).
//!
//! An overlay is a transient HUD: created per invocation, destroyed on dismiss.
//! It shares a workspace's *realm* — Grain's wrapper page, a sandboxed iframe on
//! an opaque origin, its own revocable token (see [`super::extension`]) — but
//! **not** its lifecycle. There is no sleeping, no wake, no LRU: an overlay is
//! shown, and then it goes away.
//!
//! "Goes away" is enforced by the host, not trusted to the extension (SPEC §1.2:
//! "an overlay cannot linger"). Two budgets do it:
//! - **size** is clamped to a fraction of nothing-in-particular — a HUD, never a
//!   window that could impersonate one;
//! - **lifetime** is a hard cap. Every overlay auto-dismisses on a timer, and
//!   losing focus dismisses it early. An extension that asks for no timeout, or
//!   a dishonest one, still gets a HUD that removes itself.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::extension;

const SURFACE_URL: &str = "extension-surface.html";

/// Default HUD size when the pack asks for none.
const DEFAULT_SIZE: (f64, f64) = (360.0, 120.0);
/// A HUD may not exceed this — big enough to say something, too small to pass
/// for an application window the user might trust.
const MAX_SIZE: (f64, f64) = (720.0, 480.0);
const MIN_SIZE: (f64, f64) = (120.0, 48.0);

/// Lifetime budget (SPEC §1.2). The default when none is asked, and the ceiling
/// regardless of what is: no overlay outlives this.
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(4000);
const MAX_TIMEOUT: Duration = Duration::from_millis(15000);

/// Distinguishes successive overlays for the same extension. Two purposes:
/// the auto-dismiss timer of a replaced overlay must not destroy the one that
/// replaced it, and each invocation gets a UNIQUE window label — Tauri labels
/// must be unique, and reusing one across a `show` that replaces a still-closing
/// window would race the async destroy against the rebuild.
static EPOCH: AtomicU64 = AtomicU64::new(0);

fn base_label(ext_id: &str) -> String {
    format!("ext-overlay-{}", ext_id.replace(['.', ':'], "-"))
}

fn label_for(ext_id: &str, epoch: u64) -> String {
    format!("{}-{epoch}", base_label(ext_id))
}

/// The overlay is transient, so its events are unused today; still distinct from
/// the workspace's, so the shared wrapper page listens for the right names.
fn payload_event(ext_id: &str) -> String {
    format!("ext-overlay://{ext_id}/payload")
}

fn clamp_size(requested: Option<[u32; 2]>) -> (f64, f64) {
    let r = requested.unwrap_or([DEFAULT_SIZE.0 as u32, DEFAULT_SIZE.1 as u32]);
    (
        (r[0] as f64).clamp(MIN_SIZE.0, MAX_SIZE.0),
        (r[1] as f64).clamp(MIN_SIZE.1, MAX_SIZE.1),
    )
}

fn clamp_timeout(requested_ms: Option<u32>) -> Duration {
    match requested_ms {
        Some(ms) => Duration::from_millis(ms as u64).min(MAX_TIMEOUT),
        None => DEFAULT_TIMEOUT,
    }
}

/// Show an extension's overlay. Per-invocation: any overlay already up for this
/// extension is destroyed first, so `show` twice does not stack HUDs.
pub fn show(
    app: &AppHandle,
    ext_id: &str,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let rec = reg
        .record(ext_id)
        .ok_or_else(|| format!("'{ext_id}' is not installed"))?;
    if !rec.enabled {
        return Err(format!("'{ext_id}' is disabled"));
    }
    // Capability check here as well as at the host-call boundary, so a future
    // caller that forgets cannot put a window on an ungranted capability.
    if !rec.granted.iter().any(|c| c == "surface:overlay") {
        return Err("the 'surface:overlay' capability was not granted".into());
    }
    let pack =
        crate::extension_host::load_manifest(app, ext_id).ok_or("could not read the pack")?;
    let decl = pack
        .manifest
        .surfaces
        .overlay
        .clone()
        .ok_or_else(|| format!("'{ext_id}' declares no overlay"))?;
    if decl.ui_source.trim().is_empty() {
        return Err("an overlay surface requires ui_source".into());
    }
    let ui_source = decl.ui_source.clone();

    let size = clamp_size(decl.size);
    let timeout = clamp_timeout(decl.timeout_ms);
    let epoch = EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    let label = label_for(ext_id, epoch);

    // Replace any live overlay for this extension FIRST (its own, older label),
    // then record this invocation as the current one. Unique labels mean the
    // old window can finish closing while the new one is already building.
    dismiss(app, ext_id);
    set_current(ext_id, &label);

    extension::stage(
        app,
        ext_id,
        &label,
        &ui_source,
        // No sleep/revive for an overlay; give the wrapper harmless names it
        // will simply never receive.
        &format!("ext-overlay://{ext_id}/sleep"),
        &format!("ext-overlay://{ext_id}/revive"),
        &payload_event(ext_id),
    )?;
    // The payload rides in the shared surface stash; the wrapper delivers it to
    // the iframe on mount, exactly as a workspace does.
    if let Some(p) = payload {
        extension::stash_payload(&label, p);
    }

    let app_build = app.clone();
    let label_build = label.clone();
    app.run_on_main_thread(move || {
        // The label is unique per invocation, so a collision here would be a
        // genuine bug rather than a replaced overlay — bail rather than panic.
        if app_build.get_webview_window(&label_build).is_some() {
            return;
        }
        let mut builder = WebviewWindowBuilder::new(
            &app_build,
            &label_build,
            WebviewUrl::App(SURFACE_URL.into()),
        )
        .inner_size(size.0, size.1)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(true)
        .center();
        if let Some(data_dir) = crate::portable::data_dir() {
            builder = builder.data_directory(data_dir.join("webview"));
        }
        let win = match builder.build() {
            Ok(w) => w,
            Err(e) => {
                log::error!("[GRAIN] failed to build overlay '{label_build}': {e}");
                extension::revoke_for_label(&label_build);
                return;
            }
        };
        // Focus loss dismisses early — a HUD that outlived the moment it
        // announced would be exactly the lingering the budget forbids.
        {
            let app_evt = app_build.clone();
            let label_evt = label_build.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    destroy_label(&app_evt, &label_evt);
                }
            });
        }
        // The hard lifetime cap. Guarded by epoch so a replaced overlay's timer
        // cannot reach through and destroy its successor.
        let app_to = app_build.clone();
        let label_to = label_build.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(timeout).await;
            if current_epoch(&label_to) == Some(epoch) {
                destroy_label(&app_to, &label_to);
            }
        });
    })
    .map_err(|e| e.to_string())?;

    set_epoch(&label, epoch);
    Ok(())
}

/// Dismiss (destroy) an extension's overlay if one is up. Silent otherwise.
pub fn dismiss(app: &AppHandle, ext_id: &str) {
    if let Some(label) = current().lock().unwrap().remove(ext_id) {
        destroy_label(app, &label);
    }
}

fn destroy_label(app: &AppHandle, label: &str) {
    clear_epoch(label);
    // Every destroy path lands here (explicit dismiss, timeout, focus loss), so
    // this is where the current-overlay handle is dropped — but only if it still
    // points at THIS label, never a newer overlay that already replaced it.
    current().lock().unwrap().retain(|_, live| live != label);
    // Clears the label binding, the pending token AND the stashed payload.
    extension::revoke_for_label(label);
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.destroy();
    }
}

// ── side tables ───────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// ext id → the label of its currently-live overlay. The dismiss handle: a
/// timeout or focus-loss knows its label directly, but an explicit
/// `overlay.dismiss` only knows the extension.
static CURRENT: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
fn current() -> &'static Mutex<HashMap<String, String>> {
    CURRENT.get_or_init(|| Mutex::new(HashMap::new()))
}
fn set_current(ext_id: &str, label: &str) {
    current()
        .lock()
        .unwrap()
        .insert(ext_id.to_string(), label.to_string());
}

/// epoch, keyed by (unique) window label — the timer's liveness check.
static EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
fn epochs() -> &'static Mutex<HashMap<String, u64>> {
    EPOCHS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn set_epoch(label: &str, epoch: u64) {
    epochs().lock().unwrap().insert(label.to_string(), epoch);
}
fn current_epoch(label: &str) -> Option<u64> {
    epochs().lock().unwrap().get(label).copied()
}
fn clear_epoch(label: &str) {
    epochs().lock().unwrap().remove(label);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_clamped_to_a_hud_not_a_window() {
        assert_eq!(clamp_size(Some([9999, 9999])), MAX_SIZE);
        assert_eq!(clamp_size(Some([1, 1])), MIN_SIZE);
        assert_eq!(clamp_size(Some([300, 100])), (300.0, 100.0));
        assert_eq!(clamp_size(None), DEFAULT_SIZE);
    }

    #[test]
    fn lifetime_is_always_bounded() {
        // No overlay outlives the ceiling, and one that asks for nothing still
        // gets a finite life.
        assert_eq!(clamp_timeout(Some(999_999)), MAX_TIMEOUT);
        assert_eq!(clamp_timeout(None), DEFAULT_TIMEOUT);
        assert_eq!(clamp_timeout(Some(2000)), Duration::from_millis(2000));
    }

    #[test]
    fn labels_never_collide_with_workspace_or_each_other() {
        assert_eq!(base_label("com.x.y"), "ext-overlay-com-x-y");
        assert_ne!(base_label("com.x.a"), base_label("com.x.b"));
        // A workspace and an overlay for the SAME extension are different windows.
        assert_ne!(
            base_label("com.x.y"),
            super::extension::label_for("com.x.y")
        );
        // Each invocation gets a unique label, so a replace never reuses a
        // still-closing window's name.
        assert_ne!(label_for("com.x.y", 1), label_for("com.x.y", 2));
    }
}
