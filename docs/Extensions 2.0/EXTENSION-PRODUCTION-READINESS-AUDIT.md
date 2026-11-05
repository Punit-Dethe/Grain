# Extension reliability and production readiness

Investigated 5 November 2026 against Grain `44cd5c7d8` on
`cleanup/retire-legacy-extension-platform`. This is a source/local-evidence audit,
not a new live-provider acceptance result. The user's unrelated generated-binding
edit is outside scope. No application, provider registration or publication changed.

## Reports and evidence

Two failures need separate investigation:

1. Other testers could not install GitHub or complete sign-in. The exact error,
   installer identity and failing step are unavailable. The maintainer succeeded;
   Notion has also worked. The other testers' root cause remains unconfirmed.
2. The maintainer reports that GitHub signs in and is enabled in extension-specific
   alpha builds, but Agent now cannot find its tools despite earlier success.

Read-only inspection of the local alpha profile found a verified-store GitHub
0.1.2 descriptor, `x-mcp-toolsets: all`, its account in `mcp_enabled_providers`,
Developer Mode off and no explicit per-connection client IDs. These facts do not
prove a currently usable token or that the profile matches every reported build.
No secrets or OS-vault credentials were retrieved.

The existing alpha `handy.log` contains this sequence on **2 November**:

| Observation | Meaning |
| --- | --- |
| `prepared`: `catalogs: 0`, `unavailable_catalogs: 1`, `offered_count: 1` | Initial catalogue acquisition failed; the Agent received only its discovery tool. |
| Two subsequent `loaded` traces contain `tools: {}` | Recovery searches also exposed no actions. This is not merely one desired action missing from a successful shortlist. |
| No `mcp-timing` event in that sequence | The log cannot identify how far provider setup progressed. Several early returns precede timing emission. |

This supports the reported symptom. It does **not** identify the failed network,
credential, protocol or validation stage, nor establish the installed binary's
source SHA. No live authenticated GitHub catalogue call was made in this audit.

## Confirmed source findings

### 1. Saved authentication is not verified tool availability

`grain_mcp_connections.rs::mcp_connection_status` sets `connected` from the
presence of a stored token response and reports `stored` unless a recovery
observation exists. `connect_oauth` saves/enables the account without listing
tools. The UI correctly says **Account saved**, but its Enabled switch cannot
certify successful provider negotiation or catalogue discovery.

Consequently, successful consent and enablement can coexist with no usable tools.
This is a shared MCP lifecycle boundary, including Notion; it is not proof that
GitHub issued a bad token. A saved grant can later expire, be revoked or encounter
provider/network failures.

### 2. Discovery failures lose the evidence needed for support

In `capability.rs::prepare`, failed catalogue fetches increment a count and discard
the reason. `handle_catalog` returns bounded failure reasons to the model, but
`search` logs only the successfully loaded tool map. `grain_mcp.rs::list_tools_until`
emits timing after opening the service; setup failures can return before it.
Protocol and catalogue errors also collapse multiple underlying failures into
generic messages. The inspected log therefore records the failure without a
diagnostic cause.

Keep SDK/provider strings private. Add bounded static stage/failure codes at the
actual error boundaries, with phase, task correlation, public provider identity,
elapsed time and accepted/rejected tool counts. Include early setup and recovery
failures. Do not log tokens, callback URLs, response bodies, user queries or raw
SDK errors. No additional background task or retained client is needed.

### 3. GitHub onboarding depends on registration and build identity

The existing [alpha onboarding record](GITHUB-ALPHA-ONBOARDING.md) explicitly
records **Grain GitHub Development** as private and says other accounts require
**Any account** installation visibility. That is a concrete historical explanation
for owner-only success. A read-only `GET /apps/grain-github-development` returned
404 on 5 November; that does not distinguish private, renamed or deleted apps.
Current settings must be checked by the registration owner before calling this
the testers' confirmed cause.

GitHub's [host integration guide](https://github.com/github/github-mcp-server/blob/main/docs/host-integration.md)
confirms that remote GitHub MCP does not support dynamic client registration;
GitHub App installation and organization policy can also restrict access.
Notion's [client guide](https://developers.notion.com/guides/mcp/build-mcp-client)
documents a different registration path. A Notion success does not certify the
GitHub registration or repository grants.

`grain_mcp_builtin.rs` and `grain_mcp.rs::builtin_client` restrict the compiled
GitHub desktop registration to verified `com.grain.github` at the exact official
endpoint. Both compile-time inputs must be present. `.github/workflows/build.yml`
passes them only when `github-extension-alpha` is enabled. Ordinary builds do not
inherit this identity, and runtime environment changes cannot add it afterward.
An existing saved grant can therefore be insufficient in a differently configured
binary. Keep the exact package/endpoint/issuer restrictions.

### 4. Normal release exposure remains a deliberate unfinished gate

`grain_mcp.rs::access_policy` permits store accounts/tools in debug or the dedicated
alpha identity, and refuses them in an ordinary release build. The Extensions
sidebar was exposed more broadly on 31 October without changing this backend
policy. A visible Store does not mean every installer can install/connect MCPs.
This can explain an installation refusal in the wrong build; it does not explain
the reported dedicated-alpha regression by itself.

Choose the intended distribution scope explicitly, then align frontend exposure,
backend access and build credentials. Do not simply remove credential isolation
or development-only custom-MCP restrictions.

### 5. Remote catalogue changes can break an unchanged package

The installed GitHub descriptor requests **all** toolsets. Grain bounds discovery
at 128 raw tools, 2 MiB of catalogue metadata, 32 pages and 512 KiB per SSE event.
Duplicate/invalid names and catalogue-limit failures reject discovery; unsupported
input schemas are excluded per tool. A complete catalogue with no admitted tools
can return successfully. Agent preparation also has a shared three-second budget,
while explicit recovery uses the normal 90-second operation deadline.

These are confirmed behaviors, **not confirmed causes of this incident**. The
earlier accepted GitHub catalogue had 100 tools; no current authenticated count
was measured. Provider growth, schema changes or latency can affect GitHub or
other MCPs without an extension update. Preserve resource limits; measure the
failed catalogue before adjusting bounds or changing the advertised toolset.
The observed two empty recovery attempts mean a short initial timeout alone is
not an established explanation.

### 6. Existing acceptance does not cover this ordinary-user failure

The six stabilization tester checkpoints remain valid for their recorded builds
and procedures. They do not certify every account or today's provider catalogue.
`tests/agent-harness/github-alpha.mjs` still requires Developer Mode and invokes
development catalogue status in preflight. Its workflow assertions also reference
the retired `load_extension` tool. It cannot serve as current Developer-Mode-off
store/Agent acceptance without correction.

The runner's expected installed-alpha executable is absent on this machine; the
running `Grain/handy.exe` was left untouched. No substitute UI harness or alternate
application render path was created. Existing logs were inspected read-only.

## Execution order and acceptance

| Priority | Work | Required evidence |
| --- | --- | --- |
| P0 | Preserve stage-specific discovery failures through initial preparation, recovery and support output using existing traces. | Controlled auth/setup failure, timeout, invalid catalogue and empty admitted set produce distinct safe reasons; no credentials/body/query leakage; existing cleanup remains intact. |
| P0 | Reproduce the installed-alpha no-tools failure with the exact binary, account and package; use the existing real-app runner after fixing its stale gates. | Fingerprinted binary/runner, account status, discovery stage/counts and a harmless repository read. Record success/failure before considering reconnect; no automatic replay of writes. |
| P0 | Verify GitHub installation visibility, repository grants, organization approval, callback and compiled registration for a non-owner. | Fresh non-owner account installs the Grain package, completes provider setup and reads an explicitly granted disposable repository with Developer Mode off. Separate Grain package install from GitHub App installation and consent. |
| P1 | Make saved-account state and last observed tool readiness separately understandable; provide actionable retry/setup guidance through existing commands/events. | Signed-in but failed discovery reports its cause; transient failures retain the account; definite refresh rejection asks for reconnect. Check on demand, retain no idle sessions/health polling. UI changes require the dedicated UI branch and user visual acceptance. |
| P1 | Certify the advertised provider/build/platform combinations and failure recovery. | GitHub plus Notion: fresh profile, restart, expired/revoked grant, denied consent, provider outage and catalogue rejection; account/build-specific tests cannot be replaced by fixture passes. Use affected existing suites rather than exhaustive tool execution. |
| Release gate | Resolve ordinary release access/registration, publication protection, real key backup/restore and renewal ownership. | Intended installer passes the complete signed-store-to-account-to-read journey; operator controls and recovery are exercised. Existing user-deferred custody decisions remain explicit. |

Fix the demonstrated failure stage before increasing catalogue/time limits,
replacing OAuth, adding providers or adding a new runtime. Retire stale runner
assumptions directly; do not introduce compatibility routes for retired tools,
old Grain formats or settings. The legacy platform cleanup is already complete
and is not reopened here.

## Verification limits

Graph-first exploration, current source/docs, public GitHub/Notion documentation,
read-only local profile/log projections and the GitHub App metadata lookup form
the evidence above. Tester error text, current App visibility and authenticated
provider failure details remain unavailable. This documentation change runs no
application/provider actions and claims no runtime fix or new acceptance pass.
