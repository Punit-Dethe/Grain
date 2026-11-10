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

### Rollout checkpoint

- **Slack is next.** Start with browser sign-in and read-only tool discovery in
  an internal Slack workspace. See [Slack pilot setup](#slack-pilot-setup).
- **Google Workspace is deferred.** Keep the Calendar preview in place. Resume
  once a managed Google Workspace account can enroll the Grain Cloud project.
  Then configure the desktop OAuth client and verify sign-in, refresh and an
  Agent read with an eligible tester account.
  [Google preview enrollment](https://developers.google.com/workspace/preview#how_to_join_the_program)
- **Microsoft 365 is deferred** until its tenant and app setup are ready.

### Slack pilot setup

1. Create a dedicated **Grain Slack Pilot** app in a workspace you administer.
   A Slack developer sandbox also works for internal testing.
   [Slack app settings](https://api.slack.com/apps) ·
   [Developer sandboxes](https://docs.slack.dev/tools/developer-sandboxes/)
2. In the app's **Agents** settings, enable **Slack Model Context Protocol
   (MCP) Server**. In **OAuth & Permissions**, enable PKCE, add
   `http://localhost:31938/mcp/oauth/callback` under **Redirect URLs**, and add
   `search:read.public` under **User Token Scopes** for the first public-channel
   search test. Install the app in the pilot workspace.
   [Slack MCP app setup](https://docs.slack.dev/ai/slack-mcp-server/developing/) ·
   [Desktop PKCE](https://docs.slack.dev/authentication/using-pkce/) ·
   [Public search scope](https://docs.slack.dev/changelog/2026/02/17/slack-mcp/)
3. Copy the **Client ID** from **Basic Information → App Credentials**. The
   pilot is intended to use this public ID and PKCE. Keep the client secret
   out of Grain builds.

Before bundling Slack, verify that the user-token endpoint accepts the desktop
PKCE flow with the registered app and that token refresh and MCP discovery work.
Slack's live authorization metadata currently lists only `client_secret_post`,
while its desktop guidance describes a public client. The first live check
should search for a known message in a pilot workspace's public channel.
Distribution beyond the internal pilot requires a Slack Marketplace listing.
[Slack authorization metadata](https://mcp.slack.com/.well-known/oauth-authorization-server) ·
[Slack MCP app requirements](https://docs.slack.dev/ai/slack-mcp-server/#app-identity)

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
[Google’s Calendar icon](https://www.gstatic.com/images/branding/product/1x/calendar_48dp.png),
[Linear’s Brand Guidelines](https://linear.app/brand), and the
[Notion website favicon](https://www.notion.com/front-static/favicon.ico).
