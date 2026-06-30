//! [GRAIN] Pure lifecycle arbiter for local heavyweight engines.
//!
//! Grain runs three local engine paths — Batch
//! (`src-tauri/src/managers/transcription.rs`), Rolling (`src-tauri/src/rolling.rs`),
//! and Native ASR (`crates/grain-asr-core`) — but the low-RAM, "destroy if not
//! in use" rule means **at most one heavyweight model is resident at a time**.
//! Today that exclusion is wired pairwise between Batch and Rolling; this crate
//! replaces it with one policy that all three (and any future engine) register
//! with.
//!
//! It is deliberately Tauri-free and I/O-free so the policy is unit-testable
//! without the Tauri build. The concrete engines live in `src-tauri` and
//! implement [`ManagedEngine`]; this crate only decides *what* to load, evict,
//! or keep — never *how*.
//!
//! Three rules, in order:
//!
//! 1. **Active-session protection** — an engine with a live session is never
//!    evicted, and a load that would require evicting one is blocked.
//! 2. **Heavyweight mutual exclusion** — loading an [`EngineMemoryClass::Heavy`]
//!    engine first unloads every other inactive heavy engine.
//! 3. **Admission control** — the projected resident memory after eviction must
//!    fit an optional ceiling, else the load is refused.
//!
//! [`LifecycleManager::prepare_load`] has **no side effects on error**: if a
//! load is blocked or over budget, nothing is unloaded.

mod manager;
pub mod testing;

pub use manager::{LifecycleError, LifecycleManager};

/// The three local engine paths. Native ASR is the new third slot beside the
/// existing Batch and Rolling paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineSlot {
    Batch,
    Rolling,
    NativeAsr,
}

/// How costly an engine is to keep resident. Only [`Heavy`](Self::Heavy) engines
/// are mutually exclusive; [`Light`](Self::Light) engines (e.g. a small VAD) may
/// coexist with a heavy one and with each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineMemoryClass {
    Light,
    Heavy,
}

/// A local engine the [`LifecycleManager`] can arbitrate over.
///
/// `Send + Sync` because the manager is shared across threads (hotkey thread,
/// audio worker, idle watcher). All methods take `&self`; implementors use
/// interior mutability (atomics / a mutex) — the manager never needs `&mut`.
pub trait ManagedEngine: Send + Sync {
    /// Which slot this engine occupies (unique per registered engine).
    fn slot(&self) -> EngineSlot;
    /// Whether the model is currently resident in memory.
    fn is_loaded(&self) -> bool;
    /// Whether a live recognition session is in progress (un-evictable).
    fn has_active_session(&self) -> bool;
    /// Mark recent use, resetting the idle-unload clock (engines may also use
    /// this internally; the manager additionally tracks last-touch for TTL).
    fn touch(&self);
    /// Drop the resident model now. Must be a no-op when already unloaded, and
    /// must never be called by the manager while a session is active.
    fn unload(&self) -> Result<(), String>;
    /// Coarse memory class driving mutual exclusion.
    fn memory_class(&self) -> EngineMemoryClass;
    /// Approximate resident megabytes when loaded, for admission control.
    fn approx_resident_mb(&self) -> u32;
}
