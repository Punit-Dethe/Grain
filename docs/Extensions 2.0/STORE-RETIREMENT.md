# Extension store retirement

Grain uses a bundled set of remote MCP presets and user-configured
remote MCP connections. Grain ships endpoint and authentication configuration.
GitHub and Notion host their own servers. Adding a preset requires a Grain
change and a release. Custom remote MCPs can be added directly in the app.

## Current checkpoint

- The in-repository runtime cutover is complete. Agent discovery, schema
  lookup and execution use MCP. The signed store, publisher tooling, package
  runtime and native fixtures are retired. The controlled MCP checkpoint
  covers connection ownership, dispatch, cancellation and write replay.
- The user reported that the packaged test app worked. Broader acceptance
  remains open. Test GitHub with a non-owner account and Notion
  with a known test page. Confirm sign-in, a nonzero tool count and an Agent
  read, then repeat after restarting the test app. Test one custom connection
  through add, enable, edit and remove.
- The connection detail pages and dedicated Configure MCP page are separate
  UI work. Older MCP harness cases that still expect manual approvals need
  adaptation before they can be counted as current coverage. The external
  registry repository and credentials have their own retirement step.

## Original execution order

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

### 2026-11-08 — Native package controls removed from ordinary builds

Commit `783f5309a` moved the atomic file writer and its Windows lock tests
into a shared core module. MCP endpoint and descriptor checks now use shared
host and identity validators directly. MCP still uses the main-window guard
for settings commands.

- The normal Tauri command registry and generated bindings no longer expose
  native package management or native OAuth. The Settings page now shows
  MCP presets and custom connections. The production frontend omits the
  native supervisor page. The fixture runner still builds the native host
  and registers its package commands while the remaining fixtures are retired.
- Production frontend build, scoped lint, 90 frontend tests, normal Rust
  check, and 231 core and SDK tests passed. TypeScript also passed against
  the staged binding file with the unrelated local edits excluded. The normal
  desktop MCP suite passed all 83 tests (`logic-EbEqax`).
- The rebuilt desktop harness passed 11 MCP cases covering settings access,
  connection storage and dispatch, write verification, interruption, and
  reconnect cancellation. Evidence is in `run-jMVyK4`, `run-MLMDGW`,
  `run-NgSP14`, and `run-R8K77a`.
- Next, remove the native fixture cases and harness command routing, then
  delete the native host, loopback bridge, supervisor assets and capability,
  and package registry and SDK code. Keep the shared validators, file writer,
  MCP fixtures, and first-party desktop events. Packaged GitHub and Notion
  acceptance remains open.

### 2026-11-08 — Native package machinery deleted

Commit `c929e6af3` removes the package registry, installer, native OAuth,
worker host, loopback WebSocket bridge, webview assets and permission, and
package SDK and fixtures. Both desktop targets now use the MCP extension
contract. First-party desktop events and the shared tool retrieval, storage,
endpoint admission, and MCP account code remain in use.

- The desktop harness now tests normal model replies and cancellation without
  a native fixture. It keeps the controlled MCP read/write, lost-reply,
  connection, and OAuth checks. The native and old manual-approval workflow
  cases, including their genuine-model adapter, were retired. Some older MCP
  cases still need to move to current discovery and automatic dispatch.
- Normal Rust check, frontend and harness builds, UI parity, 77 frontend
  tests, 60 Node tests, and 155 core tests passed. The 13 real-app cases
  passed in `run-ePF5i2`, `run-XLvfFz`, `run-UPqfvC`, `run-tZFBHJ`, and
  `run-JyR3Uk`. The incorrect-receipt control failed as expected in
  `run-292vZw`. All 110 targeted desktop Agent, execution, and MCP unit
  tests passed in `logic-WBW3DN`.
- The next product checkpoint is packaged GitHub sign-in and a tool read
  with a non-owner account, followed by packaged Notion sign-in and a tool
  read. External publisher retirement remains a separate operational step.

### 2026-11-08 — Packaged provider acceptance

- The isolated Windows `Grain GitHub Test` build targets `adf2b77c5` in
  [Build Test run 42495825689](https://github.com/Punit-Dethe/Grain/actions/runs/42495825689).
  The workflow passed its GitHub OAuth registration input check. It produces
  an installer artifact with its own app identity and profile.
- The owner confirmed GitHub shows **Make private** in the registration's
  Advanced settings, which means the app is currently public. The original
  tester's failure remains unclassified.
- The actual [v0.0.11 Windows release run](https://github.com/Punit-Dethe/Grain/actions/runs/41471891681)
  passed registration validation. Its packaging step supplied the registered
  GitHub client. This is build evidence and leaves account consent and tool
  execution to the live checks.
- The [6 November GitHub audit](https://github.com/Punit-Dethe/Grain/blob/508579a16644469ad8f102d9ab4bf283f3a56ef0/docs/Extensions%202.0/GITHUB-RELIABILITY-FIX.md)
  recorded a separate signed-in discovery failure. The all-toolset catalogue
  exceeded the 512 KiB SSE limit present in v0.0.11. This branch already carries
  the shared 2 MiB catalogue budget for JSON and SSE and a regression test.
  All 83 desktop MCP tests passed again in `logic-AX5nd4`.
- Anonymous discovery confirmed the GitHub MCP resource points to
  `https://github.com/login/oauth`. Notion advertises dynamic registration
  and S256 at `https://mcp.notion.com`. The two browser sign-ins still need
  testing with real accounts.

When the run succeeds, use the `grain-github-test-x86_64-pc-windows-msvc`
artifact and open **Grain GitHub Test**. In **Extensions → Browse
extensions**, connect GitHub using a non-owner account. Use a test repository
that account controls and select only that repository if GitHub asks for an
installation. Record whether browser authorization finishes and **Check
tools** returns a nonzero count. Ask Agent to read that repository's
`README.md` and return its first line, then compare it with the file.

Repeat with Notion and a known page in a test workspace. Ask Agent to fetch
the page from its URL and return the first sentence. Record installer,
browser, discovery, and Agent failures separately. The hosted connection
tests are read-only.

### 2026-11-09 — Windows test installer ready

Run `42495825689` succeeded for `adf2b77c5`. The unsigned NSIS installer
`Grain GitHub Test_0.0.10_x64-setup.exe` was downloaded from
[artifact 13625655239](https://github.com/Punit-Dethe/Grain/actions/runs/42495825689/artifacts/13625655239).
Installer SHA-256 is
`8e47a38e57070d7c4efd5c237179280dc9e99e87d3101473268b2847fe47f324`.
Packaged sign-in, discovery, and read results remain pending for both providers.

### 2026-11-09 — Main integrated into the retirement branch

- Merged `origin/main` at `c066c2a94` into the retirement branch. This brings
  in the v0.0.11 desktop changes, including Recall, file transcription, and
  the model catalogue update. The Handy source tree matches that revision.
- Reconciled the routes and styles that the upstream merge rules had kept on
  the branch's older versions. Regenerated Tauri bindings and resolved both
  Rust lockfiles from main's dependency pins with the retired crates removed.
  The GitHub desktop OAuth registration remains in the release workflow for
  the bundled MCP preset. The signed store and native worker paths stay retired.
- Passed TypeScript, Vite, 88 frontend tests, 60 harness unit tests, 157 core
  and SDK tests, 683 desktop tests, UI parity, and the model catalogue and
  native fork checks. The installer above predates this merge. Packaged account
  and provider tests remain pending for the merged branch.

### 2026-11-09 — Agent shortcut with a Streaming-only model selection

- The shortcut reached Agent, which returned before showing the pill when
  the Standard model slot was empty. Agent now uses the selected Streaming
  model in that case and keeps the capture's model for submit. The existing
  engine lease protects model selection through decoding.
- The model-selection regression and desktop suite passed (684 tests).
  Windows hotkey and microphone acceptance remain a manual check.

### 2026-11-09 — Unified dictation model integrated

- Merged `origin/main` at `99dc383bc` and brought in the single model
  picker, catalogue, and capture routing. This supersedes the two-slot
  fallback above. Agent reads `selected_model` at capture start and
  uses that model for submit regardless of its streaming capability.
- Kept the MCP-only connection paths and regenerated Tauri bindings.
  TypeScript, Vite, 90 frontend tests, 60 harness unit tests, 156 core
  and SDK tests, 680 desktop tests, catalogue parity, and the upstream
  ratchet passed. Hotkey and microphone acceptance remain pending.

### 2026-11-09 — Agent pill and GitHub fixes reconciled

- Merged `codex/fix-github-extension-auth` at `e4a683419`. The animated
  working pill, first-reply panel reveal, and deferred shortcut update
  now run with the unified speech model.
- The GitHub preset already carried the catalogue size and OAuth fixes.
  Brought over the reliability evidence and focused diagnostic checks,
  keeping the harness on the bundled MCP preset and isolated test app.
- TypeScript, Vite, scoped ESLint, 90 frontend tests, 62 harness tests,
  and 682 desktop tests passed. The Rust checks used an isolated source
  copy because the running Windows app held a runtime DLL open.
  Live pill interaction and packaged GitHub sign-in remain manual checks.

### 2026-11-09 — GitHub sign-in after the pill merge

- The merge did not change the GitHub connection path. Cargo's dependency
  record confirmed that the running development binary had neither GitHub
  OAuth build input. Notion connected, listed 77 tools, and completed an Agent
  request through dynamic registration.
- Grain now surfaces missing preregistration before opening the callback
  listener or contacting GitHub, including when an earlier account is still
  saved. The isolated Windows installer uses the existing CI registration.
- Passed 83 MCP tests, five GitHub harness tests, TypeScript, and scoped
  ESLint. Browser authorization and a live tool read remain manual checks.

### 2026-11-09 — Post-merge retirement audit

- Removed the last signed store fixture files, package descriptor and
  version validation, unused routed-action events, and deleted-host lint
  exemptions. Presets now use the same definition validation as custom
  remote MCPs. Provider endpoints, OAuth checks and vault identities remain
  covered by the catalogue and connection tests.
- Updated the author entry point and harness instructions to use the
  current MCP connection commands and suites. Dated audit evidence remains
  in the linked reports. The external publisher is a separate operation.
- Passed 682 desktop tests, 152 core and SDK tests, 62 harness tests,
  TypeScript, scoped ESLint, and a check of documented suite names
  against the runner. Packaged browser consent and a live tool read
  remain separate checks.

### 2026-11-09 — Plan reconciliation and packaged handoff

- Restricted installed GitHub automation to preset preflight and catalogue
  discovery. The old live Agent modes depended on approval prompts that the
  automatic MCP dispatcher no longer produces. Use the controlled MCP
  workflow suite for write evidence and a user-run read for hosted acceptance.
- All 60 focused harness tests passed, including refusal of the removed
  live Agent modes before launching the installer. Request a fresh isolated
  Windows artifact from this branch for the packaged checks above.

### 2026-11-09 — Cleanup closeout

- The user reported that the installed test app worked after receiving the
  Windows installers. That confirms their smoke test. The individual
  non-owner consent, restart and custom-connection checks were not reported.
- The in-repository cleanup is complete. The remaining acceptance, UI,
  harness coverage and external registry work are tracked separately above.
