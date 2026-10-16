# Extension platform: publishing and release work order

Updated 16 October 2026. This is the **single active forward plan** for finishing
extension authoring, review, publication and store delivery. It replaces this
file's earlier chronological work order and the historical E4–E8 phase counts.
Completed evidence stays in [progress](MCP-EXTENSION-PROGRESS.md) and the focused
audit documents; it is not a fresh backlog. Six bounded blocks are defined below:
**Blocks 1–5 checkpoints complete; Block 6 remains**. The user explicitly approved
[registry PR 8](https://github.com/Punit-Dethe/Grain-Extention/pull/8) at
`a8f31008bea36d927baca76faf56820e66d1abb1` and tester ownership adoption on
16 October. It is merged at `d0343cf18d0a2e1b65143923e460ee2a84061cb0`.
The exact current ownership file is now adopted outside repositories under
`%LOCALAPPDATA%/GrainPublishing/policy/ownership.json`; SHA256
`d471cf0c145dd8a83b131c2e669284f026f78659e0c649f095cd8935423771c3`.
Its new directory disables inherited access and allows the current operator and
SYSTEM. Existing signing keys and GitHub protection settings were not changed.
Block 3's exact-approved Notion 0.1.2 update is live at signed index 8, publication
commit `8931134ee42a4c02dda05369304763c77e6cc859`. Run **38790216499** confirms Git
and all 63 public files/132 HTTP checks. The corrected CLI pin and real CLI test
also pass Linux CI and are merged through registry PR 10. Block 4's author
onboarding, supported authentication routes and native callback correction are
verified below. The user accepted Block 5's grouped real-app checks on 16 October:
one GitHub/Notion card each, Notion Installed/Update and account retention, then
a harmless read before/after disable/re-enable. Next: **Block 6, operations and
release acceptance**. Store exposure outside Developer Mode is now implemented
in Block 6 with normal-test coverage; its affected app/release acceptance remains
open. No new Grain installer or stable SDK release.

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
| Real extensions | GitHub 0.1.2 and Notion 0.1.2 are live in signed index 8. Publication run 38790216499 confirms remote Git plus 63 public files/132 HTTP requests. Representative account/Agent and current store/update checks pass; not every provider tool is certified. |
| Store identity | Duplicate historical release cards fixed in 608fb263. Browse/details/icons share latest eligible publication selection; updates follow signed publication order. Thirteen ordinary store tests and six real-app native/MCP cases pass. User accepts the two-card view, update/account state and read after re-enable. Existing live-network unit test remains ignored. |
| Tool selection | Existing lexical shortlist, compact directory and exact recovery have a completed bounded GitHub/Notion checkpoint. No new retrieval research, embeddings or Agent rewrite in this work order. |

## Audit baseline: concrete gaps, not assumed missing implementations

| Finding | Basis inspected | Required action |
| --- | --- | --- |
| Authors had to compile the CLI from Grain source (fixed in Block 1) | `crates/grain-ext-cli/README.md` | Distribute a versioned CLI with clear supported OS/runtime requirements; exercise installation from a clean author directory. |
| Native scaffold needed supplied artwork to pass doctor (fixed in Block 1) | CLI scaffold and tests | Provide a clearly identified valid starter icon or an equally simple documented setup; align runtime/listing icon instructions. Do not silently copy unrelated branding. |
| Submission drafting lacked updates (fixed in Block 1) | CLI guide and `crates/grain-ext-cli/src/lib.rs` | Explicit safe update mode; preserve an existing draft on invalid input/failure. |
| Submission stops at local files | CLI guide | Small optional GitHub PR submission/update helper using existing Git/`gh`; keep manual/offline drafting usable. Authenticate through `gh`, never store a second GitHub credential. |
| Ownership enforcement and tester adoption complete; production controls pending | `authority.rs`, `review.rs`, `approval.rs`, merged registry PR 8 | Schema-3 signing requires the adopted pinned operator-owned ID/source/kind/maintainer/reviewer file. Configure approved publication protection before community release. |
| Publishing needs too much manual assembly | Actual GitHub/Notion artwork publication; existing CLI commands | One orchestration entry point around existing prepare/sign/assemble/export/activate commands; no parallel implementation of their checks. |
| Tooling pins can drift | Corrected stale producer commit and PNG capture omission in PRs 6/7 | One reviewed tooling version per publication; complete-path regression includes icons/gallery. Make mismatch diagnostics visible. |
| Error diagnostics hide useful causes | `ci/publish_github.py` captures child stderr, then reports CalledProcessError | Bounded, secret-safe stage/error output and a useful final status. Do not log keys, unlock files, tokens or raw credential-bearing requests. |
| GitHub protection is absent | Read-only API check on 15 October: publish environment `protection_rules: []`, no deployment branch restriction; main protection 404; rulesets `[]` | Configure a workable approved deployment/review policy before public community publication. Do not assume an environment name protects it. |
| Author OAuth onboarding is only a supported profile | CLI guide, host public-client validation, first-party identities | Publish the support matrix, validate declared client/issuer prerequisites and prove an independent author's supported route without a Grain code change. |
| Store MCP install/auth Developer Mode coupling fixed in Block 6 | `grain_store.rs::store_mcp_install`, source-aware MCP access policy | Alpha store install/auth/runtime no longer needs Developer Mode. Custom MCPs retain a development-build plus Developer Mode gate. Affected app/release acceptance remains. |
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

**Implementation complete, 15 October:** schema-3 review policies require an
independently pinned operator-owned ownership file. Both native/MCP paths bind
ID, source, kind, submitting maintainer, reviewer, registry and publishing key.
Independent GitHub review remains the external route; opted-in first-party grants
can use exact explicit operator approval with review ID zero. Compiled version/
digest exceptions and the old alpha policy field are removed, not migrated.
Approvals expire and changing the current ownership bytes invalidates prior pins;
live merge/source/listing and effective review checks remain before key access.
Enrollment, transfers, role changes and publication withdrawal are documented.
87 registry tests, strict all-target Clippy, CLI help and seven CLI refusal checks
pass. Scoped source review covers authorization order, input containment, bounded
strict parsing, current-file rechecks and preservation of provenance/signing gates.
No blocking source finding remains. Graph context did not map these new policy
flows reliably; actual diff/manual call-path review supplied the audit evidence.
Registry PR 8 proposes only the two existing first-party grants and instructions.
It remains draft, not adopted authority or a new release approval. Protection/key
custody remains an explicit operator decision. Adoption and integration into the
next real publication are carried into Block 3; final operational acceptance is
Block 6. No new provider, harness, app build, signing or publication was required.

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

**In progress, 15 October — joined post-approval staging implemented:**
`stage-reviewed-release` reuses the current signer, app-verified serving capture,
assembly, exact-base local promotion and verified hosting export. It reports each
stage and a final unsigned local summary. No policy creation/auto-approval, new
signer, Git writer or Agent harness. Failed work is retained; after a successful
local promotion, export recovery uses existing export/verify commands without
signing again. A canonical-path containment test exposed a Windows prefix mismatch
in the new wrapper's raw roots; canonicalizing every supplied root fixes it.
Full registry suite: 92 Pass; strict all-target Clippy Pass. Five new normal tests
exercise the shared workflow with real signed fixtures: MCP addition followed by
native update/history, stale/unsafe pre-sign refusal, approval/corrupt-fragment
failure, competing writer and export-only recovery. Three terminal CLI checks
also pass, including existing/new argument contracts and actual app-root-verified
base capture followed by invalid-policy refusal without GitHub/key access. A
scoped manual call-path review supplements graph context, which does not index
the new wrapper reliably. Local export is pinned to the signed assembly even if
another writer advances the store after promotion.

**Still open in this block:** CI selection/download and coherent tooling/policy
preparation, optional author PR helper, final commit/publication tuple integration,
bounded CI diagnostics and one full-path real-provider publication checkpoint.
Fixture staging is not claimed as a live approval/provenance/publication run.
Registry ownership PR 8 remains draft and requires operator adoption before live
signing. No live signing, key/custody/settings changes, app build or publication
occurred. Blocks 1–2 remain the only completed blocks; four including this one
remain. This is a bounded implementation step within Block 3, not a new phase.

**CI intake step complete, 15 October:** `prepare-release-review` takes the
standard `gh run download` artifact and an operator-owned template in the existing
policy format. It reuses receipt/payload/catalogue checks, exact candidate-tree
comparison, prior-index verification and pinned official GH provenance. Candidate
and producer metadata are both verified; the selected tooling source/digest and
ownership are checked, with ownership rechecked after network calls. Calculated
pins are filled without handwritten hashing scripts. Approval dates/reference
remain unset, so the result cannot authorize signing. Full approval validation
remains mandatory on every existing signing/source-review path; structural input
validation is shared with the read-only preparation route. No new approval format,
verifier, downloader/archive handler, harness or dependency. The guide documents
one tooling selection, explicit run/PR facts and download-to-review commands.

96 registry tests and strict all-target Clippy pass. Four intake tests reuse real
native/MCP author preparation and prior signatures; network substitution is only
in the private test callback. A genuine read-only run against GitHub CI
38700335101 verifies candidate and producer attestations, PNG/gallery
listing bytes for the MCP descriptor, and signed prior index 7. Its producer pin
was cross-checked from the separately downloaded trusted request-job executable,
which was hashed but never executed. Altering producer metadata while retaining
the authentic candidate is refused by the official producer-attestation verifier;
no draft is emitted. The real signer refuses the generated unapproved draft before
GitHub review/key access. This inspects an already published candidate; it grants
no new source approval or publication. The proposed ownership file was only read
for eligibility, not adopted as signing authority. Scoped diff/call-path review
passes; graph context is incomplete for the new module and does not replace it.

**Current remainder of Block 3:** optional author PR helper, final commit-bound
publication integration/diagnostics and one complete new live provider update
after reviewed operator adoption. Run/PR selection and final exact approval stay
explicit operator steps, using standard GH CLI rather than a new orchestration
framework. Blocks 1–2 complete; four blocks including Block 3 remain. No app build
or user account test was required. The earlier open-item paragraph records the
previous staging checkpoint; this paragraph is the current Block 3 status.

**Author PR helper and CI diagnostics implemented, 15 October:** `grain-ext pr`
validates a clean committed native/MCP draft, exact already-pushed head, target/
fork identity and an ID-scoped diff, then delegates draft creation or existing-PR
lookup to maintained GH CLI. Standard fork/branch/commit/push commands remain
explicit; the helper never rewrites or pushes a user's checkout or overwrites
PR text. Local Git fixtures cover hidden edits, unrelated deletion/rename,
wrong remote/fork, exact lookup and uncertain creation. The real terminal check
created [disposable PR 7](https://github.com/Punit-Dethe/grain-github-alpha-test/pull/7),
then repeated it and pushed a normal version update into that same PR with its
description unchanged. The draft is now closed; no catalogue was published.
That check caught GH list/create head-syntax differences; branch-only lookup with
exact repository filtering fixes them. The source audit also disabled rename
detection in scope checking so it cannot hide an unrelated deleted file.

Publishing child failures now identify the operation and bounded, redacted cause;
timeouts/start failures retain subprocess failure semantics, so uncertain pushes
still cannot be replayed. 22 CLI + 29 checker tests and strict CLI Clippy pass;
19 publisher/public-delivery tests pass, including real bare-Git race tests and
new redaction/timeout cases. No new dependency, app build or harness. The author
guide no longer tells authors to compile maintainer tooling just to validate a
draft. CLI artifact CI will run on this source push; new cross-OS delivery is not
claimed before its results. Source audit passes; graph context remains incomplete
for newly added modules. The new diagnostic script is not yet activated in the
remote publisher: its reviewed source pin will be adopted with final integration.

**Publication integration checkpoint, 16 October:** registry PR 8 now proposes
one immutable `ci/grain-tooling-commit.txt` selection for both trusted source
preparation and activation, including the producer receipt's source pin. This
replaces three drifting literals without a new loader/framework. Both workflows
validate the selected SHA before checkout. Actionlint passes; seven real Bash
acceptance/refusal inputs pass for each selector, including injected/multiline
values. The registry's new `PUBLISHING.md` is the maintainer entry point for
existing intake, approval, staging, exact Git layout/handoff, activation and
stage-specific recovery. Author docs use the implemented draft-PR helper.

The current production publisher's verification-only route was exercised against
the actual signed index-7 Git commit. It exposed Windows Python `nul` versus Rust
`NUL` equality drift in the serialized handoff. Matching Rust's exact spelling
fixes it; the independent literal wire-contract fixture now detects the old bug.
The real command passes with no signing or remote mutation. 96 registry tests
and 19 publisher/delivery tests pass. CLI run 38784532686 at 20845d09 passes all
three advertised OS builds/tests/artifact uploads. These do not certify a new
source release or adoption of the proposal. No new harness or installer.

**Adopted operator path, 16 October:** the user approved PR 8's exact head and
tester policy adoption. The merged main workflow run **38788435338** passes
request/preparation/attestation with the shared tooling pin; the native build is
correctly skipped for the existing Notion descriptor. Earlier draft-branch run
**38787977168** also passes, with downloaded producer/candidate bytes independently
hashed against the trusted request-job executable without executing it. That
draft evidence is not substituted for eligible main-ref release evidence.

**Complete, 16 October:** the user separately approved exact Notion 0.1.2 source
`3910dfd013c5015914cf016b2e3059932cf51752` and candidate
`ebc992cf4d8d370c7e2c6cba6c3c402c1a453f3f4838a41bd7899d9aad6b834c`.
PR 9 is merged at `64d6211d2c1c044b954a93877974d9be92e56bfe`; main CI
**38789340252** reproduces that candidate exactly. Normal intake verifies both
official attestations and exact prior/ownership bindings. The approved schema-3
policy's SHA256 is `4bdf632f823fcbb26a342853a8307849fe627e7603d6a17a134cd3ff74151f87`.
The public `stage-reviewed-release` command signs/promotes/exports the approved
update, then the existing commit-bound gate/publisher confirms live index 8.

Actual CLI execution exposed a missing `StageReviewedRelease` arm in the global
credential guard, before any key/output access. Grain `bdd5a261` fixes that single
arm. A real CLI integration test admits staging's password flag through to an
invalid-base refusal and still rejects credential flags on read-only commands.
It passes on Windows and in Linux CI **38790442365**; strict Clippy passes.
Registry PR 10 selects that corrected tooling for future runs and includes the
fast CLI regression in the trusted request job. The completed release retains
its independently recorded acf0a10c producer/publisher identity; it is not rewritten.
No signing implementation, authority grant, Agent harness or dependency was added.

Existing native/MCP signed-store/Git fixtures, 96 registry tests, one public CLI
regression, 19 publisher tests and the new real MCP update complete Block 3's
bounded checkpoint/source audit. Block 4 is next; **three blocks remain**. Actual
GitHub protection/signing-key custody changes and real-app update-state acceptance
remain Blocks 5–6; live HTTP success alone does not certify them.

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

**Complete, 16 October:** Grain `3cebf8a9` adds the publisher authentication guide
to CLI downloads and makes `doctor` distinguish anonymous, provider-discovered
registration and publisher-owned public clients without claiming live sign-in
verification. Registry PR 11 is merged at
`41772f157ea2d321679cb954726c2b9f35f66649`; author/reviewer instructions now identify
unsupported account setup before publication. GitHub's existing build-supplied
identity, extractable desktop credential limits and disabled global client
metadata URL are documented; no universal proxy or new authentication engine.

The source audit found native OAuth generated a new callback path for each
attempt, incompatible with exact provider registration. Native sign-in now uses
`/grain/oauth/callback` with the existing ephemeral loopback port, random state
and S256 PKCE. Actual loopback tests reject wrong/duplicate state and accept the
valid callback. MCP's existing fixed callback and account handling are unchanged.

Verification: 23 CLI, 29 checker, 12 core MCP admission, two SDK authentication,
83 host MCP and 24 native authentication tests pass; strict CLI Clippy, formatting,
Actionlint and scoped source audit pass. CLI run **38884701939** passes Windows,
Linux and macOS. Its Windows download has verified checksums for the executable,
both guides and source receipt; guide contents match source (checkout line endings
differ). The downloaded executable reports zero findings on the existing Notion
project and correctly says local validation does not verify live sign-in.

Reuse the user's accepted real Notion sign-in/read/create/edit evidence: its
provider-discovered registration route needs no provider-specific Grain source
entry, and this block did not change that descriptor or MCP authentication path.
No new native SaaS/provider certification is inferred from protocol tests. Existing
cancellation, refresh and account ownership regressions pass; installed-update
account retention remains Block 5. Deferred Linear token expiry stays deferred.
No new account consent, package publication, signing identity, installer or harness
was needed. **Two blocks remain.**

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

**Preparation, 16 October:** the existing ordinary store suite passes 12 tests
with its one live-network test explicitly ignored; 61 existing frontend routing,
installed-collection and extension-runtime tests pass. These reuse current tests,
not a new harness. Source inspection found a concrete remaining gap: card/detail
actions treat any unequal installed/catalogue version as an update, while both
backend acquisition paths select exact historical versions without checking their
relative signed publication order. Close that update-order gap consistently in
backend admission and presentation; do not introduce a separate guessed semver
comparator. Existing account-preserving/rotating update tests and prior real-app
store evidence remain reusable, but do not certify the current update UI.

**Update-order implementation, 16 October:** one backend rule now admits only the
latest eligible publication, and only updates a known same-kind predecessor (or
the same current version). Signed index append order is authoritative; version
strings are labels, not a second semantic-version comparator. Unknown/absent
installed history is refused rather than guessed. Both native and MCP paths
check before download. Native ownership is captured before reading the installed
version; existing final revision/lease and catalogue cancellation checks remain.
Browse and single-extension details expose the same derived `previous_versions`;
both buttons share one action-label helper and disable unavailable updates. This
does not change the signed index, package/SDK author contract or authentication.

13 ordinary host store tests pass (one existing live-network test ignored),
including pre-network refusal with unchanged registry/no account commit for both
kinds and the existing successful/failed/racing downloads. All 23 core connection
tests and 62 frontend tests pass; TypeScript, scoped ESLint, formatting and Vite
build pass. Source/call-path audit covers the earlier ownership capture, same-kind
history and both detail/browse projections; no blocking finding for this change.
The unrelated Cargo and binding-format edits remain preserved.

The existing real-app suites were attempted with one cached debug build, not an
installer. `run-d7xxJx` exposed the native store test expecting the grid after
successful installation; current UI opens details. `run-SXoBD7` retained a too-broad
Installed locator; its replacement verifies the exact detail article then returns
to the grid. `run-4xh68m` passes that step but stops because the old scripted model
rejects first-frame selected action schemas (`Initial Agent frame exposed action
schemas`). This is incompatible with the accepted fast tool-selection path;
it is not evidence of an update failure. All three runs have cleanup Pass;
later native cases and the MCP batch are **Not run**, not accepted by inference.
Stop here rather than rewrite unrelated Agent test expectations inside this fix.
The two existing affected fixture self-tests pass; the MCP fixture now retains
signed predecessor history like the real publisher. No scenario/runner/framework
was added. At that intermediate point the bounded current-contract app checkpoint
and grouped user confirmation remained open; both are now accepted below.

**Automated store checkpoint complete, 16 October:** final `run-aobPCF` (native)
and `run-tV2ue0` (MCP) pass all six existing real-app cases, cleanup Pass. They
reuse the previous debug host/source fingerprint without another build. The
maintained store tests now follow selected first-round schemas, automatic Agent
execution, tabbed account settings and the installed marker/icon fallback. One
shared result validator correlates actual tool-call IDs, refuses replay or
missing/invented results, and reuses existing MCP result/account checks. It is a
store-test fixture, not a new Agent loop, product policy or provider integration.

Native checks cover install/enable/read/restart/offline, held downloads versus
disable/removal/developer overrides and bad signatures/hash/close. MCP checks
cover CLI-authored signed install, update/account ownership, failed/removed
updates, cancel/reconnect with actual fixture SDK/vault grants, restart, signed
revocation during an actual call and pending OAuth, and independent configured
server recovery. Signed predecessor history now matches real publication for
both fixture kinds. The old pending-per-call-approval update step is retired;
stale leases and in-flight revocation remain explicitly checked. All 80 runner
self-tests pass, including wrong/missing/replayed native/MCP/account results.
Scoped graph/manual source review passes; application code/policies, real
accounts, installer and scenario IDs are unchanged.

Earlier native passes `run-9oNcKH` and MCP passes `run-LzJxPB` remain supporting
evidence. MCP `run-180Gus` and `run-qOoldy` retain adapter failures: a remaining
panel-origin check was still treated as pending approval, then the adapter
mistook pre-start inactivity for task completion. The origin check now runs on
the actual completed panel; completion waits for an observed model request.
Both failed runs have cleanup Pass and earn no acceptance.

**Grouped user catalogue/account check — now Pass, 16 October:**
one GitHub and one Notion card, current/update state with an unchanged Notion
account retained, and one harmless Notion read before/after disable/re-enable.
Requested once with the alpha development launch command; no new installer.
The user initially reported that launch failed and would provide results later;
none of the manual checks was credited at that point. CLI invocation/launch diagnosis
is separate from the six accepted isolated runs. The command plus `--no-watch`
subsequently builds the normal dev app in 2m08s and opens `C:/gt/debug/handy.exe`;
it is left available for testing. No original error text was provided, so its
cause remains unknown. A local linked-dependency font serving warning is recorded
in progress; no visual acceptance is inferred. No installer was built.
The latest user report accepts all three checks and closes the store checkpoint.
The alpha exposure gate carries into Block 6's production/operator release work.
Historical forced
discovery/per-call-approval suites are not current acceptance; retire or move
their still-relevant assertions during Block 6, not through compatibility code.

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

**Checkpoint accepted, 16 October:** automated/source audit and the three grouped
user checks pass. The ordinary alpha store/Developer Mode separation above moves
to Block 6's final exposure gate; it is not silently counted as delivered. Current
acceptance used the development alpha profile. No repeated user batch is needed.

**Done:** one native and GitHub/Notion exercise install/setup/action/update/failure/
uninstall on the real app; reuse prior passes where the behavior did not change.
Ordinary component tests cover most states. One small user visual batch at the
checkpoint; no fresh installer for each edit.

## Block 6 — Operations, contributor handoff and release acceptance

**In progress, 16 October:** add exact-version revocation through the existing
signed renewal/assembly/promotion path. The legacy `retire` command mutates an
index outside this coherent generation path and is not the operations procedure.
Use fixture keys and normal registry tests; no live revocation or publication.
The current [operator runbook](EXTENSION-OPERATIONS.md) holds recovery, custody,
responsibility and the remaining release gates.

**Alpha exposure implemented, 16 October:** signed store MCP install/update,
list/status, account controls, discovery and execution use a source-aware gate.
The dedicated `com.grain.extensions.alpha` identity and debug builds allow store
records without Developer Mode; custom records and built-in developer providers
require both a debug build and Developer Mode, even if an old enable key exists.
Custom import/replace repeat that check after scheduling, and record mutations
check their source before account/settings changes. Store provenance comes from
the verified connection record, never its display name or MCP URL. Turning off
Developer Mode cancels custom sessions without cancelling signed-store sessions.
Card/detail install buttons follow catalogue readiness for both extension kinds.

85 host MCP tests, 13 store tests (one pre-existing live-network test ignored),
62 frontend tests, TypeScript, scoped ESLint and the alpha frontend build pass.
Source/graph audit retains existing signature, latest-publication, revocation,
revision and vault ownership checks. Clippy completes with existing warnings;
this is not a claim of repository-wide lint cleanliness. The first host attempt
was blocked by an open app's DLL lock, then passed after the user closed it.
No harness source, model workflow, SDK schema or OAuth transport was added.
The affected real-app check remains Pending: Developer Mode off, store reads and
Notion reconnect, then restart and repeat. Final packaged-alpha verification is
reserved for the release checkpoint; debug policy tests do not replace it.

**Read-only preflight, 16 October:** registry API still reports publish environment
`protection_rules: []`, `deployment_branch_policy: null`, no rulesets, and main
branch protection returns `Branch not protected` (404). These confirm the
previously recorded operator gap, not a new test failure. No settings, keys,
authority file or publishing workflow was changed. Prepare concrete controls
for operator review before production activation; tester source approvals do
not establish those controls.

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
6. Finish and explicitly accept supported-store exposure for the intended alpha
   without exposing custom MCP management. This is the carried Block 5 gate;
   current Developer Mode acceptance is not ordinary-user release acceptance.

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

Blocks 1–4 need no new user account/app test. Tester ownership/tooling adoption and
the separately approved live Notion 0.1.2 update are complete. Actual GitHub
protection/signing-key custody changes remain reviewed operator decisions:

- Two-card store view and grouped Notion update/read checks: user Pass, 16 October.
- Confirm concrete publishing protection/custody changes before activation.
- Accept the final alpha exposure/packaging check when that affected path is ready.
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
