//! Parakeet TDT v2/v3 geometry and token merging for Grain Flow.
//!
//! Adapted from FluidAudio (Apache-2.0); see NOTICE and LICENSE.
//! No audio, native model, threads, or app lifecycle is owned here. In particular,
//! Fluid-derived layout/merging and Grain-owned ordinary-native window planning
//! are separate policies. The adapter selects the reviewed model's decoder.

mod layout;
mod merge;
mod native_layout;

pub use layout::{
    Window, WindowCursor, CONTENT_SAMPLES, FRAME_SAMPLES, MAX_MODEL_SAMPLES, OVERLAP_SAMPLES,
    SAMPLE_RATE, STRIDE_SAMPLES,
};
pub use merge::{merge_tokens, Token};
pub use native_layout::{NativeWindowCursor, NATIVE_MAX_SAMPLES};
