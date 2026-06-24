//! Pure rolling-window transcription engine.
//!
//! Ported from Grain's Python implementation (`open_voice_router/services`):
//! - [`cursor`]    ← `chunked_audio.py`     (absolute-frame session cursor)
//! - [`assembler`] ← `transcript_merger.py` (timeline-tagged dedup assembler)
//! - [`chunk_pump`]← `chunk_pump.py`         (serialized single-flight dispatch)
//!
//! This crate has **zero** dependency on Tauri, the daemon, audio backends, or
//! any ASR engine. It transforms audio frames + model outputs into assembled
//! text. The Python test suite is ported alongside as the behavioral spec.

pub mod assembler;
pub mod chunk_pump;
pub mod cursor;
pub mod merge;

pub use assembler::{merge_transcript, TimelineAssembler, WordTiming};
pub use chunk_pump::ChunkPump;
pub use cursor::{AudioChunk, RollingWindowConfig, SessionCursor};
