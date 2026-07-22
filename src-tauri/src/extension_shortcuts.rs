//! [GRAIN] Extension-contributed global shortcuts (SPEC §3.3, §4, §6).
//!
//! An extension declares shortcuts in its manifest; the **host** registers them
//! through the same binding registry core uses, under the namespaced id
//! `ext:<extension-id>:<shortcut-id>`. The namespacing is the same device
//! prompt packs use (`ext:<id>:<prompt>`): a collision with a core binding id,
//! or between two extensions, is unrepresentable rather than merely unlikely.
//!
//! Arbitration (SPEC §3.3) is deliberately conservative:
//! - **Core bindings always win.** An extension chord that matches one in use
//!   by core is left unregistered.
//! - **Between extensions, the earlier registrant wins** — and "earlier" means
//!   toggle order (SPEC §4.4), not install order or whatever the filesystem
//!   happened to enumerate first. The loser stays *inactive until rebound*,
//!   which the user can do from the extension's own settings section.
//!
//! Nothing here runs on a hot path: registration happens when the extension set
//! changes, and dispatch happens when a human presses a key.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{AppHandle, Manager};

use crate::settings::{self, ShortcutBinding};
use crate::shortcut::{register_shortcut, unregister_shortcut};

/// The binding-id prefix that marks an extension shortcut.
pub const PREFIX: &str = "ext:";

/// `ext:<extension-id>:<shortcut-id>`.
pub fn binding_id(ext_id: &str, shortcut_id: &str) -> String {
    format!("{PREFIX}{ext_id}:{shortcut_id}")
}

/// Split a namespaced binding id back into its parts. `None` for a core
/// binding. Both ids are guaranteed colon-free by manifest validation, so the
/// first colon after the prefix is the separator.
pub fn parse_binding_id(id: &str) -> Option<(&str, &str)> {
    id.strip_prefix(PREFIX)?.split_once(':')
}

/// One declared shortcut's live state, for the extension's settings section.
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct ShortcutStatus {
    pub id: String,
    pub label: String,
    /// The chord currently bound, or empty when the extension suggested none.
    pub binding: String,
    /// False when the chord is taken — SPEC §3.3: the later registrant is
    /// inactive until rebound, and both rows say so.
    pub active: bool,
    /// Who holds the chord, when inactive.
    pub conflicts_with: Option<String>,
}

/// Last sync's outcome per extension. Read by the settings UI; written only by
/// [`sync`], which runs off the hot path.
static STATUS: OnceLock<Mutex<HashMap<String, Vec<ShortcutStatus>>>> = OnceLock::new();

/// Extension bindings successfully registered with the active shortcut
/// backend. Keeping this separate from settings makes reconciliation
/// idempotent and lets a changed chord unregister the exact previous binding.
static LIVE: OnceLock<Mutex<HashMap<String, ShortcutBinding>>> = OnceLock::new();

fn status_map() -> &'static Mutex<HashMap<String, Vec<ShortcutStatus>>> {
    STATUS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn live_map() -> &'static Mutex<HashMap<String, ShortcutBinding>> {
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Keep reconciliation state aligned with shortcut commands that operate
/// directly on a binding (recording, rebinding, and backend switches).
pub fn note_registered(binding: &ShortcutBinding) {
    let Some((ext_id, sid)) = parse_binding_id(&binding.id) else {
        return;
    };
    live_map()
        .lock()
        .unwrap()
        .insert(binding.id.clone(), binding.clone());
    if let Some(rows) = status_map().lock().unwrap().get_mut(ext_id) {
        if let Some(row) = rows.iter_mut().find(|row| row.id == sid) {
            row.binding = binding.current_binding.clone();
            row.active = true;
            row.conflicts_with = None;
        }
    }
}

pub fn note_unregistered(binding_id: &str) {
    let Some((ext_id, sid)) = parse_binding_id(binding_id) else {
        return;
    };
    live_map().lock().unwrap().remove(binding_id);
    if let Some(rows) = status_map().lock().unwrap().get_mut(ext_id) {
        if let Some(row) = rows.iter_mut().find(|row| row.id == sid) {
            row.active = false;
        }
    }
}

/// The active shortcut backend was replaced, so none of the registrations in
/// the old backend remain authoritative.
pub fn reset_live() {
    live_map().lock().unwrap().clear();
}

pub fn status_for(ext_id: &str) -> Vec<ShortcutStatus> {
    status_map()
        .lock()
        .unwrap()
        .get(ext_id)
        .cloned()
        .unwrap_or_default()
}

/// Reconcile every extension shortcut registration with the registry.
///
/// Deferred onto the async runtime **always**, never run inline: registering a
/// global shortcut from inside a shortcut dispatch deadlocks every shortcut in
/// the app, and an extension shortcut that enables another extension would
/// reach here from exactly there. Deferring once, here, means no caller has to
/// remember the rule.
pub fn sync(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        sync_now(&app);
    });
}

/// One extension's shortcut contributions, as the planner needs them.
pub struct ExtInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// `(shortcut id, label, suggested chord, the user's rebind if any)`.
    pub shortcuts: Vec<(String, String, String, Option<String>)>,
}

/// What a sync should make true.
#[derive(Default)]
pub struct Plan {
    /// Every binding row that should exist in settings — present whether or not
    /// the extension is enabled, so a rebind survives a disable (SPEC §6).
    pub bindings: Vec<ShortcutBinding>,
    /// The subset to actually register with the OS.
    pub register: Vec<ShortcutBinding>,
    pub status: HashMap<String, Vec<ShortcutStatus>>,
}

/// Decide who gets which chord (SPEC §3.3). Pure — no settings, no OS, no
/// filesystem — because this is the part with the interesting rules.
///
/// `core_chords` maps an in-use chord to the human name of its core binding.
/// `exts` must arrive in **toggle order**: arbitration between extensions is
/// resolved by who the user turned on first, never by enumeration order.
pub fn plan(core_chords: &HashMap<String, String>, exts: &[ExtInput]) -> Plan {
    let mut taken = core_chords.clone();
    let mut out = Plan::default();

    for ext in exts {
        let mut rows = Vec::new();
        for (sid, label, suggested, rebind) in &ext.shortcuts {
            let id = binding_id(&ext.id, sid);
            // The user's rebind always wins over the manifest's suggestion.
            let chord = rebind
                .clone()
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| suggested.clone());

            let row = ShortcutBinding {
                id: id.clone(),
                name: format!("{} — {}", ext.name, label),
                description: ext.description.clone(),
                default_binding: suggested.clone(),
                current_binding: chord.clone(),
            };

            let holder = if chord.trim().is_empty() {
                None
            } else {
                taken.get(&chord).cloned()
            };
            let active = ext.enabled && holder.is_none() && !chord.trim().is_empty();
            if active {
                taken.insert(chord.clone(), row.name.clone());
                out.register.push(row.clone());
            }
            out.bindings.push(row);
            rows.push(ShortcutStatus {
                id: sid.clone(),
                label: label.clone(),
                binding: chord,
                active,
                conflicts_with: holder,
            });
        }
        out.status.insert(ext.id.clone(), rows);
    }
    out
}

fn sync_now(app: &AppHandle) {
    use grain_core::extensions as ext;
    let Some(reg) = app.try_state::<Arc<ext::ExtensionsRegistry>>() else {
        return;
    };

    let mut settings = settings::get_settings(app);

    // Chords already spoken for. Core first and unconditionally — a core
    // binding is never displaced by an extension.
    let core_chords: HashMap<String, String> = settings
        .bindings
        .iter()
        .filter(|(id, _)| !id.starts_with(PREFIX))
        .map(|(_, b)| (b.current_binding.clone(), b.name.clone()))
        .filter(|(chord, _)| !chord.trim().is_empty())
        .collect();

    let mut records = reg.records();
    records.sort_by_key(|r| r.toggle_seq);

    let inputs: Vec<ExtInput> = records
        .iter()
        .filter_map(|rec| {
            let pack = crate::extension_host::load_manifest(app, &rec.id)?;
            let mut shortcuts = pack
                .manifest
                .contributes
                .shortcuts
                .iter()
                .map(|declaration| {
                    (
                        declaration.id.clone(),
                        declaration.label.clone(),
                        declaration.default_binding.clone().unwrap_or_default(),
                        settings
                            .bindings
                            .get(&binding_id(&rec.id, &declaration.id))
                            .map(|binding| binding.current_binding.clone()),
                    )
                })
                .collect::<Vec<_>>();
            if let Some(mode) = &pack.manifest.contributes.session_mode {
                shortcuts.push((
                    mode.id.clone(),
                    mode.label.clone(),
                    mode.default_binding.clone().unwrap_or_default(),
                    settings
                        .bindings
                        .get(&binding_id(&rec.id, &mode.id))
                        .map(|binding| binding.current_binding.clone()),
                ));
            }
            if shortcuts.is_empty() {
                return None;
            }
            Some(ExtInput {
                id: rec.id.clone(),
                name: pack.manifest.name.clone(),
                description: pack.manifest.description.clone(),
                enabled: rec.enabled,
                shortcuts,
            })
        })
        .collect();

    let Plan {
        bindings,
        register,
        mut status,
    } = plan(&core_chords, &inputs);

    let mut bindings_changed = false;
    for row in bindings {
        let unchanged = settings.bindings.get(&row.id).is_some_and(|b| {
            b.name == row.name
                && b.description == row.description
                && b.default_binding == row.default_binding
                && b.current_binding == row.current_binding
        });
        if !unchanged {
            settings.bindings.insert(row.id.clone(), row);
            bindings_changed = true;
        }
    }
    let desired: HashMap<String, ShortcutBinding> =
        register.into_iter().map(|b| (b.id.clone(), b)).collect();

    if bindings_changed {
        settings::write_settings(app, settings.clone());
    }

    // Unregister removed or changed live bindings first, so a chord freed by
    // one extension is available to the next in the same pass. Settings cannot
    // supply the old chord after a rebind; LIVE deliberately retains it.
    let mut live = live_map().lock().unwrap();
    let stale: Vec<ShortcutBinding> = live
        .iter()
        .filter(|(id, current)| {
            desired
                .get(*id)
                .is_none_or(|next| next.current_binding != current.current_binding)
        })
        .map(|(_, binding)| binding.clone())
        .collect();
    for binding in stale {
        let _ = unregister_shortcut(app, binding.clone());
        live.remove(&binding.id);
    }
    for (id, b) in &desired {
        if live
            .get(id)
            .is_some_and(|current| current.current_binding == b.current_binding)
        {
            continue;
        }
        if let Err(e) = register_shortcut(app, b.clone()) {
            log::warn!("[GRAIN] could not register extension shortcut '{id}': {e}");
            if let Some((ext_id, sid)) = parse_binding_id(id) {
                if let Some(rows) = status.get_mut(ext_id) {
                    if let Some(row) = rows.iter_mut().find(|r| r.id == sid) {
                        row.active = false;
                    }
                }
            }
        } else {
            live.insert(id.clone(), b.clone());
        }
    }
    drop(live);

    let active = status.values().flatten().filter(|r| r.active).count();
    log::debug!(
        "[GRAIN] ext-shortcuts: {active} active across {} extension(s)",
        status.len()
    );
    *status_map().lock().unwrap() = status;
}

/// Drop an uninstalled extension's binding rows. Disable keeps them (a rebind
/// must survive), but uninstall is the transaction that clears everything.
pub fn forget(app: &AppHandle, ext_id: &str) {
    let prefix = format!("{PREFIX}{ext_id}:");
    let mut settings = settings::get_settings(app);
    let doomed: Vec<ShortcutBinding> = settings
        .bindings
        .iter()
        .filter(|(id, _)| id.starts_with(&prefix))
        .map(|(_, b)| b.clone())
        .collect();
    if doomed.is_empty() {
        return;
    }
    let mut live = live_map().lock().unwrap();
    for b in &doomed {
        let registered = live.remove(&b.id).unwrap_or_else(|| b.clone());
        let _ = unregister_shortcut(app, registered);
        settings.bindings.remove(&b.id);
    }
    drop(live);
    settings::write_settings(app, settings);
    status_map().lock().unwrap().remove(ext_id);
}

/// A namespaced shortcut fired. Wakes the extension if it is asleep and hands
/// it the shortcut id.
///
/// Returns immediately: this is called from the shortcut dispatch path, where
/// blocking — or worse, touching the shortcut registry — hangs every hotkey in
/// the app.
pub fn on_pressed(app: &AppHandle, ext_id: &str, shortcut_id: &str) {
    let is_session_mode = crate::extension_host::load_manifest(app, ext_id)
        .and_then(|pack| pack.manifest.contributes.session_mode)
        .is_some_and(|mode| mode.id == shortcut_id);
    if is_session_mode {
        crate::extension_session::toggle_from_shortcut(app, ext_id, shortcut_id);
        return;
    }
    crate::extension_host::wake_for_shortcut(app, ext_id, shortcut_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_ids_round_trip() {
        let id = binding_id("com.example.thing", "do-it");
        assert_eq!(id, "ext:com.example.thing:do-it");
        assert_eq!(parse_binding_id(&id), Some(("com.example.thing", "do-it")));
    }

    fn ext(id: &str, enabled: bool, shortcuts: &[(&str, &str)]) -> ExtInput {
        ExtInput {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            enabled,
            shortcuts: shortcuts
                .iter()
                .map(|(sid, chord)| (sid.to_string(), sid.to_string(), chord.to_string(), None))
                .collect(),
        }
    }

    fn core(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(chord, name)| (chord.to_string(), name.to_string()))
            .collect()
    }

    fn row<'a>(p: &'a Plan, ext_id: &str, sid: &str) -> &'a ShortcutStatus {
        p.status[ext_id].iter().find(|r| r.id == sid).unwrap()
    }

    #[test]
    fn core_bindings_always_win() {
        let p = plan(
            &core(&[("alt+space", "Transcribe")]),
            &[ext("a", true, &[("go", "alt+space")])],
        );
        let r = row(&p, "a", "go");
        assert!(!r.active, "an extension never displaces a core binding");
        assert_eq!(r.conflicts_with.as_deref(), Some("Transcribe"));
        assert!(p.register.is_empty());
        // …but the binding row still exists, so the user can rebind it.
        assert_eq!(p.bindings.len(), 1);
    }

    #[test]
    fn between_extensions_the_earlier_toggle_wins() {
        // `exts` arrives in toggle order — this is the whole contract.
        let p = plan(
            &core(&[]),
            &[
                ext("first", true, &[("go", "alt+j")]),
                ext("second", true, &[("go", "alt+j")]),
            ],
        );
        assert!(row(&p, "first", "go").active);
        let loser = row(&p, "second", "go");
        assert!(!loser.active, "later registrant is inactive until rebound");
        assert_eq!(loser.conflicts_with.as_deref(), Some("first — go"));
        assert_eq!(p.register.len(), 1);
    }

    #[test]
    fn a_disabled_extension_holds_no_chord() {
        // Disabling must FREE the chord for someone else — otherwise a
        // switched-off extension silently blocks the one replacing it.
        let p = plan(
            &core(&[]),
            &[
                ext("off", false, &[("go", "alt+j")]),
                ext("on", true, &[("go", "alt+j")]),
            ],
        );
        assert!(!row(&p, "off", "go").active);
        assert!(row(&p, "on", "go").active);
        assert_eq!(p.register.len(), 1);
        assert_eq!(p.register[0].id, "ext:on:go");
    }

    #[test]
    fn a_rebind_beats_the_manifests_suggestion() {
        let mut e = ext("a", true, &[("go", "alt+j")]);
        e.shortcuts[0].3 = Some("ctrl+shift+k".into());
        let p = plan(&core(&[("alt+j", "Transcribe")]), &[e]);
        let r = row(&p, "a", "go");
        assert!(r.active, "rebinding away from a conflict activates it");
        assert_eq!(r.binding, "ctrl+shift+k");
        // The suggestion is kept as the default so "reset" still means something.
        assert_eq!(p.bindings[0].default_binding, "alt+j");
    }

    #[test]
    fn a_shortcut_with_no_chord_is_inactive_but_not_a_conflict() {
        let p = plan(&core(&[]), &[ext("a", true, &[("go", "")])]);
        let r = row(&p, "a", "go");
        assert!(!r.active);
        assert_eq!(
            r.conflicts_with, None,
            "nothing is holding it — it is unset"
        );
        assert!(p.register.is_empty());
    }

    #[test]
    fn core_binding_ids_are_never_mistaken_for_extension_ones() {
        // The dispatch hook keys off this: anything without the prefix must
        // fall through to core's ACTION_MAP untouched.
        for core in ["transcribe", "cancel", "summon_agent", "test"] {
            assert_eq!(parse_binding_id(core), None);
        }
        // A prefix with no separator is malformed, not a half-match.
        assert_eq!(parse_binding_id("ext:no-separator"), None);
    }
}
