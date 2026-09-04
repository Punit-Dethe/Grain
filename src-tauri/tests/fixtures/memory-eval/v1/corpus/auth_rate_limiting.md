---
grain_id: 11111111-1111-4111-8111-111111111105
title: Login Rate Limiter Configuration
timestamp: 1786147200000
tldr: Redis token bucket rate limiter configuration for auth login endpoints.
question: What are the rate limit parameters for login attempts?
reminder: ""
pinned: false
entities:
  - rate_limiting
  - redis
  - token_bucket
  - brute_force
  - security
todos: []
source: test_fixture
---

# Authentication Rate Limiting Specification

To mitigate credential stuffing and brute force attacks on `/api/v1/auth/login`:

## Configuration
- Algorithm: Token Bucket backed by Redis.
- Rate: 5 requests per minute per source IP.
- Burst limit: 10 requests.
- Lockout: After 10 consecutive failed attempts within 5 minutes, source IP is quarantined for 15 minutes.
- Response: HTTP 429 Too Many Requests with `Retry-After: <seconds>`.
