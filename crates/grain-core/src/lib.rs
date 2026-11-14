//! Grain's headless core: Tauri-free, the shared substrate the daemon runs on
//! and the Tauri settings shell wraps.
//!
//! - [`context::AppContext`] — owned settings (`RwLock`), the event bus, and the
//!   resource/data paths. The headless replacement for `tauri::AppHandle`.
//! - [`event::DaemonEvent`] — the typed broadcast stream.
//! - [`settings`] — the full `AppSettings` schema + the production store.
//!
//! No dependency on Tauri, audio backends, or any ASR engine. The managers
//! (audio/model/transcription/history) migrate onto `AppContext` here over the
//! decoupling phase; until then this crate stands alone and tested.

mod atomic_file;
pub mod context;
// Provisional remote descriptors and host-owned identity, without a transport.
pub mod mcp;
// Host-owned custom connection metadata, separate from credentials/transport.
pub mod mcp_connections;
// The event/action wire types moved to grain-sdk (the dependency leaf);
// this alias keeps every `grain_core::event::X` path compiling unchanged.
pub use grain_sdk as event;
pub mod capture;
pub mod providers;
pub mod settings;
// [GRAIN] The ASR-text substrate (normalise/tokens/same_word/fuzzy_keys) shared
// by every matcher, so the V2 retriever (`capability_index`) matches speech by
// exactly the same rules the index files tokens under — the one place that
// agreement is enforced.
pub mod text;
// [GRAIN] Capability Index V2 — the schema-projection action retriever
// (docs/Extensions 2.0/PLAN.md §6–7). Pure and model-free: builds the hot set
// the Agent selects from, with an eligibility pre-filter and model-free
// exact/lexical provenance.
pub mod capability_index;
// [GRAIN] Capability Index V2 — Agent tool exposure (docs/Extensions 2.0/PLAN.md
// §6.3, §6.4, §7.3). Pure: turns the hot set into model tool definitions with
// prompt-safe sanitisation, the `search_actions` meta-tool, safe tool-name
// encoding, and the bounded cumulative-exposure budget. The host maps `ToolDef`
// onto its transport tool type and runs the loop.
pub mod capability_agent;
// [GRAIN] Capability Index V2 retrieval evaluation (docs/Extensions 2.0/PLAN.md
// §7.5). Drives the real retriever over a held-out corpus, headless. Headline is
// eligible Recall@K + fallback-rate + hot-set size — flood, not miss, is the
// failure.
pub mod capability_eval;
// [GRAIN] The host-owned interaction contract (docs/Extensions 2.0/PLAN.md §9.2,
// Amendments A & B). Structured, data-shaped interactions (confirm/choose/
// request/result/receipt) with a markdown renderer — renderer #1 for the Agent
// chat. The parallel native Dynamic UI is renderer #2 over the same enum, which
// is why extensions return data here, never authored views.
pub mod interaction;
// [GRAIN] Phase 3 execution contract (docs/Extensions 2.0/PLAN.md §8.3/§8.5/§12):
// risk classification (escalate-only floor), the prepared call approved and
// replayed as a unit (idempotency + time-of-use revalidation), and the structured
// outcome — mapped to `interaction` for rendering. Pure; the host executor is
// built on it.
pub mod execution;
// Bounded, offline JSON Schema validation shared by MCP discovery and execution.
pub mod tool_schema;

pub use context::{settings_file_exists, AppContext};
pub use grain_sdk::{DaemonEvent, SessionMode};
pub use settings::{AppSettings, PostProcessProvider, SecretMap};
// [GRAIN] Reach — the Agent's first-party OS tools (docs/Reach/PLAN.md). Pure:
// tool schemas, argument parsing, the link policy and every URL Reach builds.
// The desktop crate owns the OS adapters.
pub mod reach;
// [GRAIN] Reminders (docs/Reminders/PLAN.md). Pure: the record, validation,
// calendar arithmetic, day/time phrases and the Agent tool schemas.
pub mod reminders;
