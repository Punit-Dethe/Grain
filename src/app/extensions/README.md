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

### Admission rule

Presets are remote MCP servers hosted by the service. A preset ships only when
Connect → browser sign-in → back in Grain works with no setup by the user. A
one-time registration of Grain on the service's developer page is acceptable
only when it is instant (no review, verification, waiting period or user cap).
Services that need those stay out of the catalog.

### Catalog maintenance

Publishing another preset requires a Grain release. The review for a new
preset includes sign-in on a release build and tool discovery.
Account credentials remain in the existing runtime credential storage.

Revisit remote delivery if catalog changes need to ship faster than app
updates. A future Grain-owned feed can serve versioned presentation metadata
with a bundled fallback. Endpoint and authentication definitions
stay in the application so a catalog update cannot change where credentials
are sent.

Bundled marks are sourced from [GitHub’s Primer Octicons](https://github.com/primer/octicons/blob/main/icons/mark-github-24.svg),
[Linear’s Brand Guidelines](https://linear.app/brand), the
[Notion website favicon](https://www.notion.com/front-static/favicon.ico),
[Simple Icons](https://simpleicons.org/) (CC0; Todoist, Dropbox, Asana, Coda
and Atlassian, filled with each brand colour), and the Readwise, Raindrop.io,
monday.com and Canva website icons.
Brand colours are allowed.
