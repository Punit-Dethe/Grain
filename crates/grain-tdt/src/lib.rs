//! Parakeet TDT v2/v3 geometry and token merging for Grain Flow.
//!
//! Adapted from FluidAudio (Apache-2.0); see NOTICE and LICENSE.
//! No audio, native model, threads, or app lifecycle is owned here. In particular,
//! these functions do not turn an ordinary native run into Fluid's TDT decoder:
//! the adapter must implement the separate context/valid-frame/tail contract.

mod layout;
mod merge;

pub use layout::{
    Window, WindowCursor, CONTENT_SAMPLES, FRAME_SAMPLES, MAX_MODEL_SAMPLES, OVERLAP_SAMPLES,
    SAMPLE_RATE, STRIDE_SAMPLES,
};
pub use merge::{merge_tokens, Token};
