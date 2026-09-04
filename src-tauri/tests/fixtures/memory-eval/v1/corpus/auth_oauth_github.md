---
grain_id: 11111111-1111-4111-8111-111111111106
title: GitHub OAuth App Scopes
timestamp: 1786233600000
tldr: Configured scopes and callback URL for GitHub developer OAuth integration.
question: Which GitHub OAuth scopes are requested for developer sign-in?
reminder: ""
pinned: false
entities:
  - github
  - oauth
  - scopes
  - read_user
  - client_id
todos: []
source: test_fixture
---

# GitHub OAuth Application Setup

Details for GitHub Single Sign-On integration:

- Client ID: `gh_oauth_77a9c2b`
- Callback URL: `https://app.grain.dev/auth/callback/github`
- Requested Scopes:
  - `read:user` (to display username and avatar)
  - `user:email` (to resolve verified primary email address)
- Explicit Exclusion: The `repo` scope is deliberately NOT requested to minimize third-party attack surface.
