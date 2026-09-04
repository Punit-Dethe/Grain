---
grain_id: 11111111-1111-4111-8111-111111111104
title: Service API Keys Storage Policy
timestamp: 1786060800000
tldr: Policy for provisioning and storing service-to-service API keys in hash vault.
question: How should service-to-service API keys be stored and hashed?
reminder: ""
pinned: false
entities:
  - api_keys
  - hashing
  - sha256
  - vault
  - secrets
todos: []
source: test_fixture
---

# Service API Keys Storage Policy

Guidelines for creating and persisting API keys used by automated services and background daemons.

## Generation
- Keys are 32 random bytes, hex-encoded (64 characters), prefixed with `grn_live_`.
- Total length: 73 characters.

## Storage Invariants
- The database NEVER stores raw API keys.
- Only the SHA-256 hash of the key is stored in the `service_keys` table.
- Raw keys are displayed to the operator exactly once upon initial creation.
