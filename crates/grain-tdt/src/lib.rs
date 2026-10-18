//! Bounded, stateful Parakeet TDT v2/v3 planning for Grain Flow.
//! No audio, native model, threads or app lifecycle is owned here.

mod buffered;

pub use buffered::{
    BufferedCursor, BUFFERED_LEFT_FRAMES, BUFFERED_MAX_SAMPLES, BUFFERED_OWNED_FRAMES,
    BUFFERED_RIGHT_FRAMES,
};

pub const SAMPLE_RATE: u64 = 16_000;
pub const FRAME_SAMPLES: u64 = 1_280;

/// One bounded input request. All sample coordinates refer to the journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Window {
    pub chunk_start: u64,
    pub read_start: u64,
    pub read_end: u64,
    pub is_last: bool,
}

impl Window {
    pub fn input_samples(self) -> usize {
        (self.read_end - self.read_start) as usize
    }

    pub fn context_frames(self) -> usize {
        ((self.chunk_start - self.read_start) / FRAME_SAMPLES) as usize
    }
}

/// Append owned IDs in decode order. Detokenize the complete sequence so v3
/// byte-fallback tokens can span runs without losing Unicode text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Token {
    pub id: i32,
    pub frame: u64,
    pub confidence: f32,
    pub duration: u32,
}
