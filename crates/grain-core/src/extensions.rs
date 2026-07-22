//! [GRAIN] The extension registry (SPEC §4.2, §5.1, §10.1) — Phase 1.
//!
//! Persists which extensions are installed, enabled, and in what **toggle
//! order** (SPEC §4.4: the order the user *enabled* them in — first enabled
//! sits top; re-enabling moves to the end). Stored as owned JSON
//! (`extensions.json`) in the same data dir as settings but **physically
//! separate from `AppSettings`**, so core settings migrations never touch
//! extension state and vice versa.
//!
//! Two kinds of entry:
//! - **Built-ins** (Snippets, Context Awareness, Agent) are *not stored here*.
//!   They are descriptors in code, and their enabled state delegates to the
//!   core settings flags (`snippets_enabled`, `context_awareness_enabled`,
//!   `agent_enabled`) — manifest-first per PLAN.md D4: the registry and UI are
//!   new; the implementation stays where it is. Their toggle order is tracked
//!   here by id (a toggle bumps the sequence without creating a record's
//!   install data).
//! - **Installed packs** (the Agent centre-layout variant now; imported
//!   `.grainpack` files in the next chunk) are full records.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const EXTENSIONS_FILE: &str = "extensions.json";

/// The Agent centre-layout surface-variant pack (SPEC §10.2) — pre-known so
/// the upgrade import can install it for existing users, who had the centre
/// option before the platform existed.
pub const AGENT_CENTER_VARIANT_ID: &str = "grain.agent-center-layout";

/// Reserved occupant id standing for Grain's own built-in behaviour in a slot.
/// SPEC §3.2: "core defaults are occupants" — so a slot is never *free*, and the
/// first extension to claim one still faces an explicit takeover prompt rather
/// than silently displacing shipped behaviour.
pub const CORE_DEFAULT: &str = "grain.core";

/// The slot the centre-layout variant occupies when it is the active look
/// (SPEC §10.2). It has no `.grainpack.json` on disk — it is synthesized by
/// `load` — so its declared slots are backfilled here or nothing would know it
/// competes for the Agent's reply surface.
pub const AGENT_REPLY_SURFACE_SLOT: &str = "agent.reply-surface";

/// The slot a pill-theme pack occupies (SPEC §9). Core holds it by default;
/// whoever holds it supplies the pill's look.
pub const PILL_THEME_SLOT: &str = "pill.theme";

/// Built-in extension ids (enabled state delegates to settings flags).
pub const BUILTIN_SNIPPETS: &str = "grain.snippets";
pub const BUILTIN_CONTEXT: &str = "grain.context-awareness";
pub const BUILTIN_AGENT: &str = "grain.agent";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionRecord {
    pub id: String,
    pub enabled: bool,
    /// Position in toggle order (SPEC §4.4): set from `next_toggle_seq` every
    /// time the extension is enabled, so re-enabling moves it to the end.
    #[serde(default)]
    pub toggle_seq: u64,
    #[serde(default)]
    pub installed_version: String,
    /// Granted capability names (empty for A-inert packs, which need none).
    #[serde(default)]
    pub granted: Vec<String>,
    /// Exclusive positions this pack's manifest *declares* (SPEC §3.2). Copied
    /// from the manifest at install so occupancy is answerable from memory —
    /// no pack file is ever read to decide who owns a slot.
    ///
    /// These are **claimed on enable**: turning the pack on takes the position,
    /// which is what a pill theme or an output destination should do.
    #[serde(default)]
    pub slots: Vec<String>,
    /// Positions this pack *offers* itself for rather than claims — SPEC §10.2's
    /// **surface variant**. Enabling adds the pack as a choice in a host-owned
    /// chooser; a core setting decides who actually occupies the slot, so
    /// enabling alone is not a takeover and must not raise a conflict.
    ///
    /// Today the centre-layout variant is the only member (its occupancy is
    /// `agent_panel_position`). No manifest syntax expresses this yet —
    /// deliberately: per the capability-governance doctrine, the name is
    /// reserved and the shape waits for a real third-party consumer.
    #[serde(default)]
    pub variant_slots: Vec<String>,
    /// A load-unpacked project currently overriding this id. The effective
    /// record stays at the normal map key, so every capability/slot/lifecycle
    /// path sees exactly one extension. Any installed version is parked here
    /// verbatim and restored on unload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<DevOverride>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DevOverride {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced: Option<Box<ExtensionRecord>>,
}

/// Why an enable was refused: the slot and who holds it (`grain.core` for a
/// built-in default). Mirrors the `needsPermissions` shape the permission sheet
/// already uses, so the frontend flow is the familiar one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotConflict {
    pub slot: String,
    pub current_occupant: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    /// Installed pack records, keyed by extension id.
    #[serde(default)]
    records: HashMap<String, ExtensionRecord>,
    /// Toggle sequence for BUILT-INS (id → seq); their enabled state lives in
    /// settings, but their position in toggle order is registry business.
    #[serde(default)]
    builtin_toggle_seq: HashMap<String, u64>,
    #[serde(default)]
    next_toggle_seq: u64,
    /// Slot → current occupant id (SPEC §3.2). Every known slot is present
    /// after `load`, holding `CORE_DEFAULT` until an extension takes it.
    #[serde(default)]
    slot_claims: HashMap<String, String>,
}

pub struct ExtensionsRegistry {
    path: PathBuf,
    state: RwLock<RegistryFile>,
}

impl ExtensionsRegistry {
    /// Load (or initialize) the registry. On first initialization,
    /// `settings_file_preexisted` drives the SPEC §10.1 upgrade rule for the
    /// centre-layout variant: existing users had the centre option before the
    /// platform, so it installs enabled for them; new installs start without it.
    pub fn load(data_dir: &Path, settings_file_preexisted: bool) -> Result<Self> {
        let path = data_dir.join(EXTENSIONS_FILE);
        let state = if path.exists() {
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&raw).unwrap_or_else(|e| {
                // A corrupt registry must not brick startup; extensions revert
                // to "fresh" state, settings-backed built-ins are unaffected.
                log::warn!("extensions.json unreadable ({e}); reinitializing registry");
                RegistryFile::default()
            })
        } else {
            let mut fresh = RegistryFile::default();
            if settings_file_preexisted {
                fresh.next_toggle_seq = 1;
                fresh.records.insert(
                    AGENT_CENTER_VARIANT_ID.to_string(),
                    ExtensionRecord {
                        id: AGENT_CENTER_VARIANT_ID.to_string(),
                        enabled: true,
                        toggle_seq: 0,
                        installed_version: "1.0.0".into(),
                        granted: Vec::new(),
                        slots: Vec::new(),
                        // Offered, not claimed: the dropdown decides (SPEC §10.2).
                        variant_slots: vec![AGENT_REPLY_SURFACE_SLOT.to_string()],
                        dev: None,
                    },
                );
            }
            fresh
        };
        let reg = Self {
            path,
            state: RwLock::new(state),
        };
        reg.heal_slots();
        reg.save()?;
        Ok(reg)
    }

    /// Bring a registry file written before slots existed up to date, and make
    /// SPEC §3.2's "core defaults are occupants" literally true in storage.
    ///
    /// Two repairs, both idempotent:
    /// 1. Every known slot with no claim is claimed by `CORE_DEFAULT`. A slot is
    ///    therefore never *free*, so a claim can never look uncontested and
    ///    silently displace shipped behaviour.
    /// 2. The centre-layout variant's slot is backfilled as a **variant** slot
    ///    (offered, not claimed). It is the one record with no pack file on
    ///    disk, so nothing else can tell us it competes for
    ///    `agent.reply-surface` — and it is offered rather than claimed because
    ///    SPEC §10.2 makes enabling it merely add it to the position dropdown.
    ///    Registries written by the first slots build recorded it under `slots`,
    ///    where enabling it collided with core's own default; move it.
    fn heal_slots(&self) {
        let mut state = self.state.write().unwrap();
        for slot in grain_sdk::manifest::KNOWN_SLOTS {
            state
                .slot_claims
                .entry((*slot).to_string())
                .or_insert_with(|| CORE_DEFAULT.to_string());
        }
        if let Some(rec) = state.records.get_mut(AGENT_CENTER_VARIANT_ID) {
            rec.slots.retain(|s| s != AGENT_REPLY_SURFACE_SLOT);
            if rec.variant_slots.is_empty() {
                rec.variant_slots = vec![AGENT_REPLY_SURFACE_SLOT.to_string()];
            }
        }
    }

    fn save(&self) -> Result<()> {
        let state = self.state.read().unwrap();
        let json = serde_json::to_string_pretty(&*state)?;
        fs::write(&self.path, json).with_context(|| format!("write {}", self.path.display()))
    }

    /// All installed pack records (unordered; callers sort by toggle_seq).
    pub fn records(&self) -> Vec<ExtensionRecord> {
        self.state
            .read()
            .unwrap()
            .records
            .values()
            .cloned()
            .collect()
    }

    pub fn record(&self, id: &str) -> Option<ExtensionRecord> {
        self.state.read().unwrap().records.get(id).cloned()
    }

    /// The installed record beneath a dev override, or the normal record when
    /// no override is active. A dev-only project has no installed record.
    pub fn installed_record(&self, id: &str) -> Option<ExtensionRecord> {
        let state = self.state.read().unwrap();
        let record = state.records.get(id)?;
        match &record.dev {
            Some(dev) => dev.replaced.as_deref().cloned(),
            None => Some(record.clone()),
        }
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.state.read().unwrap().records.contains_key(id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .map(|r| r.enabled)
            .unwrap_or(false)
    }

    pub fn dev_path(&self, id: &str) -> Option<PathBuf> {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .and_then(|record| record.dev.as_ref())
            .map(|dev| dev.path.clone())
    }

    pub fn dev_overrides_installed(&self, id: &str) -> bool {
        self.state
            .read()
            .unwrap()
            .records
            .get(id)
            .and_then(|record| record.dev.as_ref())
            .is_some_and(|dev| dev.replaced.is_some())
    }

    pub fn dev_records(&self) -> Vec<(String, PathBuf)> {
        self.state
            .read()
            .unwrap()
            .records
            .values()
            .filter_map(|record| {
                record
                    .dev
                    .as_ref()
                    .map(|dev| (record.id.clone(), dev.path.clone()))
            })
            .collect()
    }

    // ── Slots (SPEC §3.2: at most one enabled occupant per slot) ───────────

    /// Who currently holds `slot` — an extension id, or `CORE_DEFAULT` for
    /// Grain's own behaviour. `None` only for a slot nothing has ever claimed
    /// and that has no core default (`overrides:*`, `provides:*`).
    pub fn slot_occupant(&self, slot: &str) -> Option<String> {
        self.state.read().unwrap().slot_claims.get(slot).cloned()
    }

    /// Every slot currently held by `id`.
    pub fn slots_held(&self, id: &str) -> Vec<String> {
        let state = self.state.read().unwrap();
        let mut held: Vec<String> = state
            .slot_claims
            .iter()
            .filter(|(_, occupant)| occupant.as_str() == id)
            .map(|(slot, _)| slot.clone())
            .collect();
        held.sort();
        held
    }

    /// The first slot this extension *claims* that somebody else holds, if any.
    /// The gate for enabling: a conflict must reach the user as a takeover
    /// prompt, never be resolved silently or by load order.
    ///
    /// Reads `slots` only. A `variant_slots` entry is an offer, not a claim —
    /// enabling a surface variant adds it to a chooser and changes no occupant,
    /// so raising a conflict there would block a pack from ever being turned on.
    pub fn slot_conflict(&self, id: &str) -> Option<SlotConflict> {
        let state = self.state.read().unwrap();
        let rec = state.records.get(id)?;
        rec.slots.iter().find_map(|slot| {
            match state.slot_claims.get(slot) {
                Some(occupant) if occupant != id => Some(SlotConflict {
                    slot: slot.clone(),
                    current_occupant: occupant.clone(),
                }),
                // Unclaimed and no core default (`overrides:*`): free to take.
                _ => None,
            }
        })
    }

    /// Hand `slot` to `challenger`, returning whoever was displaced. The
    /// displaced extension is disabled in the same transaction — SPEC §3.2 has
    /// no state where two enabled extensions both believe they own a slot.
    /// Core defaults are displaced without disabling anything (there is no
    /// record to disable; core simply stops rendering that position).
    pub fn take_slot(&self, challenger: &str, slot: &str) -> Result<Option<String>> {
        let displaced = {
            let mut state = self.state.write().unwrap();
            let declares = state
                .records
                .get(challenger)
                .map(|r| {
                    r.slots
                        .iter()
                        .chain(r.variant_slots.iter())
                        .any(|s| s == slot)
                })
                .unwrap_or(false);
            if !declares {
                anyhow::bail!("'{challenger}' does not declare slot '{slot}'");
            }
            let previous = state
                .slot_claims
                .insert(slot.to_string(), challenger.to_string());
            match previous {
                Some(prev) if prev != CORE_DEFAULT && prev != challenger => {
                    if let Some(rec) = state.records.get_mut(&prev) {
                        rec.enabled = false;
                    }
                    Some(prev)
                }
                _ => None,
            }
        };
        self.save()?;
        Ok(displaced)
    }

    /// Release every slot `id` holds, back to Grain's default where one exists.
    /// Called on disable and uninstall — SPEC §6: "slots released".
    fn release_slots_locked(state: &mut RegistryFile, id: &str) {
        let held: Vec<String> = state
            .slot_claims
            .iter()
            .filter(|(_, occupant)| occupant.as_str() == id)
            .map(|(slot, _)| slot.clone())
            .collect();
        for slot in held {
            if grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()) {
                state.slot_claims.insert(slot, CORE_DEFAULT.to_string());
            } else {
                state.slot_claims.remove(&slot);
            }
        }
    }

    /// Force a slot's occupant without the takeover checks. Only for reconciling
    /// a slot whose truth lives outside the registry — today just the centre
    /// variant, whose real state is `agent_panel_position` (SPEC §10.2: enabling
    /// it adds it to the dropdown; *selecting* it takes the slot).
    pub fn set_slot_claim(&self, slot: &str, occupant: &str) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            if state.slot_claims.get(slot).map(String::as_str) == Some(occupant) {
                return Ok(());
            }
            state
                .slot_claims
                .insert(slot.to_string(), occupant.to_string());
        }
        self.save()
    }

    /// Enable/disable an installed pack. Enabling assigns the next toggle
    /// sequence (SPEC §4.4: re-enabling moves to the end of toggle order) and
    /// claims the slots the pack declares; disabling releases them.
    ///
    /// Enabling into an occupied slot is refused here as well as at the command
    /// layer, so a caller that forgets to check cannot steal a slot by accident.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        if enabled {
            if let Some(c) = self.slot_conflict(id) {
                anyhow::bail!("slot '{}' is occupied by '{}'", c.slot, c.current_occupant);
            }
        }
        let changed = {
            let mut state = self.state.write().unwrap();
            let next = state.next_toggle_seq;
            let declared = match state.records.get_mut(id) {
                Some(rec) if rec.enabled != enabled => {
                    rec.enabled = enabled;
                    if enabled {
                        rec.toggle_seq = next;
                    }
                    Some(rec.slots.clone())
                }
                _ => None,
            };
            match declared {
                Some(slots) => {
                    if enabled {
                        state.next_toggle_seq += 1;
                        for slot in slots {
                            state.slot_claims.insert(slot, id.to_string());
                        }
                    } else {
                        Self::release_slots_locked(&mut state, id);
                    }
                    true
                }
                None => false,
            }
        };
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    /// Record a built-in's enable moment so it participates in toggle order
    /// (its actual enabled bit lives in settings; call this when flipping it on).
    pub fn touch_builtin_toggle(&self, id: &str) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            let seq = state.next_toggle_seq;
            state.builtin_toggle_seq.insert(id.to_string(), seq);
            state.next_toggle_seq += 1;
        }
        self.save()
    }

    /// Toggle-order position for any id (built-in or pack); `u64::MAX` for
    /// never-toggled (sorts last, stable).
    pub fn toggle_seq(&self, id: &str) -> u64 {
        let state = self.state.read().unwrap();
        state
            .records
            .get(id)
            .map(|r| r.toggle_seq)
            .or_else(|| state.builtin_toggle_seq.get(id).copied())
            .unwrap_or(u64::MAX)
    }

    /// Install a pack record (import path lands in the next chunk; the
    /// centre-variant import uses this today via `load`).
    pub fn install(&self, record: ExtensionRecord) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            let id = record.id.clone();
            // A store/manual install arriving while this id is overridden
            // updates the parked installed record, never the effective dev
            // record. The author can keep testing without losing the update.
            if state
                .records
                .get(&id)
                .is_some_and(|active| active.dev.is_some())
            {
                if record.dev.is_some() {
                    // Mutating the effective dev record (for example after a
                    // capability grant) must not turn it into its own parked
                    // installed version.
                    state.records.insert(id, record);
                } else {
                    state
                        .records
                        .get_mut(&id)
                        .and_then(|active| active.dev.as_mut())
                        .expect("dev record exists")
                        .replaced = Some(Box::new(record));
                }
                drop(state);
                return self.save();
            }
            // Both lists count as declared: a variant still legitimately holds
            // the slot while it is the selected look.
            let declared: Vec<String> = record
                .slots
                .iter()
                .chain(record.variant_slots.iter())
                .cloned()
                .collect();
            state.records.insert(id.clone(), record);
            // An update may drop a slot it used to declare; holding a claim on
            // a slot you no longer declare would block everyone else forever.
            let stale: Vec<String> = state
                .slot_claims
                .iter()
                .filter(|(slot, occupant)| occupant.as_str() == id && !declared.contains(slot))
                .map(|(slot, _)| slot.clone())
                .collect();
            for slot in stale {
                if grain_sdk::manifest::KNOWN_SLOTS.contains(&slot.as_str()) {
                    state.slot_claims.insert(slot, CORE_DEFAULT.to_string());
                } else {
                    state.slot_claims.remove(&slot);
                }
            }
            // A newly declared slot is NOT auto-claimed on update: an already
            // enabled pack must not gain a position the user never granted it.
            // It stays a pending conflict until the user takes the slot.
        }
        self.save()
    }

    /// Make `record` the effective load-unpacked extension for its id. Any
    /// installed record is parked verbatim; replacing one dev path preserves
    /// that original backup rather than nesting overrides.
    pub fn load_dev(&self, mut record: ExtensionRecord, path: PathBuf) -> Result<()> {
        {
            let mut state = self.state.write().unwrap();
            let id = record.id.clone();
            let replaced =
                state
                    .records
                    .remove(&id)
                    .and_then(|mut previous| match previous.dev.take() {
                        Some(dev) => dev.replaced,
                        None => Some(Box::new(previous)),
                    });
            Self::release_slots_locked(&mut state, &id);
            record.dev = Some(DevOverride { path, replaced });
            state.records.insert(id, record);
        }
        self.save()
    }

    /// Remove a load-unpacked override and restore its parked installed record,
    /// if any. A restored enabled record reclaims only still-free slots; if a
    /// different extension took one meanwhile, it is restored disabled so no
    /// takeover happens silently.
    pub fn unload_dev(&self, id: &str) -> Result<bool> {
        let changed = {
            let mut state = self.state.write().unwrap();
            let Some(mut active) = state.records.remove(id) else {
                return Ok(false);
            };
            let Some(dev) = active.dev.take() else {
                state.records.insert(id.to_string(), active);
                return Ok(false);
            };
            Self::release_slots_locked(&mut state, id);

            if let Some(mut replaced) = dev.replaced.map(|record| *record) {
                if replaced.enabled {
                    let contested = replaced.slots.iter().any(|slot| {
                        state
                            .slot_claims
                            .get(slot)
                            .is_some_and(|occupant| occupant != CORE_DEFAULT && occupant != id)
                    });
                    if contested {
                        replaced.enabled = false;
                    } else {
                        for slot in &replaced.slots {
                            state.slot_claims.insert(slot.clone(), id.to_string());
                        }
                    }
                }
                state.records.insert(id.to_string(), replaced);
            }
            true
        };
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    pub fn uninstall(&self, id: &str) -> Result<bool> {
        let removed = {
            let mut state = self.state.write().unwrap();
            let dev_active = state
                .records
                .get(id)
                .is_some_and(|record| record.dev.is_some());
            if dev_active {
                // Uninstalling while a load-unpacked copy is effective removes
                // only the parked installed version. The local project and its
                // live slot state are a separate, explicit developer action.
                state
                    .records
                    .get_mut(id)
                    .and_then(|record| record.dev.as_mut())
                    .expect("dev record exists")
                    .replaced
                    .take()
                    .is_some()
            } else {
                let removed = state.records.remove(id).is_some();
                if removed {
                    Self::release_slots_locked(&mut state, id);
                }
                removed
            }
        };
        if removed {
            self.save()?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn fresh_install_has_no_center_variant() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(!reg.is_installed(AGENT_CENTER_VARIANT_ID));
    }

    #[test]
    fn upgrade_installs_center_variant_enabled() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        assert!(reg.is_enabled(AGENT_CENTER_VARIANT_ID));
        // …and the decision persists across reloads (marker file exists now).
        drop(reg);
        let reg2 = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(reg2.is_enabled(AGENT_CENTER_VARIANT_ID));
    }

    #[test]
    fn toggle_order_is_enable_order_and_reenable_moves_to_end() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        for id in ["a", "b"] {
            reg.install(pack(id, &[])).unwrap();
        }
        reg.set_enabled("a", true).unwrap();
        reg.set_enabled("b", true).unwrap();
        assert!(
            reg.toggle_seq("a") < reg.toggle_seq("b"),
            "first enabled = top"
        );

        // Built-ins participate in the same ordering space.
        reg.touch_builtin_toggle(BUILTIN_SNIPPETS).unwrap();
        assert!(reg.toggle_seq("b") < reg.toggle_seq(BUILTIN_SNIPPETS));

        // Disable + re-enable moves to the end (SPEC §4.4).
        reg.set_enabled("a", false).unwrap();
        reg.set_enabled("a", true).unwrap();
        assert!(reg.toggle_seq("a") > reg.toggle_seq(BUILTIN_SNIPPETS));

        // Never-toggled sorts last.
        assert_eq!(reg.toggle_seq("never"), u64::MAX);
    }

    fn pack(id: &str, slots: &[&str]) -> ExtensionRecord {
        ExtensionRecord {
            id: id.into(),
            enabled: false,
            toggle_seq: 0,
            installed_version: "1".into(),
            granted: vec![],
            slots: slots.iter().map(|s| s.to_string()).collect(),
            variant_slots: vec![],
            dev: None,
        }
    }

    #[test]
    fn dev_only_record_disappears_on_unload() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        let expected = dir.path().join("project");
        assert_eq!(
            reg.dev_path("com.x.dev").as_deref(),
            Some(expected.as_path())
        );
        assert!(!reg.dev_overrides_installed("com.x.dev"));
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn dev_override_restores_the_installed_record_verbatim() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.enabled = true;
        installed.toggle_seq = 7;
        installed.granted = vec!["storage".into()];
        reg.install(installed).unwrap();

        let mut dev = pack("com.x.dev", &[]);
        dev.installed_version = "dev-2".into();
        reg.load_dev(dev, dir.path().join("project")).unwrap();
        assert!(reg.dev_overrides_installed("com.x.dev"));
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "dev-2");

        assert!(reg.unload_dev("com.x.dev").unwrap());
        let restored = reg.record("com.x.dev").unwrap();
        assert!(restored.enabled);
        assert_eq!(restored.toggle_seq, 7);
        assert_eq!(restored.granted, vec!["storage"]);
        assert!(restored.dev.is_none());
    }

    #[test]
    fn replacing_a_dev_path_does_not_nest_or_lose_the_installed_backup() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();

        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("two"))
            .unwrap();
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn replacing_a_dev_only_path_still_disappears_on_unload() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("one"))
            .unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("two"))
            .unwrap();

        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn uninstall_during_dev_override_removes_only_installed_copy() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        assert!(reg.uninstall("com.x.dev").unwrap());
        assert!(reg.dev_path("com.x.dev").is_some());
        assert!(!reg.dev_overrides_installed("com.x.dev"));
        assert!(reg.unload_dev("com.x.dev").unwrap());
        assert!(!reg.is_installed("com.x.dev"));
    }

    #[test]
    fn updating_effective_dev_record_preserves_its_installed_backup() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        let mut installed = pack("com.x.dev", &[]);
        installed.installed_version = "store".into();
        reg.install(installed).unwrap();
        reg.load_dev(pack("com.x.dev", &[]), dir.path().join("project"))
            .unwrap();

        let mut active = reg.record("com.x.dev").unwrap();
        active.granted.push("storage".into());
        reg.install(active).unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().granted, vec!["storage"]);

        reg.unload_dev("com.x.dev").unwrap();
        assert_eq!(reg.record("com.x.dev").unwrap().installed_version, "store");
    }

    #[test]
    fn core_defaults_occupy_every_known_slot() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        for slot in grain_sdk::manifest::KNOWN_SLOTS {
            assert_eq!(
                reg.slot_occupant(slot).as_deref(),
                Some(CORE_DEFAULT),
                "slot '{slot}' must not start free"
            );
        }
        // A slot with no core default stays unclaimed until someone takes it.
        assert_eq!(reg.slot_occupant("overrides:always_on_microphone"), None);
    }

    #[test]
    fn claim_conflicts_then_takeover_then_release() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["pill.theme"])).unwrap();
        reg.install(pack("b", &["pill.theme"])).unwrap();

        // Grain's own theme is the incumbent, so even the FIRST claim conflicts.
        assert_eq!(
            reg.slot_conflict("a"),
            Some(SlotConflict {
                slot: "pill.theme".into(),
                current_occupant: CORE_DEFAULT.into(),
            })
        );
        assert!(reg.set_enabled("a", true).is_err(), "no silent steal");

        // Takeover from core displaces nobody, then enabling succeeds.
        assert_eq!(reg.take_slot("a", "pill.theme").unwrap(), None);
        assert!(reg.set_enabled("a", true).unwrap());
        assert_eq!(reg.slot_occupant("pill.theme").as_deref(), Some("a"));

        // A second claimant sees the real occupant, not the core default.
        assert_eq!(
            reg.slot_conflict("b").unwrap().current_occupant,
            "a".to_string()
        );
        // Takeover disables the incumbent in the same transaction: SPEC §3.2
        // has no state where two enabled extensions both own a slot.
        assert_eq!(
            reg.take_slot("b", "pill.theme").unwrap().as_deref(),
            Some("a")
        );
        assert!(!reg.is_enabled("a"));
        assert!(reg.set_enabled("b", true).unwrap());

        // Disable releases back to Grain's default — never to the loser.
        reg.set_enabled("b", false).unwrap();
        assert_eq!(
            reg.slot_occupant("pill.theme").as_deref(),
            Some(CORE_DEFAULT)
        );
        assert!(reg.slots_held("b").is_empty());
    }

    #[test]
    fn taking_an_undeclared_slot_is_refused() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["pill.theme"])).unwrap();
        assert!(reg.take_slot("a", "output.destination").is_err());
        assert!(reg.take_slot("ghost", "pill.theme").is_err());
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT)
        );
    }

    #[test]
    fn center_variant_declares_its_slot_even_without_a_pack_file() {
        // The centre variant is synthesized by `load` and has NO
        // `.grainpack.json`, so nothing else can report that it competes for
        // the Agent's reply surface. Without this backfill the first real claim
        // would look uncontested and silently displace a shipped feature.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        assert_eq!(
            reg.record(AGENT_CENTER_VARIANT_ID).unwrap().variant_slots,
            vec![AGENT_REPLY_SURFACE_SLOT.to_string()]
        );

        // …and a registry written before slots existed is healed on load.
        let stale = format!(
            r#"{{"records":{{"{id}":{{"id":"{id}","enabled":true,"toggle_seq":0,
               "installed_version":"1.0.0","granted":[]}}}}}}"#,
            id = AGENT_CENTER_VARIANT_ID
        );
        fs::write(dir.path().join(EXTENSIONS_FILE), stale).unwrap();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert_eq!(
            reg.record(AGENT_CENTER_VARIANT_ID).unwrap().variant_slots,
            vec![AGENT_REPLY_SURFACE_SLOT.to_string()]
        );
        assert_eq!(
            reg.slot_occupant(AGENT_REPLY_SURFACE_SLOT).as_deref(),
            Some(CORE_DEFAULT),
            "the sidebar is the built-in default until the centre look is selected"
        );

        // Selecting the centre look is what takes the slot (SPEC §10.2).
        reg.set_slot_claim(AGENT_REPLY_SURFACE_SLOT, AGENT_CENTER_VARIANT_ID)
            .unwrap();
        reg.install(pack("rival", &[AGENT_REPLY_SURFACE_SLOT]))
            .unwrap();
        assert_eq!(
            reg.slot_conflict("rival").unwrap().current_occupant,
            AGENT_CENTER_VARIANT_ID.to_string()
        );
    }

    #[test]
    fn enabling_a_surface_variant_is_not_a_takeover() {
        // Regression (reported live): toggling the centre layout on failed with
        // "agent.reply-surface is occupied by grain.core". Enabling a variant
        // only adds it to the position dropdown (SPEC §10.2) — it changes no
        // occupant, so treating the declaration as a claim made the pack
        // impossible to turn on at all.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        reg.set_enabled(AGENT_CENTER_VARIANT_ID, false).unwrap();

        assert_eq!(reg.slot_conflict(AGENT_CENTER_VARIANT_ID), None);
        reg.set_enabled(AGENT_CENTER_VARIANT_ID, true)
            .expect("enabling a variant must not be refused");
        assert_eq!(
            reg.slot_occupant(AGENT_REPLY_SURFACE_SLOT).as_deref(),
            Some(CORE_DEFAULT),
            "enabling offers the look; it does not select it"
        );

        // A pack that genuinely CLAIMS the same slot still faces the prompt.
        reg.install(pack("rival", &[AGENT_REPLY_SURFACE_SLOT]))
            .unwrap();
        assert!(reg.set_enabled("rival", true).is_err());
    }

    #[test]
    fn a_selected_variant_still_releases_its_slot_on_disable() {
        // Occupancy is what releases, not declaration — so switching the
        // centre look off hands the reply surface back to core.
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), true).unwrap();
        reg.set_slot_claim(AGENT_REPLY_SURFACE_SLOT, AGENT_CENTER_VARIANT_ID)
            .unwrap();
        reg.set_enabled(AGENT_CENTER_VARIANT_ID, false).unwrap();
        assert_eq!(
            reg.slot_occupant(AGENT_REPLY_SURFACE_SLOT).as_deref(),
            Some(CORE_DEFAULT)
        );
    }

    #[test]
    fn an_update_that_drops_a_slot_releases_it() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["pill.theme"])).unwrap();
        reg.take_slot("a", "pill.theme").unwrap();
        reg.set_enabled("a", true).unwrap();

        // v2 no longer themes the pill, but does want the output slot.
        let mut v2 = pack("a", &["output.destination"]);
        v2.enabled = true;
        reg.install(v2).unwrap();
        assert_eq!(
            reg.slot_occupant("pill.theme").as_deref(),
            Some(CORE_DEFAULT),
            "a dropped slot must not stay held forever"
        );
        // The newly declared slot is not auto-granted — it needs a takeover.
        assert_eq!(
            reg.slot_occupant("output.destination").as_deref(),
            Some(CORE_DEFAULT)
        );
        assert!(reg.slot_conflict("a").is_some());
    }

    #[test]
    fn uninstall_releases_slots() {
        let dir = tmp();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        reg.install(pack("a", &["overlay.recording", "overrides:x"]))
            .unwrap();
        reg.take_slot("a", "overlay.recording").unwrap();
        reg.take_slot("a", "overrides:x").unwrap();
        assert_eq!(reg.slots_held("a").len(), 2);

        reg.uninstall("a").unwrap();
        assert_eq!(
            reg.slot_occupant("overlay.recording").as_deref(),
            Some(CORE_DEFAULT)
        );
        // A slot with no core default disappears entirely rather than being
        // left pointing at an uninstalled extension.
        assert_eq!(reg.slot_occupant("overrides:x"), None);
    }

    #[test]
    fn slot_claims_survive_a_reload() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            reg.install(pack("a", &["pill.theme"])).unwrap();
            reg.take_slot("a", "pill.theme").unwrap();
            reg.set_enabled("a", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert_eq!(reg.slot_occupant("pill.theme").as_deref(), Some("a"));
        assert_eq!(reg.slots_held("a"), vec!["pill.theme".to_string()]);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tmp();
        {
            let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
            let mut rec = pack("x", &[]);
            rec.installed_version = "2.0".into();
            reg.install(rec).unwrap();
            reg.set_enabled("x", true).unwrap();
        }
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(reg.is_enabled("x"));
        assert_eq!(reg.record("x").unwrap().installed_version, "2.0");
        // Corrupt file → reinitialize, not crash.
        fs::write(dir.path().join(EXTENSIONS_FILE), "{not json").unwrap();
        let reg = ExtensionsRegistry::load(dir.path(), false).unwrap();
        assert!(!reg.is_installed("x"));
    }
}

// ── Tier-A pack application (Phase 1: prompt packs) ─────────────────────────

/// Apply a prompt pack to settings: entries land in the user's prompt list
/// under `ext:<extid>:<id>` (SPEC #15 — namespaced, so collisions with user
/// prompts or other packs are unrepresentable). Idempotent: re-applying the
/// same pack replaces its own entries.
pub fn apply_prompt_pack(
    settings: &mut crate::settings::AppSettings,
    ext_id: &str,
    entries: &[grain_sdk::PromptPackEntry],
) {
    remove_prompt_pack(settings, ext_id);
    for e in entries {
        settings
            .post_process_prompts
            .push(crate::settings::LLMPrompt {
                id: format!("ext:{ext_id}:{}", e.id),
                name: e.name.clone(),
                prompt: e.prompt.clone(),
            });
    }
}

/// Remove a pack's prompts (disable/uninstall). If the removed pack's prompt
/// was the SELECTED one, selection falls back to the first remaining prompt —
/// never a dangling id (the §10.2 restore principle, applied to prompts).
pub fn remove_prompt_pack(settings: &mut crate::settings::AppSettings, ext_id: &str) {
    let prefix = format!("ext:{ext_id}:");
    settings
        .post_process_prompts
        .retain(|p| !p.id.starts_with(&prefix));
    if let Some(sel) = &settings.post_process_selected_prompt_id {
        if sel.starts_with(&prefix) {
            settings.post_process_selected_prompt_id =
                settings.post_process_prompts.first().map(|p| p.id.clone());
        }
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;
    use crate::settings::AppSettings;
    use grain_sdk::PromptPackEntry;

    fn entries() -> Vec<PromptPackEntry> {
        vec![PromptPackEntry {
            id: "formal".into(),
            name: "Formal".into(),
            prompt: "Rewrite formally.".into(),
        }]
    }

    #[test]
    fn apply_is_namespaced_and_idempotent() {
        let mut s = AppSettings::default();
        let before = s.post_process_prompts.len();
        apply_prompt_pack(&mut s, "com.x.zh", &entries());
        apply_prompt_pack(&mut s, "com.x.zh", &entries()); // no duplicates
        assert_eq!(s.post_process_prompts.len(), before + 1);
        assert!(s
            .post_process_prompts
            .iter()
            .any(|p| p.id == "ext:com.x.zh:formal"));
    }

    #[test]
    fn remove_clears_entries_and_heals_selection() {
        let mut s = AppSettings::default();
        apply_prompt_pack(&mut s, "com.x.zh", &entries());
        s.post_process_selected_prompt_id = Some("ext:com.x.zh:formal".into());
        remove_prompt_pack(&mut s, "com.x.zh");
        assert!(!s
            .post_process_prompts
            .iter()
            .any(|p| p.id.starts_with("ext:com.x.zh:")));
        // Selection healed to a real prompt, not left dangling.
        let sel = s.post_process_selected_prompt_id.clone();
        assert!(
            sel.is_none()
                || s.post_process_prompts
                    .iter()
                    .any(|p| Some(&p.id) == sel.as_ref())
        );
        assert_ne!(sel.as_deref(), Some("ext:com.x.zh:formal"));
    }
}
