# Extension authentication research

Status: research complete for planning
Research date: 2026-08-24

## 1. Research question

How should a local, cross-platform Grain extension obtain delegated access to a
third-party service without exposing credentials, weakening Grain's capability
boundary, adding an always-on service, or materially increasing idle RAM?

The closest product analogue is Raycast. The research also covers the IETF
native-app requirements, VS Code and Chrome extension UX, Tauri's desktop
callback mechanisms, and a current provider implementation (GitHub).

## 2. Grain's current baseline

Grain already has most of the security boundary an authentication feature
needs:

- Each scripted extension has a distinct high-entropy runtime token. Rust binds
  identity and capabilities to the connection rather than trusting an extension
  id in a message (`events_auth.rs`, `events_server.rs`).
- `grain.net.fetch` is a Rust proxy gated by exact `net:<host>` permissions. It
  rechecks redirects and enforces time and body-size limits (`host_api.rs`).
- Declared `secret` settings are write-only to extension code. Rust can inject a
  secret into a named request header without returning it to JavaScript.
- Extension settings and secrets are namespaced by extension id; uninstall
  purges the namespace. Disable/reload revokes runtime tokens and workers.
- Permission additions already disable an update until the user approves the
  diff.

The missing capability is not generic secret storage. It is a delegated-account
lifecycle: browser authorization, callback correlation, code exchange, secure
token storage, refresh rotation, disconnect/revocation, scope-change handling,
and user-visible connection state.

One important limitation is that the existing `grain.secrets.json` credential
file is separate from settings but is not an OS-backed credential vault. That
was adequate for the existing Phase 4 contract, but long-lived refresh tokens
raise the impact of local file disclosure. Authentication should therefore use
a stronger vault rather than silently treating the current file as encrypted.

## 3. What Raycast does

Raycast's official OAuth API establishes a useful developer workflow:

1. A desktop extension is a **public OAuth client** and uses Authorization Code
   with PKCE. Raycast supports S256 and tells authors not to embed a client
   secret.
2. The extension creates an authorization request from a client id, authorization
   endpoint, scopes, and optional provider parameters. The host supplies a
   verifier, challenge, state, and redirect URI.
3. Raycast shows a host-owned "Connect to provider" overlay, then opens the
   provider consent page in the browser.
4. Raycast receives the redirect and resumes the initiating extension.
5. `OAuth.PKCEClient` stores, retrieves, and removes a token set. The token set
   can include access token, refresh token, expiry, scope, and ID token.
6. When tokens exist, Raycast automatically adds a logout preference.
7. Raycast's `OAuthService`, `withAccessToken`, and built-in provider presets
   reduce repeated authorization/refresh UI and code.

Raycast supports three callback shapes: a Raycast-hosted HTTPS redirect, a
`raycast://` private-use scheme, and a URI-style app scheme. A shared hosted
redirect gives broad provider compatibility. Raycast also offers a hosted PKCE
proxy for providers that require a confidential client secret.

### What is strong in the Raycast design

- PKCE and host-owned UX are platform defaults rather than extension-by-extension
  conventions.
- Authorization state is keyed to the extension/provider client.
- Scope and expiry are first-class token-set metadata.
- The host provides a consistent logout surface even if the extension author
  forgets one.
- Provider helpers remove substantial refresh and response-parsing boilerplate.

### What Grain should not copy literally

Raycast's extension API returns access tokens to extension JavaScript. That is
practical in Raycast's general Node/fetch model, but it would regress Grain's
existing write-only-secret design. Grain already proxies extension HTTP in Rust,
so it can refresh and inject a token without disclosing it.

Raycast's HTTPS callback relay and PKCE proxy also require operated internet
infrastructure that receives sensitive authorization traffic or holds provider
secrets. Grain should not create that service as a side effect of local extension
authentication. It can be evaluated later with its own threat model, operations
plan, privacy policy, incident response, and availability budget.

Sources:

- [Raycast OAuth API](https://developers.raycast.com/api-reference/oauth)
- [Raycast OAuth utilities](https://developers.raycast.com/utilities/oauth)
- [Raycast `OAuthService`](https://developers.raycast.com/utilities/oauth/oauthservice)
- [Raycast `withAccessToken`](https://developers.raycast.com/utilities/oauth/withaccesstoken)

## 4. Standards requirements

### 4.1 Native apps use the system browser and PKCE

[RFC 8252](https://www.rfc-editor.org/rfc/rfc8252) treats installed applications
as public clients: a secret distributed in the binary cannot be confidential.
It recommends an external user-agent (the system browser), requires PKCE for
public native clients, and advises against the implicit grant.

The RFC permits three redirect families:

- app-claimed HTTPS links;
- private-use URI schemes;
- loopback IP redirects such as `http://127.0.0.1:{ephemeral-port}/...`.

For loopback, the listener should exist only for the authorization attempt, bind
only to loopback, use an ephemeral port, and close after the result. Literal
`127.0.0.1` or `[::1]` is preferred to `localhost`. Private-use schemes can be
hijacked by another local application, but PKCE prevents an intercepted code
from being redeemed without the verifier.

### 4.2 Current OAuth security BCP

[RFC 9700](https://www.rfc-editor.org/rfc/rfc9700) is the current OAuth 2.0
Security Best Current Practice. Relevant requirements are:

- public clients use authorization code plus transaction-specific PKCE;
- S256 is the safe PKCE challenge method;
- redirect URIs are matched exactly, except the loopback port allowance;
- state/PKCE binding prevents callback injection and CSRF;
- clients that can use multiple authorization servers need mix-up defenses;
- access privileges and scopes should be minimal;
- refresh tokens issued to public clients must be sender-constrained or rotated
  by the authorization server;
- authorization and token endpoints should use published server metadata when
  available.

Grain cannot force a provider to rotate refresh tokens, but it can preserve a
new refresh token atomically, detect reuse-invalidated sessions, and document
provider compliance as an acceptance criterion.

### 4.3 Device flow is not a default desktop flow

[RFC 8628](https://www.rfc-editor.org/rfc/rfc8628) is intended for devices
without a suitable browser or input method. Grain has both. GitHub's current
guidance also prefers authorization code with PKCE over device flow for public
clients because device flow is easier to abuse for remote phishing. Device flow
is therefore a compatibility feature for a later phase, not a v1 shortcut.

## 5. Comparable extension platforms

### 5.1 VS Code

VS Code exposes a host authentication namespace rather than asking every
consumer extension to own the entire login UX. `authentication.getSession`
selects a provider and requested scopes, asks the user before sharing a session,
supports silent lookup versus interactive creation, remembers extension-specific
account preferences, and models added/changed/removed sessions. It also supports
multiple accounts.

VS Code's current GitHub implementation is a useful desktop reference: it
generates PKCE, starts a short-lived loopback server, opens the browser, races
the callback against cancellation and a five-minute timeout, then always stops
the listener. Secrets use VS Code's `SecretStorage`, backed on desktop by
Electron `safeStorage`, rather than normal extension state.

Grain should adopt the session/status model and explicit interactive/silent
distinction, but v1 can remain one account per manifest auth declaration. The
storage schema should retain a future `connection_id` so multi-account support
does not require a migration.

Sources:

- [VS Code authentication API](https://code.visualstudio.com/api/references/vscode-api#authentication)
- [VS Code common capabilities and secret storage](https://code.visualstudio.com/api/extension-capabilities/common-capabilities)
- [VS Code GitHub PKCE/loopback implementation](https://github.com/microsoft/vscode/blob/main/extensions/github-authentication/src/flows.ts)

### 5.2 Chrome extensions

Chrome's `identity.launchWebAuthFlow` gives the host control of the auth window
and captures only the final redirect URL. Chrome's guidance says interactive
authorization should be triggered from explanatory UI/user intent, not
automatically on first launch. Grain should enforce the same behavior: a worker
may request connection, but Grain must present a host-owned confirmation before
opening a browser.

Source: [Chrome `identity` API](https://developer.chrome.com/docs/extensions/reference/api/identity)

### 5.3 Tauri desktop callbacks

Tauri 2's deep-link plugin supports registered desktop schemes on Windows,
macOS, and Linux and integrates with the single-instance plugin. Its own security
warning is important: a user or process can manufacture a deep-link argument,
so the receiver must validate scheme, path, state, and the pending flow rather
than trusting the callback.

Grain already uses the Tauri opener and single-instance plugin, but not the
deep-link plugin. A loopback receiver can be implemented using the existing
Tokio networking runtime with no resident listener. App-scheme support would add
the Tauri plugin and single-instance deep-link feature and must be tested in an
installed bundle on all three operating systems.

Source: [Tauri 2 deep linking](https://v2.tauri.app/plugin/deep-linking/)

## 6. Provider reality check: GitHub

GitHub's current OAuth documentation supports PKCE S256, state, and loopback
redirects whose runtime port may differ from the registered loopback callback.
It explicitly recommends literal loopback IPs. It also supports device flow but
recommends PKCE when appropriate. However, the same current token-exchange table
still labels `client_secret` as required. Phase 0 must therefore prove a truly
secretless exchange with a registered public-client shape rather than inferring
compatibility from PKCE support alone.

This makes GitHub a useful compatibility test for the callback and scope model,
not a reason to special-case or weaken the core API. Google and one provider
with refresh-token rotation should also be validated before freezing the
manifest contract, because endpoint parameters and refresh behavior vary
materially.

Sources:

- [GitHub authorizing OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub OAuth app best practices](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/best-practices-for-creating-an-oauth-app)

## 7. Adopt, adapt, or defer

| Finding | Grain decision | Reason |
|---|---|---|
| PKCE S256 for public extension clients | Adopt | Required native-app security baseline |
| System browser + host consent surface | Adopt | Preserves provider cookies, user trust, and user intent |
| Host-generated state/verifier/redirect | Adopt | Extension code must not control security parameters |
| Host token set and automatic logout UI | Adopt | Consistent lifecycle and cleanup |
| Return bearer tokens to extension code | Reject | Regresses Grain's existing write-only-secret boundary |
| Authenticated host-proxied fetch | Adopt | Reuses exact-host permission and redirect checks |
| Hosted callback relay | Defer | Requires a reliable privacy/security-sensitive service |
| Hosted client-secret/PKCE proxy | Defer | Separate infrastructure and incident-response commitment |
| Loopback redirect | Adopt first | Standards-based, no server, no persistent listener |
| Private-use app scheme | Compatibility fallback | Wider provider compatibility; adds packaging and OS integration |
| Device authorization grant | Defer | Grain has a browser; phishing and polling costs |
| Multi-account sessions | Data-model now, UX later | Avoid v1 complexity without a migration trap |
| Provider presets | Later | Start generic, promote only providers proven by real extensions |
| Client credentials/password/implicit grants | Reject | Wrong trust model or deprecated/insecure for public extensions |

## 8. Research conclusion

Grain does not need a parallel authentication service or an extension-visible
token API. It needs a small host-owned OAuth lifecycle attached to existing
manifest validation, permission grants, Rust network proxying, extension
settings UI, and uninstall cleanup. The most security-relevant design choice is
to keep token material behind the Rust boundary, which is stronger than the
Raycast API while preserving its author and user workflow.
