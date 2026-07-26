//! [GRAIN] Grain Space over MCP — the parts worth testing on their own.
//!
//! The binary is a thin `main` over this: `AppLink` is the half that talks to
//! the running app, and its handshake is only exercised when something is
//! actually listening, so it lives here where an integration test can drive it.

mod app;

pub use app::AppLink;
