# Bundled MCP catalog

Integrations is one searchable list with connected presets first, custom MCP
connections next, and available presets below. Custom connectors are added
inline. Manage connection expands the less common controls in place.

Keep the first 30–40 presets in the Grain repository and ship them with the
desktop app. The Rust catalog owns the provider name, description, endpoint,
authentication mode and release eligibility. The UI uses small provider marks.
The catalog renders without fetching catalog data from the network.

Publishing another preset requires a Grain release. The review for a new
preset includes sign-in on a release build and tool discovery.
Account credentials remain in the existing runtime credential storage.
The Integrations page reads account names through fixed, provider-specific MCP
identity tools. Only the display label reaches the renderer. It is kept for the
open page, refreshed on reconnect, and cleared on disconnect. Providers without
a recognized identity result show their connection status.

Revisit remote delivery if catalog changes need to ship faster than app
updates. A future Grain-owned feed can serve versioned presentation metadata
with a bundled fallback. Endpoint and authentication definitions
stay in the application so a catalog update cannot change where credentials
are sent.
