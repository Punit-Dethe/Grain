//! [GRAIN] Host-owned UI surfaces (SPEC §1.2).
//!
//! Extensions **never** create windows. They declare a surface in their
//! manifest; the host builds, places, sleeps and destroys it. Grain's own
//! features use the same machinery, which is the point — a surface an extension
//! opens is the one Grain Space already proved, not a second-class imitation of
//! it.

pub mod extension;
pub mod overlay;
pub mod workspace;
