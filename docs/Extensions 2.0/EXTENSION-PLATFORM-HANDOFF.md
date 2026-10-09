# Extension platform: current understanding and integration handoff

**Updated: 10 October 2026.** Main integration is implemented on
`extensions/main-integration-20261010`; see the
[integration checkpoint](MAIN-INTEGRATION-CHECKPOINT.md) for reconciled boundaries,
automated results and the remaining installed-alpha acceptance.
It records decisions and observed results, not a frozen SDK or a production
readiness certificate. Read this first, then the linked implementation guides
and focused evidence when working on a particular area. Earlier phase numbers
and test inventories are historical; they are not the current work queue.

## 1. What we are building

An extension supplies tools that the Agent can discover and execute. There are
two implementations under the same user-facing extension concept:

| Kind | What the author supplies | Who runs it |
| --- | --- | --- |
| Native Grain tool extension | A tools-only manifest and JavaScript handlers | Grain's existing restricted worker |
| MCP extension | A descriptor for a remote MCP server, plus store listing assets | The provider runs the server; Grain connects through its MCP client |
| Direct/custom MCP connection | A user-entered name, HTTPS URL and authentication setting, optionally as JSON | The provider runs the server; no extension package or publication is required |

“Native” here does not mean unrestricted machine code or a companion executable.
A Spotify native extension could declare search, play, next and previous tools,
implement their service requests and let the Agent call them. It still needs
supported provider authentication, permitted destinations and declared tools.
It does not acquire access to the user's computer or Agent context.

Extensions do not receive screen capture, OCR, selected text, cursor access,
OS/Space integration, prompt packs, prompt priorities or prompt customization.
The Agent owns contextual features. Core Agent, Snippets and context preferences
are independent of the extension registry. Some old parsers/internal surfaces
remain in the tree, but this is not permission to expose them to authors again.

The application is unreleased and this alpha is for testers in contact with us.
No legacy extension/settings migration or compatibility framework is needed.
Branding, repository and OAuth registrations can change; testers may reconnect
or reinstall. Physical cleanup should follow the integration review, without
restoring obsolete behavior or deleting still-needed internal Agent machinery.

## 2. Current implementation and reuse

Both extension kinds and direct MCP connections feed the same Agent execution
and host-owned approval/account/lifecycle boundaries. An MCP descriptor is not
a JavaScript wrapper or a second implementation of the provider's API.

- MCP transport and supported OAuth discovery/registration/refresh use the
  locked official Rust SDK, `rmcp` 3.5.0. Current product transport is remote
  HTTPS Streamable HTTP. Local stdio/process MCPs are outside this launch scope.
- Native OAuth request construction, PKCE and refresh primitives use `oauth2`
  5.0.0. Grain retains destination admission, callback handling, account identity,
  grant validation and OS-vault ownership around that library.
- The existing native worker, network broker, connection registry, account vault,
  Agent loop and signed catalogue remain. No replacement agent framework, OAuth
  proxy or marketplace backend is required for the current launch.
- Selective tool discovery already exists: provider directory/search followed by
  loading relevant schemas and offering selected tools. Keep the working path.
  Further retrieval/efficiency work is deferred; it must not delay a small initial
  catalogue. Tool schemas or remote descriptions are not authority to bypass
  Grain's approval or destination rules.
- A dispatched write with an unknown result must not be replayed automatically.
  Approval, receipts, cancellation and account/source changes remain relevant
  even when we simplify the launch product.

See the [reuse plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md),
[research record](MCP-EXTENSION-RESEARCH.md) and
[native OAuth reuse audit](NATIVE-OAUTH-REUSE-AUDIT.md). These capture the earlier
multi-source investigation, including Goose/OpenCode and other maintained
clients. Their broader feature sets are references, not our launch requirements.

## 3. User experience we have converged on

**Store:** browse a listing, understand whether it is native or MCP, install it,
then enable it and authorize an account when required. Installation, enablement
and account connection are separate states. A successful install is not proof
that a remote server is reachable or that its tools work with the user's account.

**Installed:** manage the extension and its account in one place. Store-owned
MCP definitions are read-only; changing their URL/authentication belongs to a
reviewed store update. Native settings may exist when the tool genuinely needs
configuration, but screen/prompt/core-feature controls do not belong here.

**Your MCP connections:** sits alongside installed extensions, as the user
approved. Add a name, HTTPS Streamable HTTP URL and authentication through the
form or credential-free JSON. Manage sign-in, enable/disable, edit, sign-out and
removal there. Custom connections do not receive a store verification badge or
borrow another extension's account/client identity.

The current Installed layout was approved on 9 October. Keep the existing
Developer Mode release gate until a deliberate release decision; it currently
also gates MCP acquisition/account controls. The developer drawer supports
loading native projects. It is not the ordinary-user setup flow we ultimately
want to advertise for supported extensions.

Store text is **DESCRIPTION.md**, not README. README explains development and
maintenance. Listing descriptions and optional artwork are bound to the signed
catalogue. A verified package/listing does not freeze a mutable remote server's
tools or establish Grain's endorsement of every provider action.

Ordinary users of a supported first-party integration should install and sign
in without creating provider developer apps or entering OAuth client secrets.
Provider permissions must be explained separately from which tools the model
currently sees. A GitHub MCP server can offer more tools than our initial GitHub
App permissions permit; showing a tool does not grant permission to use it.

## 4. OAuth: who owns what

There is **no universal Grain API key for extension authors**. Four different
identities must remain separate:

| Identity | Purpose | Owner |
| --- | --- | --- |
| Extension ID/version/source | Identify the reviewed tool package | Author and registry metadata |
| Provider OAuth client registration | Identify the application requesting login | Grain for supported integrations, or the appropriate integration/server operator |
| User account tokens | Authorize that user's provider account | Connection-owned OS vault in Grain |
| Catalogue signing keys | Establish trust in published catalogue bytes | Registry maintainer; unrelated to user login or the app updater key |

For MCP, Grain follows the server's supported authorization metadata through
the SDK. Where the authorization server supports registration, use that flow.
Where it requires a pre-registered client, registration must be supplied. A
remote MCP operator may also manage its own downstream service authorization;
that is separate from Grain authenticating to the MCP server.

GitHub's current integration uses a pre-registered GitHub App. We created
[Grain GitHub Development](https://github.com/apps/grain-github-development).
Its callback is `http://127.0.0.1:31938/mcp/oauth/callback`. The app registration
is not the extension repository and does not publish anything. The maintainer
registers it once; each user authorizes their own account and chooses accessible
repositories. Testers do not each need to create their own app for this supported
store integration.

The registration is currently private/owner-account testing. Before other
accounts test it, change installation availability to **Any account** and verify
their own consent/repository access. The intended initial permissions are
Contents read-only, Issues read/write and automatic Metadata read-only. Expanding
the server's tool set is not a reason to request every GitHub permission at once.
This login path needs no webhook service or GitHub App private signing key.

The alpha embeds build-supplied `GRAIN_GITHUB_OAUTH_CLIENT_ID` and
`GRAIN_GITHUB_OAUTH_CLIENT_SECRET`, with PKCE. The desktop client credential is
extractable and is not treated as a confidential server secret or user token.
The resolver is restricted to the verified store owner `com.grain.github`, exact
resource `https://api.githubcopilot.com/mcp/`, and expected GitHub issuer/exchange
metadata. Custom/developer MCPs cannot borrow it. Explicit per-connection client
configuration has precedence and retains its own vault/issuer binding.

An ordinary development build without those compile-time inputs does not gain
this registration because the installed alpha signed in successfully. Changing
runtime environment variables does not retrofit the compiled client identity.
The alpha also has a separate settings/account profile,
`com.grain.extensions.alpha`; development and alpha settings are independent.

Refresh is intended to recover expired access tokens without routine browser
login when a valid refresh grant exists. Revocation, invalid refresh grants,
changed client identity or provider policy can still require sign-in. Do not
promise uninterrupted authentication for every provider from generic fixtures.
The actual issued Linear token-expiry check remains deferred, not passed.

### External developers: current contract and the remaining gap

- A native author can declare one supported public OAuth2/PKCE account: provider
  name, public client ID, authorization/token endpoints, scopes and API hosts.
  Grain handles consent and token custody. `grain.auth` exposes status/connect/
  disconnect, not raw tokens; `grain.net.fetch(..., { auth: true })` uses the
  extension's account through the broker. Provider-specific acceptance is still
  required. This is not arbitrary confidential-client or multi-account support.
- An MCP author packages endpoint/authentication metadata, not user credentials.
  The descriptor rejects headers, tokens, client IDs/secrets, host-owned account
  IDs and trust flags. Grain discovers the server's actual tools and manages its
  connection/account itself.
- Custom/pre-registered MCP users can configure OAuth app credentials through
  the existing account controls. Secret values belong in the vault, not JSON,
  source, chat or the signed descriptor.
- The built-in no-client-fields GitHub setup is specifically a first-party host
  integration. We have **not** built a general publisher-supplied OAuth client
  onboarding contract for every third-party pre-registered MCP. Decide that
  policy before promising seamless third-party installation; do not add secrets
  to descriptors or silently reuse Grain's GitHub identity as a workaround.
- A permanent public OAuth client document/domain was considered for servers
  supporting that identity mechanism. It is deferred. The experimental Vercel
  site is not a verified permanent client identity and is not needed by the
  current GitHub App path. Moving such a document can change OAuth identity and
  require consent again; hosting it is separate from hosting the catalogue.

## 5. Developer experience and capabilities

The maintained [CLI guide](../../crates/grain-ext-cli/README.md) and
[authoring entry point](../Extension%20Platform/AUTHORING.md) define the current
provisional profile. Do not send authors to old broad-capability specifications.

| Workflow | Current route |
| --- | --- |
| Create native tools | `grain-ext init`, declare tools in `manifest.json`, implement `grain.actions`, build and add valid icon |
| Try native tools locally | Developer Mode + Load unpacked; `grain-ext dev` supplies existing native hot reload |
| Package a remote MCP | `grain-ext init --mcp-url https://...`, then `doctor` and `pack`; `mcp.json`, no Node build or worker |
| Validate/package | `grain-ext doctor`, `grain-ext pack`; canonical metadata/artifact checks, no automatic authentication or publication |
| Prepare registry submission | Commit/tag source, then `grain-ext submit --registry ... --repo ... --tag ... --commit <full SHA> --license ... --contact ...` |

Native generated types expose validated arguments, a bounded result/error
contract and a read-only idempotency key when available. Grain owns confirmation
and provenance. Supported services include own storage, allowed brokered network,
account metadata and logging. There is no ambient transcript, other-extension
catalogue, daemon subscription, arbitrary UI or screen/prompt/OS API.

Current native result support is bounded; nested JSON is not promised as arbitrary
structured model output, and author-generated follow-up interactions are not
advertised. Use the generated contract rather than assuming an unrestricted SDK.
An idempotency key only helps when the provider supports it; it does not promise
automatic deduplication, rollback or safe replay.

Future developer UX can improve through prebuilt CLI distribution, clearer error
messages and simpler submission instructions. The source package, descriptor,
listing, host credentials and registry publication are already distinct enough
to improve these without replacing the runtime. Do not start a new developer
portal/backend before validating demand and the missing OAuth policy.

## 6. Publishing and hosting: what actually happens

Three GitHub resources have different jobs:

| Resource | Current location | Contents/purpose |
| --- | --- | --- |
| Grain app | `Punit-Dethe/Grain` | Desktop runtime, SDK/CLI/maintainer tools and alpha build workflow |
| GitHub extension source | `Punit-Dethe/grain-github-extension` | Descriptor, DESCRIPTION and artwork; GitHub operates the remote MCP server |
| Extension registry | `Punit-Dethe/Grain-Extention` | Reviewed submissions and published signed catalogue/artifact/listing files |

An external author normally owns their source repository. They need not have
write access to Grain's repository. `submit` creates a local draft under
`extensions/<id>/`; the author opens a registry PR. Maintainer review, isolated
build/preparation and provenance verification precede signing and publication.
The Store fetches the published catalogue, not an arbitrary author's HEAD or
an OAuth App page. MCP packages contain canonical descriptors; native packages
contain the accepted worker artifact. Neither publishes user credentials.

The existing route is GitHub-hosted static signed content; it needs no new paid
marketplace service, database or authentication gateway. Native build execution
does not receive publisher signing keys. Data-only preparation/publication gates
bind exact source/artifact/listing bytes and verify their provenance. Serving
checks verify actual public bytes and catalogue freshness, not just CI success.

The first public GitHub entry is version 0.1.0, source/tag pinned to
`3d5e1ec25e083613aa7b76c088a93ce49c24ecb5`. Publication accepted index 3 at registry
commit `ea7498aebc028e35579878f5af5f9b1c329c48a2`. Public base:
`https://raw.githubusercontent.com/Punit-Dethe/Grain-Extention/main/v1/`.

For this exact first-party tester submission only, the user authorized explicit
maintainer approval instead of a second approving GitHub account. The temporary
policy is bound to the exact submission, maintainer/registry and alpha publisher;
it does not fake an independent review or relax normal third-party publication.
Replace/retire this exception when deciding permanent publishing governance.

Catalogue signing and app-updater signing are distinct. The existing
`TAURI_SIGNING_PRIVATE_KEY` was not reused for catalogue trust. Dedicated encrypted
alpha signing roles are kept outside repositories at
`C:\Users\watrm\AppData\Local\GrainSigning\extensions-alpha-20261009`.
Do not copy private keys or unlock files into this handoff. Secure backup/custody
and permanent registry protection remain operator work. Renew signed catalogue
metadata before its earliest **8 November 2026** expiry; that is unrelated to
OAuth access-token expiration.

The GitHub source README still describes its earlier candidate state. Treat the
app onboarding/progress records as the current status; update public source
documentation in a later reviewed source revision, without moving `v0.1.0` or
rewriting already reviewed immutable bytes.

## 7. What is actually verified at this handoff

| Area | Accepted evidence | Limit / remaining work |
| --- | --- | --- |
| Owned native and MCP Agent workflows | Earlier configured-model read/write/verify and controlled failure/recovery checkpoints passed | Not certification of every real provider, model or OS path |
| Custom MCP management | Form/JSON, connection ownership, SDK/OAuth/vault lifecycle implemented; controlled app checks and earlier user Linear tests passed | Actual issued Linear token-expiry check deferred |
| Current extension management UI | User approved Installed layout; current native/MCP acquisition and management checks passed | Launch gating/onboarding and third-party credential UX still need deliberate decisions |
| GitHub publication | Real source/provenance/signing, public HTTP delivery and genuine store visibility/installability passed | Permanent governance and catalogue renewal remain separate |
| GitHub install and login | User reports successful store installation and GitHub sign-in/connection | This alone does not prove a completed GitHub tool action |
| Corrected installed Agent | On 10 October the user reports the replacement works after the Agent-start fix | Record Agent opening as passed; do not infer all read/write/restart cases from that report |
| GitHub read/write/restart/disconnect batch | Procedure documented using the private disposable test repository | Still awaiting explicit completion reports for these specific provider checks |

The accepted corrected installer was built at
`a2215c73ad08f8394b76006045a74645cc25aacc`,
[run 37955644860](https://github.com/Punit-Dethe/Grain/actions/runs/37955644860).
Local installer:
`C:\Projects\Grain\grain-github-alpha-build-37955644860\nsis\Grain Extensions Alpha_0.0.6_x64-setup.exe`.
SHA256: `2559126b2da43ba197be0ad615eadc7caceedc637a8f3a65255d8d2d2228dd73`.
Installing a replacement is manual; CI does not update a running desktop app.
This build is a separate alpha artifact, not a beta updater release.

### The Agent-switch regression and the lesson for integration

Retiring built-in extensions changed `extension_set_enabled` to reject core
Agent/Snippets IDs, but their UI switches still called it. Generated bindings
returned a structured error that the settings store ignored. The switch could
appear enabled without changing the backend setting or registering the shortcut.

The fix added separate core Agent/Snippets commands, registered them with Tauri,
updated the bindings/settings store and handled errors so rejected changes roll
back. Agent registers/unregisters its shortcut independently of extensions.
Four frontend regressions exercise the real bindings with only IPC mocked; two
existing shortcut conflict tests, TypeScript and production frontend build pass.
The corrected Windows alpha built successfully and the user now confirms it works.

The old harness enabled Agent directly, so its runtime checks bypassed this UI
path. A passing harness is not proof that release settings/registration work.
Keep the focused regression and one installed-app checkpoint. Do not solve this
coverage gap by building more harness infrastructure or retaining obsolete APIs.

## 8. Bringing the branch up to date with main

Read-only snapshot after fetching `origin/main` on 10 October:

| Ref | Commit |
| --- | --- |
| Extension branch before this documentation commit | `556c62d5e46920363c0c45d3fdab8b650799334c` |
| Fetched app `origin/main` | `bbae21ccadc4c6e8d65263e207d644db41ab74ad` |
| Common ancestor | `076c8d7918c99f950a2ca6af0cd5248ed9045bbd` |

At that snapshot the branch is **147 commits ahead / 57 behind**. Since the common
ancestor, 262 paths changed on the extension branch and 178 on main; **21 paths
changed on both**. These are path intersections, not a predicted merge-conflict
count or a completed integration audit. Recheck refs/counts before integration.

Main is at app version 0.0.8; the tested alpha artifact is 0.0.6. Main also moved
the recording pill from the native multicall/`--pill` architecture to the Handy
WebView overlay, changed Agent input behavior, simplified onboarding and changed
settings/provider/theme UI. Many relevant paths changed only on main, so resolving
the 21 intersections alone is insufficient to prove runtime compatibility.

The intersecting implementation surfaces are:

- `src-tauri/src/agent.rs`, `grain_commands.rs`, `lib.rs`, `grain_onboarding.rs`:
  Agent lifecycle, command registration and initialization.
- `events_auth.rs`, `events_server.rs`, `extension_shortcuts.rs`, `host_api.rs`:
  internal event/auth/dispatch boundaries.
- `src/app/stores/settingsStore.ts`, generated `bindings.ts`, extension runtime
  implementation/tests and the older `ExtensionSettings.tsx` surface.
- Core/SDK `lib.rs`, Cargo manifests/lockfiles, `package.json`, `vite.config.ts`
  and README. Generated bindings and locks require final-source regeneration
  or dependency resolution, not arbitrary selection of one entire file.

Suggested integration sequence when the user starts that work:

1. Record/checkpoint any meaningful local changes and preserve the tested extension
   branch and registry identities. Current unrelated work-tree changes include
   `src-tauri/Cargo.toml` and generated `src/app/bindings.ts`; inspect rather than
   discard or sweep them into a merge. Do not touch private credentials.
2. Integrate main into the extension branch in a reviewable checkout/commit.
   Preserve main's newer recording/onboarding architecture, then adapt Agent and
   extension hooks to it. Do not restore the old pill solely to avoid a conflict,
   and do not resurrect retired extension APIs from older main-side references.
3. Reconcile the core-feature commands, settings writers, Tauri registration,
   generated bindings and frontend together. Keep the verified GitHub identity
   restriction, catalogue trust pins, native/MCP validators and connection ownership.
4. Run affected unit/type/build checks at that checkpoint. Regenerate bindings
   from the final registered commands. Resolve lockfiles through the final
   manifests; retain the chosen SDK/OAuth reuse rather than rebuilding protocols.
5. Use one real alpha checkpoint: first-run enablement, Agent shortcut/input/close,
   ordinary dictation, custom MCP management, store GitHub connection/read and one
   approved disposable write. Verify both a fresh profile and restart behavior.
   Preserve action receipts; do not automatically repeat a possibly completed write.
6. Review the integration diff, record failures separately and fix those affected
   boundaries before resuming new extension features. Existing tests should run
   where relevant; no new harness or blanket rerun of historical scenarios is
   required merely because the commit count is large.

The snapshot and sequence above describe the pre-integration handoff. The
subsequent implementation merged this main snapshot in an isolated checkout;
see [MAIN-INTEGRATION-CHECKPOINT.md](MAIN-INTEGRATION-CHECKPOINT.md). Registry and
OAuth identities were preserved. No original dirty checkout was reset.

## 9. What comes after integration

1. Complete/record the remaining specific GitHub read/write/restart/account checks
   on the integrated alpha, using `Punit-Dethe/grain-github-alpha-test` only.
2. Resolve the ordinary-user release gate and the small number of necessary
   onboarding/configuration improvements exposed by real use.
3. Before inviting external publishers, settle their pre-registered MCP OAuth
   client onboarding, publish clear current SDK/CLI instructions and review the
   normal third-party approval route. This does not require a new marketplace.
4. Add the next supported provider using the same runtime, with provider-specific
   permissions/authentication acceptance. Reuse the GitHub lessons; do not claim
   that GitHub login acceptance automatically covers Notion, Calendar or Slack.
5. Handle permanent branding/client identity, registry governance/key custody,
   alpha distribution policy and deferred expiry verification before making the
   corresponding production promises. Clean up obsolete code deliberately once
   the integrated ownership boundaries are understood.

Keep new work in coherent product blocks. Use fast affected checks while coding,
one integration checkpoint per block and a focused audit of changed boundaries.
Maintain this document and the [progress ledger](MCP-EXTENSION-PROGRESS.md).
Use [GitHub onboarding](GITHUB-ALPHA-ONBOARDING.md) for exact account/build steps,
the CLI guide for author contracts and the maintainer tool guide for publication.
Old dated audits retain their evidence but do not override these current decisions.

## Source ownership map for the next engineer

| Concern | Main trace points in this branch |
| --- | --- |
| Public native handler/services contract | `crates/grain-sdk/src/authoring.rs`, `manifest.rs`; `crates/grain-ext-cli/README.md` |
| MCP package/connection definitions | `crates/grain-sdk/src/mcp.rs`, `crates/grain-core/src/mcp_connections.rs` |
| MCP transport/OAuth and owner checks | `src-tauri/src/grain_mcp.rs`, `grain_mcp_connections.rs`, `grain_mcp_builtin.rs` |
| Native account/network boundary | `src-tauri/src/grain_auth.rs`, `host_api.rs`, `extension_host.rs` |
| Agent discovery and execution | `crates/grain-core/src/capability_agent.rs`; `src-tauri/src/capability.rs`, `agent.rs` |
| Core settings switch/shortcut regression | `src-tauri/src/grain_commands.rs`, `lib.rs`; `src/app/stores/settingsStore.ts`, `settingsStore.test.ts`, `bindings.ts` |
| Store/custom MCP UI | `src/app/pages/ExtensionsPage.tsx`, `src/app/extensions/McpConnections.tsx`, `StoreCard.tsx` |
| Store admission/trust | `src-tauri/src/grain_store.rs`; `crates/grain-core/src/trust.rs` |
| Authoring and publication tooling | `crates/grain-ext-cli/`, `crates/grain-registry-tools/`; separate registry workflows |

This map describes the extension-branch implementation. Main's overlay and
Agent changes must be traced as part of integration rather than assuming these
paths and internal event consumers are unchanged after the merge.
