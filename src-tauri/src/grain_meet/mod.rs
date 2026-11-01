//! [GRAIN] Grain Meet: local meeting recording and processing.
//!
//! - `storage`: durable sessions (Step 1). Capture (Step 4) feeds it.
//! - `analysis`, `transcript`, `processing`: saved audio to a published,
//!   speaker-attributed transcript revision (Step 3).
//!
//! See `docs/Meet/EXECUTION-PLAN.md`.
// Wired by capture and the Meet commands; exercised by tests until then.
#![cfg_attr(not(test), allow(dead_code))]

pub mod analysis;
pub mod processing;
pub mod storage;
pub mod transcript;
mod wav;
