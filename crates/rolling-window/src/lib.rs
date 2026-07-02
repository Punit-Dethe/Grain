//! Pure rolling-window transcription engine.
//!
//! Origins (Grain's Python implementation, `open_voice_router/services`):
//! - [`cursor`]    ← `chunked_audio.py`     (absolute-frame session cursor)
//! - [`assembler`] ← `transcript_merger.py` (timeline-tagged dedup assembler)
//!
//! This crate has **zero** dependency on Tauri, the daemon, audio backends, or
//! any ASR engine. It transforms audio frames + model outputs into assembled
//! text.

pub mod assembler;
pub mod cursor;
pub mod merge;

pub use assembler::{merge_transcript, TimelineAssembler, WordTiming};
pub use cursor::{AudioChunk, RollingWindowConfig, SessionCursor};
pub use merge::seam_overlap_len;
