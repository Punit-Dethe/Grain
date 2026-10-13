# GitHub breadth, Notion delivery and extension ownership

Current checkpoint: 13 October 2026. This supplements the execution plan;
it does not reopen historical acceptance inventories or introduce more phases.

**Superseded work order, 14 October:** follow
[shared platform stabilization](PLATFORM-STABILIZATION.md) first. GitHub remains
the live reference; Notion and further provider-family batches are deferred.
The earlier sequence and preparation below remain context, not the active queue.
The contract-refactor hold applied to that previous testing block and is lifted
by this platform-first instruction. Live successes/failures and the pending
all-toolset/trace checkpoint are recorded in the stabilization document.

## Decisions

- Keep the Extensions tab available in the tester alpha. Hide custom MCP
  management in all release builds, including alpha, until its own acceptance
  checkpoint. Keep the implementation for development and `agent-harness`.
- Also hide the old hosted-provider validation panel in release builds so it
  does not become another route around the installed-extension journey.
- These are presentation gates, not new security boundaries. The existing
  backend, saved custom connections and their runtime permissions are unchanged.
  Store-installed account/settings controls remain available.
- Test broad provider catalogues with the normal Agent discovery/load path;
  do not restrict the catalogue to the two previously exercised GitHub actions.
  Broad account permissions do not imply automatic approval of arbitrary actions.
- Keep actual test writes confined to disposable objects. Inspect the whole
  advertised catalogue, then exercise representative actions by capability family.
- No new release or installer workflow is dispatched for this block. Batch the
  next necessary installed-alpha build after source checks and account setup.
  The already-running previous build predates these changes.

## What we observed, and what changes

### Provider/package boundary and the bounded live batch

The user confirms expanded GitHub grants and authorizes real disposable test
writes. They ask why GitHub changes required editing Grain. The current
descriptor only declares a transport URL; it cannot declare request options.
The `X-MCP-Toolsets: all` host special case is therefore a temporary configuration
gap, not a pattern to repeat for Notion or every new provider. Ordinary endpoint,
listing/artwork and provider option changes should belong in package repositories.
Record removal of that special case when the shared descriptor/connection contract
can carry validated, non-credential transport options; account secrets and protected
auth/protocol headers must stay host-owned. Do not start that contract refactor
while this user-requested testing block is running. Shared platform fixes and
diagnostics legitimately remain in Grain.

The bounded live batch reuses the existing installed-alpha script: create one
uniquely marked issue, update/close it, create one temporary branch and write one
synthetic file there. Actual approval arguments must match the exact test scope;
independent CLI reads verify effects and that main stays unchanged. No unrelated
account settings, source repository, automatic write replay or new fixture server.
Retain the closed issue/branch as inspectable test objects rather than deleting
them blindly. Normal Agent/capability tests cover broad cancellation, unknown
outcomes, schema loading and budgets; do not reproduce their whole matrix live.

Grain now logs small `agent-tool-trace` metadata at the real model boundary:
offered tool names/counts, selected offered names and a loaded-name-to-canonical-ID
map. Arguments, prompts, results, approval tokens and credentials are excluded;
unoffered model names are redacted. The script consumes only those new log entries
and restores the original log level. This uses ordinary diagnostics, not another
testing API/framework. Actual exposure cannot be inferred from tests or the old
binary: missing trace is a distinct Blocked result, even if writes passed.
One batched tester build is necessary for live all-toolset/trace acceptance;
do not create a production release or repeated per-case installers.

The installed binary successfully read the private test README before and after
restart (`github-alpha-iuyNiW`, four stages Pass, cleanup Pass). After the user
closed the app, `github-alpha-yTL7At` discovered **49 supported tools**, including
file writes, branches, issues, pull requests, repository creation and Copilot
actions. Both preflight and discovery passed; cleanup Pass. This is the current
default catalogue, not proof of all toolsets or successful execution of 49 tools.
The earlier open-app attempt (`github-alpha-5Kd48k`) was Blocked without stopping
the user's process. All reports remain under ignored `tests/agent-harness/.runs/`.

GitHub's official default endpoint selects default toolsets. Its documented
`X-MCP-Toolsets: all` header requests all available toolsets without changing
the resource URL. Grain now sets that header only for the exact official
`https://api.githubcopilot.com/mcp/` endpoint through the maintained SDK transport.
The verified extension/client/issuer binding and vault ownership stay intact;
Notion and unrelated hosts receive no such header. No descriptor republishing,
new OAuth app or client identity change is required for this transport change.
See [GitHub's remote server configuration](https://github.com/github/github-mcp-server/blob/main/docs/remote-server.md).

There are three different limits to measure:

1. **Advertised tools:** provider toolset configuration and token type determine
   what discovery returns. GitHub documents different filtering behavior for
   classic PATs, OAuth and other tokens; narrow grants do not universally shrink
   the visible list. [Scope behavior](https://github.com/github/github-mcp-server/blob/main/docs/scope-filtering.md).
2. **Executable actions:** GitHub App permissions, installation repositories,
   the signed-in user's own rights, organization policy and product entitlements
   determine which calls can succeed. More toolsets do not grant those rights.
3. **Model context:** Grain's existing tiered discovery selects and loads tool
   schemas. Keep that path and measure selection against the full catalogue.
   Do not equate a small selected schema set with a restricted provider catalogue.

The 128-tool, 2 MiB metadata, 32-page and operation-deadline bounds remain.
If live all-toolset discovery hits a bound, record the failure and adjust only
the demonstrated limiting bound, with its existing boundary tests. Never silently
truncate and call that full-catalogue acceptance. Unsupported schemas, account
restrictions and unavailable paid features must be recorded separately.

## GitHub account setup and breadth check

The user authorizes broad MCP-related test permissions. The earlier Contents
read / Issues write setup is superseded as a test policy, not silently reported
as already expanded. GitHub App settings require the owner's authenticated
settings session; the ordinary GitHub CLI login cannot edit that registration.
No Computer Use or account-token extraction is used to get around this.

Update [Grain GitHub Development permissions](https://github.com/settings/apps/grain-github-development/permissions),
save and accept the requested permissions on its installation. Reconnect in
Grain if a verified call still uses an older grant. No new client secret is
needed just because permissions changed.

| Capability family | Relevant GitHub App permission candidates | Verification target |
| --- | --- | --- |
| Files, commits, branches and releases | Contents read/write; Workflows write when editing workflow files | Disposable branch/file; verify bytes and branch independently |
| Issues, labels, comments and sub-issues | Issues read/write | Create, read, update and close one marked test issue |
| Pull requests and reviews | Pull requests read/write; Contents for branch operations | Draft PR between disposable branches; read/update/close |
| CI and workflow operations | Actions read/write; Checks and Commit statuses where the advertised tool requires them | Inspect an existing harmless run; trigger only a disposable workflow |
| Discussions | Discussions read/write where available | Use a test discussion/category; record missing repository setup separately |
| Security findings | Code scanning alerts, Dependabot alerts, Secret scanning alerts, Repository security advisories at read/write where offered | Catalogue and harmless reads; empty findings are valid, entitlement failures are separate |
| Projects, organizations and teams | Organization Projects read/write and Members read as applicable | Requires a disposable organization/project; personal-repo testing alone cannot certify this |
| Gists and stars | User Gists and Starring read/write where offered | Disposable gist or explicitly chosen star; separate from repository grants |
| Repository creation and governance | Repository creation and Administration as required by the exact provider API | Separate disposable-repository/ruleset procedure; never change app/source repositories |
| Notifications, Copilot and other remote-only tools | Exact endpoint/token support, account rights and subscription/policy requirements | Record supported, provider-restricted or untested per family; permissions alone are insufficient |

This is a family-to-permission setup map, not a claim that every listed permission
is necessary for every tool. Some fields vary with account/organization features.
Use [GitHub's per-endpoint App permission reference](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps)
and observed provider errors to resolve exceptions. Do not select unrelated
permissions merely because GitHub exposes them in the settings page.

Execution order:

1. **Source and account setup:** hide custom surfaces, request all GitHub toolsets,
   run focused checks and obtain the expanded installation grant.
2. **One necessary installed-alpha checkpoint:** verify the hidden surfaces and
   list all-toolset names/count/digest using `github-alpha.mjs --catalogue`. This
   needs a binary containing the transport change; the existing installed binary
   cannot certify it. Keep default-catalogue evidence distinct.
3. **Broad GitHub use:** run the issue/file/PR/read groups with the configured
   model and full catalogue. Verify actual effects independently, then check
   cancel/denial and disconnect/reconnect. Existing read/restart evidence stays
   accepted, while all-toolset behavior needs its own verification.
4. **Focused GitHub review:** fix observed faults and review the affected path.
   Record a per-family result rather than claiming every advertised action works.
   Unavailable organization/subscription features do not count as Grain passes.
5. **Notion acceptance**, then a small two-provider routing check. Do not launch
   an unbounded project to execute every possible provider API combination.

## Notion next

Use Notion's official hosted `https://mcp.notion.com/mcp` server, not a native
wrapper around its deprecated local package. Its public authorization metadata
currently advertises issuer `https://mcp.notion.com`, dynamic registration at
`https://mcp.notion.com/register`, S256 and authorization-code/refresh-token grants.
This is unauthenticated metadata evidence, not a completed Grain sign-in.

Prepare the same small store package: a tools-only MCP descriptor, author
`DESCRIPTION.md`, official artwork/source attribution and normal catalogue
submission. Reuse Grain's existing OAuth discovery/registration/vault/runtime
and the unified extension page. Do not add a Notion-specific auth engine.
Package preparation can overlap GitHub work; live Notion tests follow the broader
GitHub checkpoint. Do not publish it as working before those tests.

The user will choose/sign in to a disposable Notion workspace and authorize its
available read/write capabilities. Request the provider's normal full consent;
do not impose a host tool-name allowlist for acceptance. Keep writes under a
dedicated test parent. Exercise search/fetch, create/update page, create/query
database where available, cancellation/denial, sign-out/reconnect and restart.
Inspect the full live tool catalogue and record workspace/plan restrictions.
Finally enable both extensions and test one GitHub task, one Notion task and a
small cross-provider workflow with independently checked effects.
See [Notion connection instructions](https://developers.notion.com/guides/mcp/get-started-with-mcp)
and [current supported tools](https://developers.notion.com/guides/mcp/mcp-supported-tools).

## Who maintains five or six extensions?

| Owner | Responsibility |
| --- | --- |
| GitHub, Notion or another official hosted provider | Server operation, provider API implementation, tool schemas and provider-side fixes |
| Extension package maintainer, initially Grain or later community | Descriptor endpoint/auth declaration, listing/artwork, provider compatibility notes and package releases |
| Grain platform maintainers | One shared native/MCP runtime, vault/account ownership, Agent/tool approvals, store verification and install/manage UI |
| OAuth client registration owner | Registered app/client lifecycle and requested grants for providers requiring pre-registration |

The current GitHub repository contains a descriptor/listing, not a GitHub MCP
server implementation. Prefer official hosted MCPs so first-party packages stay
small. Use the existing CLI, signed catalogue and maintainer approval workflow
for both first-party and community packages; no new hosting/registry service.
Community contributions can start with PRs to the package repository. A maintainer
handoff needs an explicit owner and normal reviewed publication; do not assume
all packages will become community-maintained automatically.

GitHub's registered Grain desktop client remains platform-owned even if the
listing becomes community-maintained. Its current compiled binding is specific
to verified `com.grain.github` and the official endpoint. General third-party
pre-registered-client onboarding remains a separate unfinished contract. Notion's
advertised dynamic registration should use the shared path, subject to live
verification. Native extensions include their actual tool implementation, so
their publisher carries that additional code maintenance responsibility.
