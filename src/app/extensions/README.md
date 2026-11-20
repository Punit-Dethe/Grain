# Extensions

The Extensions page has one search field and three areas:

- **Connected:** the built-ins (Exa web search, Grain Pilot) always first, then
  every connected Composio app. Each has a switch; turning one off keeps its
  account signed in and hides its tools from the Agent. An in-progress turn
  drops those tools on its next model round.
- **Browse apps:** Composio's toolkit directory. Connect opens Composio's
  managed sign-in in the browser and returns to Grain when it completes.
- **Custom MCP servers:** remote MCP servers the user adds themselves.

## Composio

Each user brings their own Composio project API key (`ak_…`, from the
developer platform, not the "For You" app). Grain keeps it in the OS
credential store and sends it only to Composio. Apps use Composio-managed
auth, so there is nothing to register per service. The backend lives in
`src-tauri/src/grain_mcp_composio.rs`; connected toolkits reach the Agent as
MCP-shaped providers (`composio-<slug>`), so the Agent's discovery and
execution paths are shared with custom MCP servers.

## Built-ins

Exa and Grain Pilot are defined in the Rust catalog in
`src-tauri/src/grain_mcp.rs`. The bundled Exa mark is Exa’s GitHub avatar;
the Grain Pilot mark is Grain’s own. The app marks in the Composio setup
illustration come from [Simple Icons](https://simpleicons.org/) (CC0).
