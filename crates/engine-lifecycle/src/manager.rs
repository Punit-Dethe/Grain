//! The arbiter itself: registration, load admission, idle-TTL eviction.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{EngineMemoryClass, EngineSlot, ManagedEngine};

/// Why a [`LifecycleManager::prepare_load`] was refused. On any error the
/// manager has made no changes (no engine was unloaded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    /// The slot has no registered engine.
    NotRegistered(EngineSlot),
    /// Another heavyweight engine is holding RAM with a live session and cannot
    /// be evicted to make room.
    Blocked { by: EngineSlot },
    /// Even after evicting inactive heavy engines, the projected resident memory
    /// would exceed the configured ceiling.
    OverBudget { projected_mb: u32, ceiling_mb: u32 },
    /// An engine's `unload()` failed during eviction.
    UnloadFailed { slot: EngineSlot, reason: String },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleError::NotRegistered(s) => write!(f, "no engine registered for {s:?}"),
            LifecycleError::Blocked { by } => {
                write!(f, "load blocked: {by:?} has an active session and cannot be evicted")
            }
            LifecycleError::OverBudget {
                projected_mb,
                ceiling_mb,
            } => write!(
                f,
                "load over budget: projected {projected_mb} MB exceeds ceiling {ceiling_mb} MB"
            ),
            LifecycleError::UnloadFailed { slot, reason } => {
                write!(f, "failed to unload {slot:?}: {reason}")
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

/// Arbitrates which local engines may be resident. Holds shared handles to every
/// registered engine and the last-touch time per slot for idle eviction.
pub struct LifecycleManager {
    engines: Mutex<Vec<Arc<dyn ManagedEngine>>>,
    /// Last-use instant per slot, set on a successful load and on [`touch`].
    last_touch: Mutex<HashMap<EngineSlot, Instant>>,
    /// Optional resident-memory ceiling (MB). `None` disables admission control.
    ceiling_mb: Option<u32>,
}

impl LifecycleManager {
    /// Create a manager with an optional resident-memory ceiling (MB).
    pub fn new(ceiling_mb: Option<u32>) -> Self {
        Self {
            engines: Mutex::new(Vec::new()),
            last_touch: Mutex::new(HashMap::new()),
            ceiling_mb,
        }
    }

    /// Register an engine. One engine per slot; re-registering a slot replaces it.
    pub fn register(&self, engine: Arc<dyn ManagedEngine>) {
        let mut engines = self.engines.lock().unwrap();
        let slot = engine.slot();
        engines.retain(|e| e.slot() != slot);
        engines.push(engine);
    }

    fn find(&self, slot: EngineSlot) -> Option<Arc<dyn ManagedEngine>> {
        self.engines
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.slot() == slot)
            .cloned()
    }

    /// Decide whether `slot` may load, and if so evict whatever must go.
    ///
    /// Order of checks (no side effects until all pass):
    ///
    /// 1. the slot is registered,
    /// 2. no *active* heavy engine blocks the load,
    /// 3. the projected memory after evicting inactive heavies fits the ceiling.
    ///
    /// Only then are the inactive heavy engines unloaded and the slot's
    /// last-touch stamped to `now`.
    pub fn prepare_load(&self, slot: EngineSlot, now: Instant) -> Result<(), LifecycleError> {
        let incoming = self.find(slot).ok_or(LifecycleError::NotRegistered(slot))?;
        let engines = self.engines.lock().unwrap();

        // Heavyweight engines that would have to leave to satisfy exclusion.
        let incoming_heavy = incoming.memory_class() == EngineMemoryClass::Heavy;
        let mut to_evict: Vec<Arc<dyn ManagedEngine>> = Vec::new();
        if incoming_heavy {
            for e in engines.iter() {
                if e.slot() == slot || !e.is_loaded() {
                    continue;
                }
                if e.memory_class() == EngineMemoryClass::Heavy {
                    if e.has_active_session() {
                        return Err(LifecycleError::Blocked { by: e.slot() });
                    }
                    to_evict.push(Arc::clone(e));
                }
            }
        }

        // Admission control: sum what STAYS loaded (not evicted, not the incoming
        // slot) plus the incoming engine. Computed before any eviction so a
        // rejection leaves state untouched.
        if let Some(ceiling) = self.ceiling_mb {
            let evict_slots: Vec<EngineSlot> = to_evict.iter().map(|e| e.slot()).collect();
            let surviving: u32 = engines
                .iter()
                .filter(|e| {
                    e.is_loaded() && e.slot() != slot && !evict_slots.contains(&e.slot())
                })
                .map(|e| e.approx_resident_mb())
                .sum();
            let projected = surviving + incoming.approx_resident_mb();
            if projected > ceiling {
                return Err(LifecycleError::OverBudget {
                    projected_mb: projected,
                    ceiling_mb: ceiling,
                });
            }
        }

        // All checks passed — perform the evictions.
        for e in &to_evict {
            e.unload().map_err(|reason| LifecycleError::UnloadFailed {
                slot: e.slot(),
                reason,
            })?;
            self.last_touch.lock().unwrap().remove(&e.slot());
        }
        drop(engines);

        self.last_touch.lock().unwrap().insert(slot, now);
        Ok(())
    }

    /// Record activity on a slot, resetting its idle clock and notifying the
    /// engine. Call on each recording/session touch.
    pub fn touch(&self, slot: EngineSlot, now: Instant) {
        if let Some(e) = self.find(slot) {
            e.touch();
            self.last_touch.lock().unwrap().insert(slot, now);
        }
    }

    /// Unload every loaded engine that is idle (no active session) and has not
    /// been touched within `ttl`. Returns the slots actually unloaded.
    pub fn tick(&self, now: Instant, ttl: Duration) -> Vec<EngineSlot> {
        let mut unloaded = Vec::new();
        let engines = self.engines.lock().unwrap();
        for e in engines.iter() {
            if !e.is_loaded() || e.has_active_session() {
                continue;
            }
            let idle = {
                let lt = self.last_touch.lock().unwrap();
                match lt.get(&e.slot()) {
                    Some(t) => now.saturating_duration_since(*t) >= ttl,
                    // No stamp (never loaded through us) — leave it alone.
                    None => false,
                }
            };
            if idle && e.unload().is_ok() {
                self.last_touch.lock().unwrap().remove(&e.slot());
                unloaded.push(e.slot());
            }
        }
        unloaded
    }

    /// Manually unload all loaded engines without an active session (the
    /// "unload now" affordance). Returns the slots actually unloaded.
    pub fn unload_all_inactive(&self) -> Vec<EngineSlot> {
        let mut unloaded = Vec::new();
        let engines = self.engines.lock().unwrap();
        for e in engines.iter() {
            if e.is_loaded() && !e.has_active_session() && e.unload().is_ok() {
                self.last_touch.lock().unwrap().remove(&e.slot());
                unloaded.push(e.slot());
            }
        }
        unloaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeEngine;

    fn mgr(ceiling: Option<u32>) -> LifecycleManager {
        LifecycleManager::new(ceiling)
    }

    #[test]
    fn loading_heavy_evicts_inactive_heavy() {
        let m = mgr(None);
        let batch = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        let rolling = Arc::new(FakeEngine::heavy(EngineSlot::Rolling, 500));
        m.register(batch.clone());
        m.register(rolling.clone());

        // Batch is resident and idle.
        batch.set_loaded(true);
        // Load Rolling → Batch must be evicted.
        m.prepare_load(EngineSlot::Rolling, Instant::now()).unwrap();
        assert!(!batch.is_loaded(), "inactive heavy Batch should be evicted");
        assert_eq!(batch.unload_calls(), 1);
    }

    #[test]
    fn active_heavy_blocks_and_has_no_side_effects() {
        let m = mgr(None);
        let batch = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        let native = Arc::new(FakeEngine::heavy(EngineSlot::NativeAsr, 500));
        m.register(batch.clone());
        m.register(native.clone());

        batch.set_loaded(true);
        batch.set_active(true); // a live batch session

        let err = m.prepare_load(EngineSlot::NativeAsr, Instant::now()).unwrap_err();
        assert_eq!(err, LifecycleError::Blocked { by: EngineSlot::Batch });
        // No side effects: Batch still loaded, never unloaded.
        assert!(batch.is_loaded());
        assert_eq!(batch.unload_calls(), 0);
    }

    #[test]
    fn light_engine_coexists_with_heavy() {
        let m = mgr(None);
        let vad = Arc::new(FakeEngine::light(EngineSlot::Rolling, 40));
        let heavy = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        m.register(vad.clone());
        m.register(heavy.clone());

        vad.set_loaded(true);
        // Loading a heavy engine must NOT evict a light one.
        m.prepare_load(EngineSlot::Batch, Instant::now()).unwrap();
        assert!(vad.is_loaded(), "light engine should survive a heavy load");
        assert_eq!(vad.unload_calls(), 0);
    }

    #[test]
    fn admission_rejects_over_budget_without_side_effects() {
        // Ceiling 600 MB; a 40 MB light stays, incoming heavy wants 700 MB.
        let m = mgr(Some(600));
        let light = Arc::new(FakeEngine::light(EngineSlot::Rolling, 40));
        let heavy = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 700));
        m.register(light.clone());
        m.register(heavy.clone());
        light.set_loaded(true);

        let err = m.prepare_load(EngineSlot::Batch, Instant::now()).unwrap_err();
        assert_eq!(
            err,
            LifecycleError::OverBudget {
                projected_mb: 740,
                ceiling_mb: 600
            }
        );
        // Nothing changed.
        assert!(light.is_loaded());
    }

    #[test]
    fn admission_passes_after_evicting_heavy() {
        // Ceiling 600. Old heavy 500 (idle) is evicted; new heavy 500 then fits.
        let m = mgr(Some(600));
        let old = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        let new = Arc::new(FakeEngine::heavy(EngineSlot::NativeAsr, 500));
        m.register(old.clone());
        m.register(new.clone());
        old.set_loaded(true);

        m.prepare_load(EngineSlot::NativeAsr, Instant::now()).unwrap();
        assert!(!old.is_loaded(), "old heavy evicted to make room");
    }

    #[test]
    fn tick_unloads_idle_but_keeps_active_and_fresh() {
        let m = mgr(None);
        let idle = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        let active = Arc::new(FakeEngine::heavy(EngineSlot::Rolling, 500));
        let fresh = Arc::new(FakeEngine::heavy(EngineSlot::NativeAsr, 500));
        m.register(idle.clone());
        m.register(active.clone());
        m.register(fresh.clone());

        let t0 = Instant::now();
        idle.set_loaded(true);
        active.set_loaded(true);
        active.set_active(true);
        fresh.set_loaded(true);
        // Stamp all three at t0 via prepare_load bookkeeping would evict; instead
        // touch directly.
        m.touch(EngineSlot::Batch, t0);
        m.touch(EngineSlot::Rolling, t0);
        let later = t0 + Duration::from_secs(30);
        m.touch(EngineSlot::NativeAsr, later); // fresh touched recently

        let ttl = Duration::from_secs(60);
        // Batch idle 70s (> ttl) → unload; NativeAsr idle 40s (< ttl) → survive.
        let now = t0 + Duration::from_secs(70);
        let unloaded = m.tick(now, ttl);

        assert_eq!(unloaded, vec![EngineSlot::Batch]);
        assert!(!idle.is_loaded());
        assert!(active.is_loaded(), "active session must not be unloaded");
        assert!(fresh.is_loaded(), "recently touched engine must survive");
    }

    #[test]
    fn unload_all_inactive_skips_active() {
        let m = mgr(None);
        let a = Arc::new(FakeEngine::heavy(EngineSlot::Batch, 500));
        let b = Arc::new(FakeEngine::heavy(EngineSlot::Rolling, 500));
        m.register(a.clone());
        m.register(b.clone());
        a.set_loaded(true);
        b.set_loaded(true);
        b.set_active(true);

        let unloaded = m.unload_all_inactive();
        assert_eq!(unloaded, vec![EngineSlot::Batch]);
        assert!(!a.is_loaded());
        assert!(b.is_loaded());
    }
}
