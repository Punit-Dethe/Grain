# Extension store retirement

Grain is moving to a bundled set of remote MCP presets and user-configured
remote MCP connections. Grain ships endpoint and authentication configuration.
GitHub and Notion host their own servers. Adding a preset requires a Grain
change and a release. Custom remote MCPs can be added directly in the app.

## Execution order

1. Switch Browse to the bundled GitHub and Notion presets. Expose the existing
   custom connection form in regular builds. Reuse the existing OAuth, vault,
   destination checks, Agent approvals, discovery and tool execution paths.
2. Disable production catalogue and store MCP access. Remove the publishing,
   review and signing tools and author CLI release workflow. Preserve cached
   native extension revocation checks during the transition.
3. Replace harness scenarios that still depend on store-installed accounts or
   author CLI packages with direct MCP and local runtime fixtures. Remove the
   remaining catalogue commands, state, signing and store artifacts, CLI crates,
   pack and unpacked developer tooling that no longer serves a runtime feature,
   and the dedicated store delivery profile. Keep native first-party behavior
   and its required security checks.
4. Review the external registry repository and hosting together before
   retiring it. Verify other consumers and jobs, then retire its workflows
   and credentials with its owner.

## Acceptance

- Browse opens without a catalogue request or Developer Mode.
- GitHub and Notion can connect in the packaged app, discover tools and make a
  representative read call. Verify GitHub with a non-owner account.
- A custom MCP can be added, enabled, edited and removed in a normal build.
- Repeat after restart, expiration and revocation. Check cancellation and
  approval behavior using the maintained Agent harness.

The catalog location and existing page layout are interim decisions. A larger
provider list and its interaction design can follow the runtime cutover.

## Implementation log

### 2026-11-08 — Built-in catalog cutover

- Branch `cleanup/retire-legacy-extension-platform` in
  `C:/Projects/Grain/grain`. Starting commit `f8be135e9`. The newer GitHub
  work was in `codex/fix-github-extension-auth` in a separate checkout.
- Ported the trusted-build OAuth inputs and bounded GitHub SSE catalog fix
  from `508579a16`. Kept GitHub's `x-mcp-toolsets: all` header on its
  bundled preset. Ported the Agent quota changes from `15c7798e6` to keep
  the earlier long tool-calling behavior.
- Browse now reads bundled GitHub and Notion metadata from the Rust host.
  Custom MCPs are managed from My connections without Developer Mode.
  A preset sign-in runs the existing tool discovery check after OAuth.
- The app no longer offers store browse, package installation, or the
  extension author drawer. Store commands and manual pack loading are
  guarded for the acceptance harness. Cached revocation checks are still
  used for already-installed native extensions.
- Deleted the registry publication crate and its Python tooling, along with
  the author CLI release workflow. The author CLI remains in the workspace
  because the real-app harness uses it for package fixtures.
- Old store MCP accounts and the bundled presets use separate credential
  identities. Existing testers should reconnect from Browse. GitHub
  OAuth requires `GRAIN_GITHUB_OAUTH_CLIENT_ID` and
  `GRAIN_GITHUB_OAUTH_CLIENT_SECRET` at build time.
- Verification includes the frontend production build, TypeScript, scoped
  ESLint, 94 frontend tests, 80 harness runner tests, a Rust library
  check, 88 MCP tests, and 25 Agent tests. Packaged browser consent and
  a GitHub non-owner account still need acceptance.

For the next session, start with step 3 above. The remaining coupling is in
`grain_store.rs`, `grain_mcp_connections.rs`, the Rust store fixtures and
`tests/agent-harness/`. Replace the fixtures before deleting their runtime
support so OAuth, revision ownership and tool execution still get exercised.


### 2026-11-08 — Store runtime and author tooling removal

- Removed catalogue IPC, hosted catalogue requests, the signed cache, embedded
  roots and revocations, signed package acquisition, the publisher format and
  the author CLI. Normal app startup skips the legacy native registry, worker
  reaper and loopback developer bridge. The isolated acceptance harness still
  builds local native fixtures to exercise runtime permissions and ownership.
- GitHub and Notion still use provider-hosted MCP endpoints. Preset metadata
  ships in the app and uses the same runtime as user-configured remote MCPs.
  GitHub's build-supplied OAuth identity remains restricted to the bundled
  provider and exact resource. The isolated Windows build is now Grain GitHub
  Test. The acceptance script checks the bundled GitHub preset directly.
- Custom connections use `mcp-custom-connections.json`. The old file may mix
  store and custom rows, so Grain starts a fresh registry and leaves old files
  on disk. Re-add custom servers that were configured on earlier builds.
- The harness stamp initially failed because it fingerprinted the deleted
  signing seeds and assumed every directory in `crates/` had a manifest.
  It now fingerprints active crates and skips leftover directories without a
  manifest.
- Verification includes the production frontend build, 94 frontend tests,
  78 runner and GitHub checks, 231 core and SDK tests, 83 desktop MCP tests,
  and normal and harness desktop compilation.
- The real app passed all three configured MCP cases covering ownership,
  access controls and storage, with isolated cleanup
  (`tests/agent-harness/.runs/run-HH19eB/evidence/report.md`). The first
  attempts exposed outdated store fields and a wait for a manual Agent
  approval. The access check now uses the real Agent panel with the
  existing direct tool-selection oracle.

The external publisher repository is a separate operational cleanup. The next
product checkpoint is packaged GitHub and Notion sign-in and a tool read,
including GitHub with a non-owner account.
