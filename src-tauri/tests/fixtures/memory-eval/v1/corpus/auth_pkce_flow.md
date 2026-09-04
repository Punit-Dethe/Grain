---
grain_id: 11111111-1111-4111-8111-111111111101
title: Auth0 PKCE Exchange Flow
timestamp: 1785801600000
tldr: Auth0 PKCE authorization code exchange and redirect handler implementation.
question: How is PKCE code exchange handled for Auth0 login?
reminder: ""
pinned: false
entities:
  - auth0
  - pkce
  - oauth
  - token_exchange
  - security
todos: []
source: test_fixture
---

# Auth0 PKCE Authorization Code Exchange

This document details the Auth0 PKCE (Proof Key for Code Exchange) flow implemented for web clients.

## Overview
1. The client generates a high-entropy cryptographic random `code_verifier` (43 to 128 characters).
2. The client calculates the SHA-256 hash of the verifier and base64url-encodes it to produce the `code_challenge`.
3. The authorization request sends `code_challenge` and `code_challenge_method=S256` to the `/authorize` endpoint.
4. Upon redirection back to `https://app.grain.dev/auth/callback/auth0`, the client exchanges the received `code` along with the original `code_verifier` via a POST to `/oauth/token`.

## Invariants
- Code verifier must be generated using OS cryptographically secure random bytes.
- Redirect URI must strictly match the registered origin.
- The `state` parameter must be verified to prevent CSRF attacks before token exchange.
