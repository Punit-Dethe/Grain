//! [GRAIN] Extension-facing workspace surfaces (SPEC §1.2, §7.1).
//!
//! An extension **declares** a workspace; it never builds one. This module is
//! the whole bridge between "a manifest said `surfaces.workspace`" and the
//! host-owned sleeping window in [`super::workspace`] — the same machinery
//! Grain Space uses, so an extension workspace is not a second-class imitation
//! of the real thing.
//!
//! **The realm boundary is the point.** The window loads Grain's own page
//! (`extension-surface.html`), and the extension's markup goes inside a
//! sandboxed iframe on an opaque origin: no Tauri IPC, no access to the page
//! around it, no shared global with any other extension. That page is the only
//! holder of the surface token, so identity is bound to the *channel* and an
//! extension cannot forge one by asserting it in a payload.
//!
//! Each surface gets its **own** token, distinct from the extension's worker
//! token, and it is revoked the moment the window is destroyed — a UI that is
//! gone must not leave a usable credential behind.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::workspace::{self, Surface, WorkspaceSpec};

/// Grain's page that wraps every extension surface. NOT the SPA root — no
/// extension markup ever shares Grain's main global.
const SURFACE_URL: &str = "extension-surface.html";

/// Default window size for a declared workspace, when the pack asks for none.
const DEFAULT_SIZE: (f64, f64) = (1000.0, 700.0);
/// Floor for a pack's requested `min_size`, so a workspace cannot be declared
/// too small to contain its own close affordance.
const MIN_FLOOR: (f64, f64) = (360.0, 240.0);

/// What the wrapper page needs to boot. Handed over once, in response to the
/// page asking for it — never placed in the URL, where it would be readable
/// from the window title bar, logs and crash dumps.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInit {
    pub extension_id: String,
    pub token: String,
    /// The extension's HTML, rendered into a sandboxed iframe.
    pub ui_source: String,
    pub sleep_event: String,
    pub revive_event: String,
    pub payload_event: String,
}

/// window label → the init a surface page is waiting for.
static PENDING: OnceLock<Mutex<HashMap<String, SurfaceInit>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, SurfaceInit>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// window label → extension id. The label is a lossy sanitization, so this is
/// how a command resolves *which* extension its calling window belongs to —
/// derived from the caller, never from an argument the caller supplies.
static LABELS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn labels() -> &'static Mutex<HashMap<String, String>> {
    LABELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Which extension owns the window with this label, if it is a surface at all.
pub fn id_for_label(label: &str) -> Option<String> {
    labels().lock().unwrap().get(label).cloned()
}

/// Tauri window labels accept `a-zA-Z0-9-/:_` — an extension id is reverse-DNS
/// and carries dots, so the label is derived rather than reused. The surface id
/// stays the real extension id; only the OS-facing label is sanitized.
pub fn label_for(ext_id: &str) -> String {
    format!("ext-surface-{}", ext_id.replace(['.', ':'], "-"))
}

fn sleep_event(ext_id: &str) -> String {
    format!("ext-surface://{ext_id}/sleep")
}
fn revive_event(ext_id: &str) -> String {
    format!("ext-surface://{ext_id}/revive")
}
fn payload_event(ext_id: &str) -> String {
    format!("ext-surface://{ext_id}/payload")
}

/// Clamp a pack's requested minimum size into something a window can actually
/// be. A manifest is untrusted input: `[0, 0]` or `[99999, 99999]` must produce
/// a usable window, not a broken one.
fn clamp_min(requested: Option<[u32; 2]>, size: (f64, f64)) -> (f64, f64) {
    let r = requested.unwrap_or([MIN_FLOOR.0 as u32, MIN_FLOOR.1 as u32]);
    (
        (r[0] as f64).clamp(MIN_FLOOR.0, size.0),
        (r[1] as f64).clamp(MIN_FLOOR.1, size.1),
    )
}

/// Register (idempotently) the workspace an extension declares.
///
/// `Err` is the honest answer whenever the extension may not have one: not
/// installed, not enabled, no `surface:workspace` grant, or no declaration. The
/// capability check happens here as well as at the host-call boundary, so a
/// future caller that forgets cannot open a window on an ungranted capability.
fn register(app: &AppHandle, ext_id: &str) -> Result<Arc<Surface>, String> {
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
    if !rec.granted.iter().any(|c| c == "surface:workspace") {
        return Err("the 'surface:workspace' capability was not granted".into());
    }
    let pack = crate::extension_host::load_manifest(app, ext_id)
        .ok_or_else(|| format!("could not read the pack for '{ext_id}'"))?;
    let decl = pack
        .manifest
        .surfaces
        .workspace
        .as_ref()
        .ok_or_else(|| format!("'{ext_id}' declares no workspace"))?;

    let size = DEFAULT_SIZE;
    let ext_owned = ext_id.to_string();
    labels()
        .lock()
        .unwrap()
        .insert(label_for(ext_id), ext_owned.clone());
    Ok(workspace::ensure(WorkspaceSpec {
        id: ext_owned.clone(),
        label: label_for(ext_id),
        url: tauri::WebviewUrl::App(SURFACE_URL.into()),
        title: decl.title.clone(),
        size,
        min_size: clamp_min(decl.min_size, size),
        sleep_event: sleep_event(ext_id),
        revive_event: revive_event(ext_id),
        payload_event: payload_event(ext_id),
        // Extension surfaces are ordinary decorated windows: Grain Space's
        // custom chrome is drawn by Grain's own React tree, which an extension
        // does not have. A borderless window it cannot paint a title bar into
        // would be a window it cannot close.
        decorations: true,
        transparent: false,
        // Counts against the residency cap — this is exactly what the cap is for.
        capped: true,
        on_sleep: None,
        on_destroy: Some(Arc::new(move |_app| {
            revoke_for(&ext_owned);
        })),
    }))
}

/// Open (or wake) an extension's workspace.
pub fn open(
    app: &AppHandle,
    ext_id: &str,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    let surface = register(app, ext_id)?;
    stage_init(app, ext_id)?;
    workspace::open(app, &surface, payload);
    Ok(())
}

/// Put an extension's workspace to sleep. Silent when it was never opened —
/// closing something that is not open is not an error worth surfacing.
pub fn close(app: &AppHandle, ext_id: &str) {
    workspace::close(app, ext_id);
}

/// Destroy an extension's workspace outright and revoke its token. Called when
/// the extension is disabled or uninstalled: a disabled extension must keep no
/// window and no live credential.
pub fn destroy(app: &AppHandle, ext_id: &str) {
    workspace::destroy(app, ext_id);
    revoke_for(ext_id);
}

/// Mint the surface's token and park the init payload for the page to collect.
/// Re-staging replaces (and revokes) any previous one, so a reopened surface
/// never reuses a credential the old page might still hold.
fn stage_init(app: &AppHandle, ext_id: &str) -> Result<(), String> {
    use grain_core::extensions as ext;
    let reg = app
        .try_state::<Arc<ext::ExtensionsRegistry>>()
        .ok_or("extensions registry unavailable")?;
    let rec = reg.record(ext_id).ok_or("not installed")?;
    let pack =
        crate::extension_host::load_manifest(app, ext_id).ok_or("could not read the pack")?;
    let decl = pack
        .manifest
        .surfaces
        .workspace
        .as_ref()
        .ok_or("declares no workspace")?;

    let token =
        crate::events_server::mint_extension_token(ext_id, rec.granted.iter().cloned().collect());
    let init = SurfaceInit {
        extension_id: ext_id.to_string(),
        token,
        ui_source: decl.ui_source.clone(),
        sleep_event: sleep_event(ext_id),
        revive_event: revive_event(ext_id),
        payload_event: payload_event(ext_id),
    };
    let mut map = pending().lock().unwrap();
    if let Some(old) = map.insert(label_for(ext_id), init) {
        crate::events_server::revoke_token(&old.token);
    }
    Ok(())
}

/// The wrapper page asking for its identity, by window label. Consumed once —
/// the page holds it for the window's lifetime, and a second asker (which
/// should not exist) gets nothing rather than a working credential.
pub fn take_init(label: &str) -> Option<SurfaceInit> {
    pending().lock().unwrap().remove(label)
}

/// Revoke whatever token this surface was issued. Safe to call repeatedly.
fn revoke_for(ext_id: &str) {
    if let Some(init) = pending().lock().unwrap().remove(&label_for(ext_id)) {
        crate::events_server::revoke_token(&init.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_window_safe_and_collision_free() {
        // Tauri labels reject dots; two extensions must still never collide.
        let a = label_for("com.example.spaces");
        assert_eq!(a, "ext-surface-com-example-spaces");
        assert!(!a.contains('.'));
        assert_ne!(label_for("com.example.a"), label_for("com.example.b"));
    }

    #[test]
    fn a_nonsense_min_size_still_yields_a_usable_window() {
        // A manifest is untrusted input.
        assert_eq!(clamp_min(Some([0, 0]), (1000.0, 700.0)), MIN_FLOOR);
        assert_eq!(
            clamp_min(Some([99999, 99999]), (1000.0, 700.0)),
            (1000.0, 700.0),
            "a minimum larger than the window would make it unresizable"
        );
        assert_eq!(clamp_min(Some([900, 600]), (1000.0, 700.0)), (900.0, 600.0));
        assert_eq!(clamp_min(None, (1000.0, 700.0)), MIN_FLOOR);
    }

    #[test]
    fn init_is_handed_over_exactly_once() {
        let init = SurfaceInit {
            extension_id: "com.x.y".into(),
            token: "t".into(),
            ui_source: "<p>hi".into(),
            sleep_event: "s".into(),
            revive_event: "r".into(),
            payload_event: "p".into(),
        };
        pending()
            .lock()
            .unwrap()
            .insert("ext-surface-once".into(), init);
        assert!(take_init("ext-surface-once").is_some());
        assert!(
            take_init("ext-surface-once").is_none(),
            "a second asker must not receive a working credential"
        );
    }
}
