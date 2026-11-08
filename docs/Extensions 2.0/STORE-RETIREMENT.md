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

### 2026-11-08 — Connection hardening

- Cancel and page exit now stop the pending OAuth attempt without using the
  disconnect command. GitHub and Notion presets have a Cancel sign-in button.
  The user can retry if the preset list fails to load. Browser and provider
  errors expose the failed OAuth stage without showing raw responses.
- Removed orphaned store card and grid styles. The connection tabs expose
  their selected state to assistive technology.
- The real desktop harness verified that cancelling a replacement sign-in
  preserves the prior enabled account, refuses the late callback without a
  new token exchange, and still discovers tools. Run evidence is in
  `tests/agent-harness/.runs/run-v4QQcR/evidence/report.md`. The three
  custom-connection management cases passed in `run-tmJS1H`.
- Frontend build, type check, scoped lint, 94 frontend tests, 78 runner and
  GitHub checks, and 83 desktop MCP tests passed. Live packaged provider
  sign-in and tool reads are still the next checkpoint.

### 2026-11-08 — MCP-only extension decision

Bundled GitHub and Notion presets and user-configured MCP servers share one
extension runtime. New Grain integrations use MCP. The native package runtime
still exists in this branch and supports some Agent harness fixtures.

#### Remaining execution order

1. Move shared Agent acceptance cases to the existing MCP fixtures. Keep
   coverage for tool selection across connections, tool results, approval,
   cancellation, and preventing replay of writes with an unknown outcome.
   Drop cases specific to native package loading or authoring.
2. Make the Agent catalog, schema lookup, and dispatch MCP-only in
   `src-tauri/src/capability.rs`. Reuse the Agent executor and its safeguards.
   Preserve MCP discovery, OAuth, account storage, endpoint validation, and
   per-connection error handling.
3. Remove the unused native host, package registry, developer bridge, host
   window and capability, and native-only SDK and frontend code. Check callers
   before deleting shared code. Move the atomic writer used by the MCP
   connection store and the SDK manifest helper used for MCP endpoint
   validation to retained modules. Preserve desktop event types and Handy hooks.
4. Verify normal and harness desktop builds, frontend checks, and real-app
   MCP acceptance. Test packaged GitHub with a non-owner account and Notion
   through sign-in, discovery, and a read call.

Existing native packages require no migration. The external publisher
repository remains a separate operational cleanup. After each stage, record
the commit, checks and results, open issues, and the next step in this log.

#### Connection UI to carry forward

- Put **Configure MCP** in the top-right of **My connections**. Open a
  dedicated page for adding, refreshing, and managing custom MCPs.
- Show clickable provider cards in **Browse extensions** with an icon, name,
  and description. Open provider details from the catalog or a connected
  card. Put Connect, Reconnect, Check tools, and Disconnect in that detail view.
- Track the UI work separately from the runtime removal. The user will verify
  the layout in the real desktop app.

### 2026-11-08 — Configured MCP acceptance without native tools

Commit `5e4ef5cfa` updates the configured MCP harness to follow the Agent's
current automatic dispatch. The non-main-window settings check opens the
real Agent panel with a scripted model reply and needs no native fixture.
The runtime cases verify typed reads, independent same-endpoint connections,
revalidation between selection and dispatch, and cancellation of held calls
with exactly one wire dispatch and no replay.

- The harness build and 75 runner tests passed. The three configured metadata
  cases and three configured runtime cases passed in `run-1MJnX0` and
  `run-0Oegcu`. The wrong-owner control failed as expected in `run-XM2Iq3`.
  Each real-app run reported successful cleanup.
- The existing `agent.workflow-mcp` case still times out waiting for the
  removed manual approval step (`run-OSq6E4`). This checkpoint covers the
  configured cases only. Next, update the shared read/write and interruption
  cases to verify dispatch and outcomes over MCP, then remove native Agent
  dispatch and its package fixtures. Packaged GitHub and Notion sign-in and
  tool reads remain open.

### 2026-11-08 — Agent discovery and dispatch cut over to MCP

Commit `c4a9d879b` removes native packages from the live Agent directory,
schema lookup and dispatch. The native metadata index was removed with its
last caller. The existing package quarantine still runs wherever the native
registry remains. The Agent still validates MCP schemas and checks connection
ownership and the loaded tool digest before executing calls.

- `node tests/agent-harness/run.mjs --suite mcp-agent-workflows` passed all
  four real-app cases with no native fixture loaded (`run-0HJeFe`). It covers
  a complete read/write/verify turn, a lost write reply with blocked retry,
  model failure after a completed write, and cancellation of pending model
  tool calls on both sides of a write. The incorrect-receipt control failed
  as expected (`run-W1J2uR`).
- The rebuilt harness, 75 runner tests, the three configured runtime cases
  (`run-7sXr4y`), and configured OAuth reconnect cancellation (`run-WvUhUc`)
  passed. Those real-app runs reported successful cleanup.
- The normal desktop `cargo check` passed. Direct Windows test startup hit
  `STATUS_ENTRYPOINT_NOT_FOUND`. The maintained runner's isolated copy and
  activation manifest ran the same binary successfully. All 83 MCP tests,
  18 capability tests and the Agent no-replay test passed. One catalog
  evaluation remains ignored because it requires external fixtures.
- The remaining native host, registry, legacy confirmation code, and
  native or approval-dependent harness scenarios still need removal.
  Before deleting them, move the shared atomic writer and MCP endpoint
  validator out of native modules. Use the focused suite above to verify the
  cutover. Packaged GitHub and Notion sign-in and tool reads remain open.
