---
grain_id: 11111111-1111-4111-8111-111111111102
title: Clerk Session Token Expiration and Refresh
timestamp: 1785888000000
tldr: Clerk session token lifecycle, 15-minute expiration, and sliding window refresh.
question: What is the expiration window for Clerk session tokens?
reminder: ""
pinned: false
entities:
  - clerk
  - session_token
  - jwt
  - refresh_token
  - expiry
todos: []
source: test_fixture
---

# Clerk Session Token Lifecycle

Clerk session tokens are short-lived JWTs designed to minimize security exposure.

## Token Characteristics
- Lifespan: 15 minutes (900 seconds) from issuance.
- Expiration claim: standard `exp` timestamp in JWT payload.
- Refresh Mechanism: Sliding window refresh via `/v1/client/sessions/{id}/touch`.

## Refresh Policy
When user activity occurs within 60 seconds of expiration, the frontend client automatically invokes the touch endpoint to rotate the session token. If token reuse is detected on the refresh token, all sessions in the user family are immediately revoked.
