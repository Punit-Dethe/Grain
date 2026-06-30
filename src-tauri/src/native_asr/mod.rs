//! [GRAIN] Native ASR subsystem (the third engine path beside Batch & Rolling).
//!
//! * [`input`]   — the capture-thread fan-out sink (pre-roll + bounded queue).
//! * [`worker`]  — the session driver + `AsrEvent` → `DaemonEvent` bridge.
//! * [`manager`] — owns the single live session and its worker thread.
//!
//! The pure protocol, stabilizer, and registry live in `grain-asr-core`; this
//! module is the Tauri-side glue that connects the audio recorder, the backend,
//! the stabilizer, and the pill event bus.

mod input;
mod manager;
mod worker;

pub use input::NativeAsrInput;
pub use manager::NativeAsrManager;
