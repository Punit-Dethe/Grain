# Extension authentication v1 implementation

Status: implemented; manual acceptance remains
Last updated: 2026-08-24

## Confirmed contract

- A scripted extension may declare up to eight independent OAuth services.
  This supports workflows such as reading Linear and writing GitHub in one
  extension.
- Each declaration has one active `default` connection in API 1.x. Status and
  vault keys retain `connection_id`, allowing a later API to add more accounts
  without changing the existing identity.
- Only OAuth 2.0 Authorization Code with PKCE S256 and a loopback redirect is
  accepted. Auth declarations reject unknown fields, including `clientSecret`.
- Grain owns consent, browser launch, callback, exchange, refresh, disconnect,
  OS-vault storage, and bearer injection. JavaScript sees metadata and bounded
  API responses, never codes, verifiers, or tokens.
- Vault failure is closed. There is no plaintext fallback or hosted proxy.

## Manifest

```json
{
  "permissions": ["auth:linear", "net:api.linear.app", "auth:github", "net:api.github.com"],
  "contributes": {
    "authentication": [
      {
        "id": "linear",
        "type": "oauth2-pkce",
        "providerName": "Linear",
        "clientId": "public-linear-client-id",
        "authorizationEndpoint": "https://linear.app/oauth/authorize",
        "tokenEndpoint": "https://api.linear.app/oauth/token",
        "scopes": ["read"],
        "redirectMethods": ["loopback"],
        "apiHosts": ["api.linear.app"]
      },
      {
        "id": "github",
        "type": "oauth2-pkce",
        "providerName": "GitHub",
        "clientId": "public-github-client-id",
        "authorizationEndpoint": "https://github.com/login/oauth/authorize",
        "tokenEndpoint": "https://github.com/login/oauth/access_token",
        "scopes": ["read:user"],
        "redirectMethods": ["loopback"],
        "apiHosts": ["api.github.com"]
      }
    ]
  }
}
```

Every declaration needs `auth:<id>`. Every API host needs exact `net:<host>`.
Changing an endpoint, scope, client id, parameter, or API host changes the
approval fingerprint and holds the extension disabled for reapproval.

## Extension API

```ts
const github = await grain.auth.status("github");
if (!github || github.state !== "connected") await grain.auth.connect("github");

const response = await grain.net.fetch("https://api.github.com/user", {
  auth: "github",
  headers: { Accept: "application/vnd.github+json" },
});
```

Extension-triggered connect/disconnect requests show native Grain-owned
confirmation. Authenticated requests require both grants, must target
`apiHosts`, cannot override Grain's Authorization header, and re-check both
policies on redirects.

## Lifecycle and storage

- Vault service: `com.grain.extension.oauth`.
- V1 vault identity: `<extension-id>:<auth-id>:default`.
- Windows uses Credential Manager, macOS uses Keychain, and Linux uses Secret
  Service through `keyring`. A missing backend reports unavailable.
- In-memory token structs and token-response buffers are zeroized on drop;
  request-library header storage is short-lived. Token responses are capped at
  64 KiB, redirects are disabled, and provider error descriptions are hidden.
- Refresh is on demand with a 60-second expiry skew and no refresh daemon.
- Loopback receivers bind `127.0.0.1`, require GET, random path, exact Host and
  state, and end on success, cancellation, or five-minute timeout. Replacement,
  disable, and uninstall cancel an active receiver.
- A vault-resident authentication inventory lets uninstall remove credentials
  even when a later manifest has removed their declarations or ordinary
  extension data is retained.

## Required manual acceptance

Run the real Tauri app with registered public clients on Windows, macOS, and
Linux. Connect Linear and GitHub independently, exercise refresh/cancel/
reconnect/disable/disconnect/uninstall, verify Linux without Secret Service
fails closed, inspect logs for token absence, and visually approve the real
connection rows and native confirmation dialogs.
