# Connect an MCP server to Grain

Grain bundles GitHub and Notion presets in **Extensions → Browse extensions**.
For another provider, run a remote Streamable HTTP MCP server and use
**Extensions → My connections → Add MCP**. Enter its HTTPS endpoint and
authentication mode. Connect the account if required, then enable the
connection and check tool discovery.

Maintainers add bundled presets in `src-tauri/src/grain_mcp.rs`. See the
[retirement log](../Extensions%202.0/STORE-RETIREMENT.md) and
[contributor guide](../../CONTRIBUTING.md#mcp-connections) for the current
contract and verification commands.
