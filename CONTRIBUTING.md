# Contributing to Grain

## Development setup

Follow the [build guide](BUILD.md) for platform prerequisites, then run these
commands from the repository root.

```sh
bun install
bun run tauri dev
```

The [architecture guide](docs/ARCHITECTURE.md) describes the Rust, React, and
native inference boundaries. Use the generated Tauri bindings for frontend
commands and events. Follow the [upstream runbook](Upstream/UPSTREAM.md) for
Handy integration.

## Verification

Run the tests that cover your changes. Use `bun run test:unit` for frontend
unit tests and the [documented Cargo commands](BUILD.md#windows-backend-tests)
for desktop library tests on Windows. For settings changes, run
`python scripts/ui_parity.py`. Test relevant app behavior in the real Tauri app.
The [Agent acceptance harness](tests/agent-harness/README.md) documents the
isolated application profile and scenario commands.

## MCP connections

The [extensions guide](src/app/extensions/README.md) explains built-ins,
Composio, and custom MCP connections. Bundled providers are defined in
`src-tauri/src/grain_mcp.rs`. Verify discovery and execution with the affected
frontend tests and `cargo test --locked --lib grain_mcp::` from `src-tauri/`.
On Windows, pass the Cargo runner configuration from the build guide.

## Issues and pull requests

Search existing [issues](https://github.com/Punit-Dethe/Grain/issues) and
[pull requests](https://github.com/Punit-Dethe/Grain/pulls) before filing a
report or proposing a change. For a bug, include the Grain version, OS,
reproduction steps, expected and actual behavior, and relevant logs with
private information removed.

Keep pull requests focused. Describe the problem, why the change is useful,
and how you tested it. Link related issues and include screenshots when visuals
change. If you used AI assistance, describe its role in the PR.

Contributions are licensed under the [MIT License](LICENSE).
