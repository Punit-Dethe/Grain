//! [GRAIN] Grain Meet: local meeting recording and processing.
//!
//! Step 1 (this module so far): durable session storage. Capture (Step 4)
//! feeds [`storage::SessionWriter`]; the saved-audio pipeline (Step 3) reads
//! sessions. See `docs/Meet/EXECUTION-PLAN.md`.
// Wired by capture and the saved-audio pipeline; exercised by tests until then.
#![cfg_attr(not(test), allow(dead_code))]

pub mod storage;
mod wav;
