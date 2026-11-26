# Grain architecture

## App boundaries

Grain is a Tauri desktop app. The React UI in [`src/app`](../src/app) calls Rust through the generated Tauri commands in [`bindings.ts`](../src/app/bindings.ts) and subscribes to typed events for state changes. Rust owns settings, shortcuts, capture, model lifetimes, credentials, and tool execution. Rust manages the lifetime of each operation independently of its React component.

Grain builds on Handy's shared backend in [`src-tauri/src/handy`](../src-tauri/src/handy). [`lib.rs`](../src-tauri/src/lib.rs) wires it to Grain's modules. Add Grain-specific behavior in those modules or in [`crates`](../crates). Review upstream frontend changes for behavioral fixes and port applicable changes into Grain's owned code. The [upstream runbook](../Upstream/UPSTREAM.md) covers integration and review.

[`grain-core`](../crates/grain-core) owns the application context, settings schema, and pure policy. [`grain-sdk`](../crates/grain-sdk) defines shared wire types. Keeping them independent of Tauri and inference lets tests exercise routing, settings, and protocol rules without opening the app or loading a model.

## Dictation and capture

The coordinator resolves the shortcut and selected model at capture start and retains the action through stop or push-to-talk release. Model capabilities determine standard batch decoding or streaming. Only reviewed artifacts are eligible for Flow. The eligibility rules live in [`grain-core/src/capture.rs`](../crates/grain-core/src/capture.rs), with host checks in [`grain_dictation_routing.rs`](../src-tauri/src/grain_dictation_routing.rs).

Flow decodes stable windows as audio arrives. A temporary PCM journal lets a serial worker reread ranges into a reusable buffer, which bounds audio memory as the recording grows. The journal is removed when its session owners release it. [`rolling.rs`](../src-tauri/src/rolling.rs) manages that lifecycle, and [`tdt_flow.rs`](../src-tauri/src/tdt_flow.rs) selects the window and token policy. The [native fork contract](TRANSCRIBE-CPP-FORK.md) explains the inference boundary and model-specific policies.

Speech recognition runs locally. After decoding, optional AI cleanup can use a configured text provider. [`context_detect.rs`](../src-tauri/src/context_detect.rs) chooses applicable context at finalization, and [`prompt_stack.rs`](../src-tauri/src/prompt_stack.rs) composes the prompt layers. That work stays off the rolling audio path.

## Recording and Agent surfaces

The shared [overlay backend](../src-tauri/src/handy/overlay.rs) owns native window lifecycle and placement. Grain's [capture adapter](../src-tauri/src/grain_overlay.rs) supplies session state and events to the [recording UI](../src/app/overlay). Native placement and input behavior stay in Rust so the WebView can focus on presentation. The [overlay maintenance contract](../Upstream/OVERLAY-MAINTENANCE.md) describes the integration points.

[`agent.rs`](../src-tauri/src/agent.rs) owns Agent turns and tool dispatch. [`grain_mcp.rs`](../src-tauri/src/grain_mcp.rs) and its adapters expose configured connections and built-ins through a shared tool interface. The host controls which tools are available and validates calls. The UI renders chat replies and supported interactive views through its own components. See the [extensions guide](../src/app/extensions/README.md) and the [real-app acceptance harness](../tests/agent-harness/README.md).

## Recall

Recall owns meeting microphone and system-audio capture independently of dictation. [`grain_meet`](../src-tauri/src/grain_meet) stores recordings and publishes speaker-attributed transcript revisions after local processing. [`grain-recall-mcp`](../crates/grain-recall-mcp) provides optional read access to saved meetings over stdio. The AI client launches the helper on demand and the Grain setting controls its access.
