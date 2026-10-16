# Account setup for extension authors

Choose the account route before submitting an extension. `doctor` and `pack`
check local metadata and package bytes. They do not contact a provider, register
a client, sign in, or prove users can connect. No Grain API key is needed.

## Supported remote MCP routes

| Provider setup | Author supplies | User experience | Support today |
| --- | --- | --- | --- |
| Intentionally anonymous | HTTPS Streamable HTTP URL; `authentication.type: none` | Install/enable and use tools | Supported if the server actually permits anonymous access. |
| Dynamic client registration (DCR) | URL; OAuth; compatible discovery/registration | Grain registers the client, then opens consent | Supported through the MCP SDK; registration must work without an author-supplied initial-access credential. |
| Publisher-registered public desktop client | URL, OAuth, public client ID and exact issuer in `oauthClient` | Browser consent, without each user creating a developer app | Supported when the provider accepts Grain's callback, S256 PKCE and token authentication `none`. |
| Existing host integration | Supported service/extension identity and endpoint | Browser consent using that Grain build's registration | GitHub is the current specific tester integration; this registration is not shared with arbitrary extensions. |
| Secret-required client, API-key-only server or another custom grant | Separately supported host integration or publisher service | Depends on that reviewed integration | Not supported merely by adding package fields. Do not embed credentials or publish a setup requiring ordinary users to register developer apps. |

Grain discovers authorization endpoints/scopes and uses the existing MCP SDK's
authorization-code/PKCE and refresh flow. Account tokens stay in the OS vault.
Metadata advertising a registration endpoint alone does not prove it works.
Usable renewable grants refresh without browser consent. Revocation, rejected
refresh, organization policy or registration changes can still require sign-in;
there is no promised daily login interval.

## Publisher-owned public client

Register a public/native desktop application with the provider once, with this
exact current **MCP** callback:

```text
http://127.0.0.1:31938/mcp/oauth/callback
```

Add these fields to `mcp.json`, alongside its identity/version/transport:

```json
{
  "authentication": { "type": "oauth" },
  "oauthClient": {
    "clientId": "your-registered-public-client-id",
    "issuer": "https://auth.example.com/tenant"
  }
}
```

The issuer must match discovered metadata exactly, including path/trailing slash.
Provider metadata must advertise S256 and `token_endpoint_auth_methods_supported`
containing `none`. The provider must accept Grain's callback for this client.
A client ID is public identification, not a token or proof of correct registration.
No client secret, scope, endpoint or redirect override is accepted here. Public
transport headers must not carry credentials either.

Explicit per-connection setup takes precedence, then package metadata, then
supported host registration/discovery. A declared registration failure never
silently switches to a different client. Manual credential entry is a development/
advanced route, not a substitute for store onboarding. Changing the URL, auth
type or client declaration changes the account binding and needs fresh consent.
An unchanged binding is intended to retain its account across version/artwork/
description updates; store acceptance is checked separately. Direct MCP JSON
uses the same declaration; custom management stays hidden in alpha/release.

## Current first-party identities

**Notion:** the installed descriptor uses OAuth discovery without `oauthClient`.
Representative sign-in and read/create/edit actions have been accepted through
Grain on a disposable workspace. No Notion-specific client identity is compiled
for that installed extension. This does not certify every tool or tenant policy.

**GitHub:** tester builds supply `GRAIN_GITHUB_OAUTH_CLIENT_ID` and
`GRAIN_GITHUB_OAUTH_CLIENT_SECRET` for `com.grain.github` and exactly
`https://api.githubcopilot.com/mcp/`. The host pins GitHub's issuer and exchange
endpoints before using the credential. Desktop build values are extractable;
they are **not confidential server credentials** or signing keys. Do not reuse
this registration for another service/package or assume a dev build includes it.
Provider permissions, app installation and organization approval still apply.

Grain's global Client ID Metadata Document URL is disabled until a permanent
hosting identity is deployed and verified. Experimental websites do not enable
it. This does not block supported DCR or preregistered public clients.

## Prepublication checklist

1. State the route, endpoint, registration owner, issuer/callback where needed,
   provider permissions and organization/workspace prerequisites in README. Put
   ordinary install/sign-in instructions in DESCRIPTION; include no secrets.
2. Run `grain-ext doctor` and `grain-ext pack`. Their account message is a setup
   reminder, not a network probe or sign-in certificate.
3. In a development Grain profile, use the existing MCP connection flow with
   the same URL, public headers and `oauthClient`. Custom management is developer-
   only. Another application's login is not evidence that Grain works. A supported
   installed test candidate can supply the real-user checkpoint when needed.
4. Using a disposable account/workspace, check consent and one harmless read;
   cancel sign-in and confirm retry works. Record Grain build/source, declared
   route and result, without tokens/codes. Reuse accepted evidence when that
   route/runtime has not changed.
5. Maintainers review actual provider registration requirements before activation.
   If the route needs an unsupported secret, transport or callback, resolve support
   first. Do not send ordinary users to developer credential settings. Source
   publication approval and each user's account consent are separate.

## Native tools

Native tools use the existing `contributes.authentication` public OAuth
`oauth2-pkce` declaration and declared API hosts, rather than MCP's `oauthClient`.
The host brokers authorized requests instead of returning raw service tokens to
tool code. Follow generated SDK declarations and the native guide. The native
callback is `http://127.0.0.1:<ephemeral-port>/grain/oauth/callback`: register a
public desktop client that allows a variable loopback port with that fixed path.
Providers insisting on one fixed port or another path are outside this native
profile. The listener exists only during sign-in; random state and S256 PKCE
still bind each attempt. Do not copy MCP's callback into a native registration.

## References

Checked 16 October 2026. Grain-specific support and limits above come from the
current source; provider behavior remains independently verified at onboarding.

- [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization): discovery, registration alternatives and PKCE.
- [Notion client integration](https://developers.notion.com/guides/mcp/build-mcp-client): provider OAuth/connection requirements.
- [GitHub host integration](https://github.com/github/github-mcp-server/blob/main/docs/host-integration.md): provider registration and desktop credential limitations.
- [RFC 8252](https://www.rfc-editor.org/rfc/rfc8252): public/native OAuth clients and embedded shared-secret limitations.
