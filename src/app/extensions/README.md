# Bundled MCP catalog

Extensions is one searchable list with connected presets first, custom MCP
connections next, and available presets below. Custom connectors are added
inline. Turning a preset off keeps its account signed in and removes its tools
from Agent discovery and execution. An in-progress turn drops those tools on
its next model round. Disconnect removes the saved account.

Keep the first 30–40 presets in the Grain repository and ship them with the
desktop app. The Rust catalog owns the provider name, description, endpoint,
authentication mode and release eligibility. Set `bundled_preset` on a reviewed
catalog entry to include it in Extensions. The UI uses small provider marks
where bundled and a generic icon otherwise.
The catalog renders without fetching catalog data from the network.

Google Workspace will use one preset per app. Google Calendar is the first
preview preset and only appears in builds with the Google desktop OAuth
registration. It supports event lookup and availability with read-only scopes.
Gmail, Drive, Docs and Sheets remain separate follow-ups.

Publishing another preset requires a Grain release. The review for a new
preset includes sign-in on a release build and tool discovery.
Account credentials remain in the existing runtime credential storage.

Revisit remote delivery if catalog changes need to ship faster than app
updates. A future Grain-owned feed can serve versioned presentation metadata
with a bundled fallback. Endpoint and authentication definitions
stay in the application so a catalog update cannot change where credentials
are sent.

Bundled marks are sourced from [GitHub’s Primer Octicons](https://github.com/primer/octicons/blob/main/icons/mark-github-24.svg),
[Google’s Calendar icon](https://www.gstatic.com/images/branding/product/1x/calendar_48dp.png),
[Linear’s Brand Guidelines](https://linear.app/brand), and the
[Notion website favicon](https://www.notion.com/front-static/favicon.ico).
