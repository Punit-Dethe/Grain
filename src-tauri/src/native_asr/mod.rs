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

/// [GRAIN] Initialize the transcribe-cpp native backend ONCE at startup: route
/// native + ggml diagnostics into `log`, and register the compute backend
/// modules. With `dynamic-backends` (Windows/Linux) this dlopen's the per-ISA
/// CPU + Vulkan ggml DLLs sitting next to the exe; on the static macOS Metal
/// build it's a harmless no-op. **Must run before the first `Model::load`**, or
/// the engine registers zero compute devices and every model load fails.
pub fn init_transcribe_backend() {
    transcribe_cpp::init_logging();
    match transcribe_cpp::init_backends_default() {
        Ok(()) => {
            let devices = transcribe_cpp::devices();
            log::info!(
                "[GRAIN] transcribe-cpp initialized with {} compute device(s): [{}]",
                devices.len(),
                devices
                    .iter()
                    .map(|d| format!("{} ({})", d.name, d.kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Err(e) => log::warn!("[GRAIN] failed to initialize transcribe-cpp backends: {e}"),
    }
}
