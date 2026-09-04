---
grain_id: 11111111-1111-4111-8111-111111111103
title: Okta SAML Signature Validation Issue
timestamp: 1785974400000
tldr: Resolving Okta SAML 2.0 assertion signature validation failure on staging IdP.
question: Why did Okta SAML assertion signature validation fail on staging?
reminder: ""
pinned: false
entities:
  - okta
  - saml
  - idp
  - x509_cert
  - signature_verification
todos: []
source: test_fixture
---

# Okta SAML Signature Validation Troubleshooting

During staging testing on August 3, users encountered SAML authentication errors with code `SAML_SIGNATURE_MISMATCH`.

## Root Cause
The Okta staging Identity Provider (IdP) certificate reached its annual expiration date on August 1st. Okta rolled over to a new self-signed X.509 certificate for XML assertions. The Grain backend truststore was still validating against the cached old certificate.

## Resolution
1. Fetched fresh metadata XML from `https://grain-staging.okta.com/app/exk123/sso/saml/metadata`.
2. Extracted the active signing certificate.
3. Updated the truststore configuration and re-enabled XML assertion signature verification.
