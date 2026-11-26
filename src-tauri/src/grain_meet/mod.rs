//! [GRAIN] Grain Meet: local meeting recording and processing.
//!
//! - `storage`: durable sessions written by meeting capture.
//! - `sources`, `recorder`: Recall's own microphone and system-audio
//!   capture, configured independently of dictation.
//! - `analysis`, `transcript`, `processing`: saved audio to a published,
//!   speaker-attributed transcript revision.
#![cfg_attr(not(test), allow(dead_code))]

pub mod ai;
pub mod analysis;
pub mod commands;
pub mod processing;
pub mod recorder;
pub mod sources;
pub mod storage;
pub mod transcript;
mod wav;
