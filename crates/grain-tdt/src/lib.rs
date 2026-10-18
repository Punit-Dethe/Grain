//! Parakeet TDT v2/v3 geometry and token merging for Grain Flow.
//!
//! Adapted from FluidAudio (Apache-2.0); see NOTICE and LICENSE.
//! No audio, native model, threads, or app lifecycle is owned here. In particular,
//! Fluid-derived v3 layout/merging and NVIDIA-derived v2 buffered planning
//! are separate policies. The adapter selects the reviewed model's decoder.

mod buffered;
mod layout;
mod merge;

pub use buffered::{
    BufferedCursor, BUFFERED_LEFT_FRAMES, BUFFERED_MAX_SAMPLES, BUFFERED_OWNED_FRAMES,
    BUFFERED_RIGHT_FRAMES,
};
pub use layout::{
    Window, WindowCursor, CONTENT_SAMPLES, FRAME_SAMPLES, MAX_MODEL_SAMPLES, OVERLAP_SAMPLES,
    SAMPLE_RATE, STRIDE_SAMPLES,
};
pub use merge::{merge_tokens, Token};
