//! A thread-safe fake [`ManagedEngine`] for exercising the arbiter (and, later,
//! the `src-tauri` wiring) without real models.
//!
//! Compiled into the normal crate (not `#[cfg(test)]`) so the engine-lifecycle
//! integration in `src-tauri` can register fakes in its own tests.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::{EngineMemoryClass, EngineSlot, ManagedEngine};

/// A scriptable fake engine. Toggle `loaded`/`active` from a test, then observe
/// `unload_calls` and `touch_calls` to assert the arbiter's decisions.
pub struct FakeEngine {
    slot: EngineSlot,
    class: EngineMemoryClass,
    mb: u32,
    loaded: AtomicBool,
    active: AtomicBool,
    unload_calls: AtomicUsize,
    touch_calls: AtomicUsize,
    /// When set, `unload()` returns `Err` (simulates a stuck engine).
    fail_unload: AtomicBool,
}

impl FakeEngine {
    pub fn new(slot: EngineSlot, class: EngineMemoryClass, mb: u32) -> Self {
        Self {
            slot,
            class,
            mb,
            loaded: AtomicBool::new(false),
            active: AtomicBool::new(false),
            unload_calls: AtomicUsize::new(0),
            touch_calls: AtomicUsize::new(0),
            fail_unload: AtomicBool::new(false),
        }
    }

    pub fn heavy(slot: EngineSlot, mb: u32) -> Self {
        Self::new(slot, EngineMemoryClass::Heavy, mb)
    }

    pub fn light(slot: EngineSlot, mb: u32) -> Self {
        Self::new(slot, EngineMemoryClass::Light, mb)
    }

    pub fn set_loaded(&self, v: bool) {
        self.loaded.store(v, Ordering::SeqCst);
    }

    pub fn set_active(&self, v: bool) {
        self.active.store(v, Ordering::SeqCst);
    }

    pub fn set_fail_unload(&self, v: bool) {
        self.fail_unload.store(v, Ordering::SeqCst);
    }

    pub fn unload_calls(&self) -> usize {
        self.unload_calls.load(Ordering::SeqCst)
    }

    pub fn touch_calls(&self) -> usize {
        self.touch_calls.load(Ordering::SeqCst)
    }
}

impl ManagedEngine for FakeEngine {
    fn slot(&self) -> EngineSlot {
        self.slot
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }

    fn has_active_session(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn touch(&self) {
        self.touch_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn unload(&self) -> Result<(), String> {
        self.unload_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_unload.load(Ordering::SeqCst) {
            return Err("fake: unload failed".into());
        }
        self.loaded.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn memory_class(&self) -> EngineMemoryClass {
        self.class
    }

    fn approx_resident_mb(&self) -> u32 {
        self.mb
    }
}
