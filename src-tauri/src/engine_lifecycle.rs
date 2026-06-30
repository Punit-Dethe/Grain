//! [GRAIN] M2 wiring: `ManagedEngine` adapters over the live managers + builder.
//!
//! The pure arbiter lives in `crates/engine-lifecycle` (imported here as
//! `engine_lifecycle_core`). This module only adapts Grain's existing Batch
//! (`TranscriptionManager`) and Rolling (`RollingTranscriber`) managers to its
//! `ManagedEngine` trait, plus a Native ASR placeholder, and assembles the
//! shared `LifecycleManager` that both load paths consult for mutual exclusion.
//!
//! Idle-unload TTL stays owned by each manager's own watcher; the arbiter is
//! consulted only at load time (for mutual exclusion and future admission
//! control), so `touch()` here is a no-op and `tick()` is never driven for these
//! engines. Each adapter is a live VIEW over its manager — it duplicates no
//! state, so `is_loaded`/`has_active_session` can never drift.

use std::sync::Arc;

use engine_lifecycle_core::{EngineMemoryClass, EngineSlot, LifecycleManager, ManagedEngine};

use crate::managers::transcription::TranscriptionManager;
use crate::native_asr::NativeAsrManager;
use crate::rolling::RollingTranscriber;

// Rough resident footprints (MB) for admission control. Admission is inert until
// a ceiling is configured; these are conservative placeholders pending a real
// per-model estimate (TODO: derive from the loaded model's size).
const BATCH_MB: u32 = 700;
const ROLLING_MB: u32 = 700;
const NATIVE_MB: u32 = 350;

/// Batch (`TranscriptionManager`) as a managed engine.
struct BatchEngine(Arc<TranscriptionManager>);

impl ManagedEngine for BatchEngine {
    fn slot(&self) -> EngineSlot {
        EngineSlot::Batch
    }
    fn is_loaded(&self) -> bool {
        self.0.is_model_loaded()
    }
    fn has_active_session(&self) -> bool {
        // Busy = a load is in flight or a transcription is running; either way
        // the engine must not be evicted mid-work.
        self.0.is_busy()
    }
    fn touch(&self) {}
    fn unload(&self) -> Result<(), String> {
        self.0.unload_model().map_err(|e| e.to_string())
    }
    fn memory_class(&self) -> EngineMemoryClass {
        EngineMemoryClass::Heavy
    }
    fn approx_resident_mb(&self) -> u32 {
        BATCH_MB
    }
}

/// Rolling (`RollingTranscriber`) as a managed engine. Maps 1:1 — the manager
/// already exposes `is_loaded`/`has_active_session`/`unload`.
struct RollingEngine(Arc<RollingTranscriber>);

impl ManagedEngine for RollingEngine {
    fn slot(&self) -> EngineSlot {
        EngineSlot::Rolling
    }
    fn is_loaded(&self) -> bool {
        self.0.is_loaded()
    }
    fn has_active_session(&self) -> bool {
        self.0.has_active_session()
    }
    fn touch(&self) {}
    fn unload(&self) -> Result<(), String> {
        self.0.unload();
        Ok(())
    }
    fn memory_class(&self) -> EngineMemoryClass {
        EngineMemoryClass::Heavy
    }
    fn approx_resident_mb(&self) -> u32 {
        ROLLING_MB
    }
}

/// Native ASR (`NativeAsrManager`) as a managed engine.
///
/// The Native ASR model is resident only for the duration of a session
/// (loaded at session start, unloaded at session end), so `is_loaded` and
/// `has_active_session` both track `is_running()`. This is what lets the arbiter
/// enforce ≤1 heavyweight engine across Batch/Rolling/Native: while a Native
/// session runs, it reports loaded+active, so Batch/Rolling loads are blocked;
/// when idle it reports nothing resident.
struct NativeAsrEngine(Arc<NativeAsrManager>);

impl ManagedEngine for NativeAsrEngine {
    fn slot(&self) -> EngineSlot {
        EngineSlot::NativeAsr
    }
    fn is_loaded(&self) -> bool {
        self.0.is_running()
    }
    fn has_active_session(&self) -> bool {
        self.0.is_running()
    }
    fn touch(&self) {}
    fn unload(&self) -> Result<(), String> {
        // A Native model is only resident while a session is active, and active
        // sessions are never evicted, so the arbiter never calls this on a live
        // session. Stopping here is a safe no-op in the idle case.
        self.0.stop();
        Ok(())
    }
    fn memory_class(&self) -> EngineMemoryClass {
        EngineMemoryClass::Heavy
    }
    fn approx_resident_mb(&self) -> u32 {
        NATIVE_MB
    }
}

/// Build the shared arbiter with Batch, Rolling, and the Native ASR placeholder
/// registered. `ceiling_mb = None` keeps admission control inert (today's
/// behavior); set it to enforce a resident-memory budget.
pub fn build_manager(
    tm: Arc<TranscriptionManager>,
    rt: Arc<RollingTranscriber>,
    native: Arc<NativeAsrManager>,
    ceiling_mb: Option<u32>,
) -> LifecycleManager {
    let m = LifecycleManager::new(ceiling_mb);
    m.register(Arc::new(BatchEngine(tm)));
    m.register(Arc::new(RollingEngine(rt)));
    m.register(Arc::new(NativeAsrEngine(native)));
    m
}
