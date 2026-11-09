# Bundled MCP catalog

The Extensions workspace has compact rows for connected presets, an artwork
grid for browsing, and a detail page for each preset. Account recovery, tool
checks and disconnect live on the detail page. Configure MCP remains the place
to manage custom servers.

Keep the first 30–40 presets in the Grain repository and ship them with the
desktop app. The Rust catalog owns the provider name, description, endpoint,
authentication mode and release eligibility. The frontend artwork map is keyed
by the same provider IDs and holds only presentation assets. Store small SVGs
or optimized WebP files beside it and reuse the same banner for the card and
detail page. Browse renders without fetching catalog data from the network.

Publishing another preset requires a Grain release. The review for a new
preset includes its artwork, sign-in on a release build, and tool discovery.
Account credentials remain in the existing runtime credential storage.

Revisit remote delivery if catalog changes need to ship faster than app
updates. A future Grain-owned feed can serve versioned presentation metadata
and artwork with a bundled fallback. Endpoint and authentication definitions
stay in the application so a catalog update cannot change where credentials
are sent.
