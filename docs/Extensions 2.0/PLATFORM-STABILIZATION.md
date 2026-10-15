# Extension platform: publishing and release work order

Updated 15 October 2026. This is the **single active forward plan** for finishing
extension authoring, review, publication and store delivery. It replaces this
file's earlier chronological work order and the historical E4–E8 phase counts.
Completed evidence stays in [progress](MCP-EXTENSION-PROGRESS.md) and the focused
audit documents; it is not a fresh backlog. Six bounded blocks are defined below: **Block 1 complete; five remain**.
Current implementation: Block 1 is complete for the supported author workflow.
Local/three-OS checks, artifact delivery verification and focused source audit
pass. Blocks 2–6 remain unopened. No application runtime, account, custody or
catalogue change. No new installer or stable public SDK release.

## Target experience

- **Author:** get the supported CLI, create a native tool or remote MCP project,
  develop locally, validate/package, submit a new extension or update, see useful
  review feedback and publication status. No Grain source checkout is needed.
- **Maintainer:** review the source/listing/account prerequisites, approve the
  exact candidate, run one documented publication entry point and see a confirmed
  live result. No source-code allowlist edit for each extension version.
- **User:** see one extension card, install, sign in where needed, use its tools,
  update, disable, reconnect or uninstall. Native/MCP remains an implementation
  detail. Custom MCP management stays hidden in alpha/release for now.

These are different approvals: source publication approval, user account consent,
and Agent execution policy. Do not conflate them. The current user-approved
Agent automatic execution policy remains; this plan does not restore per-call
confirmation dialogs.

## What is already working — retain it

| Area | Current evidence and limit |
| --- | --- |
| Runtime | Native tools and remote HTTPS Streamable HTTP MCP descriptors share the Agent capability path. SDK transport/OAuth reuse, vault ownership, lifecycle, cancellation and unknown-outcome handling exist. Not a promise to support every MCP transport/server/schema. |
| Authoring | `grain-ext init`, native `dev`, `doctor`, `pack`, typed native declarations, MCP descriptors and local `submit` drafting exist. Tools-only checks reject retired host capabilities. Public API remains provisional. |
| Listing | Separate DESCRIPTION, 512×512 PNG icon, up to six wide gallery images, signed sizes/hashes and client checks exist. User accepts the current artwork/detail layout for now. |
| Review/build | Pinned source/tag, isolated native builds, fresh trusted preparation, genuine CI attestations, exact review and signing checks exist. Native author execution is separated from signing authority. |
| Hosting | Signed roots/index/revocations, content-addressed assets, metadata freshness/cache, conditional publication and anonymous delivery checks exist. GitHub remains the host. |
| Real extensions | GitHub 0.1.2 and Notion 0.1.1 are live in signed index 7. Publication run 38701547203 confirms remote Git plus 55 public files/116 HTTP requests. Representative account/Agent workflows already passed; not every provider tool is certified. |
| Store identity | Duplicate historical release cards fixed in 608fb263. Browse/details/icons now share latest eligible publication selection. Real Windows store suite: 12 Pass, one existing live-network test ignored. User confirmation of the two-card view remains pending. |
| Tool selection | Existing lexical shortlist, compact directory and exact recovery have a completed bounded GitHub/Notion checkpoint. No new retrieval research, embeddings or Agent rewrite in this work order. |

## Audit baseline: concrete gaps, not assumed missing implementations

| Finding | Basis inspected | Required action |
| --- | --- | --- |
| Authors had to compile the CLI from Grain source (fixed in Block 1) | `crates/grain-ext-cli/README.md` | Distribute a versioned CLI with clear supported OS/runtime requirements; exercise installation from a clean author directory. |
| Native scaffold needed supplied artwork to pass doctor (fixed in Block 1) | CLI scaffold and tests | Provide a clearly identified valid starter icon or an equally simple documented setup; align runtime/listing icon instructions. Do not silently copy unrelated branding. |
| Submission drafting lacked updates (fixed in Block 1) | CLI guide and `crates/grain-ext-cli/src/lib.rs` | Explicit safe update mode; preserve an existing draft on invalid input/failure. |
| Submission stops at local files | CLI guide | Small optional GitHub PR submission/update helper using existing Git/`gh`; keep manual/offline drafting usable. Authenticate through `gh`, never store a second GitHub credential. |
| Permanent ownership/review governance is unfinished | `review.rs`, `approval.rs`, registry REVIEW-POLICY | Define ID ownership, authorized updates and maintainer eligibility; replace exact tester hash exceptions with a protected operational approval policy. |
| Publishing needs too much manual assembly | Actual GitHub/Notion artwork publication; existing CLI commands | One orchestration entry point around existing prepare/sign/assemble/export/activate commands; no parallel implementation of their checks. |
| Tooling pins can drift | Corrected stale producer commit and PNG capture omission in PRs 6/7 | One reviewed tooling version per publication; complete-path regression includes icons/gallery. Make mismatch diagnostics visible. |
| Error diagnostics hide useful causes | `ci/publish_github.py` captures child stderr, then reports CalledProcessError | Bounded, secret-safe stage/error output and a useful final status. Do not log keys, unlock files, tokens or raw credential-bearing requests. |
| GitHub protection is absent | Read-only API check on 15 October: publish environment `protection_rules: []`, no deployment branch restriction; main protection 404; rulesets `[]` | Configure a workable approved deployment/review policy before public community publication. Do not assume an environment name protects it. |
| Author OAuth onboarding is only a supported profile | CLI guide, host public-client validation, first-party identities | Publish the support matrix, validate declared client/issuer prerequisites and prove an independent author's supported route without a Grain code change. |
| Store MCP install/auth still depends on Developer Mode | `grain_store.rs::store_mcp_install`, shared `require_developer_mode` callers | Separate supported-store enablement from custom/development provider gates before release. Do not expose custom MCP editing as a side effect. |
| Native/MCP compatibility projection differs | SDK native `validate_tool_only` vs MCP `validate_mcp_installable` | Choose and test consistent latest/unsupported/update behavior. No automatic downgrade to evade incompatibility or revocation. |
| Operations and release handoff are not accepted end to end | Renewal/custody audits, temporary local tester custody, stale historical docs | Establish renewal, withdrawal, recovery and ownership runbooks; consolidate public instructions and run a bounded final acceptance. |

Repository settings above were **inspected, not changed**. Runtime findings are
source observations; items marked for verification are not newly demonstrated bugs.

## Architecture to keep

Author repository → source-pointer registry PR → unprivileged validation/build →
trusted candidate preparation/provenance → exact authorized review/signing →
conditional static publication → verified store install → connection/tools.

The registry contains source references and signed distribution files. Native
implementation stays in the author's repository; MCP implementation stays with
its server provider. Grain does not fork GitHub/Notion servers or host arbitrary
community MCP servers. A normal extension addition must change its own source
and registry submission, not Grain application code or a compiled hash allowlist.
Actual new protocol/auth capabilities may still require an application change.

Keep current GitHub hosting and existing signer/verifier. A website is not needed
for this store route. A future OAuth public client document may need stable HTTPS
hosting for providers requiring that registration method; that is a separate
provider prerequisite, not a store-hosting dependency.

## Block 1 — Finish the supported author contract and local workflow

**Complete, 15 October:** native scaffolds include a neutral valid PNG;
`submit --update-from <version>` stages/revalidates complete drafts and preserves
the old draft on refusals or ordinary replacement failure. Native/MCP update,
source/kind/version checks, stale locks and recovery are documented. A small
read-only three-OS artifact workflow prepares executable/guide/checksums/source
identity, with no installer or release. Local 17 CLI + 29 checker tests pass;
generated TypeScript/type-negative/bundle checks pass. [Artifact run
38711004583](https://github.com/Punit-Dethe/Grain/actions/runs/38711004583) passes
Windows x64, Linux x64 and macOS arm64 (17 CLI/29 checker tests on Windows;
18 CLI/29 checker tests on Unix). All three bundles were downloaded and their
checksums/source identities verified. The delivered Windows executable also
passes clean MCP init/doctor/pack without a Grain checkout. Native CLI packaging
and new/update drafting passed separately from a clean generated npm project.
Focused source audit passes. PR automation stays in Block 3; SDK freeze/public
release is not implied. No user test is required to close this CLI-only block.

Keep the current native and remote MCP formats; reconcile the
public guide/generated declarations/checker/host behavior before calling the SDK
stable. Clearly state native worker restrictions, supported argument/result/error
forms, brokered network/storage/auth, MCP transport/auth support, public headers,
listing rules, versioning and cancellation/unknown-outcome expectations. Retired
screen/prompt/OS features stay absent from the supported author surface.

Implement together:

1. Smooth the valid native/MCP starter journey, including icon setup and actionable
   diagnostics. Validate generated native code with the generated types.
2. Add explicit submission-update drafting with identity/version/source checks;
   never silently overwrite another ID or partial/corrupt draft.
3. Publish accurate commands for build/dev/pack/new submission/update. Separate
   runtime package files, listing images and source development documentation.
4. Prepare versioned CLI distribution through the existing GitHub release/build
   infrastructure, with checksums and minimum versions. No new application
   installer release now; choosing an actual public CLI release is a later action.

**Files:** `crates/grain-ext-cli`, `crates/grain-extension-checks`, supported SDK
contract/declaration generator and their guides. Touch host validation only for
an observed producer/consumer mismatch.

**Done:** a clean author directory can create, build/validate/package both kinds,
draft v1 then v2, and get precise failures without losing its existing work.
Normal CLI/type/checker tests cover this. No live OAuth or new harness needed.

## Block 2 — Make source ownership and approval a real policy

Record extension IDs with authorized source repository/maintainers in a small
protected registry policy. Define new-ID reservation, updates, transfers,
maintainer changes and withdrawals. GitHub identity and source ownership matter;
a display name or reverse-DNS-looking ID alone proves neither.

Use the user's existing choice of **explicit maintainer approval** as the initial
operating model. Distinguish external author submissions from first-party/self-
authored ones. External work can use ordinary independent GitHub review; a
maintainer's own release needs a deliberate protected approval record/workflow,
not a fabricated self-review or a newly compiled digest exception. Approvals bind
source/listing/produced artifact, expire and are invalidated by changed input.

Choose how that authorization is enforced before replacing the tester exception:
eligible reviewer list, protected environment and/or protected operator policy,
with a narrow publication writer compatible with branch rules. Do not enable a
rule requiring PR-only writes while leaving the current exact-lease writer unable
to operate. Prefer the smallest workable GitHub mechanism; do not build an IAM
service or force a second human into the tester workflow without agreement.

**Done:** unauthorized ID takeover, stale/revoked approval and changed-source
publication refuse; authorized external and first-party paths pass without an
application rebuild or source edits per release. Test using disposable policies
and existing signature/Git fixtures. Actual repo protection/custody changes are
reviewed concrete operator changes, not hidden in a code patch.

## Block 3 — Reduce publishing to one maintainable operator workflow

Reuse existing trusted CI and local/protected signer. Provide one documented
maintainer entry point that selects the reviewed candidate, downloads/verifies
its evidence, prepares the exact policy, signs only after authorization, assembles
and verifies the serving bundle, and presents the final publication tuple.
Then the existing protected publish action activates it and records public HTTP
status. The final approval must be meaningful and reviewable, not approval of
unknown hashes. Signing can remain operator-local; moving keys to CI is not a
prerequisite for a simple workflow and needs its own custody decision.

Add the small optional author PR helper here, once Block 2 ownership rules are
known. Reuse `gh` for fork/branch/PR creation and update; avoid resetting user
branches, overwriting unrelated changes or silently publishing dirty source.

Keep build success, source approval, signed candidate, confirmed Git activation
and public delivery as separate visible results in the same command/run summary.
A failed public check after a confirmed push reruns the read-only check only.
Concurrent changes require re-preparation; uncertain outcomes are inspected.
Batch two extensions without duplicating the pipeline. Resolve tool version pins
once and check producer provenance against that selection. Surface bounded child
errors at the failing stage.

**Done:** one native and one MCP candidate plus one ordinary update traverse the
real production command sequence using existing disposable signed/Git fixtures.
Include PNG/gallery, stale base, interrupted/failed staging and delivery failure.
Run one real already-configured provider update only after the complete path
passes. The next ordinary version needs neither Grain edits nor hand-written
signing scripts. No new publishing server/database/portal or Agent harness.

## Block 4 — Make account onboarding portable for publishers

Keep the official MCP SDK/OAuth library path. Document and validate these cases:

| Route | What the author supplies | What the user does |
| --- | --- | --- |
| No authentication | Explicit public HTTPS MCP endpoint | Install/enable; no sign-in. |
| Provider-supported dynamic registration | MCP endpoint; correct provider discovery | Browser account consent through Grain. |
| Pre-registered public desktop client | Public client ID + exact issuer; compatible loopback/PKCE/provider metadata | Browser consent; no developer-app registration per user. |
| Secret-required client / special provider integration | Explicitly supported host or publisher service route, separately reviewed | Supported account setup; never a secret embedded in a public extension. |

Verify which first-party identities are compiled/build supplied today, especially
GitHub. Do not claim a public package can carry confidential credentials or that
every MCP provider supports DCR/public clients. Define the extension author's
responsibility for provider registration, permissions and endpoint availability.
A provider requiring a confidential client is a support decision, not grounds to
invent a universal OAuth proxy now. Unsupported store onboarding must be identified
before publication, rather than sending ordinary users to developer settings.

**Done:** a supported publisher-owned public-client or registration route works
without adding the provider to Grain source; cancel/reconnect/update/account
separation retain existing tests. One eligible live account check, not every
OAuth test again. Deferred real Linear token-expiry observation stays deferred;
refresh logic tests already exist. A user should not need to log in every day
when their provider issues a usable renewable grant.

## Block 5 — Close the store install/update/account journey

Keep the accepted layout. Check one-card identity across browse/search/details/
installed, latest publication and API compatibility, Installed vs Update states,
version changes and consistent icon/gallery selection. Never offer a downgrade
as an update or silently substitute a revoked release. Explicit publisher order
and supported version policy must agree; avoid a second ad hoc version comparator.

Finish setup/action states: not installed, installing, needs sign-in, connected,
needs attention, disabled, update available, updating, failed and removed. Use
existing state/commands; do not create a new state-machine framework. Account
binding changes require fresh consent; unchanged updates should retain the account.
Failed updates preserve the working installation. Description links/content stay
sanitized and image failures have a usable fallback. Recheck keyboard focus and
small-window behavior only on affected controls.

Separate supported store extensions from Developer Mode for the intended alpha
build, once its gate is explicitly accepted. Keep custom MCP form/JSON management
hidden and keep backend validation/account ownership. Native and MCP remain one
user-facing extension collection.

**Done:** one native and GitHub/Notion exercise install/setup/action/update/failure/
uninstall on the real app; reuse prior passes where the behavior did not change.
Ordinary component tests cover most states. One small user visual batch at the
checkpoint; no fresh installer for each edit.

## Block 6 — Operations, contributor handoff and release acceptance

1. Document current key custody, separate root/publisher roles, backup/recovery
   and authorized access without copying any secrets into the repository. Decide
   when tester identity becomes the release identity; branding migration is not
   an implementation blocker for today's tester work.
2. Exercise metadata renewal before expiry, withdrawal/revocation of a bad release,
   recovery by publishing a corrected version, and failed/stale publication.
   Existing cryptographic/unit fixtures do most of this; no insecure rollback.
3. Define who responds to provider outages, OAuth registration changes, abuse and
   abandoned extensions. A first-party listing is a thin package, not a promise
   to maintain the provider's server. Community source changes still get review.
4. Keep a short author guide, reviewer checklist and operator runbook as the public
   entry points. Update stale registry links/gates; make old audits clearly
   historical. Record/retire test-only helpers and redundant procedures in the
   existing harness inventory rather than add another inventory framework.
5. Run affected unit/integration suites and a final bounded real-app batch. Build
   the intended installer once only when release packaging is needed. Verify only
   the OSs/providers being advertised; label other support honestly.

**Done:** a second contributor can follow the docs without private chat context,
release gates are explicitly accepted and one final signed-store/application
journey passes. No claim of exhaustive provider/model/OS correctness.

## Implementation locations

Paths are relative to the Grain repository unless prefixed `registry/` (the
separate `Punit-Dethe/Grain-Extention` repository).

| Block | Existing code/documents to work in |
| --- | --- |
| 1 | `crates/grain-ext-cli/src/lib.rs`, CLI guide, `crates/grain-extension-checks`, supported declarations/validators in `crates/grain-sdk`; existing release workflows only for CLI distribution. |
| 2 | `crates/grain-registry-tools/src/review.rs`, `approval.rs`; registry `REVIEW-POLICY.md` and protected ownership/approval configuration. |
| 3 | `crates/grain-registry-tools`, its `ci/publish_github.py` / `check_hosted_github.py`, registry `.github/workflows` and `ci/provenance`; reuse existing assembly/capture/hosting modules. |
| 4 | `crates/grain-sdk/src/mcp.rs`, host `src-tauri/src/grain_mcp.rs` and shared MCP connection/auth adapters; publisher guide and first-party source descriptors. |
| 5 | `src-tauri/src/grain_store.rs`, `src/app/pages/ExtensionsPage.tsx`, existing account/settings components and MCP gate callers; keep custom management hidden. |
| 6 | Existing operator/author guides, registry contributor docs/workflows, existing normal test suites and `tests/agent-harness` documentation/retention inventory. |

## Verification and delivery discipline

- Implement a complete small block, run affected fast tests, then perform its
  scoped audit. Fix material findings before the next dependent block.
- Keep current tests in place; reuse them instead of rebuilding the harness.
  Source validation/policies/signatures/publication races use normal tests.
  Existing real-app harness is only for behavior that needs a real application.
- No full installer on ordinary iterations. Use the dev app and existing cached
  test builds. On Windows, run host tests from `src-tauri/` with its configured
  target cache and existing activation-manifest runner.
- Manual work is limited to actual provider consent, protected operator decisions
  and a small grouped UI check. Do not make the user repeat old accepted cases.
- Per block report: changed behavior, affected checks, scoped audit result,
  remaining user action and which of these six blocks are complete.
- Do not derive a new pending-test total by adding historic counters. Current
  acceptance requirements below are the active list; past failures/passes stay
  traceable in progress/audits.

## Current user/operator dependencies

Block 1 needs no user test. Nothing is needed from the user to begin Block 2
implementation; actual protection/custody changes remain later operator decisions:

- Confirm the two-card store view after the recent fix; current artwork is accepted.
- Confirm concrete publishing protection/custody changes before activation.
- Approve one eligible live browser sign-in if Block 4 needs it; no credentials in chat.
- Check the grouped final store setup/update states on the real app.
- Decide public release identity/platform scope before the final release, not now.

The deferred Linear expiry observation, advanced provider tool families, custom
MCP release exposure, extra first-party extensions, automatic background updates,
embeddings, elaborate marketplace analytics and a web publisher portal are **not
prerequisites for these six blocks**. Custom MCP implementation must remain intact
and compatible with the supported shared connection path. No compatibility work
for obsolete unpublished extension formats is added.

## References and why this approach

- [Raycast publication](https://developers.raycast.com/basics/publish-an-extension):
  a CLI command opens/updates a GitHub PR; review leads to store publication.
  Borrow that simple author workflow, not Raycast's whole runtime or web product.
- [Raycast security](https://developers.raycast.com/information/security): reviewed
  community extensions and version/distribution handling inform the developer UX.
- [GitHub secure workflow use](https://docs.github.com/en/actions/reference/security/secure-use):
  least-privilege jobs and treating untrusted checkout/artifacts carefully support
  retaining our existing build/sign separation.
- [GitHub deployment protection](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments):
  environment reviewers/branch restrictions are real configuration, not implied
  by YAML. Check availability and self-review behavior for our repository/plan.
- [OpenCode MCP account setup](https://opencode.ai/docs/mcp-servers/): automatic
  registration plus explicitly pre-registered client routes are normal patterns;
  they do not make provider-specific client requirements disappear.
- [Official MCP Registry requirements](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md):
  namespace/publisher identity and standardized MCP metadata are useful reference
  points. That registry does not replace Grain's native runtime, review policy,
  signed store packages or application account lifecycle.

References checked 15 October 2026. The recommendation to keep GitHub and wrap our
existing verified steps is an engineering judgment from the inspected code and
actual publication failures, not a claim of universal industry best practice.
