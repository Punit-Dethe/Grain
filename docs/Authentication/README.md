# Extension authentication

Status: planning
Last updated: 2026-08-24

This folder defines how Grain extensions connect to third-party accounts. It
does not cover authentication between an extension and Grain itself; that is
already enforced by the per-extension WebSocket token and capability registry
described in `docs/Extension Platform/SPEC.md` section 7.1.

## Documents

- [RESEARCH.md](RESEARCH.md) records the platform and standards research,
  especially Raycast's extension OAuth workflow.
- [PLAN.md](PLAN.md) is the proposed Grain contract, security model, and phased
  implementation plan.

## Proposed direction

1. OAuth 2.0 Authorization Code with PKCE S256 is the only OAuth flow in the
   first release. An extension is a public client and may never ship a client
   secret.
2. Grain owns the browser launch, callback, code exchange, refresh, storage,
   revocation, and account UI.
3. Extension code receives connection status, never access tokens, refresh
   tokens, authorization codes, or PKCE verifiers.
4. Authenticated API calls extend the existing exact-host Rust network proxy:
   `grain.net.fetch(..., { auth: "provider-id" })`. Rust refreshes and injects
   the bearer token after checking both `auth:<provider-id>` and `net:<host>`.
5. Token material is stored through an OS-backed credential vault. Plaintext
   metadata may live in Grain's extension registry, but tokens must not be
   written to settings, extension storage, logs, exports, or diagnostics.
6. No hosted callback relay or client-secret proxy is required for v1. Providers
   that cannot support a public PKCE client use the existing manual secret/PAT
   path until a separately reviewed broker exists.

The three product decisions still requiring confirmation are listed at the end
of [PLAN.md](PLAN.md#decisions-to-confirm).
