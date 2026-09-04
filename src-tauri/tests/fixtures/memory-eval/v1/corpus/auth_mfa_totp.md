---
grain_id: 11111111-1111-4111-8111-111111111107
title: TOTP MFA Recovery Codes Setup
timestamp: 1786320000000
tldr: RFC 6238 TOTP implementation and 10 single-use recovery codes generation.
question: How are MFA backup recovery codes handled during enrollment?
reminder: ""
pinned: false
entities:
  - mfa
  - totp
  - recovery_codes
  - rfc6238
  - two_factor
todos: []
source: test_fixture
---

# Two-Factor Authentication & Recovery Codes

Grain accounts support RFC 6238 Time-based One-Time Password (TOTP) hardware and software tokens.

## Backup Recovery Codes
- 10 alphanumeric backup codes are minted during initial enrollment (8 characters each).
- Each code is hashed with bcrypt (cost factor 10) before database persistence.
- Each backup code is single-use: upon successful verification, its hash is erased from the user record.
