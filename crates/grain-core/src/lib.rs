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

pub mod context;
pub mod extensions;
// The event/action wire types moved to grain-sdk (the dependency leaf);
// this alias keeps every `grain_core::event::X` path compiling unchanged.
pub use grain_sdk as event;
pub mod capture;
pub mod settings;
// [GRAIN] Phase 5A: pinned-key verification of the signed extension catalogue.
pub mod trust;
// [GRAIN] Phase 5A: pack format v2 detection + path-safe archive extraction.
pub mod pack;
// [GRAIN] Phase 5A: install/update/remove transaction + the trust invariant.
pub mod install;
// [GRAIN] The ASR-text substrate (normalise/tokens/same_word/fuzzy_keys) shared
// by every matcher. Extracted from `action_router` so the V2 retriever
// (`capability_index`) matches speech by exactly the same rules the index files
// tokens under — the one place that agreement is enforced.
pub mod text;
// [GRAIN] Lexical matching, Tier L (docs/Extensions V1/PLAN.md §4). Pure
// functions over declared text: no model, no state, nothing held between
// invocations — so the eval harness can drive it without a running app.
// Under V1 its job is name/alias detection, not topical ranking.
pub mod action_router;
// [GRAIN] Capability Index V2 — the schema-projection action retriever
// (docs/Extensions 2.0/PLAN.md §6–7). Pure and model-free: builds the hot set
// the Agent selects from, with an eligibility pre-filter, exact/lexical/dense
// provenance tiers, and host-injected dense scores fused by rank. The embedder
// stays host-side, exactly as in `recommend`.
pub mod capability_index;
// [GRAIN] Recommendation ranking (docs/Extensions V1/PLAN.md §3.1). Which
// searchable extension should be offered a request. Pure and model-free: the
// host injects semantic scores, so grain-core carries no embedder.
pub mod recommend;
// [GRAIN] Cross-extension recommendation evaluation. Separate from `eval`,
// which measures one extension's private command matching; this drives the live
// recommendation ranker over a whole installed pool, including no-match cases.
pub mod recommend_eval;
// [GRAIN] The `match.*` primitives an extension calls to rank its own commands
// (docs/Extensions V1/PLAN.md §4). Pure: lexical ranking + a confidence policy.
// `match.semantic` needs the embedder and stays host-side.
pub mod matching;
// [GRAIN] The author-facing eval core (docs/Extensions V1/PLAN.md §10 P3).
// Pure: accuracy, confusion matrix, and an operating-point sweep over a labelled
// test set. The headless `grain-ext eval` subcommand feeds it the real embedder.
pub mod eval;

pub use context::{settings_file_exists, AppContext};
pub use grain_sdk::{AgentInputKind, DaemonEvent, PillAction, RecommendCandidate, SessionMode};
pub use settings::{
    AppSettings, PostProcessProvider, SecretMap, SttProvider, SttProviderKind,
    STT_LOCAL_PROVIDER_ID,
};
