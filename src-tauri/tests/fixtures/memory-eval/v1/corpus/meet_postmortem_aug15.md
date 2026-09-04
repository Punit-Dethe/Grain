---
grain_id: 22222222-2222-4222-8222-222222222206
title: Postmortem: Auth Token Outage (2026-08-15)
timestamp: 1787011200000
tldr: Postmortem analysis of the 45-minute staging auth token validation deadlock.
question: What caused the auth token outage on August 15?
reminder: ""
pinned: false
entities:
  - postmortem
  - incident
  - deadlock
  - jwt
  - outage
todos: []
source: test_fixture
---

# Postmortem: Staging Auth Token Outage

Date: 2026-08-15
Duration: 45 minutes

## Root Cause
A blocking Mutex lock in the token validator collided with a background key-set refresh thread under heavy concurrent load, causing all authentication requests to hang until timeout.

## Corrective Actions
1. Replaced `std::sync::Mutex` with `tokio::sync::RwLock`.
2. Added proactive background prefetching of signing keys before expiration.
