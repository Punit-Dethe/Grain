//! [GRAIN] Capability Index V2 — host wiring for the Agent tool loop
//! (`docs/Extensions 2.0/PLAN.md` §7–§8).
//!
//! The bridge between the pure retriever/exposure layer in `grain-core` and the
//! Agent's live tool loop (`agent::run_with_note_tools`). It:
//!
//! - retrieves the hot set for a request and projects it — plus the
//!   always-present `search_actions` meta-tool — into `llm_client::ToolSpec`s
//!   under a bounded [`ToolExposure`];
//! - dispatches the capability tool calls the model makes: `search_actions` runs
//!   real retrieval and widens the exposed set; an `act__…` call is a *discovered*
//!   installed-extension action.
//!
//! ## The Phase-2 execution boundary
//!
//! Executing a third-party action safely — prepared calls, host risk policy,
//! confirmation, the extension worker — is Phase 3. Until it lands, calling a
//! discovered action returns an honest "found, not yet runnable" result. It is
//! never reported as a success: the model is told, in the tool result itself, not
//! to claim it ran. This is the same discipline as the provider tool-capability
//! gate (a provider without native tool calls simply never calls one, so nothing
//! executes and nothing false is claimed).
//!
//! Grain Space (built-in) actions keep executing through their own tools; a turn
//! on a machine with no installed action extensions exposes nothing here and is
//! byte-for-byte the old behaviour.

use crate::llm_client::{ToolCallOut, ToolSpec};
use grain_core::capability_agent::{
    self, search_actions_tool_def, tool_name, ToolBudget, ToolExposure, SEARCH_ACTIONS,
};

/// Hot-set size retrieved for the initial exposure. Matches the benchmark's K.
const HOT_SET_K: usize = 8;

/// `search_actions` result size — enough to choose from, few enough not to flood.
const SEARCH_LIMIT: usize = 8;

fn to_spec(def: capability_agent::ToolDef) -> ToolSpec {
    ToolSpec {
        name: def.name,
        description: def.description,
        parameters: def.parameters,
    }
}

/// Open the capability surface for one request: retrieve the hot set, seed the
/// exposure, and return the current tool specs. Returns an empty spec list (and a
/// fresh, empty exposure) when no installed extension declares an action — the
/// caller then behaves exactly as before.
pub fn open(request: &str) -> (ToolExposure, Vec<ToolSpec>) {
    let mut exposure = ToolExposure::new(ToolBudget::default());
    if !crate::extension_host::has_capability_actions() {
        return (exposure, Vec::new());
    }
    let hot = crate::extension_host::capability_retrieve(request, HOT_SET_K);
    exposure.expose(&hot);
    let specs = specs(&exposure);
    (exposure, specs)
}

/// The capability tool specs implied by the current exposure: one per exposed
/// action, plus `search_actions` while the hop budget remains. Called after every
/// dispatch round because `search_actions` may have widened the exposed set.
pub fn specs(exposure: &ToolExposure) -> Vec<ToolSpec> {
    let ids = exposure.exposed_canonical_ids();
    let mut specs: Vec<ToolSpec> = crate::extension_host::capability_tool_defs(&ids)
        .into_iter()
        .map(to_spec)
        .collect();
    if exposure.can_search() {
        specs.push(to_spec(search_actions_tool_def()));
    }
    specs
}

/// Dispatch one tool call if it belongs to the capability surface. Returns
/// `Some(result_text)` when handled, `None` when the call is not ours (the caller
/// routes it elsewhere — e.g. the Grain Space note tools).
pub fn dispatch(call: &ToolCallOut, exposure: &mut ToolExposure) -> Option<String> {
    if call.name == SEARCH_ACTIONS {
        return Some(handle_search(call, exposure));
    }
    // An action tool resolves through the session's exposure map — the
    // authoritative reverse of `tool_name`. An undisclosed or invented name
    // resolves to nothing here and falls through to the caller (which will report
    // "no such tool"): discovery is not authorisation.
    let canonical = exposure.resolve(&call.name)?;
    Some(handle_action_call(&canonical))
}

fn handle_search(call: &ToolCallOut, exposure: &mut ToolExposure) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);
    let str_arg = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let Some(query) = str_arg("query") else {
        return "search_actions needs a query.".to_string();
    };
    if !exposure.can_search() {
        return "You have reached the action-search limit for this turn. Choose from the actions \
                already available, or answer the user directly."
            .to_string();
    }
    let extension = str_arg("extension");
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|n| (n as usize).clamp(1, SEARCH_LIMIT))
        .unwrap_or(SEARCH_LIMIT);

    let hot = crate::extension_host::capability_search(&query, extension.as_deref(), limit);
    exposure.note_search_hop();
    exposure.expose(&hot);

    if hot.entries.is_empty() {
        return format!("No installed actions match \"{query}\".");
    }
    let mut out = String::from("Found actions — call one by its tool name:\n");
    for entry in &hot.entries {
        out.push_str(&format!(
            "- {} — {}\n",
            tool_name(&entry.canonical_id),
            if entry.title.is_empty() {
                entry.canonical_id.as_str()
            } else {
                entry.title.as_str()
            }
        ));
    }
    out
}

/// A discovered third-party action was called. Phase 2 stops at discovery, so
/// this is deliberately honest and never a false success (§ execution boundary
/// above). Phase 3 replaces this with prepared-call preparation, host risk
/// policy, confirmation, and extension-worker execution.
fn handle_action_call(canonical_id: &str) -> String {
    format!(
        "The action \"{canonical_id}\" was found and matches the request, but running \
         installed-extension actions is not enabled in this build yet. Tell the user you found \
         the right action and what it would do — do not claim you performed it."
    )
}
