# Extension implementation: progress, changes and user testing

**Block 4 complete — publisher account onboarding, 16 October 2026:** Grain
`3cebf8a97899919b23eb02e5ecb7a2b4651e32c5` adds a supported-route authentication
guide, includes it in checksummed CLI downloads and gives `doctor` actionable
offline account-setup reminders. It explicitly does not certify live sign-in.
Registry author/reviewer guidance is merged through PR 11 at
`41772f157ea2d321679cb954726c2b9f35f66649`. Public client registration, DCR,
GitHub's host-specific identity, extractable desktop credentials and unsupported
secret-required flows are distinguished; global client metadata registration
remains disabled until a verified permanent identity exists.

The scoped audit found native OAuth used a changing callback path. It now uses
the registerable `/grain/oauth/callback` path with the existing ephemeral port,
random state and PKCE. Normal HTTP callback tests reject wrong/duplicate state
before valid completion; token refresh, cancellation and ownership tests pass.
No new transport, authentication engine, dependency or harness. MCP authentication
and first-party package declarations are unchanged. Reuse the user's accepted
Notion browser consent/read/create/edit evidence for the provider-discovered
registration route; this is not a newly executed live test or native SaaS claim.
Installed-update account retention stays in Block 5; Linear expiry stays deferred.

Checks pass: 23 CLI, 29 shared checker, 12 core MCP admission, two SDK auth,
83 host MCP and 24 native auth tests; strict CLI Clippy, formatting, Actionlint
and scoped source audit. [CLI run 38884701939](https://github.com/Punit-Dethe/Grain/actions/runs/38884701939)
passes all three advertised OS builds. The downloaded Windows executable, guides
and source receipt match their checksums/source identity; guide text matches
source after line-ending normalization. Downloaded `doctor` reports zero findings
on the existing Notion project with the correct discovery/setup reminder.

**Blocks 1–4 complete; two remain. Next: Block 5, store install/update/account
journey.** Preparation: all 12 ordinary host store tests and 61 existing frontend
routing/installed-collection/runtime tests pass; the existing live-network test
remains explicitly ignored. Source inspection identifies unequal-version update
labels plus exact historical backend selection as the next update-order gap;
resolve it using signed publication order rather than another version comparator.
This does not certify the current
real-app update state. No new user batch or installer is needed for Block 4;
the native callback fix will be present in the next development build. No package
publication, key/protection changes, or new signing approval. Custom MCP management,
Agent policy and the unrelated Cargo/binding edits remain unchanged.

**Block 3 complete — approved ordinary publication, 16 October 2026:** the user
separately approved exact Notion 0.1.2 source
`3910dfd013c5015914cf016b2e3059932cf51752`, registry PR 9 head
`a99ac7184edcafb00a4917d747dafda179f6f041` and candidate
`ebc992cf4d8d370c7e2c6cba6c3c402c1a453f3f4838a41bd7899d9aad6b834c`.
Merged at `64d6211d2c1c044b954a93877974d9be92e56bfe`; main run **38789340252**
reproduces that exact candidate. The trusted request-job executable was hashed
without execution, and normal intake verifies both genuine GitHub attestations,
current adopted ownership and signed prior index 7. Exact approved schema-3
policy SHA256: `4bdf632f823fcbb26a342853a8307849fe627e7603d6a17a134cd3ff74151f87`.
Current live source review also passes before protected signing.

The actual public staging command initially refused its password-file flag:
`StageReviewedRelease` was missing from the global credential admission match.
This was a confirmed pre-key/pre-output refusal, not an uncertain write. A
single missing arm fixes it in Grain **bdd5a261**. A normal CLI integration test
checks staging admission before invalid-base refusal and continued credential
refusal on read-only commands; it passes on Windows and Linux. The initial test
used a nonexistent directory and expected a later error context; changed only
that fixture to an existing empty store, then passed. Strict all-target Clippy
passes. No signing implementation, bypass, dependency or Agent harness was added.

With the corrected CLI, `stage-reviewed-release` completes normal approval,
provenance, signing, assembly, conditional local promotion and verified hosting
export. Live signed index **8** includes GitHub **0.1.2** and Notion **0.1.2**.
Publication commit `8931134ee42a4c02dda05369304763c77e6cc859`, snapshot
`711552ba7468b80c91f932cbb2da0b6d813f9bcc4d59284aa8314d8f475d7603`, bundle receipt
`b28efe05f84ad7fc560a5f8dc2ed7497e2ef0402e5766671a3513c92477c2a69`.
[Run 38790216499](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/38790216499)
confirms one leased Git activation and **63 public files / 132 HTTP requests**.
Evidence/retained work: `C:/Projects/Grain/grain-notion-012-publication-20261016`.
The existing encrypted publisher key/password file stayed in its current custody;
only the protected CLI unlocked it for this authorized release. No secret was
printed, copied into evidence or sent to CI. Root keys were not accessed.

Registry PR **10** is merged at `0629a6aac633a1542c7c9c911cb6fea052480b0b` after
exact-head CI **38790442365** passes request/preparation/attestation, including the
real CLI regression on Linux. It updates the shared tooling pin to **bdd5a261**
and current adoption wording only; approved ownership bytes and catalogue data
are unchanged. The completed release's acf0a10c producer/publisher identity stays
recorded as used, rather than retroactively changed. Source/policy/candidate/Git/
HTTP boundaries and the scoped fixes are audited; no blocking source finding
remains for this bounded checkpoint. Existing signed native/MCP/Git fixtures,
96 registry tests, one CLI regression and 19 publisher tests supply automated
coverage; real application/update/production-control gates are not inferred.

**Blocks 1–3 complete; three remain. Next: Block 4, portable publisher OAuth
onboarding.** Notion 0.1.2 is an extension update, not a Grain installer. The real
app's update/account retention check stays in the grouped Block 5 acceptance;
no new user test is required now. Custom MCP release management stays hidden,
Agent execution policy and unrelated Cargo/binding edits remain unchanged.

**Block 3 operator adoption and real author workflow, 16 October 2026:** the user
answered "Approve merge and tester policy adoption" for registry PR 8's exact
head `a8f31008bea36d927baca76faf56820e66d1abb1`. Marked ready and merged with
head matching at `d0343cf18d0a2e1b65143923e460ee2a84061cb0`. Adopted exact ownership
bytes (SHA256 `d471cf0c145dd8a83b131c2e669284f026f78659e0c649f095cd8935423771c3`)
to `C:/Users/watrm/AppData/Local/GrainPublishing/policy/ownership.json`. The new
policy directory has inheritance disabled, owner/current operator and SYSTEM
access only. No existing key was moved/read/unlocked by adoption; GitHub branch/
environment controls and hosted catalogue bytes are unchanged. This is tester
authority adoption, not a new source-version approval or community release gate.

Shared-pin draft CI **38787977168** and adopted main CI **38788435338** both pass
trusted request/preparation/attestation, correctly skipping native execution for
Notion. Downloaded draft evidence's source pin equals reviewed Grain
`acf0a10c71fb8b906ecb4e2a23b44fc80d6c44b0`; its producer
`b5b5d0d24a14866610e298ed1ad204036c57f3789911bbaec8d92a83cecde50b`
and candidate hashes match independent downloaded trusted-builder/candidate bytes.
No downloaded binary was executed. Public evidence remains in
`C:/Users/watrm/AppData/Local/Temp/grain-shared-pin-ci-38787977168`.
Draft provenance is not treated as eligible approved-main signing evidence.

Prepared a useful ordinary update to close Block 3: Notion **0.1.2**, source/tag
`3910dfd013c5015914cf016b2e3059932cf51752` / `v0.1.2`. It corrects store text that
implied per-call approval and stale author/package instructions. JSON comparison
confirms only descriptor version changes; endpoint, OAuth, tools and artwork stay.
Doctor reports zero findings; pack and scoped diff/source review pass. Real
`grain-ext submit --update-from 0.1.1` plus `grain-ext pr` produced
[registry draft PR 9](https://github.com/Punit-Dethe/Grain-Extention/pull/9), head
`a99ac7184edcafb00a4917d747dafda179f6f041`, with only the two expected submission/
description files. It remains unapproved/unpublished; its build-only CI run is
**38788799821**, and approved merged-ref provenance must precede signing.
Blocks 1–2 complete; Block 3 remains open for that fresh exact-approved live update.
Four blocks including it remain; no new application test is requested now.

**Block 3 publication integration checkpoint, 16 October 2026:** registry PR 8
proposes a single immutable Grain tooling pin for the source builder, producer
receipt and publisher. The value is selected from the reviewed registry checkout,
validated before Grain checkout and passed between trusted jobs; author inputs
and downloaded producer metadata do not select it. Existing native author-build,
attestation, exact approval, local key and one-shot activation separation stays.
`PUBLISHING.md` now gives maintainers one command sequence and explicit recovery
boundaries using existing tools, not another runner. Contributor docs use the
already implemented PR helper; provenance docs distinguish current policy from
historical fixtures and remove obsolete schema-2/legacy-hold instructions.

Verification: Actionlint 1.7.12 checks the three affected workflow definitions;
14 real Bash selector cases pass (seven per workflow), including branch names,
short/uppercase/multiline/injected values and empty input refusal. 96 registry
tests and 19 publisher/delivery tests pass. Actual verification-only publication
of signed commit `15b87cf75c36f5d330470196094738001a6b9426`, parent
`143b06ad21d8a7cd7c97463d6d3e1722870f5359`, receipt
`af06f30276f8040ebe59e2f4dc155ed836f3853b4f3a497bace92d06a86ee649`
passes with the current Rust verifier, complete committed PNG/gallery/history and
snapshot `95842e9643eac6b1714b83f28e233bf4778fc0a166e88d34e0640c5b8173a803`.
The first attempt failed on Python's lowercase Windows `nul` versus Rust's exact
`NUL` wire value. Fixed without loosening handoff equality; the test now uses an
independent literal Rust-contract fixture rather than deriving both sides from
the same Python helper. Retained public checkout:
`C:/Users/watrm/AppData/Local/Temp/grain-publication-verify-_8o1w3xw`.
No previously blocked cleanup was retried. No signing, catalogue activation,
Computer Use, account mutation or application build occurred.

CLI run [38784532686](https://github.com/Punit-Dethe/Grain/actions/runs/38784532686)
at `20845d09ee63d290f4387dfc84a1f803fea25b6e` passes Windows x64, Linux x64 and
macOS arm64 build/check/package/upload. This closes the prior pending build
result, not a fresh installer or stable SDK release. Scoped source review covers
trusted workflow pin selection, producer propagation, unchanged token/signing
boundaries and exact Python/Rust handoff comparison. The graph has no indexed
registry nodes, so direct diff/call-path review supplies its missing coverage.

**Blocks 1–2 complete; Block 3 remains open:** reviewed operator ownership/tooling
adoption and one exact-approved live update remain. Four blocks including Block 3
remain. New application/provider testing is not required from the user now.
Unrelated local Cargo/binding edits remain untouched.

**Block 3 in progress — author PR convenience and publishing diagnostics,
15 October 2026:** added `grain-ext pr` using the shared native/MCP submission
validator, local read-only Git checks and maintained GH CLI. It requires a clean
checkout on a named submission branch, exact committed bytes, an already-pushed
matching head, a target/direct personal-fork identity and changes confined to
one ID's submission/description. It creates a draft or returns the matching open
PR. Normal Git/GH fork/branch/commit/push stays explicit. There is no automated
stash/reset/fetch/commit/push/merge/approval/signing or PR-body/title rewrite.
Native and MCP paths share this helper. Source/tag validation and ownership/
review remain separate gates; the helper grants no publication authority.

22 CLI + 29 checker tests pass; strict all-target CLI Clippy passes. Five new
normal Git tests cover both kinds, existing-PR reuse, dirty/untracked and hidden
assume-unchanged edits, an unrelated deletion masked by rename detection, wrong
remote/fork, ambiguous identities and uncertain/changed-head creation. No harness
or dependency was added. Child operations are bounded/reaped, strip Git redirect
environment and disable optional checkout writes; diagnostics show operation and
exit status without raw credential errors. New CLI artifact CI runs on source
push; cross-platform binary delivery is pending its results, not assumed passed.

Terminal-only real workflow: created draft [test PR 7](https://github.com/Punit-Dethe/grain-github-alpha-test/pull/7)
in the existing disposable repo, verified its exact head, reran lookup and pushed
a normal version update. Both later calls returned the same PR with its body and
clean checkout preserved. It is now closed and never entered the signed catalogue.
The first repeat exposed GH's unsupported owner:branch syntax on `pr list` (it is
valid on `pr create`); fixed to branch-only lookup with exact repository filtering,
then reran the same real PR successfully. Source review additionally caught Git
rename detection hiding an out-of-scope deleted file; `--no-renames` plus a real
rename fixture closes that gap. Public test source/checkout remains in the owned
Temp directory `grain-author-pr-live-c_wrh52w`; no cleanup of previously blocked
temporary files was retried. No Computer Use, provider consent or installer.

The publisher's existing child runner now surfaces operation/exit status and up
to 2048 characters of credential-redacted diagnostics; timeout/start errors remain
subprocess failures handled by the existing uncertain-push guard. 19 normal
publisher/delivery tests pass, including actual bare-Git races and new redaction/
timeout cases. Raw argv/credentials are not added to displayed errors. This only
bounds the displayed diagnostic; it does not replace the existing capture runner
or claim secure-memory erasure. New remote script/tool pins remain to be reviewed
and activated with final publication integration. Scoped source audit passes;
graph context did not fully index the new helper, so direct call-path/diff review
was used. Unrelated Cargo/binding changes are preserved.

**Blocks 1–2 complete; Block 3 remains open.** Next: final commit-bound publication
and coherent tooling pins, operator adoption, then one full-path new provider
update. Four blocks including this one remain. No new user test needed now.

**Block 3 in progress — CI intake to exact review draft, 15 October 2026:**
added `prepare-release-review` over the existing receiver/catalogue/prior-index/
official GH verification boundaries. It consumes the standard downloaded CI data
artifact, independently selected tooling commit/producer pin, reviewed registry
submission, operator-owned policy template/ownership file and current prior index.
It regenerates and compares the entire candidate, verifies both candidate and
producer attestations, fills exact calculated policy pins and emits a source/
version/kind/tooling summary. Existing schema-3 policy format is reused. No new
signer, downloader/archive engine, approval format, dependency or Agent harness.
Approval fields must start empty and remain empty in the draft; the signer refuses
it until an exact independent operator approval is recorded. Full approval/time
validation remains on every existing source-review/signing gate. Shared structural
validation only serves read-only intake and the same full validator. Ownership is
checked again after network provenance; existing output refuses before expensive
verification. Operator run/PR/tool selection stays explicit using maintained GH CLI.

Verification: **96 registry tests Pass**, strict all-target Clippy/build/format
Pass. Four new intake tests reuse real native/MCP author fixtures, media and prior
Minisign signatures; only their private network callback is substituted. They
cover exact unapproved drafts, tooling/producer/receipt/submission/candidate drift,
unknown metadata, provenance failure, ownership changes during verification,
already approved templates, bad prior signatures and nested/existing output.
Scoped source review checks that the input-validator extraction preserves full
approval checks in signing, both proof subjects use the same pinned GH boundary,
and no output or key access occurs on provenance refusal. Graph review was used
but did not reliably map the new module; manual diff/call-path review supplies
the missing context. No blocking source finding remains.

Real terminal-only evidence: selected trusted GitHub run **38700335101** at registry
`63407e89023f56358ad352f8a8544bbf6529a7c0`, PR 6 head
`62f8bad2ee7c699635890b1297562563630a7a3e`, tooling source
`3e5862451f556f766391e47167d0c4ddecab5da7`. The producer executable pin
`3b572d73d8d746d3300614cd1974c20d536e12d25f91b0c91a525a2004b2acc6`
was cross-checked by hashing the separate trusted request-job artifact without
executing it. The new command verifies both genuine GitHub candidate/producer
attestations, descriptor/PNG/gallery bytes and the current signed index 7, producing
the expected candidate `47e1c9553d50542948c414ed28a6886d3820d832b6a4f6e9417fb538cd12197a`.
Deliberately altered producer metadata is refused by official producer provenance
despite the authentic candidate; no review output is created. The real signer
also refuses the unchanged draft before live source-review/key access.
This is a read-only check of an already published candidate, not a new release or
approval. The ownership proposal was read for eligibility only; no protected file
or GitHub settings were changed. Automatic approval review rejected deletion of
the owned disposable CI download directory without a detailed reason; it remains
at `C:/Users/watrm/AppData/Local/Temp/grain-ci-intake-live-xptadc_0`. It contains
public CI bytes and explicitly unapproved test drafts, not a new maintained harness
or adopted operator policy. No installer, Computer Use, provider-account action
or app change.

**Blocks 1–2 complete; Block 3 remains in progress.** Still open here: optional
author PR helper, commit-bound publication integration/diagnostics and a complete
new provider publication after operator adoption of the ownership policy. Four
blocks including this one remain. No new user test is needed for this tooling step.

**Block 3 in progress — joined reviewed-release staging, 15 October 2026:**
added `grain-registry stage-reviewed-release`, a thin composition of existing
review/provenance signing, selected-store capture, serving assembly, conditional
local promotion and hosting export/verification. The common signer arguments and
input construction are shared with `sign-reviewed-candidate`; its existing CLI
contract is retained. The new path derives the prior index from the verified
selected store. It prints stage outcomes and writes `release.json` only after a
verified complete bundle exists. That unsigned summary grants no approval or
remote activation. No new dependency, verifier, service, Agent harness or public
runtime API. Automatic Agent execution policy and account connections are untouched.

Normal tests reuse the existing real Minisign serving fixtures through a private
signer seam; production always calls the unchanged protected review signer with
app-pinned roots. They check MCP addition/native update with retained history,
stale/unsafe pre-sign refusal, approval/corrupt update failure, a competing local
writer, and export failure after promotion followed by export-only recovery.
The first targeted run found a Windows path-prefix mismatch in raw input roots;
all wrapper roots are now canonicalized before the existing containment check.
The export pin is calculated from the signed assembly, preventing another writer's
new pointer from silently selecting a different release. Failed owned work is
retained for diagnosis; the workflow is not claimed crash-atomic. The guide
documents pointer inspection and recovery without re-signing or replaying a push.

Verification: **92 registry tests Pass**, strict all-target Clippy Pass, Rust build/
format checks Pass. Three real CLI checks Pass: old/new argument contracts and
app-root-verified capture of the current public catalogue into a disposable local
store followed by invalid-policy refusal. The latter preserved the local pointer,
accessed no GitHub/key and removed its temporary work. The scoped source audit
reviewed argument reuse, signing boundary, canonical containment, conditional
promotion, post-promotion generation binding and failure messages; no blocking
finding remains. Graph review was used but does not reliably map the new wrapper.

This completes the post-approval local staging part, **not all Block 3**. Still
open: CI intake/tooling pins and exact policy preparation, optional author PR helper,
commit-bound publication integration/diagnostics and a full-path live provider
update after operator policy adoption. No actual operator file, key, GitHub
protection, hosted catalogue, extension source or application was changed. Registry
ownership PR 8 remains draft. Blocks 1–2 complete; four blocks including Block 3
remain. No new user test, installer or Calendar extension needed for this step.

**Block 2 implementation complete — ownership and approval, 15 October 2026:**
replaced compiled tester-version/hash exceptions with schema-3 protected approval
policy plus a strict, digest-pinned operator-owned ownership JSON. Signing and
read-only source-review verification require `--authority`; both native and MCP
releases bind the enrolled ID/source/kind, authorized submitter, eligible reviewer,
registry branch and publishing key. External submissions retain independent live
GitHub review. Explicit first-party approval requires an opted-in grant, an owning
eligible reviewer and a bounded exact-release record reference; no fabricated
self-review. The reference is audit text, not a separately authenticated credential.
Operator custody of the approval policy/independent digest remains the trust anchor.
Prior exact source/listing/artifact, producer/attestation, expiry, previous-index
and delayed key checks remain. Ownership is rechecked around mutable GitHub reads.
Changed current ownership bytes invalidate old pins; operators must supply the
current protected file. Restoring old operator inputs is inside that trust boundary.
This is not a new identity service, OS sandbox or remote revocation service.

Verification: **87 registry tests Pass**, strict all-target registry Clippy Pass,
both real CLI help paths Pass, and seven real CLI refusal checks Pass: unauthorized
submitter/reviewer, changed ownership digest, old schema/alpha field, required
ownership input on both commands. Refusals occur before live GitHub reads/key
access; temporary CLI inputs were removed. Existing tests cover independent/
first-party native/MCP ownership, role removal/takeover, strict limits/containment,
stale/revoked review, changed source and real Minisign updates. Focused manual
diff/call-path audit passes; graph review was used but did not reliably map new
policy flows, so its reported gaps are not substituted for source/test evidence.

[Registry draft PR 8](https://github.com/Punit-Dethe/Grain-Extention/pull/8), commit
`994b66b`, proposes the existing GitHub/Notion grants and documents enrollment,
updates, transfers, role changes and withdrawal. Public policy intent is not
automatically operator authority. The draft is unmerged; no operator file, key,
GitHub protection, account, hosted catalogue or application changed. Review/adopt
the current config before the next real signing run; operational acceptance is
still open. Old tester approvals stay historical and authorize no future version.
The registry author guide now uses the supported `submit --update-from` workflow.
No new provider/account, harness or installer was needed; the user's optional
Calendar example is reserved for a later workflow gap, not added speculatively.
**Two of six implementation blocks complete; next Block 3 publishing workflow,
four blocks remain.** No new user test is needed for this tooling-only block.
No plan scope expansion: policy adoption accompanies Block 3 live publication;
protection/custody decisions remain in operational acceptance.

**Block 1 complete — author workflow, 15 October 2026:** implemented native
starter artwork, explicit `submit --update-from <existing-version>` for native/MCP
source drafts, and corrected new/update/listing instructions. Reuses the existing
shared validator, tempfile dependency and GitHub artifact infrastructure; no new
runtime, service, protocol, harness or application installer. Updates verify the
old draft/version/source/kind, stage both files, restore ordinary failed moves,
and preserve recoverable backup on failed restore. Tests cover stale/new versions,
source/kind/tag/commit mismatches, corrupt or unrelated content, lock contention,
listing mismatch and rollback; interruption recovery is documented, not claimed
crash-atomic. Read-only Windows/Linux/macOS CLI artifact workflow is prepared.
Local checks: **17 CLI + 29 checker Pass**, strict CLI Clippy Pass; existing
four-stage generated author contract Pass (including 15 expected type refusals).
Clean real-CLI native and MCP journeys now pass: init/doctor/pack/new draft/
update; native packaging used the generated npm/TypeScript/esbuild project.
Disposable sources/drafts were removed. Final author-contract evidence is
`tests/agent-harness/.runs/author-lBtr0M` (existing check, four stages/15 expected
type refusals, no app launch). Cross-platform artifact checks also pass.
The first artifact run found an inherited Windows-only Cargo output path on Unix;
its successor rejected runner context at job-env level. Both are corrected by
initializing a runner-local target in the build step, shared through GITHUB_ENV.
Final [workflow run 38711004583](https://github.com/Punit-Dethe/Grain/actions/runs/38711004583)
passes on Windows x64, Linux x64 and macOS arm64. Unix jobs run 18 CLI/29 checker
tests each; Windows runs 17 CLI/29 checker tests. All delivered bundles were
downloaded and their executable/README/BUILD checksums and source pin verified.
The delivered Windows CLI also passes help/version and clean MCP init/doctor/
pack without a checkout or application launch. Earlier failed/cancelled runs
remain evidence. Download verification evidence is under the disposable local
temp directory `grain-cli-delivery-20261015`, not a new maintained harness.
The focused source audit covers complete-draft staging, refusal preservation,
restore failure backup, per-ID CLI locking, link/extra-file refusal and secret-free
read-only artifact packaging. No new blocking source finding remains.
Whole-checker `-D warnings` exposed two existing listing lint findings; not folded
into this implementation. No user test needed for this CLI-only block.
**Remaining: five blocks; next Block 2 ownership/approval policy.** No new Agent
harness code, installer, application/store runtime change or live extension
publication. The public SDK remains provisional until release acceptance.

**Publishing/developer/store readiness plan, 15 October 2026:** audited current
CLI, shared contract, source review/signing, GitHub workflows/settings and store
gates. [The existing stabilization plan](PLATFORM-STABILIZATION.md) is now the
single current six-block work order: author workflow; ownership/approval;
publishing orchestration; publisher account onboarding; store lifecycle;
operations/release. Historical phase/test counters do not define this backlog.
Retain the current runtime, libraries, signer/verifier and GitHub host. Actual
read-only GitHub inspection confirms no publish-environment protection rules,
main branch protection or rulesets; no settings were changed. Source publication
still needs a permanent policy replacing exact tester hash exceptions. This
checkpoint updates plans only; Block 1 implementation is next, with no user
input needed to begin. No harness expansion, installer or new publication.

**Store duplicate-version fix, 15 October 2026:** the user accepts the artwork
layout for now. Their screenshot exposes five cards: three GitHub releases and
two Notion releases. The signed catalogue correctly retains versioned artifacts,
but browse projected every release as a separate extension. Browse, installed
details and installed icons now share one selection per id: the latest eligible
publication in the existing signed publisher order. Revoked/deprecated state is
still projected for that exact version; selection does not silently substitute
an older unrevoked release. Install and Update retain the existing installed
version comparison. No catalogue republish, account change or installer release.
Regression covers the five-release/two-id case, native and MCP listings, matching
icons, a later retired privileged entry and exact-version revocation. The real
Windows host store suite passes **12 tests**, with its one existing live-network
test ignored. The dev app rebuilt successfully; reopen the store to check one
GitHub card (0.1.2) and one Notion card (0.1.1), with Update only for an older
installed release. Real-app confirmation of the two cards remains pending.
Run host tests from `src-tauri/` to use its configured `C:/gt` cache and the
existing Windows activation-manifest test runner; running from the workspace
root selected a separate native build cache and hit an unrelated transcribe.cpp
CMake generation failure. No speech build configuration was changed.

**Artwork publication reauthorized, 15 October 2026:** the user explicitly
authorized publishing both exact artwork candidates after withdrawing the earlier
approval. PR #5 is merged; follow-up PR #6 corrects a stale producer metadata
reference to match the existing pinned tool build. Both genuine CI preparations pass (runs 38700335101 and 38700339217).
Both candidates are signed and published in signed index **7** at registry commit
`15b87cf75c36f5d330470196094738001a6b9426`. [Publication run
38701547203](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/38701547203)
passes: remote main confirmed, all **55 public files / 116 HTTP requests**
matched verified bytes against exact-commit and main URLs, including both PNG
icons and all six new gallery images. Bundle SHA256:
`af06f30276f8040ebe59e2f4dc155ed836f3853b4f3a497bace92d06a86ee649`.
The first publication run stopped before pushing: committed-path capture had
not allowed the new PNG icon role. Fixed with the shared 512 KiB icon bound,
a real signed-icon capture regression and all eight capture tests passing.
Registry PR #7 pins that verifier fix; the source artifacts are unchanged.
The prior draft hold below is historical.
The duplicate author line beneath the detail title is removed; Creator remains
in Details and the summary remains below the title. Frontend TypeScript/Vite
build and focused source checks pass; the user accepts the artwork/header
layout for now. The separately reported duplicate store cards are tracked above.

**Separate icon/gallery contract and restored hero, 15 October 2026:** the user
accepted the Overview/Settings direction and requested a large image above it.
The detail page now restores that wide gallery, with previous/next and numbered
selection, and a separate 64px square title icon. Installed uses square icons;
store browsing uses the first wide gallery image. Only the active full-size
preview is loaded; switching images cannot temporarily display the previous
image. Missing listing icons use a neutral initial, never a gallery cover.

Audited the real inconsistency: native runtime icons already required 512×512
PNG, but hosted listings had no distinct icon metadata or landscape/minimum-size
rule. GitHub's old cover was 1024×575; Notion's was 192×192. Added reserved
`media/icon.png` (512×512, ≤512 KiB), a separate signed catalogue icon, and up to
six gallery images (640–2048×360–1152, ratio 3:2–2:1, ≤4 MiB each; recommended
1280×720). The host uses a consistent 16:9 contain viewport. Source checks,
catalogue staging, serving inventory, HTTP verification, client projection and
signed-size lookup now preserve those roles. The client also validates PNG icon
dimensions and decode. Existing 16 MiB listing budget remains.

GitHub 0.1.2 (`d5ae55e7dafaa0f9749a05f1518d0152bffcc803`) and Notion 0.1.1
(`d44b782581702935d5b1c5f75457d3ac2bac7b70`) are prepared on source branches/tags
with an attributed square icon and three official-provider wide images each.
Local doctor/pack, source-pinned preparation and unsigned catalogue staging pass;
each candidate has one PNG icon and three gallery-only WebP references. Verified
descriptor differences are version-only: endpoint, authentication and public
options are unchanged. **These preparation checks preceded the completed signed publication recorded above.**
The existing publisher makes each version's listing immutable; new exact source
candidates require review/approval/signing, not a fresh Grain installer release.
Both source branches and fixed tags are pushed. Registry [PR #5](https://github.com/Punit-Dethe/Grain-Extention/pull/5)
contains the two source submissions, corrected author instructions and workflow
pins to shared artwork tooling at `3e5862451f556f766391e47167d0c4ddecab5da7`.
At this preparation checkpoint, no live `v1` data or approval exceptions had
changed. The later explicit authorization and completed publication are recorded
above; it supersedes this initial draft state.
**Historical approval withdrawal, superseded by reauthorization above, 15 October:** the user said the approval was accidental
and requested that PR #5 remain a draft. No candidate build/signing/publication
was started following that withdrawn approval. Publication proceeded only after
the next explicit user request authorized it again.

Frontend 123 unit tests, TypeScript/Vite build, ESLint/Prettier, SDK/checker/CLI
tests, normal registry suite, both-role catalogue staging and HTTP inventory
tests pass. The real host store suite passes 11 with its one existing live-network
test ignored. Direct Windows unit-test launch initially failed with 0xC0000139;
the existing `scripts/run-rust-test.ps1` activation-manifest runner resolves it.
Independent source review caught/fixed disabled-row opacity and icon object-fit
specificity; final confirmation passes. Visual acceptance of the restored
gallery remains Pending, including actual multiple-image navigation after signed
publication. No Computer Use, visual harness, new dependency or installer.
See [artwork rules and complete publishing explanation](EXTENSION-ARTWORK-AND-PUBLISHING.md).

**Extension detail UI refinement, 15 October 2026 (latest):** adopted the scoped
frontend changes from `origin/main` through `f6c9be19`: Home's compact shortcut
strip and light-theme History surfaces. Kept the extension branch's other work.
The shared individual extension page now leads with artwork, identity and
install/update/uninstall actions, followed by Overview / Settings navigation.
Overview renders the description directly beside ordinary metadata, trust and
permissions; the boxed Information disclosure and nested permission scrolling
are gone. Settings groups account/access and preferences into labeled rows,
with advanced connection configuration kept secondary. Native and MCP-backed
extensions continue to share the same presentation; store and installed lists
were outside this refinement.

Both views stay mounted so switching them does not cancel OAuth or reset form
state. The portalled header Uninstall action reveals Settings before showing
its existing confirmation. No account, tool-discovery, execution, approval or
publication contract changed. No new dependency, backend code, installer,
release or visual test harness. Layout references: [Raycast Settings](https://manual.raycast.com/settings)
and [Chrome extension management](https://support.google.com/chrome_webstore/answer/2664769?hl=en).

Verification: all **123 frontend unit tests**, TypeScript + Vite production
frontend build, scoped ESLint and Prettier pass. The source design detector has
no findings. Independent source review passes with no material blocking finding;
it covered responsive/light-theme rules, account lifetime and uninstall focus.
These are nonvisual checks; real-app visual approval and interaction
acceptance remain **Pending**, not inferred from existing runtime/tool tests.
User review batch: open GitHub and Notion details from installed/store views;
inspect Overview and Settings in both themes and a narrower window; confirm
header Uninstall exposes its confirmation (choose Keep extension); when next
signing in, switch views and verify the pending sign-in remains active.

**Tool-discovery policy adopted and scoped audit closed, 15 October (latest):**
the normal Agent now combines the lexical first shortlist, at most 8 KiB of inert
deferred names/purposes, and exact-name schema recovery. Removed the debug-only
switch/guards so debug and release use the same policy. Names do not make tools
callable; existing schema, account/source/digest, same-frame, cancellation and
unknown-write non-replay checks remain. No embedding model or second router.

The twelve frozen comparison journeys had eleven task passes and one failure.
All six directory cases passed. Hinglish control issued **12 searches and four
discovery calls**, never fetched the full page and answered after 61.80 s; the
directory path performed one recovery/search/fetch and answered in 16.05 s.
This is a completion improvement, not a matched successful-task speed percentage.
Spanish README answered in 11.12 s with directory versus 15.71 s control; control
also attempted an unoffered tool, which was refused before dispatch and corrected.
Japanese both completed, but the directory run prepared only one provider, so
exclude that pair from matched timing. Both paths asked which duplicate-title
issue to change, refused the nonexistent target and respected disconnected Linear.
Independent GitHub checks proved both ambiguity fixtures stayed open/uncommented
and the missing issue was not created.

Final source was rebuilt once as a debug alpha (41.68 s, no installer) and run
with the old experiment variable unset. **All nine default-path journeys pass:**

| Default journey | Final answer | Actual outcome |
| --- | --- | --- |
| Hinglish Notion full page | 15.64 s | Search, exact recovery and fetch; correct issue topics |
| Spanish GitHub README | 12.62 s | Exact README matches separate GitHub CLI retrieval |
| Japanese Notion full page | 16.86 s | Search/recovery/fetch; correct issue topics |
| Ambiguous GitHub issue | 10.89 s | Asked which of #4/#5; neither changed |
| Missing GitHub issue | 14.74 s | Reported missing; no issue/comment created |
| Unavailable Linear | 7.61 s | No provider action; no substitution with another service |
| GitHub create/update/close | 18.39 s | Exactly one issue (#6), exact final body, closed; independent CLI proof |
| Notion private page create/update | 21.25 s | One creation and one update; final body returned |
| Fresh Notion verification | 14.25 s | Title-only request recovered the unseen random final body marker |

Every final frame verified Harbor `deepseek-v4.1-flash:free`; fallback disabled.
No provider errors, identical duplicate calls, budget exhaustion, unknown writes
or wrong targets in this batch. Notion final verification is a fresh conversation
through Grain's actual search/fetch, not an independent Notion REST client. The
plain-text page workflow did not demonstrate rich Markdown prerequisites or async
polling; neither is certified here. One Notion write prepared only its catalogue
before the shared deadline, and completed correctly despite the unavailable
GitHub metadata. Existing provider/exposure boundaries remained intact.

Capacity correction: reproduced a failing focused test with ~21 KiB already
offered and a 20 KiB missing definition. The former 32 KiB ceiling could not
recover it. The new task cap is **64 KiB**, with initial ranked/eager publication
unchanged at 24/32 KiB, fixed action/provider limits and no eviction/truncation.
The test now passes and still proves saturation refusal and unchanged prior
schemas. The existing exported-catalogue evaluator checked 32 required sets:
initial K=4/6/8 coverage 19/19/20, after visible-name exact recovery **32/32 at
each K**. Three additional late-operation capacity cases using actual large
Notion definitions all recover at each K (maximum 62,386 bytes). These deliberately
oracle-selected recovery checks prove host availability, not semantic accuracy.
The old >=95% first-shortlist target failed and remains failed; lexical-only is
rejected, not retroactively passed. Measured initial setup/clone/index/packing
p50/p95 was 57/63 ms across these varied fresh sessions, not isolated query p95
or end-to-end latency. No heavier dependency follows from that diagnostic.

Scoped audit checked default/release reachability, untrusted context bound,
provider fairness/partial coverage, exact lookup eligibility, both initial caps,
total bytes, stable earlier schemas, same-frame refusal, model identity and
unknown-write behavior. It caught and fixed eager publication accidentally growing
with the recovery cap. Final 13 backend capability + 45 core capability + eight
continuation tests, evaluator, Clippy correctness and scoped format/diff checks
pass (existing warnings). Conservative graph gaps were checked against these
explicit tests and real `run_with_tools` preparation/execution; no whole-repo audit
or every-tool/model/OS certification is claimed.

The two temporary ambiguity issues (#4/#5) were closed by scoped CLI cleanup only
after unchanged-target checks passed. New issue #6 is closed; private page
`Grain Default Check` with its unique run suffix is retained in the disposable
workspace. All staged alpha model/fallback/key fields restored, ordinary settings/
credentials byte-identical and no owned Grain process remains. The four disposable
Python staging adapters are removed; no new maintained harness suite/engine.
Ignored reports, requests/fixture data and hashes remain under
`tests/agent-harness/.runs/tool-policy-20261015/` and `tool-policy-default-20261015/`.
Case SHA-256: `ae38826cf7162ae94da31cec65ede76811a67457e2569de50f4741204b927306`.
An initial Windows text-decoding failure happened before any model request and
restored fields; it is not a passing Agent test. Earlier comparison failures stay
recorded. **No new user test is required for this block.** Close the discovery
implementation checkpoint; broader platform release gates and measured receipt
work retain their own scope. No new release or installer was published.

**GitHub + Notion tool-calling checkpoint, 15 October (latest):** both installed
extensions were enabled. Actual discovery exported 100 supported GitHub tools
and 56 Notion tools. Implemented exact-name recovery: a complete action/canonical
ID is looked up directly, retains extension scope and eligibility, and reports an
already-offered tool instead of filling schemas with unrelated neighbours. An
exact ID outside scope or disabled/quarantined returns no candidate. Prose queries
still use the existing lexical index. Deferred hints now fit names first, sharing
the same 8 KiB bound across providers; optional purposes use only remaining bytes.
No embeddings, additional router, dependency, SDK contract or harness suite.

Seven genuine Agent journeys used saved Harbor `deepseek-v4.1-flash:free`, fallback
disabled and actual provider/model verified. One debug binary/profile, both
providers prepared in every run. All seven task verdicts pass:

| Journey | Successful actions / discovery calls | Model requests | Final answer | Offered schemas, initial → maximum |
| --- | --- | --- | --- | --- |
| Notion full-page control | 2 / 0 | 3 | 12.18 s | 7 → 7 |
| Notion full-page hints | 2 / 0 | 3 | 13.87 s | 7 → 7 |
| GitHub README | 1 / 1 | 3 | 11.26 s | 7 → 8 |
| GitHub README + Notion full page | 3 / 2 | 4 | 22.73 s | 7 → 10 |
| GitHub issue create → update → close | 3 / 0 | 4 | 24.32 s | 7 → 7 |
| Private Notion page create → update → fetch | 4 / 1 | 6 | 23.10 s | 7 → 10 |
| Fresh Notion verification read | 2 / 1 | 4 | 14.42 s | 7 → 8 |

Counts include the discovery schema: six initial actions, at most nine action
schemas later, rather than all 156. Mixed-read discovery used two calls in the
same model round; the subsequent GitHub read, Notion search and Notion fetch all
completed. Both README replies exactly matched the separately retrieved GitHub
CLI text. There were zero duplicate calls, provider errors, exhausted budgets or
unexpected action tools. Notion creation fetched provider Markdown guidance first,
then created/updated synchronously and fetched the saved result. A separate fresh
conversation received only the unique page title, not either random body marker:
actual search/fetch returned the final marker and no original marker. This is a
blind follow-up through Grain, not an independent Notion REST client. GitHub CLI
independently verified exactly one matching issue (#3), exact final body and closed
state in the existing `Punit-Dethe/grain-github-alpha-test` repository. The private
page `Grain Tool Check 013826c022` is retained as a disposable reviewed fixture.

The control/hint Notion pair had identical initial IDs and 22,862 schema bytes;
fetch was already offered, so neither needed discovery. **Hints were slower in
this pair**, and this is not proof they repair every recovery or improve overall
latency. Keep the hint switch debug opt-in. The bounded functional checkpoint
passes; initial-shortlist quality, broader language/ambiguous workflows and the
adoption decision remain open. These are backend timings and synchronous issue/
page checks, not new native-worker, UI, every-tool or async-provider certification.

Scoped review caught and corrected out-of-scope exact IDs falling back to prose
ranking. Final checks: 13 backend capability, 45 core capability and eight Agent
continuation tests pass; Clippy correctness and scoped format/diff checks pass
(existing warnings). The graph's conservative test-gap flags were checked against
these explicit cases. An earlier truncation fixture was enlarged because names
now fit; the earlier test failure is not acceptance evidence. An accidentally
separate-target test build was stopped, then the final checks reused `C:/gt`.

Initial catalogue attempts failed authentication before any actions. The local
debug executable lacked the GitHub desktop registration compiled into the
installed alpha. Reused that extractable desktop-client ID/credential in memory
from the user's installed alpha as compile inputs, without printing values,
writing a credential file or changing account grants. The final debug alpha build
took 36.18 s; no installer, release or browser/Computer Use. A pre-audit read also
passed, but its disposable reporter stopped on a stale exported name map; mapping
now uses actual per-task offered/loaded canonical IDs, preserving that old run.
All staged Harbor selection/fallback/key fields were restored, normal profile
settings/credentials stayed byte-identical, and no owned Grain process remains.
Evidence: ignored `tests/agent-harness/.runs/toolcalling-mixed-20261015/`, including
comparison, exact-binary hash, reports/traces and independent GitHub checks.
Its `run.py`, request/fixture files and old attempts are disposable evidence, not
maintained infrastructure; retire the adapter after review. No new manual batch.

**Harbor comparison, 15 October (latest):** exercised the genuine Agent using
the user's saved `harbor` provider and `deepseek-v4.1-flash:free` model. The saved
Harbor selection was in ordinary Grain, not the alpha profile. Temporarily staged
that selected registration/key in the closed alpha profile, disabled fallback,
and verified the actual provider/model on every offered frame. No secret was
printed or committed. The ordinary settings/credentials stayed byte-identical;
alpha selection, fallback and borrowed key were restored, preserving SDK account
refresh changes. No owned Grain process remains.

Two read-only tasks ran once per condition through the same debug binary and
Notion account. Order: search control, search hints, full-page hints, full-page
control. Both pairs prepared identical initial action IDs and schema bytes.

| Task / condition | Action calls | Discovery calls | Model requests | Last action result | Final answer | Task verdict |
| --- | --- | --- | --- | --- | --- | --- |
| Find page titles / control | 8 | 0 | 8 | 29.18 s | 33.10 s | Pass; one identical duplicate |
| Find page titles / hints | 1 | 0 | 2 | 4.71 s | 7.36 s | Pass; no duplicates |
| Read full page / hints | 2 | 1 | 4 | 9.14 s | 12.15 s | Pass; actual search then fetch |
| Read full page / control | 5 | 2 | 7 | 22.73 s | 27.04 s | **Fail: partial answer, no full-page fetch** |

No provider errors, budget exhaustion or unexpected/write tools were observed.
This is **three task passes and one task failure**, not four passes: the control
explicitly qualified its answer as potentially incomplete. Merely finding the
expected title and returning nonempty text was insufficient acceptance. Traces
show recovery loaded other tools but never fetch in that run; raw discovery query
arguments were not retained, so the exact query/ranking cause is not established.
The hint run published fetch before calling it in the next model frame. The
control's last action is a search result, not a full-page confirmation.

Initial publication was six actions plus recovery, 22,524 schema bytes for search
and 20,974 for the full-page task. Hint metadata added 7,665 / 7,660 bytes, with no
additional initial schemas. Catalogue preparation took 1.29 / 1.01 / 0.40 / 0.31 s
in run order; all four prepared successfully. The existing unsupported
`notion-query-data-sources` schema warning remains (over 64 KiB); this batch does
not certify every Notion capability.

Hints were quicker and avoided the observed repeated-search/partial-read behavior
in this small sample. The earlier Gemini hint run answered the same full-page
prompt in 17.39 s versus Harbor's 12.15 s here, but these are different sessions
and provider conditions, **not a controlled model-speed comparison**. One sample
per task/condition does not establish stable latency or reliability. Keep hints
debug-only opt-in; the offline quality gate, writes, mixed-provider/native checks
and broader adoption decision remain open. No embeddings or product code changes.

An initial startup attempt hit the alpha-identity guard before model/tool work:
the shared debug executable had been rebuilt as ordinary Grain. Rebuilt the debug
alpha executable in 23.88 s, then ran all four journeys without further builds.
No installer, release, browser or Computer Use. Evidence: ignored
`tests/agent-harness/.runs/harbor-20261015/` contains comparison, per-run reports,
events/traces and binary hash; `identity-guard-*` preserves the failed startup.
Its `run.py` is a disposable credential-staging adapter, **not maintained harness
infrastructure**; retire it after evidence review rather than extending it into
a runner. All five restoration assertions passed. No new manual test is assigned.

**9router restored and live recovery verified, 15 October (latest):** after the
user started the existing gateway, `/v1/models` returned HTTP 200 and genuine
Grain Agent requests completed using the configured Gemini route. No gateway
configuration, installation or credentials were changed. Ran the same read-only
Notion find/full-page request with one debug binary/profile, hints off, on, then
off again. Every journey executed one search and one fetch successfully, returned
the expected page title/issue topics, and recorded zero duplicate calls.

| Run | Initial catalogues / tools offered | Discovery calls | Model requests | Full-page result | Final answer |
| --- | --- | --- | --- | --- | --- |
| First control | 0 / 1 | 2 | 5 | 28.55 s | 33.87 s |
| Hints on | 1 / 7 | 1 | 3 | 13.38 s | 17.39 s |
| Warm control | 1 / 7 | 1 | 4 | 21.71 s | 25.44 s |

The first control hit the three-second catalogue preparation deadline; normal
recovery succeeded, but this is **not a matched performance control**. The other
two prepared the same six action IDs and schema-byte count (20,974), with fetch absent.
Preparation took 0.92 s with hints and 2.04 s in the warm control. Hints occupied
7,660 bytes. In the experimental journey the model combined recovery and the
already-offered search in its first round, then fetched on the next round; the
warm control used a separate discovery round. Same-frame exposure authority held:
fetch became usable only after publication. Raw search arguments were not retained,
so this verifies successful combined recovery, not the exact query text used.

These are backend timings, not speech/UI latency, and only one comparable
experimental sample with provider/preparation variability. Do not claim a general
speedup, completed accuracy gate or production readiness. Hints remain debug-only
opt-in. Next: a small broader task comparison before deciding to adopt/retire them.
No writes, browser control, installer, new runner or retained probe process.
Evidence: ignored `tests/agent-harness/.runs/lexical-fields-20261015/comparison.json`
plus reports, monotonic traces and binary hash. Task IDs: first control
`2dcecd15-bb11-45f1-9b06-27007d1d729c`, experiment
`5b58ae51-a90a-4ccf-a86d-c7c75e4f920a`, warm control
`17401426-e9e0-45fb-a6da-7d668813c638`. Earlier connection-refused attempts stay
recorded as failed/blocked; they were not retroactively passed.

**Precise-recovery experiment, 15 October (latest):** separated purpose/prose
statistics and purpose-paragraph enrichment were tried against the same 72-case
catalogue corpus. At K=6 the existing path covered 57/72 complete required sets,
separate-field scoring 54/72, and scoring plus purpose enrichment 56/72. Both
trials were rejected and reverted; the retriever and production projection remain
unchanged. Reports and the temporary diagnostic trace are ignored evidence only;
no additional ranking engine, parser, dependency or diagnostic code was retained.

Prepared a smaller **debug-only opt-in** experiment: an inert deferred-tool
catalogue in model context, capped at 8 KiB including its host instructions. It
contains exact action IDs and short author-supplied purpose hints, grouped by
enabled/prepared provider with round-robin fairness and explicit listing counts/
truncation. Already-offered and statically ineligible tools are omitted. Metadata
is marked untrusted, contains no argument schemas, and grants no call authority.
The model can batch exact missing IDs through existing `search_tools`; normal
schema publication, source/account/digest and same-frame dispatch checks remain.
The experiment reuses the existing Agent loop, not a second model or service.

Set `GRAIN_AGENT_DEFERRED_HINTS=1` only for a debug acceptance process. Default
and release behavior is unchanged. The switch allows matched control/experiment
journeys using one binary; retire it at the adoption decision rather than adding
settings, SDK flags or a long-lived experiment framework. Tests cover the 8 KiB
bound, provider fairness, UTF-8/control sanitization, truncation, loaded/quarantined
exclusion, empty/eager catalogues and unchanged exposed schemas/name mappings.

The genuine recovery prompt started with Notion search available and fetch
missing, six actions plus recovery, 20,974 schema bytes and 11,794 bytes free.
The hints fit in 7,524 bytes. **Live acceptance is Blocked, not Pass:** both
attempts failed before any tool call because local 9router on port 20128 refused
the connection. The client-facing "Gemini could not produce a response" is not
evidence of a Gemini inference/auth failure. One retry was safe because traces
confirm no dispatched action. Both probe processes exited. Launching the existing
9router CLI was rejected by automatic approval review as "blocked by policy";
no startup, configuration change, reinstall or bypass occurred. User was asked
to open the existing app normally. Evidence: ignored
`tests/agent-harness/.runs/lexical-fields-20261015/` (first task
`cfda5b75-d60a-473b-b0a9-594bada76f19`). These attempts preceded the final debug
opt-in guard/static-eligibility filter; no actual model behavior is claimed.

Next: after 9router is available, run the exact same read-only request with hints
off and on; verify one combined recovery, actual search/fetch, duplicates and
confirmed result timing. Then decide whether to retain the extra context. No
offline quality/latency improvement or release adoption is claimed. Embeddings
remain deferred; the initial required-set quality gate remains open.

Final source checks: 11 capability, eight continuation and 29 MCP protocol tests
pass, plus normal backend Clippy correctness (existing warnings), format/diff
checks and a debug alpha build. Audit covered context bytes/fairness, inert
metadata, eligibility/source scope, preserved schema authority, same-frame
exposure, unknown-outcome handling and debug/release guards. It found and fixed
static-ineligibility filtering; the new helper also compiles for release tests
without enabling the experiment in a release app. This is a scoped source audit,
not completed model acceptance or whole-platform release certification.

**Recovery-capacity checkpoint, 15 October (latest):** the first lexical shortlist
now uses at most 24 KiB of the existing 32 KiB task schema budget, retaining at
least 8 KiB for combined recovery. Tiny catalogues that fit are still offered
eagerly under 32 KiB. Recovery can use the full task budget; earlier tools stay
stable. Publication reports bytes/budget/remaining capacity, skips definitions
that do not fit, and ignores repeated already-offered IDs without double-counting.
This fixes recovery starvation, not semantic ranking or every possible large-tool
combination. A focused test verifies incremental recovery, saturation, unchanged
prior schemas/name mappings, and duplicate-ID accounting.

Rejected trials: shorter purpose-only descriptions, clause views and prioritizing
literal matches in high-weight fields did not improve aggregate regression
coverage and introduced other misses. All trial production changes were reverted;
no parser, additional ranking tier or provider-specific rules remain. Inspiration
was checked against Elastic field-weighting and Meilisearch ranking documentation;
see the plan. Disposable reports remain ignored, not new maintained suites.

Before these trials, froze 24 fresh Notion requests with actual prerequisite
labels (including fetch for provider Markdown guidance before writing content).
Same 55-tool supported snapshot: complete-set coverage at K=4/6/8 was 14/17/18 out
of 24 with the prior 32 KiB publication, versus 14/17/17 under the new 24 KiB initial
budget. **Quality gate still fails; no retrieval improvement claimed.** Fresh-case
SHA-256: `bd3a5fb6ad2506b2e74bf97391a175efa3371b02a79a32fb27902dab18f25061`.
They were evaluated after the final reserve change; no subsequent tuning used them.
The earlier corpus's single create-page label omits that provider prerequisite,
so it understates complete workflow requirements and is not an adoption gate.

Real configured-model terminal check: find/read a Grain Notion page, one search
and one fetch, **zero discovery calls and zero duplicates**, both outcomes succeed.
First result 9.20 s, full-page result 13.80 s, final answer 18.38 s from backend
start. Initial definitions: six actions plus recovery, 23,603 bytes, 9,165 bytes
remaining. Both needed tools were initially offered, so this is ordinary-path
acceptance, not a live recovery or matched speedup comparison. No writes, account
changes, browser, installer or retained process. Task `809999ea-5f01-4b51-91b0-98e34790fac2`;
ignored evidence: `tests/agent-harness/.runs/lexical-multistep-20261015/`.

Correction to the prior resource note: the actual exported `notion-fetch`
description explicitly accepts `notion://docs/*` URIs as a tool fallback. Grain
has no general resource reader, but that alone is **not evidence** of a Notion
write blocker; this fallback still needs live verification. Neither read check
validates writes. Next work remains required-operation selection, actual model
recovery and a bounded paired comparison. Embeddings remain deferred.

Scoped audit checked both publication budgets, stable exposure, schema accounting,
execution freshness and unknown-outcome handling. Ten backend capability tests,
29 MCP protocol tests and eight continuation tests pass, as does normal backend
Clippy correctness (existing warnings). Graph coverage flags were checked against
the focused tests and the genuine preparation/Agent run; no whole-platform audit
or release-readiness claim is made.

**Terminal lexical checkpoint, 14 October (latest):** added a debug-only one-shot
MCP probe through the existing headless app path. It requires the alpha identity,
explicit execution, bounded input and a three-minute deadline; it starts no
browser, audio stack, native worker listener or test service. The first run
revealed normal tray keep-alive retaining the process; intentional exit now
releases it after reporting. Unknown request fields are rejected. This is
MCP/backend acceptance, not frontend, speech or native-worker certification.

The saved alpha profile enabled only Notion. Its actual supported catalogue
contained 55 tools (~158 KiB of raw schemas); `notion-query-data-sources` remains
excluded by the existing 64 KiB per-schema limit. One genuine read used the
unchanged `ag/gemini-3.8-flash` model through 9router: six action schemas plus
`search_tools` (~30 KiB), direct `notion-search`, **zero discovery calls**, one
execution and no duplicates. Preparation took 1.36 s, confirmed provider outcome
9.28 s and final answer 17.10 s from backend preparation start. The process exited.
This precedes the later lexical corrections below; it proves the fast path works,
not a controlled speedup or complete provider support. No live write or installer.

The opt-in Rust experiment reuses production projection/ranking/schema packing
against the exported catalogue, with 24 tuning and 24 initially withheld requests.
Baseline K=6 covered every required tool for 22/24 tuning and 16/24 holdout cases.
Added a small generic operation vocabulary, reduced fuzzy near-word dominance
when a literal query term exists, and indexed exact MCP tool names for recovery.
Latest **regression** complete-set coverage under the actual schema budget:

| Offered actions | Tuning | Former holdout |
| --- | --- | --- |
| 4 | 21/24 | 18/24 |
| 6 | 22/24 | 19/24 |
| 8 | 23/24 | 20/24 |

The 95% adoption gate **does not pass**. Multi-step find/read/edit and mixed-language
requests still miss required tools. Those cases were inspected during corrections,
so they no longer constitute independent holdout evidence. This is one provider,
not the planned multi-provider/negative/ambiguous corpus or end-to-end accuracy.
Keep embeddings deferred; next inspect operation coverage, schema-budget packing
and combined recovery, then use fresh cases and a matched real-Agent comparison.
Write descriptions also reference Notion Markdown resources: Grain's tool-only
surface does not yet expose a resource reader; record this separate compatibility
gap before claiming broader Notion write coverage.

Focused core/backend/continuation/protocol/probe/guard tests and normal backend
Clippy correctness checks pass (existing warnings). The experiment passing means
the runner worked, **not** that retrieval passed its quality gate. Local evidence
is ignored at `tests/agent-harness/.runs/lexical-terminal-20261014/`; case SHA-256
is `58EA04674FFF0AD8EDC21B6F125845B2F1568809050B3E8BA26EF559AC0168F1`.
Keep raw account-bound exports out of commits. The harness README documents
commands, scope and probe retirement if a maintained direct CLI replaces it.

**Lexical source checkpoint, 14 October (latest):** first-round lexical tool
publication and combined search/schema recovery are implemented using the
existing `CapabilityIndex`; no embedding/runtime dependency. MCP and native
catalogues belong to the task; preparation uses four concurrent providers with a
shared short I/O deadline and existing SDK cleanup. Cached metadata does not
replace execution-time identity/digest checks. Tiny fitting catalogues are
offered eagerly; larger ones use six candidates under the existing schema budget.
Recovery accepts a bounded query batch and loads directly; the live loader branch
is removed. Backend monotonic timing and offered-tool diagnostics are added.

Verification: 43 core capability tests, 8 backend exposure tests, 8 continuation
tests, 29 MCP protocol tests and 4 existing lexical benchmark tests Pass (92).
Normal backend Clippy correctness checks Pass with existing warnings. Direct
Windows test startup needed the maintained activation-manifest runner; native
dependency checks needed the existing short-target build environment. Neither
failure is reported as a passing direct invocation. Existing benchmark queries
are old fixtures, not real GitHub/Notion holdout or end-to-end improvement evidence.

The attempted dev browser-IPC check was stopped at the user's request before an
Agent/provider action; owned app/launcher exited, no live result claimed. Browser
control remains off. Next: a thin development-only one-shot terminal probe reusing
the existing headless app path, then actual catalogue and configured-model
comparison. No new test server, browser runner, transport engine or installer.
Old scripted search/load harness procedures are explicitly marked obsolete in
the harness README, not passed or given a runtime compatibility workaround.
See [the execution plan](FAST-TOOL-EXECUTION-PLAN.md).

**Lexical-first execution plan, 14 October (latest):** reviewed the user's supplied
deeper research and mapped it to the source. User chose no embeddings for the
first implementation/comparison. The [new work order](FAST-TOOL-EXECUTION-PLAN.md)
reuses existing BM25/rank-fusion code, adds parameter-aware projection and
complete-required-set evaluation, then publishes first-round callable tools with
combined search/load recovery. Bounded dev-app checks compare correct persisted
effects, discovery rounds and request-to-confirmed-action latency. Later receipts
and transport optimizations are separate measured follow-ups. SDK cache hints
exist but Grain currently drops them; existing dispatch digest revalidation stays.
Notion queued async writes must not count as completed actions. Planning only;
no new test Pass, runtime change, embedding dependency, provider action or build.

**Request-to-action research, 14 October (latest):** user clarified the goal as
submit-to-confirmed-action latency. Expanded the existing research document with
voice/action systems (LiveKit/Composio), host-side semantic filters, Model2Vec,
Rust local inference candidates and Google's new EmbeddingGemma 2. Recommendation
now favors preselecting a recoverable tool shortlist before the first LLM request,
with combined search/load as fallback. Compare lexical and optional embedding
variants offline before choosing a dependency; actual Windows latency/accuracy
is unmeasured. Existing traces show last provider success at ~39/53/46 s after the
first model round, before answers at ~43/58/50 s; they exclude STT/earlier submit.
No application changes, model downloads, tool execution, new harness or build.
See [the second pass](TOOL-DISCOVERY-RESEARCH.md#second-pass-request-to-action-latency-latest-direction).

**Tool-discovery research, 14 October:** compared released/pinned Codex, Goose
and OpenCode source plus OpenAI, Anthropic and Cloudflare primary documentation.
No published per-task average search count found. Identified Grain's mandatory
separate search/load decision, simple ranking, repeated remote metadata fetches
and rediscovery on new Agent calls. Recommendation: one search that ranks and
publishes matching schemas, task-local catalogue reuse and bounded measurement;
small eager inventories/conversation reuse are a follow-up comparison, Code Mode
is deferred. Findings, version distinctions and proposed checkpoint are in
[TOOL-DISCOVERY-RESEARCH.md](TOOL-DISCOVERY-RESEARCH.md). Research/documentation only;
no application/harness changes, actions, model switch or build.

**User Notion trace review, 14 October (latest):** user confirms the submitted
message now appears correctly and reports trying note retrieval, creation and
updates. Reviewed the three subsequent completed Agent tasks in the local alpha
log; excluded our earlier 20:43 read probe. Times below are Asia/Calcutta (IST).

| Start | Observed workflow | Tool rounds / model requests | All calls | Discovery calls | Notion executions | Exact repeats | Elapsed |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20:49:32 | List recent and private pages | 5 / 6 | 6 | 4 | 2 succeeded | 0 | ~43 s |
| 20:51:52 | Fetch, then create pages | 8 / 9 | 8 | 6 | 2 succeeded | 0 | ~58 s |
| 20:55:52 | Search, fetch, update page | 8 / 9 | 8 | 5 | 3 succeeded | 0 | ~50 s |

Across these requests: 22 calls, 15 discovery calls, 7 automatic approvals and
7 provider-reported successful executions. All tasks have `finished` events;
none reaches a limit, returns a traced execution error or blocks a retry.
Duplicate detection means the same tool plus normalized identical arguments
within one task, not semantic equivalence or duplicates across conversations.
The creation/update tools each execute once. Traces show only 2 discovery schemas
initially, then 5 schemas (the same 2 plus 3 selected Notion tools); no full tool
catalogue is exposed to the model. These are observed/user-reported basic flows,
not independent persisted-content verification or complete provider acceptance.

**Findings:** 68% of calls are discovery. Each task performs 3–5 separate
`search_tools` rounds before a single selected load. About 141 of the combined
151 seconds fall between model-request/response events (timestamps have only
second precision); dispatch durations total ~11.2 seconds. Reducing unnecessary
model discovery rounds is the main latency opportunity, rather than raising
limits again. Current logs omit query text and match/page counts, so they do not
prove which searches were necessary, empty, paginated or semantically redundant.
Source review confirms each metadata search/load obtains a fresh short-lived MCP
catalogue; task-local metadata reuse is a possible secondary improvement, with
execution-time ownership/digest revalidation retained. Do not add a persistent
connection/cache engine or change selection policy without a bounded checkpoint.

Separate concrete gap: `notion-query-data-sources` is repeatedly excluded because
its schema exceeds 64 KiB. The basic page workflows above do not cover database
queries. Inspect the actual schema/size and provider contract before changing
admission/context limits; no assumption that a larger arbitrary cap solves it.
No runtime/UI/harness changes or new actions performed during this review.
Nonsecret captured trace analysis: `tests/agent-harness/.runs/notion-review-20261014/analysis.json`.

**Agent submitted-message UX, 14 October:** the first typed/transcribed request
now enters visible React conversation state before the Agent command awaits its
model/tool work. Both side and center layouts show it above the loading dots and
keep it visible with the reply/error. Selected source text remains separate;
retries replace the initial request instead of duplicating it. Voice text appears
once transcription supplies it. TypeScript, scoped ESLint/format checks and source
review pass. The design detector reports only the unchanged typing-dot animation.
User visual confirmation now passes; no app restart, installer build,
Computer Use or new harness test was performed while the user tests Notion.

**Agent automatic execution and observation, 14 October (latest):** user asked
to remove repeated tool confirmations and permit substantially longer requests.
Native/MCP capability calls now auto-approve through the existing revalidated
executor; extension enablement, OAuth consent, schema/identity/account checks and
unknown-outcome replay protection remain. Tool ceilings change from 8 rounds /
24 calls to 50 / 200; these are generous finite observation limits, not unlimited
execution. Duplicate calls after success are allowed and counted. Structured
`agent-tool-trace` records task, rounds, call/duplicate counts, names, approval,
outcome and timing without raw arguments/results/secrets. Terminal inspection:
`Get-Content "$env:LOCALAPPDATA\com.grain.extensions.alpha\logs\handy.log" -Tail 200 | Select-String 'agent-tool-trace'`.

Root evidence: earlier live logs show repeated discovery/search/list calls using
the eight-round ceiling. At exhaustion Grain offered zero tools but still sent
`tool_choice: auto`; the model requested an unoffered tool and the continuation
returned the reported budget error. Finalization now sends explicit `none` with
a final-answer instruction; noncompliant requests cannot run and retained action
results survive model errors. This fixes host policy/finalization; it does not
claim every earlier repeat has the same cause or that an empty Notion result is
an authentication failure.

**Verification:** 25 focused Rust tests pass (8 continuation, 5 executor,
5 capability, 7 LLM-client). One real development-app Agent check passes with
the configured `ag/gemini-3.8-flash`, installed/connected store Notion, no approval
dialog and no writes: “No pages matching 'Grain' were found in Notion.” Trace:
5 tool rounds, 5 calls, 0 duplicates, 1 auto-approval; ~32 seconds. Owned app
exit requested. No installer/release or maintained harness feature added.
Ignored evidence/probe: `tests/agent-harness/.runs/notion-auto-20261014/`.
Initial probe used a wrong IPC command and stopped before model/tool execution;
correcting the probe to existing `mcp_connections_list` produced the passing run.
Longer user tasks, Notion writes and cross-provider workflows remain unverified.

**Deferred:** old confirmation acceptance procedures need updating/retiring;
the current native/MCP runtime does not emit them. The provider's
`notion-query-data-sources` schema is excluded by the existing 64 KiB limit;
this is separate from the approval/budget fix. See the active stabilization plan.

**Notion publication complete, 14 October:** human maintainer explicitly approved
the exact 0.1.0 tester candidate, source `401f784ec986c4bf183594549dd75eedb057cb9d`.
Registry PR 4 merged at `baba3bef97a7099393ce457b081a5940e21a1b3d`, head
`9ac4c9e445db5463461114627d71e7460a979207`. The maintainer-only signer admits exact
Notion submission digest `c9eaad5d89f26d5439d6a9a1e91a3533370aa576953e62ef9e85a5a63d0a36f3`
alongside the approved GitHub digest; registry/human/publisher identity, bounded
operator-held approval/expiry and merged-source/listing/provenance checks remain
required before key access. Ten affected tests and focused diff review pass.
The exact alpha exceptions remain temporary; retire them with permanent policy.

CI preparation/attestation `38579878509` and publication `38580862065` succeed.
Receiving/source review pass before signing. Existing tools assemble, conditionally
promote and export the complete signed update. Registry commit
`4c857307435099e5a97d24bac985c0bd1d224496` publishes catalogue **5**, retaining GitHub.
Independent public delivery passes **72 requests**, verifying exact signed metadata,
packages, description/artwork and retained proof at pinned/mutable URLs, with
remote main checked before/after. Bundle receipt:
`158eaf37c8ba765be31aa0af5447ad20bd8c699f23dee2247be8a52494cfb488`.
No app build/release, runtime provider branch, signing key in CI or new test engine.
Nonsecret evidence is at `C:/Projects/Grain/grain-notion-publication-20261014/`;
operator approval/unlock stay outside repositories under `GrainSigning`.

**Next live gate:** install Notion in the existing alpha, approve browser sign-in
with the authorized disposable workspace, then verify its catalogue, bounded page
read/create/edit/restart and GitHub/Notion selection. Actual Notion account/tool
acceptance is not claimed yet; package/listing delivery is a separate result.

**GitHub representative checkpoint accepted / Notion prepared, 14 October:**
the configured `ag/gemini-3.8-flash` through repaired 9router created issue #2 with
exact title/body (`github-alpha-hU7Agp`). Its immediate issue-list check remained
Fail because the new issue was not visible in that response; subsequent independent
reads verify it. `github-alpha-M3F0FS` independently verifies the existing issue,
then passes update/close and creation of `grain-check-feb200ee`. File approval
was refused because our fixture wrongly required REST base64 encoding; current
[GitHub MCP source](https://github.com/github/github-mcp-server/blob/main/pkg/github/repositories.go)
requires literal content and encodes it server-side. `github-alpha-IHmv7r` then
passes exact file creation on that existing branch, with main unchanged. No
completed write was replayed; all three runs restore logging and clean up.

Tiered routing is directly observed in the update/branch run: two initial
discovery tools, four after loading issue read/write or three after loading the
branch tool, with canonical IDs/selected aliases matching the verified actions.
The successful file continuation captures no trace; its exposure verdict stays
Blocked, not retrospectively converted to Pass or blamed on an old binary.
Prior catalogue/read/restart/signed-update evidence remains valid. This accepts
representative GitHub use, not execution of all 100 tools or all account grants.

Only a 15-second read-only verification poll, action-attempt receipt and corrected
plain-text guard are retained. Temporary continuation switches are removed;
the exact used script is saved under ignored `IHmv7r/evidence/runner.used.mjs`,
matching the recorded SHA, for review and later disposal. No new harness engine.

Focused contract audit covered descriptor/header admission, signed artifact
reload, revision invalidation, account/client/issuer rotation, explicit OAuth
precedence, no-secret public-client setup, SDK PKCE/refresh and CLI packing.
Normal checks pass: 35 core MCP, 4 CLI MCP, 2 public-client auth, 2 package-option
and 3 approval regressions. The audit found stale generated MCP README guidance
forbidding now-supported public headers/client metadata; that is corrected.
No unresolved runtime defect was found in that changed contract scope. Third-party
public-client live acceptance and Linear's deferred real-expiry test remain separate;
this is not a universal OAuth or production-readiness certificate.

Notion 0.1.0 candidate is now at
[grain-notion-extension](https://github.com/Punit-Dethe/grain-notion-extension),
tag `v0.1.0`, source `401f784ec986c4bf183594549dd75eedb057cb9d`.
Doctor (0 findings), pack, listing/artwork and public discovery pass. Official
documentation and live metadata advertise DCR, S256 and public-client `none`;
the package has only HTTPS transport/OAuth, without extra headers, client secret,
publisher client override or Grain provider-specific runtime. User authorized a
disposable workspace for browser approval and one test-page create/edit.
[Registry PR 4](https://github.com/Punit-Dethe/Grain-Extention/pull/4) holds its
source submission and removes a retired transcript-transform example that broke
`check-submission`; all four current submissions validate. It is not published
or signed in yet. The current GitHub-only tester exception does not approve
Notion: concrete maintainer/source approval, provenance/signing and real account/
tool acceptance remain next. No new alpha build/release was dispatched.

**Gateway restored, 14 October:** user approved applying the verified backport
and restarting 9router. The installed 0.5.65 chunk now matches fixed SHA256
`0185b1685d655c957cbb63fee0226f224612be79ffd11cf98a409544a496488c`.
Stopping the original child did not restore the endpoint within 30 seconds;
automatic approval review then blocked launcher-restart and direct-server-start
commands, so no success was claimed. After the user quit 9router and explicitly
requested a launch, the normal CLI started hidden with `--skip-update --no-browser
--tray`; its owned server restored port 20128 and `/v1/models` returned HTTP 200.
Gateway startup is confirmed, not genuine model/GitHub write acceptance. No
account database, provider selection or app build was changed. The original
chunk and guarded rollback remain in the separate repair folder.

**Gateway repair prepared, 14 October:** 9router fix lives separately at
`C:/Projects/9router-grain-fix-20261014`, with source PR
[decolua/9router#4831](https://github.com/decolua/9router/pull/4831).
Schema passes visit subschemas instead of field-name dictionaries; no Grain MCP
argument rename or provider branch is introduced. The 17 regression cases include
12 failures before the fix; all pass afterward, alongside 10 existing keyword
tests. All 17 also pass against the installed-version 0.5.65 source backport and
prepared compiled module. Version/hash guards, original-file backup and rollback
are prepared, with maintenance/retirement notes in that folder's `LOCAL-INSTALL.md`.
The running/shared gateway has not been changed; apply/restart timing is requested
from the user because other applications' active requests may be interrupted.
Live configured-Gemini GitHub write acceptance is still pending. No new Grain
build, harness engine or full gateway rewrite is needed for this repair.

**Matching installed checkpoint and gateway diagnosis, 14 October:** artifact
`38547281600` succeeded and the isolated alpha was updated without an application
release. Installed binary SHA256 is
`5d830826565169568cfc00fe7c22fc6e483c0f72a87916fd60fa21057e055fe4`.
`github-alpha-kgOwRL` passes real signed 0.1.0 → 0.1.1 update, unchanged connection,
retained account, revised lease, disabled-before-explicit-enable behavior and
package-owned toolset option. The user accidentally closed the app during tool
discovery; no issue was created. That interrupted run's logging restoration and
cleanup are Fail, not rewritten as acceptance. Subsequent runs record the prior
log level and can relaunch an exited owned host solely to restore it, never to
resume or replay a tool. No old unrecorded log-level value is guessed.

On the matching app/package, `github-alpha-dQ3MHw` discovers 100 supported tools;
`github-alpha-vbh65Q` passes all four private-read/restart/read stages with cleanup
and logging restoration Pass. The fresh write run `github-alpha-3JahkO` still
omits `title`; zero provider writes are approved, no matching issue exists, and
cleanup/restoration Pass. The earlier generic prompt candidate did not fix this
and is reverted in source; the installed artifact still contains it until a later
batched build. No second build was dispatched merely to remove an inert instruction.

The configured model routes through local **9router 0.5.65**. Executing only its
installed pure schema converter offline reproduces removal of a real parameter
named `title`, leaving method/owner/repo/body. The current published **0.5.99**
converter also deletes `title`, even removing it from a required-fields array.
This identifies a gateway schema-corruption defect consistent with the live
omission; it does not prove the cause of every older update/file failure. Neither
gateway was modified or restarted, no credentials/model requests/tools were used
by the offline check, and downloaded package code is only disposable diagnosis.
Evidence is outside the app at `C:/Projects/Grain/grain-github-options-review-20261014/`
(`gateway-schema.json`, `gateway-schema-0.5.99.json`); retire downloaded archives/
extracted chunks after the gateway issue is resolved, keep the small verdicts.

Remaining write acceptance needs approval for an alternate model route or a
separately approved gateway fix. Do not rename MCP arguments, weaken approvals,
upgrade 9router blindly or credit the failed write as Pass. The current official
GitHub read-tool title `Get issue details` is admitted alongside its stable tool
name with the same exact read scope. Notion remains deferred until GitHub is okay.

**GitHub 0.1.1 publication, 14 October:** human maintainer explicitly approved
the exact tester update in registry PR 3. The temporary signer exception now
binds submission `d667ca2fab05aae393497efb46ced148740957f2e443814ab6206aa91523375b`;
the former 0.1.0 exception is refused rather than retained. Eight affected review
tests pass. Independent-review requirements for other submissions are unchanged.
Registry CI builder/verifier pins now use shared contract `14ba2903`; genuine
source preparation/attestation run `38548315482` and publication run `38548884112`
pass. Source tag `v0.1.1` is pinned to `394176037b46ecbfde34d7af0996a505b3352400`.

Public catalogue index 4 at registry commit `36704a63fd14bcabb28a4bc5a4f349fed2010ddc`
contains the signed package-owned all-toolset descriptor. Full local hosting
verification and independent commit-pinned/mutable-URL HTTP delivery pass
(54 requests). Listing, artwork, MCP URL, account client identity, root and
publisher keys are unchanged. Operator approval/unlock files remain outside
repositories; no credentials were copied into artifacts, reports or chat.

Matching alpha artifact run `38547281600` builds runtime source `0461eb2c`;
the later maintainer-only exception change does not affect its runtime. Installed
update/account/Agent acceptance remains pending. The existing GitHub runner has
one explicit `--update-version=0.1.1` option for this real signed update before
the bounded workflow; no new engine or mode. Publication does not certify writes,
live public-client OAuth or the original update/close failure. Notion remains next
after the GitHub/shared-platform checkpoint.

The affected-boundary source review confirms header-only updates preserve the
account identity while invalidating the operation owner and revision leases.
`commit_store_update` deliberately removes enablement on any package change;
the real checkpoint now verifies retained sign-in plus disabled tools, then
explicitly enables the new revision. Public-client declarations require the
exact issuer/`none` method and return before reading another client's secrets;
public options reach only the MCP resource SDK transport. This is a focused
source review, not completion of the post-acceptance audit or live guarantees.

**GitHub execution diagnosis, 14 October:** the trace-capable installed alpha
from source `7d7b96f2` discovers 100 supported GitHub tools (`github-alpha-Oq3oFk`).
Private README read, persisted-account restart and read again pass all four
stages (`github-alpha-QX5iKK`); each read offers two discovery schemas, then three
including the selected getter, and independently verifies the real file bytes.
These results predate the package-header/public-client source changes and do
not certify GitHub 0.1.1 or the newest source. Owned cleanup passes.

Fresh write runs `github-alpha-xtT5DP` and `github-alpha-lOGzZe` select and load
`issue_write` correctly but propose arguments without the requested issue title.
The exact approval guard refuses both; zero writes approved and no matching issue
exists. This is a model argument-fidelity failure, not evidence of bad alias
routing or insufficient GitHub permissions. Original update/close failure remains
unattributed. The older file test was a fresh conversation without earlier branch
history; its prompt now explicitly states that the verified branch exists.

Added a small provider-independent Agent instruction to include all user-requested
values, including schema-optional fields, and explain unsupported operations.
This is a candidate improvement, not a deterministic safety check or a proven
live fix. No schema rewriting, provider prompt branch, approval weakening, budget
increase or automatic write replay. The existing runner now retains refusal
diagnostics across independent-effect assertions, records complete noncumulative
trace lines, and checks developer-catalogue isolation. No new runner or engine.
Checks: 19 Agent Rust tests and two exact-approval tests pass; targeted formatting
passes. Matching-build acceptance and the original write failures remain open.

**Publisher public-client OAuth, 14 October:** implemented optional `oauthClient`
with public `clientId` and exact HTTPS `issuer` for MCP descriptors and direct
connection JSON. Other client fields, secrets and endpoint/scope/callback overrides
are rejected. The current SDK owns authorization requests, S256 PKCE, code
exchange and refresh; Grain owns metadata/destination validation, callback checks,
per-connection vault namespace, enablement and approvals. No proxy, permanent
website, new authentication engine or harness API was added.

Client selection is explicit connection setup → publisher declaration → existing
host-supported identity/registration paths. A publisher declaration must match
discovered issuer exactly and the provider must advertise client authentication
`none`. Its path returns without reading manual or compiled client secrets;
unsupported/mismatched declarations do not fall back silently. GitHub's candidate
has no declaration and retains its restricted built-in identity. Supported public
client publishers register the fixed loopback callback once; users sign in normally.
Confidential-only integrations and live third-party acceptance remain separate.

Adding/changing/removing a declaration rotates account ownership and retires the
old grant; endpoint/auth/public-client comparisons now share one core helper.
Signed store projections refuse unsigned client overrides at reload. Custom
form/JSON edits preserve metadata; choosing no authentication in the form removes
the incompatible declaration. Public-client status does not ask ordinary users
to supply client credentials. Only the related binding/type changes are staged;
unrelated local formatting and Cargo.toml changes stay untouched.

Checks: **320 core, 14 CLI, 28 authoring-check, 83 MCP runtime and 123 frontend
tests Pass**, plus types, targeted lint/format and scoped source review. Existing
SDK public-client tests now validate the declaration gate, exact callback/state/
issuer/resource/S256, code exchange and refresh without a secret. Wrong issuer,
secret-only/missing client-auth metadata, account rotation and persisted store
tampering have focused coverage. The earlier projection count assertion was
updated from 10 to 11 public fields; no private vault identity is exposed.
This is normal/local SDK evidence, not a live provider or issued 24-hour expiry
certificate. No catalogue activation, installer or external-account action.

Stabilization item 1's supported package/public-account source contract is now
implemented. Next: attribute the recorded Agent routing failures, then one
batched matching install/publication checkpoint and affected-boundary audit.
Live publisher-public-client acceptance stays explicit; Notion remains deferred.

**Package-owned transport options, 14 October:** implemented optional public
`transport.headers` for MCP descriptors and top-level `headers` for direct
connection JSON. Shared validation uses the existing HTTP parser, bounds
name/value/count/total bytes, refuses duplicate names and reserves credential,
routing and MCP protocol/session controls. Values are public metadata, not a
secret mechanism or environment/vault substitution. Publisher review remains
responsible for values; validation cannot recognize every possible secret.

Options survive CLI doctor/pack, signed-artifact projection, registry reload,
store updates and the maintained SDK transport, including legacy negotiation.
Unsigned persisted store overrides are refused. Option-only custom/store updates
retain account ownership while invalidating old revision leases and tool stamps.
Also enforced the serialized connection size at descriptor admission, avoiding
a package that installs successfully but exceeds the registry's 4 KiB limit at
restart. Custom form/JSON edits preserve existing options and remain hidden in
release/alpha. Native contracts and first-party OAuth URL/client binding unchanged.

Removed the GitHub-specific toolset branch from Grain. Prepared the GitHub
repository's 0.1.1 candidate with `x-mcp-toolsets: all` in its descriptor; doctor
reports 0 findings and pack succeeds. No signed catalogue update, new installer,
account mutation or live test batch was dispatched. Published/installed evidence
is not carried forward as acceptance of this candidate.

Focused verification: **318 core, 14 CLI, 28 authoring-check and 123 frontend
tests Pass**; type checking, targeted lint/format checks Pass. The new runtime
tests exercise actual SDK header forwarding for modern and legacy negotiation,
without tool writes or a new harness. An initial native compile needed an explicit
test-map type; an existing UI-projection assertion also required updating from
9 to 10 public fields. The corrected MCP suite passes 82 cases; final checks
are recorded at the commit checkpoint. Unrelated generated-binding formatting
and Cargo.toml working changes are preserved and excluded from this commit.

This closes the transport-options source task, not all of stabilization item 1.
General publisher pre-registered OAuth onboarding is next, then the observed
Agent routing failures and a batched install/publication checkpoint plus focused
audit. GitHub remains the single live reference; Notion remains deferred.

**Platform-first reset, 14 October:** the user's latest instruction supersedes
the GitHub breadth → Notion sequence below. Audited the shared descriptor,
registry/runtime ownership, authoring/publication and client resolver boundaries;
recorded the bounded work order in [PLATFORM-STABILIZATION.md](PLATFORM-STABILIZATION.md).
The shared native/store/custom foundations already exist. Package-owned provider
options and generic publisher pre-registered OAuth onboarding are actual remaining
contract gaps, not reasons to add more provider-specific code or another engine.
No new provider package, production contract change or live batch in this reset.

Latest installed-alpha results are retained separately: `github-alpha-F01z0v`
created issue #1 with independently verified title/body, then update/close Fail;
the issue remained open with its original body. `github-alpha-OwevZN` created
and independently verified `grain-check-d7acad35`, then file-create Fail because
the model proposed another Create branch action and the exact guard refused it.
Both owned-host cleanup and original log-level restoration Pass. No failed write
was replayed. Root cause of the update failure remains unresolved; the older
binary has no new tool trace. Full-toolset/model-exposure acceptance remains
Pending, not inferred from the 49-tool default inventory.

Removed the temporary `--files` mode after its diagnostic run instead of growing
the maintained harness. Retained small metadata-only failure diagnostics so
future failed checks report assertion type/counts and approved-action counts
without dumping prompts, arguments, results or secrets. Existing two approval
guard tests are the affected check; no repeated full suite/build is warranted
for documentation and test diagnostics. Another LLM account is unnecessary now;
independent read-only contract review is a later parallel opportunity.

**Bounded GitHub workflow preparation, 13 October:** the user confirms expanded
grants and authorizes real disposable GitHub work plus observing model exposure.
Added ordinary, metadata-only tool diagnostics at the actual model boundary
and loaded-schema map; no new testing IPC, fixtures, model relay or screenshot
automation. Only public tool names/counts, provider/model identifiers and loaded
canonical IDs are logged; prompts/arguments/results/credentials are excluded and
unoffered model names are redacted. Existing normal Agent tests **19 Pass** and
capability tests **5 Pass**; focused approval guards **2 Pass**.

The existing installed-alpha script has one bounded `--workflow` mode for a
marked issue create/update/close and one branch/file write. Exact approvals,
independent GitHub CLI reads, default-branch preservation, no automatic write
replay, owned-host teardown and original log-level restoration are required.
An initial attempt (`github-alpha-b2JtMW`) was Blocked because the user's alpha
was open; no action or settings change was made. Live workflow/exposure verdicts
remain Pending here. The old binary cannot establish the new all-toolset/trace
behavior; one batched tester build is necessary, without a production release.

Recorded the provider/package boundary in the delivery roadmap: the current
GitHub header special case is a temporary descriptor-option gap to replace with
shared validated transport options, not a pattern for new providers. Do not
start that contract refactor during this testing block. Ordinary provider package
changes belong in extension repos; runtime/diagnostics/client registration remain
platform responsibilities. Notion stays the next provider after this checkpoint.

**GitHub breadth and Notion sequencing, 13 October:** the user chooses Notion
next and requests broad provider capabilities before its live tests. Added
[the bounded delivery/ownership roadmap](GITHUB-NOTION-DELIVERY.md), separating
provider catalogue, account permissions and Agent schema selection. Current
installed GitHub discovery advertises **49 supported tools**, including writes
outside the two previously exercised actions (`github-alpha-yTL7At`, preflight
and catalogue Pass, cleanup Pass). An earlier open-app attempt was Blocked and
left the user's process alone. No writes occurred in this inventory check.

The SDK transport now requests `X-MCP-Toolsets: all` only for GitHub's exact
official endpoint, preserving the URL/OAuth binding. Existing catalogue bounds
are unchanged; live all-toolset discovery remains Pending until the binary
contains this change. The user is updating App grants. The supplied screenshot
confirms Actions/Discussions/Issues/Pull requests read/write but still shows
Contents read-only and Workflows/security permissions No access; completion
and installation acceptance were requested. Do not mark broad execution passed.

Custom MCP management and the old hosted-provider validation panel are hidden
in production and alpha source builds, retained for development/agent-harness.
Installed extension/account controls remain; these presentation gates do not
delete saved connections or disable their backend execution. **123 frontend
unit tests, 81 focused MCP Rust tests and the existing read-approval guard Pass**;
targeted lint, formatting, ordinary/alpha frontend builds and scoped review Pass.
The Rust header test checks the exact SDK configuration; the existing wire test
also verifies the header survives the bounded transport. Initial Windows test
attempts used the wrong cache/omitted the maintained manifest runner; the first
new assertion also used an unsuitable string lookup for a HeaderName-keyed map.
Correcting test setup/typed lookup yielded the final passing MCP suite; no
production bypass or runtime-library change was needed. Unrelated dirty files
remain untouched. No fresh release/installer workflow was dispatched.

Notion package preparation may overlap GitHub work; live Notion acceptance
follows broader GitHub checks. Its official hosted OAuth metadata advertises
dynamic registration, S256 and refresh support, verified without account access.
No Notion package, sign-in or live tool use is claimed complete yet. First-party
hosted extensions are small descriptor/listing packages; providers own their MCP
servers. Community package maintenance and Grain's shared runtime/OAuth client
registration responsibilities are documented in the roadmap.

**Live GitHub read/restart checkpoint, 13 October:** the user requested no
Computer Use. Added a focused installed-alpha script using existing Playwright
and production Tauri IPC instead of screenshot/click automation. No new
framework, fixture server, model relay or production testing seam. It launches
only its owned installed alpha, confines approval to the test repository's
README getter and independently compares the returned private file with GitHub
CLI. Credentials stay in Grain and the configured model is used.

Initial actual GitHub calls returned 404 even though the separate CLI account
could read the file. The user updated the GitHub App installation/repository
access and supplied a screenshot showing code read, issues read/write and all
repositories. After that change, `.runs/github-alpha-iuyNiW` passed installed
preflight, real Agent README read, account persistence after restart and real
Agent read after restart; **4/4 Pass, cleanup Pass**, one narrowly approved
getter per read. Both returned the independently observed README. Installed
binary SHA-256: `425610fd66921c14c9c26ebddc21b757b8d7c15419a8029aec567f7c0e72d3c2`.
Earlier guard refusals and observed 404 failures remain retained, not accepted.
This is command-path acceptance, not a physical shortcut/UI test. **Remaining
in this GitHub block: one disposable issue creation and disconnect/reconnect.**
No write has been dispatched by this script; no further sign-in is needed for
the now-passing read. Second provider remains undecided, not assumed Linear.

Also fixed an alpha-specific entry-point defect: the sidebar hid Extensions
in every non-development build. Explicit `extensions-alpha` and existing
`agent-harness` build modes now expose the entry point; ordinary production
builds still hide it. Alpha configuration selects that frontend mode; backend
Developer Mode and account gates are unchanged. Scoped review, **122 frontend
unit tests**, the approval-guard unit test, targeted lint and ordinary/alpha
frontend builds passed. The installed binary used for the read predates this
navigation fix; a rebuilt alpha still needs installed entry-point verification.
Unrelated Cargo.toml/generated bindings changes are preserved. Usage,
limitations and consolidation/cleanup conditions are in the harness README.

**Functional continuation, 13 October:** the user says the current design is
fine and asks to resume actual extension/MCP work. Record the current design
checkpoint as accepted; stop visual refinement. This does not certify new live
GitHub tool or account-recovery results. The next bounded block is supported
GitHub end-to-end use on the integrated alpha: real repository read, one
explicitly approved disposable issue creation, restart/read and disconnect/
reconnect. Keep exact action receipts and audit/fix that path before claiming
provider acceptance. Reuse existing tests; no new harness or tool-discovery
optimization block is needed. Build `38286531636` succeeded at `b0a0f397`.
After this checkpoint, choose a second supported provider; the user clarified
it may not be Linear. Then address publisher OAuth onboarding before
inviting general third-party pre-registered MCP publishers. Permanent identity,
broader distribution and deferred issued Linear expiry remain separate.

**Extension detail refinement, 12 October:** the user approves the shared
extension direction and requests a simpler detail page. Uninstall now sits
beside Installed for both implementations. Settings use labels/help on the
left and controls on the right; account actions and enablement are separate
rows, with advanced credentials still collapsed. Confirmation receives focus
and cancellation returns focus to its triggering control. The author's
DESCRIPTION renders directly, without the host's About/filename header,
background, border or horizontal inset, in both themes.

Fixed an actual artwork defect: `store_covers` applied native-only validation
to MCP entries. It now validates each artifact kind correctly and still
rejects privileged/invalid listings. Real artwork suppresses placeholder
gradient/decorative layers. Extended the existing two-kind listing test and
existing real-app store case; no new scenario/framework/runtime was added.
Frontend unit tests **121 Pass**, focused store backend tests **11 Pass / 1
network test intentionally ignored**, targeted lint/type/build and store Rust
formatting Pass; runner self-tests **78 Pass**. Existing real-app store suite
**3/3 Pass**, cleanup Pass (`.runs/run-Rz8zNq`), including actual signed image
loading, Uninstall beside Installed, confirmation focus, sign-in/cancellation,
removal and revocation. Earlier `run-2GyW8h` retains its failed artwork assertion
and cleanup Pass: the test catalogue had no image. Its existing fixture now
serves one tiny hash-bound public GIF; no test scenario was added. Visual
acceptance of this refinement remains Pending. Whole-workspace formatting still reports
preexisting unrelated audio-journal/rolling differences; those are untouched.
Preexisting Cargo.toml and generated bindings working changes are preserved
and excluded from this UI commit.

**Unified extension experience, 10 October:** native/MCP implementation is no
longer displayed on store cards. Store-installed extensions share one Installed
card layout, artwork, search, state, toggle and detail-page entry. Installation
opens the common detail page; account/sign-in, advanced connection configuration
and uninstall are managed there. Native enablement is also available on its
detail page. Manually configured MCPs remain alongside installed extensions.
Missing catalogue metadata does not hide the installed card or block account
management. Original native IDs and MCP connection ID/revision owners are kept;
transport, OAuth, vault, signed metadata and Developer Mode gates are unchanged.
No runtime, publishing service or test framework was added. Existing store-MCP
acceptance selectors follow the new flow; no scenario was added.

Frontend unit tests **121 Pass**, targeted lint/type checks and frontend build
Pass; existing runner self-tests **78 Pass**. Existing real-app store-MCP cases
**3/3 Pass**, cleanup Pass (`.runs/run-HEzMRJ`): acquisition/update/removal,
sign-in/cancellation/restart and revocation. Two earlier runs retain their
failures: `run-jX8j0K` expected a hidden store card after automatic detail-page
navigation; `run-1eWHkj` seeded installation through host IPC without refreshing
the common Installed inventory. Both cleanup Pass; assertions now use the new
detail journey and a restart after out-of-band seeding. Version, account/revision,
provider-contact and revocation assertions remain. No production workaround or
new case was added. User visual acceptance of this changed layout remains Pending.
Historical acceptance below is retained with its original scope.

**Main integration, 10 October:** implemented on
`extensions/main-integration-20261010`, merging app main
`bbae21ccadc4c6e8d65263e207d644db41ab74ad` into the extension platform. Main's
WebView recording, selected-text Agent, local transcription, fallback and
onboarding changes coexist with tool-only/native/MCP/store/account ownership
and the dedicated Agent/Snippets controls. Original dirty checkout preserved.
See [MAIN-INTEGRATION-CHECKPOINT.md](MAIN-INTEGRATION-CHECKPOINT.md) for the
source review, small fresh-checkout repairs and exact acceptance limits.
**24 real-app cases Pass**, with cleanup Pass across all four existing suites;
720 normal-backend, 116 frontend and 81 runner/client-metadata tests Pass;
the full Rust workspace and frontend build also Pass. The integrated
installed-alpha shortcut/dictation/live GitHub batch remains Pending; issued
Linear-token expiry remains Deferred. No blanket historical backlog is reopened.

**Continuation handoff, 10 October:** the user confirms the corrected installed
alpha works after the Agent-start fix. Record **Agent opening: user-tested Pass**;
GitHub installation/sign-in were already user-confirmed. Specific GitHub
read/write/restart/disconnect procedures remain Pending unless separately
reported; no blanket provider acceptance is inferred. Corrected installer:
build [37955644860](https://github.com/Punit-Dethe/Grain/actions/runs/37955644860)
at `a2215c73ad08f8394b76006045a74645cc25aacc`, successful and downloaded.

The [current platform handoff](EXTENSION-PLATFORM-HANDOFF.md) consolidates user
and developer experience, native/MCP contracts, OAuth ownership and third-party
onboarding limitations, publishing/hosting, verified results, deferred work and
the next integration checkpoint. After fetching app `origin/main`, the pre-doc
snapshot is **147 ahead / 57 behind**, with **21 intersecting changed paths**.
Main's WebView recording overlay, Agent and onboarding changes need semantic
integration. No merge/rebase or application code changes were made in this
documentation step. Continue by integrating main, not by expanding the harness
or interpreting historical phase/test counts as the current backlog.

**First GitHub catalogue publication accepted, 9 October:** implemented the
user-authorized explicit maintainer approval in the existing signer, scoped to
the exact GitHub 0.1.0 submission digest, registry/owner and alpha publisher.
No approving GitHub review was fabricated; the policy records review ID zero.
Normal third-party review, merged source/head/listing, provenance/byte pins,
expiry and delayed key checks remain. **83 maintainer tests Pass**, then the
affected approval/review cases passed after the final source-binding check;
strict maintainer Clippy/build Pass. This temporary approval profile and its
regression must be retired/replaced with the permanent publishing policy.

Registry PRs #2 and #1 are merged. Fresh source CI
[37944954523](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37944954523)
passed at `db6f4f31bd28668574239c655d4d66f256571194`. The real signer verified
current merged source, recorded maintainer approval and cryptographic provenance
before key access. Bootstrap
[37945302910](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37945302910)
and GitHub update
[37945822859](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37945822859)
both confirmed their single conditional push and public HTTP delivery.
Current registry `main` is `ea7498aebc028e35579878f5af5f9b1c329c48a2`, signed
index 3 with the sole GitHub entry. Exact public delivery checks **17 files / 40
requests**, status Pass; retained evidence is
`C:\Projects\Grain\grain-alpha-publication-20261009\evidence\github\publication.log`.

The existing normal-build live store test now requires an installable, nonempty
GitHub listing and uses each kind's actual validator. Its first run found a
native-only test assumption; after correcting that test, the genuine network
check **Pass** against the activated catalogue. Runtime store validation was
already correct. No new harness, runtime feature, dependency or third-party
approval relaxation. Replacement alpha build **37932435112** is accepted and
downloaded; later changes affect maintainer tooling and test-only code, so no
additional application rebuild is required.

Created the private disposable `Punit-Dethe/grain-github-alpha-test` repository
with README for the user's provider checks. Actual store install, App installation,
GitHub account consent, Agent read/write and restart/disconnect were initially **Pending**,
with the five-step batch in [onboarding](GITHUB-ALPHA-ONBOARDING.md#current-user-test-batch).
Generic earlier tests do not certify these live-provider actions. Catalogue
renewal before 8 November and secure alpha key backup remain maintainer operations;
permanent production governance/branding and deferred Linear expiry remain separate.

**User GitHub acceptance update, 9 October:** the user reports successful Store
installation and GitHub sign-in/connection in the installed alpha. Agent does not
open there, while it opens in development. Selective inspection of the alpha's
own settings found `agent_enabled: false`; its log contains no Agent shortcut
registration. The first diagnosis incorrectly treated this as a missed setup
step. After the user confirmed using the switch, source tracing identified an
application regression: the switch still called `extension_set_enabled`, which
rejects the retired core-feature IDs, and the settings store ignored the
structured error. Agent and Snippets now use dedicated core settings commands;
both switches roll back backend rejection, and Agent registers/unregisters its
shortcut outside the extension registry. Existing pre-initialization behavior
is preserved: save the setting and let normal shortcut initialization register
it. No retired extension capability was restored. Four frontend regression
cases and the two existing capture shortcut conflict cases pass; TypeScript
checking and compilation of the normal Tauri commands pass. The original
harness enables Agent directly, so its earlier runtime checks did not cover
this settings UI path. No harness changes were made. A corrected alpha build
and real-user Agent acceptance remain required. Agent read/write and
restart/disconnect tests remain Pending until verified. Ordinary development
builds also need the compile-time GitHub OAuth
registration supplied separately; successful alpha sign-in does not configure
the development profile/build. No account credentials were inspected.

**Alpha catalogue identity prepared, 9 October:** the first Windows build
[37926347284](https://github.com/Punit-Dethe/Grain/actions/runs/37926347284) succeeded
and its installer was downloaded. Publication inspection found only the old
public catalogue key; registry repository/environment signing secrets are absent.
The user clarified the available GitHub key is the updater's
`TAURI_SIGNING_PRIVATE_KEY`; its value was not retrieved or reused.

Prepared separate encrypted alpha root A, spare B and publisher using the
existing CLI, with role-specific random unlock files in an account-restricted
folder outside the repositories. Updated app public pins/empty seed signatures
and the existing signed test fixtures. No migration, runtime/authentication
feature, dependency or harness code was added. The completed old installer needs
replacement to trust the new identity; key availability should have been checked
before starting that build.

The existing publisher produced and verified an unpublished bound empty
bootstrap, local store and hosting bundle. Both genuine root signatures verify.
**12 trust / 4 bootstrap / 4 initial-publication / 11 store tests Pass**, store
cleanup Pass; report `tests/agent-harness/.runs/logic-q1xTsS/evidence/report.md`.
Fresh CI producer/candidate attestations and GitHub prepared-byte checks Pass.
Actual GitHub listing activation remains Pending. The current signer additionally
requires approval by a different GitHub account; PR #2 has no reviews. Decide
the first-party tester-alpha review route explicitly before signing, and update
the publication verifier pin to the new Grain identity. No fabricated review,
publication, protection change or private credential logging occurred.
[Onboarding](GITHUB-ALPHA-ONBOARDING.md#catalogue-signing-identity-reset-9-october)
records public pins, exact bundle hash, local custody/backup and remaining steps.

**Registered GitHub alpha build wiring, 9 October:** the user supplied
[Grain GitHub Development](https://github.com/apps/grain-github-development).
The app page exists and currently reports private visibility. Both required
Actions secret names are present on `Punit-Dethe/Grain`; no values were retrieved
or logged. Registration exists, but callback/permissions/expiry settings and
credential correctness still require actual account acceptance. Private
visibility supports owner-account testing; other tester accounts need Any account.

Reused Build Test and the existing reusable build workflow with one explicit
alpha toggle: Windows only, compile-time GitHub credentials only for this build,
early static-value validation, installer artifact upload and no release ID.
The alpha config uses its own product name/app identifier and empty updater
endpoints, with updater artifact generation off. No beta release/tag workflow,
signing key, Agent tool loader or harness was changed. Installers are not
Authenticode signed; updater checks cannot use the beta feed. Alpha replacements
are manual until a separate alpha update delivery is chosen.

Focused checks pass: workflow YAML/routing/secret scoping and beta-vs-alpha
configuration assertions, merged Tauri schema (validator ignores unrelated
numeric-format annotations), five actual PowerShell preflight cases including
missing/malformed inputs and refusal of release publication/wrong platform,
credential-output assertions, formatting and scoped diff checks. Existing MCP
backend acceptance is unchanged; these are build-configuration checks, not live
GitHub login tests. The CI installer build and signed public extension catalogue
delivery are the next checkpoints; no installer or provider action is accepted
from these local checks alone.

[First alpha CI build](https://github.com/Punit-Dethe/Grain/actions/runs/37926347284)
was dispatched at `2aa54d0aabd067adb343aeac360a87de3930aab5`. GitHub accepted the
reusable workflow and skipped the ordinary matrix. Checkout and the actual
registration-input preflight passed without retrieving/displaying secret values.
The installer build was still running at handoff; compilation/upload and actual
OAuth credential validity are not yet accepted. Stopping a local status watcher
does not cancel the GitHub Actions job.

**Tester-only GitHub identity, 9 October:** the user authorized proceeding under
the existing account and accepts reconnect/reinstall when experimental branding,
repositories or registrations change. The existing beta without extensions and
the extension alpha are separate deliveries; no compatibility/migration system
is needed. Reused the already published source/draft submission rather than
creating another repository. [Alpha onboarding](GITHUB-ALPHA-ONBOARDING.md)
documents a dedicated registered desktop client with PKCE, following GitHub's
official public/extractable desktop credential approach.

The backend now resolves compile-time alpha client credentials for the current
verified `com.grain.github` store owner and exact official resource. Exact issuer,
authorization and token endpoints are checked before supplying that credential;
custom MCPs, revoked owners and other packages cannot borrow it. Existing
explicit registrations keep their vault binding. Account tokens, lifecycle,
SDK transport and Connect/Disconnect controls are reused. No new OAuth service,
runtime engine, UI, author contract or Agent harness code was added.

The GitHub CLI is authenticated, but no signed-in browser session was available
for app registration. Opened GitHub's actual registration/sign-in page and asked
the user to sign in there, without sharing passwords/codes. Actual registration,
live GitHub sign-in/tool execution, alpha packaging and public signed catalogue
activation remain **Pending**. The unchanged unrelated Cargo/bindings edits are
preserved.

Checkpoint: the normal production MCP test group passed **80 tests**, with owned
process cleanup passing; evidence is
`tests/agent-harness/.runs/logic-9UlL44/evidence/report.md` and `mcp.log`.
Coverage includes exact build-client/resource/metadata binding, revoked and
custom owners, and a real SDK token exchange proving S256 PKCE and reuse of the
registered desktop credential for refresh. Scoped Rust formatting and diff
checks pass. Backend Clippy completed successfully with existing warnings; its
first attempt hit the existing native-build temporary-directory issue, and the
retry used the production runner's command-local Windows temp workaround.
Scoped graph/diff review checked registration lookup, token destinations,
reconnection and current store ownership; no claim of a platform-wide audit is
made. Live GitHub metadata matches the pinned issuer/endpoints and advertises
S256. These results do not certify actual GitHub sign-in or provider tool calls.

**GitHub first-party candidate, 9 October:** created and published
[source](https://github.com/Punit-Dethe/grain-github-extension) on
`extensions/github-mcp`, tag `v0.1.0`, commit
`3d5e1ec25e083613aa7b76c088a93ce49c24ecb5`. The 337-byte descriptor uses GitHub's
official hosted MCP server and OAuth. Original GitHub integration marks, supported
store media, DESCRIPTION, license exclusions and maintainer/verification notes
are included; there is no native worker, Node build or new harness/runtime engine.
Existing author doctor/pack, scoped submission inspection, source/tag preparation,
prepared verification and unsigned catalogue staging pass.
[Draft registry PR #2](https://github.com/Punit-Dethe/Grain-Extention/pull/2) carries
the real source pin and a small LF checkout fix for signed listing hashes.
A fresh Windows checkout validates the new GitHub and both current source
fixtures; the unrelated obsolete `com.example.hello` sample still fails the
repository-wide checker. No overall registry validation Pass is claimed.
[Linux source CI](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37887958577)
passed at registry `7666249f0d808066f7672beb337ea49a14ef7dac`, skipping native build.
Downloaded descriptor, DESCRIPTION and artwork exactly match Windows bytes;
producer/candidate attestation checks pass for that branch/head and expected signer.
Those checks are build provenance, not human review or production authorization.

Public endpoint preflight returned 401 with usable OAuth discovery and S256 PKCE;
no automatic registration endpoint exists. The user confirmed no GitHub app is
registered yet. The plan now includes exact initial app/callback/limited-repository
setup. Actual sign-in, GitHub tool calls, supported end-user onboarding and signed
public store installation remain **Pending**, not Pass. No catalogue activation,
production signing key access, model/tool action or account data access occurred.
Local nonsecret prepared/CI evidence is at sibling `grain-github-prepared`,
`grain-github-catalogue-candidate` and `grain-github-ci-evidence`; the registry draft
is in its own `grain-github-registry` worktree, preserving the original checkout's
unrelated changes. No acceptance service/listener was created. The shared platform
checkpoint remains accepted; this provider-specific block is now current.

**Current delivery direction, 9 October:** the user approved the current Installed
extension/configured-MCP layout and asked to move to supported first-party
extensions. Custom MCP management is implemented and sufficiently complete for
that move. The next block is one complete first-party store journey, using GitHub
as the reference: listing/browsing, installation, straightforward account sign-in,
usable state, real-provider Agent actions and account/update/removal controls.
Existing descriptor, official-SDK runtime, vault, CLI and GitHub publishing routes
are reused. GitHub-specific registration remains a concrete gap: current Grain
expects caller-configured client credentials; ordinary users of the supported
extension should not have to register OAuth apps or paste credentials/JSON.
Provider desktop OAuth/secret handling must be decided and verified, not assumed
solved by the generic connection tests. The public catalogue remains inactive;
signing/operator setup and real published-extension acceptance remain outstanding.
Current layout approval does not authorize future visual changes or release
activation. No application/harness code changed in this planning update.

**Current launch acceptance, 8 October:** steps **1 and 2 implemented; step 3's
automated checkpoint complete, release gates remain**. The two remaining genuine
configured-model recovery cases now **Pass**, each with cleanup Pass:
native `launch-native-recovery-idle/run-NDheTo/evidence` and MCP
`launch-mcp-recovery-idle/run-vfG0s8/evidence`, under ignored
`tests/agent-harness/.runs/`. Each case records two accepted stages: declining
the write leads to zero writes, independent verification and consumed-approval
refusal; an outage after the approved write preserves its actual receipt and
shows an unfinished-task notice without repeating it. Normal genuine native/MCP
read/write/verify and the eight controlled failure cases remain accepted. No
application or harness source, timeout, approval or receipt check changed in
these repeats. Recorded host PIDs no longer exist after both runs.

The plan now has one compact
[remaining-work checklist](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#remaining-launch-work--use-this-checklist-not-the-historical-counts)
separating application completion, accepted genuine-model recovery cases, user
visual approval and actual public catalogue activation. Older phase/inventory
counts do not add new launch milestones. No application or harness source changed
in this clarification. One fresh native recovery attempt received global Escape
before its first tool approval; the Agent closed and its model request was
cancelled. Its case remains Fail, cleanup Pass, with evidence retained at ignored
`tests/agent-harness/.runs/launch-native-recovery-final/run-4Y6tmv/evidence`.
This is an interrupted attempt, not demonstrated crash or recovery acceptance.
The user then confirmed desktop availability and both fresh repeats passed.
Confirm availability again for future interactive work. User
visual review needs `npm run dev:asr` and Extensions → Installed with Developer
Mode enabled; no credentials need to be shared. Signing custody, GitHub operator
controls, first reviewed entries, public delivery and fresh-profile acceptance
remain release gates under the existing maintainer runbook. Deferred Linear
expiry and permanent OAuth identity remain explicit follow-ups.

**Previous launch acceptance, 8 October (superseded by the accepted repeats above):** step **3 of the reduced three-step
launch plan** is current. Configured-model native **and MCP** read/one approved write/independent
verification **Pass**. The existing eight-case real-app interruption batch
**Pass**, cleanup Pass: native/MCP declines, actual approval expiry/restart,
closing pending/running work, post-write model failure and unknown-outcome
verification without replay. Five focused live-oracle/privacy/cancellation tests,
syntax and formatting pass. No application code or new harness framework changed.
The first genuine MCP run exposed a test-only refusal to let the model correct
invalid arguments. The existing single non-dispatched correction allowance now
also applies to ordinary workflows; attempts are recorded separately from
receipts, with exact action order, three dispatches and one write still required.
Two earlier isolated MCP attempts hit the relay's 60-second request cap; failures
are retained. The next genuine MCP run **Pass**, cleanup Pass: nine model requests,
three actual dispatches, one write and one corrected, non-dispatched read attempt.
Production deadlines and application code did not change. The test relay's prior
60-second cap was shorter than Grain's 120 seconds (`agent.rs` and
`grain_llm_client.rs`). The existing relay now matches 120 seconds; live stage
waits account for search/load/call and one refused-argument correction. Action,
approval, exact receipt/write counts and cancellation checks remain unchanged.
Five focused oracle/privacy/cancellation checks pass again. Additional genuine-model
interruption acceptance remains open: native **Fail** at 120 seconds; MCP's decline
stage **Pass** (zero writes, independent verification, consumed-approval refusal),
but its later task ended before writing, so the whole MCP case is still **Fail**.
Both remain unaccepted; cleanup Pass on all attempts. No retries were made after
those production-matched failures. Evidence is under ignored
`tests/agent-harness/.runs/launch-agent-acceptance-01/run-ifDUg4/evidence`,
`launch-agent-mcp-02/run-tEvphc/evidence`, `launch-agent-mcp-03/run-iRVG1b/evidence`,
`launch-agent-failure-safety-02/run-TJjapr/evidence`,
`launch-agent-mcp-04/run-65kZ7G/evidence` and
`launch-agent-interruption-live-01/run-03vWQk/evidence`,
`launch-agent-interruption-live-02/run-s6qVqI/evidence` and
`launch-agent-mcp-interruption-01/run-2CvBSw/evidence` in the same `.runs` root.
The first interruption batch passed its behavior checks but failed source identity
because a comment was edited during execution; the unchanged repeat above is the
accepted checkpoint. User UI approval and public catalogue activation remain;
live Linear expiry/permanent OAuth identity retain their existing follow-up status.

**Current store MCP journey, 8 October:** the reduced launch plan's ordinary
extension journey is implemented; user visual acceptance remains pending.
Verified store cards now include MCP descriptors and distinguish native/MCP.
Install/update route through the existing secure store commands. Installed MCPs
show version, account status, sign-in/cancel, enable/disable, sign-out and confirmed
removal; detail pages reuse those controls. Store endpoint/auth definitions stay
read-only. The app labels the signed DESCRIPTION document **About this extension**.
The [author entry point](../Extension%20Platform/AUTHORING.md) and existing
[CLI guide](../../crates/grain-ext-cli/README.md) describe native/MCP authoring,
direct connections and the current GitHub submission/maintainer workflow.
No new publisher, authentication layer, runtime or harness framework was added.
Existing acceptance cases were extended for real UI installation, updating,
OAuth cancellation/sign-in, search and detail removal. Scoped review found stale
browse rows after detail actions and a misleading native-only search message;
both are fixed and regression-verified. Verification: **19 backend / 26 frontend
unit / 16 real-app MCP checkpoint / 3 real-app native-store Pass**, cleanup Pass,
typecheck, scoped lint, formatting and frontend/native build. One existing live
network backend test remains ignored. Evidence: ignored
`tests/agent-harness/.runs/store-mcp-ui-final-02/run-jka49U/evidence` and
`tests/agent-harness/.runs/store-mcp-ui-native-store/run-CvXknf/evidence`.
These runs use the real application with scripted model/owned providers; they
do not certify live-provider or genuine-model behavior. Next: launch acceptance
against the configured model and first native/MCP extensions, plus user UI approval.
Public catalogue activation/signing custody remain operator gates. Live Linear
actual expiry and permanent OAuth identity remain explicit follow-ups. Unrelated
local bindings formatting and nested registry changes are preserved.

**Current launch MCP management UI, 8 October:** follows the reduced
[launch scope](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#launch-scope-reset--8-october-2026).
`McpConnections.tsx` and `mcp-connections.css` add Your MCP connections to the
Installed view in Developer Mode. The surface inherits Grain's existing
buttons, theme colors and hairline rows; fields have visible keyboard focus,
actions wrap, long names/URLs wrap, and closing forms restores trigger focus.
Remote HTTPS MCPs support form/strict JSON import, editing, browser sign-in,
cancellation, enable/disable, sign-out and confirmed removal. Pending sign-in
is cancelled when leaving. Optional OAuth app credentials use the system vault;
the callback URI is shown. Generated typed Tauri commands retain
configured/store/catalogue ownership; account IDs are not displayed.
Verification passed **8 backend tests**, typecheck, scoped lint and the real-app
frontend/native build. The final existing configured suite is **3 Pass, cleanup
Pass**, including UI add, enable/disable, strict JSON refusal, edit, removal and
keyboard focus. Scoped review closed callback instructions, long-name wrapping,
focus restoration and text encoding. Only the existing ownership case was extended.
Ignored local evidence: `tests/agent-harness/.runs/mcp-management-ui-01` and
`tests/agent-harness/.runs/mcp-management-ui-reviewed/run-ZwhrGg/evidence`. No live sign-in was
tested in this block; user visual confirmation remains outstanding. Live Linear
actual-expiry verification is deferred; permanent public OAuth identity is undecided.
Next: finish the existing native/MCP install/detail and GitHub author/publishing
journey, then basic real-Agent acceptance under the reduced launch plan.

**Current E4 signing-key tooling block, 8 October:** [Scoped custody audit](EXTENSION-SIGNING-KEY-CUSTODY-AUDIT.md)
records explicit protected Minisign generation/unlocking across bootstrap,
renewal and independently reviewed native/MCP signing. Missing/wrong passwords,
redirected prompts, unsafe names and existing key outputs refuse; current
public review/metadata gates still precede key access. Windows **82 maintainer /
17 publisher-transport / 13 initial CLI / 12 credential CLI** pass with locked
build, strict scoped Clippy and formatting. No app, SDK, UI or Agent harness
source changes. The existing Linux checkpoint is pinned to Grain
`84a579ff3b7f04ee8f3bd0dcb2019d977e37eb13`, registry
`2a074adbd1d062b23210f6a635a119352383228e`. Verified [Linux checkpoint 37758436225](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37758436225)
passes **85 maintainer / 12 trust / 5 cache / 17 publisher-transport / 25 CLI**.
Downloaded actual checkout evidence and all 25 CLI verdicts match Windows.
Actual production custody/authorization, GitHub controls, bound public
publication and fresh-profile app acceptance remain E4 gates. E5 management UI,
E6 Agent modules, E7 measured integration and E8 release readiness follow.
Foundation **52 Pass / 1 Deferred**, forward **71**, identity **56 Pending**,
inventory **108 / 93 / 81** and parked website remain unchanged. No new manual
batch or legacy compatibility work. Earlier entries are history.

**Current E4 HTTP-delivery block, 8 October:** [Scoped audit](EXTENSION-HTTP-DELIVERY-AUDIT.md)
records the read-only committed-bundle/anonymous-HTTP checker and post-push
workflow handoff. Exact commit/live files, final metadata, signed freshness and
remote head must agree; HTTP failure never retries publication. Windows **77 maintainer / 17
publisher-transport / 13 actual public CLI checks** pass, plus a real
anonymous HTTPS byte comparison and refusal of the incomplete public baseline.
No app/runtime/SDK/UI or Agent harness changes; no new manual batch. Read-only
GitHub inspection found **no publish-environment protection rules** and **main
not branch-protected**; configure/review actual operator controls and key custody
before activation. The initial Linux run exposed a close-only lock lifetime race; explicit owner
release and a deterministic Unix retained-descriptor regression address it.
Verified [Linux checkpoint 37685777643](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37685777643)
passes **79 maintainer / 12 trust / 5 cache / 17 publisher-transport / 13 CLI**
checks against Grain `0105abd7124d9eeb3c14ea8e14edde73c808ba01` and registry
`e33adb8060ee2141367c9fb1fad8583b299ba9e6`. Downloaded source pins, all 17
Python verdicts and all 13 public CLI verdicts match Windows.
E4's remaining acceptance is operator custody/authorization, the actual bound
public publication and fresh-profile app acceptance. E5 management UI, E6 Agent
modules, E7 measured integration and E8 release readiness follow. Foundation
**52 Pass / 1 Deferred**, forward **71**, identity **56 Pending**, inventory
**108 / 93 / 81** and parked website remain unchanged. Earlier entries are history.

**Current hosted-generation block, 8 October:** [Scoped audit](EXTENSION-METADATA-GENERATION-AUDIT.md)
records exact root/revocation JSON hashes inside the existing signed catalogue,
connected bootstrap/renewal/review, hosting/capture/publication and app acquisition
checks. Mixed/unbound publications cannot authorize installs; authenticated
negative policy still applies. No new service, signing role or metadata request.
Windows **300 core / 85 SDK / 77 maintainer / 11 backend store / 13 public CLI /
4 publisher / 78 runner / 6 real-app store** checks pass. Scoped strict lint,
real-app build and formatting pass; SDK all-target lint separately retains two
unchanged test warnings. One live-network test remains ignored. The obsolete
unbound-seed CI shell lane is retired; positive signed product coverage remains.
No new Agent scenario/framework/manual batch. E4 remains current for operator
signing/publication custody and actual signed public HTTP/app activation; E5–E8
follow. Foundation **52 Pass / 1 Deferred**, forward **71**, OAuth identity **56
Pending**, inventory **108 / 93 / 81** and parked website stay unchanged.
Verified [Linux checkpoint](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37681876972) passes **78 maintainer / 12 trust / 5 cache / 4 publisher / 13 CLI** checks; downloaded source pins and all 13 Windows/Linux verdicts match. Publication/checkpoint verifier pins are updated; the unchanged isolated source builder retains its independently verified producer pin. Earlier dated entries are historical.

**Current atomic app cache block, 8 October:** [Scoped audit](EXTENSION-STORE-CACHE-AUDIT.md)
records one bounded six-piece cache, reuse of the existing atomic writer,
monotonic/exact signed version identity, persistence failure refusal and restored
restart floors with no resident idle catalogue. Corrupt cache acquisition stays
disabled; independently valid signed policy/floors are retained. Windows **299
core / 11 backend store / 6 real-app store** checks, strict core lint, build and
formatting pass, including actual sharing-lock refusal and genuine signed
same-version replacement. One existing live-network test remains ignored.
No harness source/scenario/framework or manual batch. Verified [Linux checkpoint](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37674003172)
passes **5 cache / 11 trust / 77 maintainer / 4 publisher / 79 CLI checks**;
downloaded evidence matches both source pins and all retained verdicts.
E4 remains current for
hosted generation contract, operator signing/publication custody and actual
signed HTTP/app activation; E5–E8 follow. Foundation **52 Pass / 1 Deferred**,
forward **71**, OAuth identity **56 Pending**, inventory **108 / 93 / 81** and
parked website remain unchanged. Older dated sections are historical.

**Previous app freshness block, 8 October:** [Scoped audit](EXTENSION-STORE-FRESHNESS-AUDIT.md)
records the corrected signed-seed GitHub host, pending root adoption only after
verified fresh catalogue/revocations, mandatory authenticated fresh revocation
responses and root/revocation expiry checks before download and final native/MCP
commit. Cached signed kill switches remain enforced offline. Windows **294 core /
76 maintainer / 11 backend store / 6 real-app store / 78 runner tests** pass;
strict scoped Clippy, real-app build and formatting pass. One existing live
store-network test is ignored. The first native desktop run correctly refused a
fixture's missing revocations; a signed empty list repairs the existing fixture,
then both complete suites pass. No new scenario/framework or manual batch.
E4 remains current. Next address atomic durable cache selection/write failure,
restart floors and metadata-generation identity, then authorized signed public
activation/HTTP/app acceptance. E5–E8 follow. Foundation **52 Pass / 1 Deferred**,
forward **71**, permanent OAuth identity **56 Pending**, inventory **108 / 93 / 81**
and parked website remain unchanged. Existing lower dated sections are history.

**Previous first-publication/workflow block, 7 October:**
[Audit](EXTENSION-FIRST-PUBLICATION-AUDIT.md) records the clean initial Git gate,
manual current-contract source build and already-signed publication workflows.
The initial route preserves current seed trust/policy, requires fresh empty
metadata with next versions, verifies exact committed bytes, omits fabricated
previous proof and refuses an already-established publication base. The publisher
runs pinned trusted code, never author builds/signing keys, and uses a single
expected-base conditional push with remote confirmation and no ambiguous retry.
Windows **76 Rust / 4 publisher / 9 core trust / 12 real CLI checks** pass,
alongside locked build, scoped strict Clippy, rustfmt and scoped audit. Verified
[Linux checkpoint](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37662540187)
passes **77 Rust / 4 publisher / 9 core trust / 79 CLI checks**; all 67 retained
and 12 shared verdicts match downloaded evidence. The actual native/MCP source
builder [checkpoint](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37662540836)
also passes with both exact producer pins. No live catalogue/main activation,
new manual batch or Agent harness framework. E4 remains current for actual signed
activation and HTTP/app coherence; E5–E8 follow. Existing product acceptance
counts and deferred live expiry are unchanged. Earlier dated sections below are
historical and do not override the current clean-break scope.

**Previous clean-break block, 7 October:** [Cleanup/bootstrap audit](EXTENSION-CLEAN-BREAK-AUDIT.md)
records physical removal of E4i/E4j archive/migration commands, consumers,
legacy bindings/reservations and their exclusive fixtures/tests. The new
`bootstrap-serving-tree` creates an empty signed catalogue from current app
trust without reading old registry data. Windows **72 maintainer / 9 core trust /
8 public CLI checks**, locked build/strict Clippy and scoped audit pass. Linux
[checkpoint 37561458613](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37561458613)
passes **73 maintainer / 9 core trust / 67 CLI checks**. Source pins and all
59 retained verdicts plus eight Windows/Linux shared verdicts are independently
verified. Next implement the clean first-publication GitHub gate
(no previous legacy bundle), then current hosted/app coherence and publishing.
Five E4–E8 stages retain work; no new manual batch or Agent harness framework.
Other exclusive old paths may be retired in their own coherent blocks.

**Scope correction, 7 October — authoritative:** Grain is unreleased, before
pre-alpha, with no deployed users. The user explicitly removes old-extension,
old-settings and registry carry-forward requirements and supersedes the earlier
physical-removal hold. No more compatibility-only migration/archive work.
E4i/E4j's completed checks remain historical facts; their preservation machinery
is now cleanup debt. Remove its exclusive consumers/bindings/reservations/CLI
tests in one scoped block while retaining current source/signing/hosting safety.
Build a clean new tool-only catalogue; old routes/versions/settings do not need
support. Current authentication, protocol support, security, lifecycle, Agent
context and dictation correctness still require verification. This turn updates
policy/planning only; implementation has not yet been removed. See the
[revised execution policy](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#current-policy-unreleased-platform-clean-break--7-october).

**E4j migration implementation, 7 October:** [Migration audit](EXTENSION-LEGACY-MIGRATION-AUDIT.md)
records the connected archive verifier, genuine-key empty-baseline signer,
sticky signed version reservations through store/update/renewal/hosting, and
distinct first Git migration handoff. The actual legacy archive still preserves
all 109 files; active old versions remain retired. Windows **95 maintainer /
9 core trust / 25 public CLI checks**, build/strict Clippy and scoped audit pass.
[Pinned Linux checkpoint](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37525774777)
passes **96 maintainer / 9 core trust / 85 CLI checks**. Exact source, all 60
earlier Linux verdicts, all 12 earlier archive verdicts and all 109 identical
archive files are independently verified. No production key was accessed and
no production baseline was signed, approved or activated. E4 remains open for protected
signing/authorization, conditional GitHub activation, operational controls and
actual hosted-client coherence; **five E4–E8 stages retain work**. No new manual
batch or Agent harness expansion; existing foundation/forward/inventory counts,
parked website and physical-removal hold are unchanged.

**E4i legacy history preservation, 7 October:** [Archive audit](EXTENSION-LEGACY-HISTORY-AUDIT.md)
records a maintainer command that saves the original signed catalogue records
and referenced files without making the old extensions installable. Real history:
**18 proofs / 37 files / 27 content variants / 8 conflicted version identities**.
Every one of the **109 proof/asset files** is checked against committed bytes.
An early package added after its catalogue is explicitly recovered and attributed;
expired proof and retired presentation tier stay historical only. **80 Windows
maintainer tests / 9 core trust tests / 12 public CLI verdicts**, scoped audit,
strict Clippy/format/build Pass. [Pinned Linux run](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37516026483)
passes **81 maintainer / 9 core trust / 72 CLI checks**; source, all prior 60
verdicts and all 109 identical archive files are independently checked.
No Agent harness expansion, new manual batch, website work, old-file deletion or
production activation. This completes history capture, not migration or release.
Next enforce archive reservations in protected migration/hosting, establish a
genuinely signed/reviewed baseline, then conditional hosted activation and client
coherence. **Five E4–E8 stages retain work**; E4 remains open and existing
foundation/deferred/forward/inventory totals are unchanged.

**E4h committed publication capture, 6 October:** [Capture audit](EXTENSION-GITHUB-CAPTURE-AUDIT.md)
records a maintainer CLI command that reconstructs exact signed history/files
directly from a pinned Git commit, including empty folders, without reading dirty
checkout bytes or changing HEAD. Captured previous/current bundles feed the
existing conditional publication gate. Final Windows **69/69 full maintainer
tests and 12 actual CLI verdicts**, locked build/strict Clippy/format and scoped
audit pass; the real app-pinned positive CLI path and actual legacy baseline
refusal are verified. Read-only [Linux checkpoint 37502693151](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37502693151)
passes **70/70 full maintainer tests and 60 actual CLI verdicts**. Downloaded evidence
matches the exact source and preserves all 50 prior label/exit/oracle triples.
The legacy registry lacks signed revocations/new publication
proof, so first migration remains Pending; missing authority is never fabricated.
Protected migration/activation and actual hosted client coherence remain next.
No Agent harness source/scenario or manual batch. Five E4–E8 stages retain work;
baseline **52 Pass / 1 Deferred (12)**, forward **71**, full **56 Pending**, inventory
**108 / 93 / 81**, parked website and physical-removal hold are unchanged.

**E4g GitHub publication handoff, 6 October:** [Focused audit](EXTENSION-GITHUB-PUBLICATION-AUDIT.md)
records a maintainer CLI gate that binds the verified current/history bundles to
exact Git commits, requires a single expected parent, retains old proofs/assets
and emits an explicit expected-base conditional push handoff. A real disposable
Git remote accepts the exact base and rejects a stale writer while preserving
the winner. Final Windows **62/62 maintainer tests**, four production CLI admission
checks, locked build/strict Clippy/format and focused review pass. Pinned Linux
[checkpoint 37450479331](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37450479331)
passes **63/63 full maintainer tests and 50 actual CLI verdicts**. Downloaded evidence
matches source and all 46 earlier CLI verdicts, with four new precise refusals.
The command itself does not authenticate or publish. Protected GitHub execution,
initial legacy-history migration and coherent hosted client reads remain next.
No Agent harness source/scenario or manual batch. Five E4–E8 stages retain work;
baseline **52 Pass / 1 Deferred (12)**, forward **71**, full **56 Pending**, inventory
**108 / 93 / 81**, parked website and physical-removal hold remain unchanged.

**E4f renewal block, 6 October:** Stay on the confirmed GitHub registry host; the
optional OAuth website PR is parked. [Renewal audit/evidence](EXTENSION-METADATA-RENEWAL-AUDIT.md)
records metadata-only renewal and preservation of native/MCP files, entries,
unknown fields, roots and revocation rules. Fresh candidates use the existing
stale-parent-safe promotion and retained-history export. Final **54 Windows / 55
Linux full maintainer Rust tests**, **7 Windows / 46 Linux actual CLI verdicts**,
locked build/strict Clippy/format and scoped audit pass. Successful pinned Linux
CI `37425315653` records final source; failed candidate `37424961867` receives no
acceptance credit. No Agent harness expansion, desktop or
new manual batch. E4 deployment/coherence, protected history/review/key custody
and operational renewal/revocation remain. **Five E4–E8 stages retain work**;
forward **71**, baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**,
inventory **108 / 93 / 81** and physical-removal hold unchanged. Earlier Vercel
registry recommendations are superseded by the user's GitHub confirmation.

**Provisional hosting decision, 6 October:** [Decision and evidence](EXTENSION-HOSTING-DECISION.md)
records the user's authorization to use the experimental Vercel site, source/review
on GitHub and separately controlled static registry staging. Public OAuth JSON and
one-path headers are pushed on website branch `oauth/public-client-metadata`;
[website PR 1](https://github.com/Punit-Dethe/Grain-Website/pull/1) is open/unmerged.
Local document validation and three existing tests pass; actual Vercel preview
deployment succeeds but unauthenticated access redirects to SSO. Canonical JSON
still returns 404; no public/provider acceptance or production activation is
claimed. The document holds public client identity only, and URL replacement can
require fresh consent. No Agent harness source/scenario or manual batch was added.
E4 hosted coherence/coordination, history migration, protected review/key custody
and renewal/revocation remain. **Five E4–E8 stages retain work**, forward **71**,
baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81**
and physical-removal hold unchanged. Previous entries are historical.

**E4e archive-complete hosting handoff, 6 October:** [Scoped audit/evidence](EXTENSION-HOSTING-BUNDLE-AUDIT.md) records static bundle production and independent receipt-pinned verification. Fresh selected metadata and all retained native/MCP/document/media hashes are preserved with signed historical proofs; conflicting old version identities, altered/missing/extra files and stale/corrupt pointers/receipts refuse. **21 Windows / 22 Linux affected Rust**, **39 actual CLI verdicts per platform**, locked build/format/strict Clippy, scoped audit and owned-process checks Pass. No Agent harness source/scenario, desktop or human batch. This closes local archive preparation, not hosted activation: canonical routes/conditional deployment, protected prior-history migration, real independent release review/protections/key custody and renewal/revocation remain E4 work. **Five E4–E8 stages retain work**; forward **71**, baseline **52 Pass / 1 Deferred (12)**, full **56 Pending** and inventory **108 / 93 / 81** unchanged. No production/main/admin/key activation, legacy publisher change or held physical deletion; existing edits remain preserved. Earlier entries are historical.

**E4d historical reservation/export checkpoint, 6 October:** [Scoped audit/evidence](EXTENSION-SERVING-HISTORY-EXPORT-AUDIT.md) records retained signed-version identity checks across withdrawal/restoration and the new independently pinned selected-snapshot export. Changed code/description/media/type cannot return under an old version; valid installed orphans reserve identity while unsigned staging remains inert. Fresh complete exports retain exact bytes after later promotion. **15 affected Windows Rust / 25 actual CLI verdicts**, successful pinned Linux **16 Rust / 25 CLI verdicts**, locked build/format/strict Clippy and scoped audit pass. No Agent harness source/scenario, desktop rebuild or human batch. The local store remains append-only/operator-protected with explicit history budgets; deployment/history migration/protected review/key custody and renewal/revocation remain E4 gates. **Five E4–E8 stages retain work**; forward **71**, baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81** unchanged. Existing edits and physical-removal/public-hosting/release holds remain. Earlier checkpoint entries are historical.

**E4c complete local serving checkpoint, 6 October:** [Scoped audit and evidence](EXTENSION-SERVING-PROMOTION-AUDIT.md) records four product maintainer commands for complete signed native/MCP snapshots and serialized local promotion. The actual app-pinned trust chain, exact referenced files, fresh metadata, expected current-pointer digest, OS lock, retained old snapshots and crash-leftover refusal are verified. **36 Windows registry tests**, final **10 affected repeats**, **16 Windows CLI verdicts**, and [Linux CI 37417629606](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37417629606) **11 Rust / 16 CLI verdicts** pass, with build/format/strict Clippy and scoped audit. No Agent harness source/scenario, desktop rebuild or human batch. This accepts the local primitive, not hosted publication: coherent hosting/history, protected review/protections/key custody and operational renewal/revocation remain E4 work. Immediate-base version immutability is checked; withdrawn/reintroduced historical identity needs the protected publishing policy. Draft registry PR stays unmerged and the incomplete legacy publisher untouched. **Five E4–E8 stages retain work**; forward **71**, baseline **52 Pass / 1 Deferred (12)**, full **56 Pending** and inventory **108 / 93 / 81** unchanged. Existing edits and physical-removal/public-hosting/release holds remain. Disposable evidence and branch-only checkpoint wrapper have retirement conditions. Earlier entries below are historical.

**E4b3 pinned remote source/review checkpoint, 6 October:** [Scoped audit/evidence](EXTENSION-SOURCE-REVIEW-BUILD-AUDIT.md) records real tagged native/MCP source checkout, isolated native build, fresh trusted preparation and separate OIDC attestation. Final CI **37415224367** passes both paths. The current signer requires **schema-2** policy and actual merged GitHub submission/latest independent approval; schema-1/self/bot/stale/revoked approval refuses. **26 focused Rust / 20 actual CLI verdicts**, final affected repeats, build/format/Clippy and runtime cleanup pass, including two complete provenance-valid signing refusals with no key. Failed tool-size/skip/oracle attempts are retained and corrected. [Registry PR 1](https://github.com/Punit-Dethe/Grain-Extention/pull/1) is draft and unmerged; live positive protected review-to-signing remains **Pending**, not replaced by CI. Next E4 complete serving-tree/serialized promotion, then governance/renewal/revocation closure. No harness source/scenario, desktop build or user batch added. **Five E4–E8 stages retain work**; latest numbered forward acceptance **71**, baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81** unchanged. Physical-removal/public-hosting/release holds remain. Fixture branches/tags/submissions and scratch tools have a retirement ledger. Earlier entries are historical, including schema-1 fixture signing results.

**E4b3 genuine CI checkpoint, 6 October:** [Audit/evidence](EXTENSION-CI-PROVENANCE-AUDIT.md) records successful actual registry CI for controlled native/MCP candidates, with read-only preparation separated from maintained GitHub OIDC attestation. **24 affected Rust / 13 actual CLI verdicts** pass: independent producer provenance, two full positive signings and eight wrong workflow/commit/branch refusals; scoped audit/build/Clippy and owned cleanup complete. The signer now accepts scoped branch refs. Registry branch `registry/trusted-provenance` is pushed; its legacy publisher and main/settings/keys remain untouched. No harness source, desktop rebuild or user batch added. Genuine cryptographic provenance is now verified; **real author builds, protected production review and complete serving/release governance remain Pending**, so full E4b3/E4 is not certified and forward 71 remains latest numbered acceptance. Next actual pinned author-source/build and protected review-policy handoff, then E4 serving/promotion/renewal/revocation. **Five E4–E8 stages retain work**; baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81** and physical-removal/public-hosting/release holds remain. Temporary CI fixtures/trigger have an explicit retirement ledger. Earlier entries are historical.

**E4b3 local signing boundary, 5 October:** [Scoped audit/evidence](EXTENSION-REVIEWED-SIGNING-AUDIT.md) records the new maintainer `sign-reviewed-candidate`: independently pinned protected review policy, exact source/candidate snapshot, official GitHub attestation verifier, prior-index signature/version and delayed key access produce a verified signed update fragment. **149 focused Rust / 18 actual CLI commands (14 signing refusals)**, final affected repeats, build/Clippy and owned-process inspection pass. Audit corrected prior unknown-field preservation, UTC review dates and protected submitter attribution. Disposable test keys are removed; no harness source/scenario, application rebuild or new user batch. **Full positive trusted-CI/review-to-signing acceptance remains Pending**; forward 71 is still the latest accepted checkpoint. Next pinned isolated workflow/review/signing handoff and genuine attestation acceptance, then complete serving/promotion/renewal/revocation. E4 remains open; **five E4–E8 stages retain work**. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81** and physical-removal/public-hosting/release holds remain unchanged. Existing bindings and nested registry edits/workflows are preserved. Earlier entries are historical.

**E4b2 catalogue listing accepted, 5 October:** [Scoped audit/evidence](EXTENSION-CATALOGUE-LISTING-AUDIT.md) accepts forward **71**. Native/MCP catalogue candidates now carry separate user-facing DESCRIPTION/media metadata and exact content-addressed bytes; the app consumes signed DESCRIPTION through its existing detail command with new-listing size/hash checks. Candidates remain unsigned/dev with empty review fields; CI/review/signing authority is still next. **184 focused Rust / 81 Node / 6 executable expected verdicts / 6 real-app store cases Pass**, final affected component repeats and independent two-scope cleanup complete. Existing cases were extended; no new scenarios, framework or user batch. Next **E4b3 independent CI/review policy and signing-only publication boundary**. MCP cards/store redesign remain E5. E4 is in progress; **five E4–E8 delivery stages retain work**. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81**, public-hosting/release/physical-removal holds remain. Existing bindings and nested registry edits are preserved. Older entries are historical.

**E4b1 receiving accepted, 5 October:** [Scoped audit and evidence](EXTENSION-PUBLISHING-HANDOFF-AUDIT.md) accepts forward **70** for local received-byte/contract verification. In plain language: the maintainer can now check that a native or MCP bundle matches the reviewed submission and independent expected hashes before handing it onward. **139 locked Rust / 12 actual executable expected verdicts**, build/Clippy/scoped checks Pass. No new harness source, scenario or desktop batch. This is not CI provenance, signing or catalogue migration; next **E4b2 DESCRIPTION/media producer/consumer and independently trusted CI/review/signing handoff**. E4 remains in progress; **five E4–E8 delivery stages retain work**. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81**, public-hosting/release gates and physical-removal hold remain unchanged. No new user batch. Retained nonsecret one-off evidence is disposable; unrelated and nested registry edits remain preserved.

**E4a preparation accepted, 5 October:** [Implementation, scoped audit and evidence](EXTENSION-PUBLISHING-PREPARATION-AUDIT.md) accepts forward **69** for the unprivileged local producer. Native packaging now shares the author's validator/icon/bytes; native and MCP preparation checks local source/tag/project/listing identity and records unsigned artifact/listing digests. **133 locked Rust tests**, actual CLI/registry two-kind preparation/parity, overwrite preservation and changed-source refusal, builds/Clippy/scoped checks Pass. The audit closes Git configuration extensions and corrects the unsupported summary flag in author documentation. No new harness scenarios or desktop reruns were needed for this maintainer-only boundary. Next **E4b trusted review/artifact handoff and DESCRIPTION/media catalogue migration**; E4 stays in progress and **five E4–E8 delivery stages remain**. Registry workflows/remote ownership/protections/signing/publishing are not certified. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81**, release gates and physical-removal hold remain. No new user batch; nested registry changes are preserved.

**E3b accepted and E3 provisionally complete, 5 October:** [Submission/listing implementation, scoped audit and evidence](EXTENSION-SUBMISSION-LISTING-AUDIT.md) accepts forward **68**. In plain language: authors can now draft either native or MCP submissions with one strict contract, a separate user-facing DESCRIPTION and bounded image metadata. The maintainer checker uses the same rules; the old publisher refuses this new format until E4 migration. **126 Rust / 81 Node**, four generated contract checks, actual CLI/registry round-trip and expected tamper refusal, **12/12** real-app contract plus **3/3** store and fresh **1/1** repeat Pass. Three desktop scopes / **25 host PID references** independently clean. No harness source/scenario expansion or new manual batch. Next **E4 publishing**, then E5 UI, E6 runtime/recovery, E7 measurements and E8 certification: **five delivery stages remain**. E3 is provisional author tooling, not public freeze/publishing. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81**, release gates and physical-removal hold remain. Earlier dated entries preserve their original scope.

**E3a authoring accepted, 5 October:** [Focused implementation/audit/evidence](EXTENSION-AUTHORING-CLI-AUDIT.md) accepts forward **67** within provisional authoring scope. An author can now create native tool projects or metadata-only MCP projects using the same CLI; MCP doctor/pack reuse the actual host validator without Node builds or server processes. Both scaffolds include a future listing DESCRIPTION; publishing/listing migration is not complete. **35 Rust / 81 Node**, four generated native contract checks, **12/12** real-app contract cases, **3/3** store cases and a fresh CLI-to-store repeat pass; scoped audit and independent cleanup complete. Next **E3b structured submission and listing/media contract**, then E4 publishing and E5 UI. Six E3–E8 stages retain work. No new user tests, scenario or testing engine. Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory **108 / 93 / 81** and obsolete-code removal hold remain unchanged.

**E2i connection block accepted, 5 October:** [Exact evidence and focused block audit](STORE-MCP-ACQUISITION-AUDIT.md#e2i-real-app-activation-checkpoint-and-block-audit) accepts forward **66**. In plain language: signed store MCPs now connect and run through the same working system as directly configured MCPs. Updates invalidate old permission requests; endpoint/auth changes retire the old account; signed revocations stop running work and block old logins, including offline after restart. **16/16** combined actual-app cases and final **3/3** repeat Pass, with wrong-owner detection and independent seven-scope cleanup. **75 MCP / 81 Node**, build/types/Clippy and source audit Pass. Next **E3 SDK/CLI**; six E3–E8 stages remain. Store UI/publishing and production certification are still later work. No new tests for the user; baseline **52 Pass / 1 Deferred (12)**, full **56 Pending** and physical-removal hold remain. Inventory **108 / 93 / 81**. Earlier entries retain their dated scope.

**E2h backend update/revocation ownership, 5 October:** [Changes, scoped audit, evidence and remaining acceptance](STORE-MCP-ACQUISITION-AUDIT.md#e2h-verified-update-and-revocation-ownership). Verified store updates now require the exact installed revision, invalidate old work and retire/rotate accounts when the endpoint/auth changes. Resident revocations refuse copied runtime/vault owners and cancel/disable matching store records. **20 core / 10 store / 75 MCP** tests pass, including a final affected repeat; types/build/Clippy pass. No harness expansion or new human batch. Next is the combined real-app store checkpoint and full-block audit before activation, then SDK/CLI. E2 remains in progress; baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, earlier forward verdicts, inventory **105 / 90 / 80**, seven E2–E8 stages and physical-removal hold are unchanged.

**E2g backend acquisition foundation, 5 October:** [Implementation, review, tests and remaining checkpoint](STORE-MCP-ACQUISITION-AUDIT.md). MCP descriptors now use explicit signed catalogue metadata, bounded verified downloads and inactive source-separated registry persistence. Core/SDK and normal backend component checks passed; no new harness scenarios, instructions or fault modes. Store activation stays disabled until verified updates/revocation and real-app checkpoint acceptance. No numbered Pass or new user testing assignment; existing baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, forward verdicts and seven E2–E8 stages remain unchanged. [Testing policy](EXTENSION-COMPATIBILITY-CONFIGURATION-AUDIT.md#5-faster-development-without-weaker-acceptance) now prioritizes focused tests during development and real-app acceptance at coherent feature checkpoints.

**E2f registered clients accepted and audited, 4 October:** [Implementation, limits and evidence](CUSTOM-MCP-CLIENT-REGISTRATION-AUDIT.md) accepts forward **65**. Configured connections share SDK public/confidential setup, exact issuer/account/revision bindings, same-ID secret rotation, logout retention, explicit reset and retirement; old approvals/consent/active work are invalidated honestly without replay. **36/36** broad checkpoint (`run-e3Ewfy`), final **5/5** repeat (`run-CtYa7D`) and detected fault (`run-Gty0cN`, exit 1) have expected verdicts. **73 MCP / 13 registry / 80 Node** checks, builds/type checks and scoped checks pass; Clippy retains 68 existing warnings. Nine completed scopes / **121 PID references** independently clean; all new completed cleanup Pass. Failed/incomplete scopes and harness-only corrections remain documented, with no invented acceptance. Inventory **105 / 90 / 80**. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60/61/62/63/64/65 Pass**, full **56 Pending**. E2 remains in progress; next verified store descriptor acquisition. Seven E2–E8 stages retain work. No user test batch, UI redesign, release, public freeze or physical obsolete-code removal. Older entries are historical.

**E2e configured OAuth accepted and audited, 4 October:** [Implementation, scope and evidence](CUSTOM-MCP-OAUTH-AUDIT.md) accepts forward **64**. Direct OAuth connections use shared SDK DCR/vault execution with exact host account/revision ownership, guarded connect/status/disconnect, independent same-endpoint accounts, refresh/restart, stale-consent refusal and retired-account disposal. Disable/re-enable preserves login; removal and destination/auth changes clear the old account before metadata publication. Stable **34/34** checkpoint passes with original deadlines; after an equivalent Clippy guard refinement, final-build management/runtime/smoke/OAuth repeats (**11 cases**) and exact wrong-account control pass their expected verdicts. **73 MCP / 13 registry / 79 Node** tests, builds/type checks and scoped checks pass; Clippy retains 68 existing warnings. Twelve completed scopes / **109 host PID references** independently clean. Interrupted `run-54t29c` has no acceptance credit and retains three policy-blocked temporary directories, with scoped credentials cleared and no owned processes/listeners. Inventory **103 / 88 / 79**. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60/61/62/63/64 Pass**, full **56 Pending**. **E2 remains in progress**: custom preregistered client/issuer setup, then verified store acquisition. Seven E2–E8 stages retain work. No new manual batch, UI change, shipping claim, public freeze or physical obsolete-code removal. Earlier checkpoints are historical.

**E2d configured anonymous runtime accepted and audited, 4 October:** [Implementation, source audit and exact evidence](CUSTOM-MCP-RUNTIME-AUDIT.md) accepts forward **63**. Saved anonymous servers now use the existing MCP executor with guarded enablement, exact revision/account approvals, real restart persistence and pending/active mutation cancellation. Unchanged saves and another instance's removal preserve valid work; dispatched cancellations report uncertainty without replay. Configured OAuth stays inactive. Final stable checkpoint **31/31 Pass** in **664,936 ms** of scenario time preserves all genuine deadlines; fresh custom repeat **3/3 Pass**, exact wrong-owner fault detected, seven scopes / **126 host PID references** independently clean. Two startup defects and an in-flight help-edit identity rejection were fixed/retained without weakening requirements. **71** normal MCP, **13** registry and **78** Node tests, builds/type checks and scoped static checks pass; Clippy retains 68 existing warnings. Inventory **100 / 85 / 78**. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60/61/62/63 Pass**, full **56 Pending**. **E2 remains in progress**, next configured OAuth/current consent/vault/retired-account disposal, then store acquisition; seven E2–E8 stages retain work. No manual batch, UI change, public freeze, release or physical removal. Older entries below retain historical scope.

**E2c host management accepted and audited, 4 October:** [Commands, audit and exact evidence](CUSTOM-MCP-HOST-MANAGEMENT-AUDIT.md) accepts forward **62**. Direct MCP definitions can be imported/listed/edited/removed through guarded real Tauri commands. They stay inactive; JSON cannot choose account identity, enablement or credentials. Exact string revisions, stale/concurrent edits, account rotation, damaged/external state, 32-record bounds and real restarts are verified. Three new cases pass repeatedly in one shared build, final scenario time **6,812 ms**; catalogue transport/OAuth and two native smoke regressions pass. Exact cleanup fault fails at its named stage. Eight scopes / 42 host PID references independently clean. 69 normal MCP tests, 13 core filesystem tests, 76 Node self-tests, locked build/type checks and Clippy pass. Inventory **97 IDs / 82 self-contained / 76 self-tests**. Baseline stays **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60/61/62 Pass**, full **56 Pending**. **E2 remains in progress**, next shared configured runtime/authentication/current ownership and retired-account disposal, then store acquisition. Seven E2–E8 stages retain work. No manual batch, UI change, public freeze, release or obsolete-code deletion. Earlier checkpoints below are history.

**E2b shared destination boundary accepted and audited, 3 October:** [Implementation, research, audit and evidence](MCP-DESTINATION-BOUNDARY-AUDIT.md) accepts forward **61**. Public HTTPS/DNS checks now guard the shared MCP client and official SDK OAuth HTTP hook; private/mixed DNS answers, literal/local/unsafe metadata URLs and oversized OAuth bodies refuse. No parallel OAuth state machine or dependency is added. Final 68 normal MCP tests and 75 self-tests Pass; all 23 real-app MCP cases Pass before redundant factory hardening, followed by final destination/issuer, two actual public reads, two smoke checks and detected refusal-oracle fault. Seven scopes independently clean. Inventory **94 IDs / 79 self-contained / 75 self-tests**. Baseline stays **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60/61 Pass**, full **56 Pending**. **E2 remains in progress**: direct configured import/management/runtime/auth ownership and verified store acquisition remain. Seven E2–E8 stages still retain work. No human batch, public freeze, production release or obsolete-code deletion. Older checkpoints below are history.

**E2a storage/admission foundation accepted and audited, 3 October:** [Direct MCP definition, registry, source audit and evidence](CUSTOM-MCP-CONNECTION-REGISTRY-AUDIT.md) accepts forward **60** for strict package-free metadata and host-owned persistent connection/account/revision identity. Saves are atomic/conflict checked; endpoint/auth edits retire account bindings, label edits preserve them while invalidating stale leases, unchanged saves preserve bytes, and reload/remove/re-add reject old ownership. No credential, enabled state, client, process or custom import/runtime/UI route is created by this library. Final 498 workspace tests, thirteen fresh storage cases, 74 maintained self-tests, the twelve-case broad real-app regression followed by final-build MCP/Agent/negative checks, and independent five-scope cleanup Pass. Inventory stays **93 IDs / 78 self-contained / 74 self-tests**. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59/60 Pass**, full **56 Pending**. **E2 remains in progress**, next custom acquisition/runtime/auth integration; seven E2–E8 stages still retain work. No manual batch, release claim or physical removal. Older checkpoints are history.

**E1 provisional contract completed and audited, 3 October:** [Compatibility/configuration audit](EXTENSION-COMPATIBILITY-CONFIGURATION-AUDIT.md) accepts new forward **59**. Native installed/developer/doctor paths share bounded API validation; future catalogue requirements refuse download/staging while keeping valid tool cards visible. Configuration/account/version/error ownership is settled provisionally, with future E2/E5 setup clearly separated. Twelve real-app checkpoint cases, fresh compatibility repeat, exact negative oracle, 485 workspace tests, four developer/eight store normal backend tests, 74 Node self-tests and independent three-scope cleanup Pass. One checkpoint took **59,082 ms of scenario time**, without shortening clocks or removing tests. Inventory **93 / 78 / 74**. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58/59 Pass**, full **56 Pending**. **Seven delivery stages E2–E8 remain**, next E2 custom MCP acquisition/persistence. No new human batch, public freeze, production release or physical deletion. Older entries below are historical.

**E1b descriptor/identity accepted and audited, 3 October:** [Contract, research, audit and final evidence](MCP-DESCRIPTOR-IDENTITY-AUDIT.md) closes forward **58** for strict remote MCP metadata and separate host-owned connection/account/source identity. Publisher metadata cannot provide credentials, trust, account IDs, enablement or runtime commands. Existing catalog endpoints/authentication and all vault keys remain exact; new configured/store namespaces are defined but not yet connected to import/runtime. Tests caught and fixed permissive nested auth parsing, URL authority/DNS syntax and normalized URL growth. Nine new Rust matrices, 357 workspace tests, 59 normal MCP backend tests, feature catalog invariant, 73 harness self-tests, generated author regression, all 22 real-app MCP foundation cases/fresh ownership repeat/two smoke cases Pass; deliberate wrong-account corruption is detected and four scopes independently clean. Baseline **52 Pass / 1 Deferred (12)**; forward **54/55/57/58 Pass**, **56 Pending**. Inventory stays **92 / 77 / 73**. E1 remains open for remaining compatibility/configuration rules before E2 custom connections. All eight E1–E8 lanes still have work. No new manual test, physical deletion or public hosting activation. Older entries below are dated history.

**E1a author contract accepted and audited, 3 October:** [Public contract, audit, evidence and retained work](NATIVE-AUTHOR-CONTRACT-AUDIT.md) closes forward **57** for generated public-type isolation and native handler/result/context parity. CLI scaffolds no longer export daemon/pill types, compile before bundling, and expose the existing write key without ambient context. Worker envelope recognition now uses key presence, fixing null/empty and malformed false-valued branches. Audit repaired optional-undefined typing and a consent locator colliding with Grain's update dialog. Generated project/15 negative compiler cases, 82 SDK/CLI tests, six worker tests, 73 harness self-tests, three real-app foundation cases/fresh contract repeat, failure handling and both smoke cases Pass; deliberate error-to-success corruption is detected and six scopes independently clean. Baseline **52 Pass / 1 Deferred (12)** stays; forward **54/55/57 Pass**, **56 Pending**. Inventory **92 IDs / 77 self-contained / 73 self-tests**. No manual tests now. E1 is partially complete; next is **E1b MCP descriptor/connection identity**, with all eight E1–E8 lanes still open. Hosting and physical-removal holds remain. Older entries below are dated history.

**Client metadata integration verified; public activation gated, 3 October:** [Focused implementation/audit](MCP-CLIENT-METADATA-AUDIT.md) records the SDK request/callback seam, owned server-side HTTPS document, registration priority/no-DCR, DCR compatibility, exact document refusal, cancellation/port conflict and modern/restart/legacy reads. Full thirteen-case auth batch and fresh metadata repeat Pass; the wrong-mechanism fault is detected. The regression caught and repaired preregistered callback conflict moving after metadata requests; the strict zero-request oracle remains. The user confirms all existing websites are experimental, so production has **no metadata URL enabled**. Baseline **52 Pass / 1 Deferred (12)** and forward **54/55 Pass** stay; full forward **56 Pending for permanent hosting and eligible provider acceptance**. Inventory **91 IDs / 76 self-contained / 73 self-tests**. No new manual account batch. Remaining: one auth activation prerequisite, then eight E1–E8 lanes; seven R0–R6 release gates remain open within certification. Physical removal remains on hold.

**Authentication recovery/status accepted, 3 October:** [Implementation, focused audit and evidence](MCP-AUTH-STATUS-AUDIT.md) closes additional forward criterion **55**. Temporary metadata/network/refresh failures preserve stored accounts and give retry-later guidance; definitive grant refusal gives explicit sign-in advice; client registration and vault/setup failures have distinct recovery. Real developer-panel Test refreshes failure status and shows Sign in again only for reconnect. The final twelve-case MCP-auth batch, fresh seven-schedule recovery repeat, detected negative control and six Agent/two smoke regressions Pass. SDK OAuth ownership and post-dispatch no-replay remain intact. Original baseline **52 Pass / 1 Deferred (12)**; forward **54 and 55 Pass**, inventory **90 IDs / 75 self-contained / 70 self-tests**. No new manual auth batch. Next bounded unit: client metadata registration after its real public HTTPS document/redirect prerequisite; E1–E8 delivery and R0–R6 gates remain open. Physical removal stays on hold.

**Registry source limitation resolved, 3 October:** [Local repository audit](LOCAL-EXTENSION-REGISTRY-AUDIT.md) finds the separate `grain-extensions/` checkout and accessible `Punit-Dethe/Grain-Extention` remote at `af6e244…`. The previous 404 used the app's different bootstrap repository. The follow-up records twelve publishing requirements, actual failed workflow runs/no releases, absent release protections and four retired catalog entries. Existing nested edits/deletions/untracked draft are preserved; no registry/product/GitHub changes or new tests occurred. E4 source inventory is complete within the audit's limits, while publishing/keys/serving acceptance remains open. Ledger stays **52 baseline Pass / 1 Deferred (12), forward 54 Pass**; inventory **89 / 74 / 70**. Earlier inaccessible-source notes below are historical.

**Broader ecosystem planning recorded, 3 October:** [Blast-radius audit and delivery lanes](EXTENSION-ECOSYSTEM-BLAST-RADIUS-AUDIT.md) distinguish the already reduced runtime/author API from unfinished public SDK type generation, CLI parity, MCP descriptors/custom JSON connections, external repository/publishing, catalog migration, DESCRIPTION/media, management UX and measured discovery. The [forward plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md) now includes these before public contract freeze/release. No application/harness change or new test was performed. Ledger stays **52 baseline Pass / 1 Deferred (12), forward 54 Pass**; inventory **89 IDs / 74 self-contained / 70 self-tests**. Proposed suites are not new completed checks. External repository/workflow access is unverified after a 404; removal stays on hold, all R0–R6 gates remain open. Next bounded implementation is still authentication recovery/status, not a simultaneous ecosystem rewrite.

**Configured-client issuer ownership accepted, 3 October:** [Focused audit](MCP-REGISTRATION-ISSUER-AUDIT.md) reproduces and fixes actual old-secret forwarding when an MCP resource switches authorization servers. Saved public/confidential registrations now have exact issuer/client ownership in the OS vault, retained across logout. Connect and stored-account use refuse mismatches before consent/secret use; explicit Save discovers first, preserves existing credentials on discovery failure, and publishes safely under existing operation/generation guards. Legacy preregistrations require one explicit credential re-save; Linear dynamic registration is unchanged. Final 11 MCP-auth cases, clean issuer repeat, six shared Agent and two smoke cases Pass; deliberate missing rotation is detected. Logic/admission/self-tests: 53 normal MCP, 55 feature MCP, 18 guards, 70 Node Pass. **Forward check 54 Pass**, separate from baseline **52 Pass / 1 Deferred (12)**. Inventory **89 IDs / 74 self-contained / 70 self-tests**. No new human batch. Next: typed authentication recovery/status, then metadata registration. Physical removal remains on hold; the three older interrupted scratch exceptions remain Pending.

**B5b native OAuth reuse accepted, 3 October:** standard authorization URL/PKCE/code exchange/refresh now use exact `oauth2` 5.0.0 with the bounded Grain HTTP adapter. Existing account/vault formats, cancellation, scopes and refresh/logout ownership remain. Final verification: 24 normal and 24 feature auth logic tests, 69 harness self-tests, eight native-auth cases, ten MCP-auth cases and six Agent workflow cases Pass; deliberate partial-consent corruption fails correctly. Build/check/Clippy and scoped formatting pass, with pre-existing lint debt retained. [Focused audit, evidence, boundaries and cleanup](NATIVE-OAUTH-REUSE-AUDIT.md). Ledger stays **52 Pass / 1 Deferred (12)**; inventory **88 IDs / 73 self-contained / 69 self-tests**. No new manual batch. Next: configured-client issuer ownership and typed recovery, then client metadata registration. Physical retirement stays on hold. The session-interrupted `run-uZZ7J8` has zero scoped grants/secrets after recovery, but scratch/TLS deletion was policy-blocked; it is a third separate Pending cleanup exception, not a passed run.

**Current direction, 3 October:** original baseline **52 Pass / 1 Deferred (12) / 0 active baseline Pending**, 15 human and 37 reviewed automated results; additional forward **54 Pass**. The user explicitly defers live Linear expiry/refresh and authorizes forward implementation; it is not passed and remains a live-provider release prerequisite. SDK/native OAuth reuse and the focused registration-issuer unit are accepted; typed recovery/status is next. **B5c obsolete-code removal is on hold**. Tool calling is outside this comparison. Inventory **89 IDs / 74 self-contained / 70 harness self-tests**; no new human batch. All seven release gates and retained findings remain open. Older dated counts below are historical snapshots.

**B5a SDK compatibility accepted, 3 October:** official `rmcp` is pinned to exact 3.5.0; only its lockfile version/checksum and the deprecated client type name change in production. Two complete nineteen-case real-app foundation runs pass with identical 258 observations/172 calls, unchanged real deadlines and cleanup. Affected shared workflows/smoke, public DeepWiki/Hugging Face, Linear no-consent checks, 49 normal/51 feature MCP tests, six marker guards and 69 self-tests pass. The deliberate catalog corruption fails correctly with zero dispatch; official legacy/modern tools pass while standalone initialization remains Blocked. Independent inspection verifies fifteen application scopes, 88 recorded PIDs, 27 listener ports, absent scratch data/TLS and eight zero-count vault inventories. [Research, focused audit, identities and exclusions](MCP-AUTH-LIFECYCLE-REUSE-AUDIT.md). No baseline count inflation, new test ID, human assignment, wrapper deletion or tool-calling policy change. Native OAuth is next; configured-client issuer binding, recovery classification and metadata registration remain separate follow-ups. Full resource/release certification and live 12 stay open.

**Live Linear browser acceptance (3 October):** [Focused audit](LINEAR-LIVE-AUTH-AUDIT.md) reviews the user's actual `run-Flbf3s`: browser cancellation, reusable callback/zero grant, fresh explicit read-only consent, 38 supported tools, real grant restoration after restart and scoped cleanup all Pass. Independent binary/process/listener/vault inspection, 64 runner self-tests and 18 Agent logic tests pass. Accept check **9**; current ledger **51 Pass / 2 Pending (12/17)**, 15 user-reported / 36 reviewed automated. The actual grant lifetime is 86,100 seconds; refresh is available but untested, and this completed procedure removed its grant. The user also reports ordinary Linear issue creation and assignment working; intermittent task-budget termination is retained as **AGENT-BUDGET-01**, with no immediate limit increase or tool replay. Next: actual safe/nested Agent read plus restart, then real-clock expiry/refresh evidence. Whole B2/B3 and all release gates remain open.

**Earlier user-reported Linear connection (3 October, before harness review):** in ordinary Grain, the user reports cancelling an initial browser sign-in attempt, then successfully approving read/write access on a subsequent Connect attempt. The provider's Test button reported 68 tools. Record ordinary connection/discovery as user-reported successful observations; the isolated browser procedure, callback reuse, restart, actual grant expiry/refresh and genuine nested Agent read remain unverified by this report. Ledger remains **50 Pass / 3 Pending (9/12/17)**. Cancelling ends the current normal sign-in attempt; only the explicit harness procedure starts a separate second attempt. The user identifies this as an empty test workspace and authorizes disposable project/issue creation, edits and deletion for subsequent reviewed tests. Do not copy its ordinary-profile grant into the isolated harness or broaden the authentication-only harness's read-only/no-tool boundary.

**Historical status after check 9, before check 17:** 3 October 2026. **Branch:** `extensions/tool-only-retirement`. **Status:** testing-only execution; **51 Pass / 2 Pending** (15 human, 36 reviewed automated). All ten B1 requirements, B2 checks 8/20/23/31, B3 checks 9/10/11/13/14/15 and all baseline B4 checks 46–48 are accepted. The [workflow interruption audit](AGENT-WORKFLOW-INTERRUPTION-AUDIT.md) closes denial, real expiry/restart, Stop, uncertainty and completed-receipt preservation, with controlled and configured-model repeats, detected faults and cleanup. Remaining numbered work: **B2 17 (1) + B3 12 (1)**. Historical official tools remain Pass; standalone initialization stays Blocked. Whole B2/B3 and all seven release gates remain open. No SDK/OAuth replacement or new feature starts. Two interrupted-run scratch cleanups remain policy-blocked/Pending with no final acceptance credit. The [provider-independence audit](MCP-PROVIDER-INDEPENDENCE-AUDIT.md) accepts check 14 with repeated controlled account isolation, dynamic-login cancellation and explicit fixed-port conflict recovery. The user selected Linear for the remaining live-account preparation; live browser cancellation/consent and grant restoration are reviewed; actual expiry/refresh and nested Agent reads remain pending.

**Controlled check 12 prerequisite:** the [MCP expiry/refresh audit](MCP-REFRESH-RECOVERY-AUDIT.md) adds actual SDK expiration, rotated-grant restart and refused-refresh recovery coverage. This does not change the **50 Pass / 3 Pending** ledger. Linear resource/issuer metadata is reachable from this machine; the finite isolated live-account path and human consent remain next. No ordinary account grant is copied, and API documentation cannot certify the actual MCP lifetime.

**Historical Linear browser-handoff preparation (2 October):** the [focused audit](LINEAR-BROWSER-CONSENT-AUDIT.md) adds explicit human sign-in mode, private native browser handoff, pre-publication read-scope validation, metadata-only grant/lifetime/discovery reporting and bounded terminal/IPC cleanup. Real control guards and existing preflight/regressions are verified; actual browser consent/token exchange remains unrun until the account owner is present. Current inventory **86 IDs / 64 runner self-tests**; ordinary `all` remains **73**. The ledger stays **50 Pass / 3 Pending (9/12/17)**. No tool executes in this authentication-only procedure and no idle grant/profile is retained afterward.

**Linear no-account prerequisite:** the [Linear consent preflight audit](LINEAR-CONSENT-PREFLIGHT-AUDIT.md) records two opt-in real-SDK/live-issuer cancellation cases, fresh-restart repeats, a detected missing-cancellation fault, exact read scope/resource/PKCE and zero retained scoped credentials. Browser consent, granted scope/lifetime and harmless authenticated reads remain the next isolated account unit; no human sign-in is needed yet. The ledger stays **50 Pass / 3 Pending (9/12/17)**; no new platform features start. Current inventory: **84 IDs, 73 self-contained, 60 harness self-tests**.

**Accepted check 48:** Eight controlled interruption cases passed their first run and clean repeat; two genuine configured-model adapter cases each passed denial and post-write outage tasks in two fresh profiles. Evidence includes the actual 120-second approval deadline and host restart, blocked changed-argument write retries, pending/active close, displayed completed receipts with unfinished work, and one uncertain MCP write independently verified without replay. Three deliberate faults were detected; 16 affected regression cases and all 49 runner self-tests passed, with cleanup and independent resource inspection passing. The maintained inventory is 78 scenarios. No new manual batch or production implementation change is assigned by this unit; see the [focused audit](AGENT-WORKFLOW-INTERRUPTION-AUDIT.md).

**Agent selected-tools and continuation accepted (2 October):** [Focused audit](AGENT-WORKFLOW-AUDIT.md) accepts **46 and 47** together. Six real-app controlled cases and two genuine-model cases pass twice in fresh profiles. Metadata stays schema-free; incremental tool loading preserves earlier selections, catalog/directory/byte limits are enforced, and both native and MCP read → approved disposable write → verify tasks execute exactly one write. Batched tails, three duplicate approvals per adapter and disabled-owner writes cannot dispatch. Four deliberate faults are detected at their exact assertions; affected authentication/transport/foundation/smoke regressions and cleanup pass. **48 Pass / 5 Pending**, inventory **68 IDs / 44 self-tests**. The user-selected configured model supplied actual calls and final replies; no personal provider objects or keys entered the isolated profile/evidence. Check 48 and whole B4 remain open. No new manual batch or ordinary production Agent/auth/dependency change.

This is the maintained progress report beside the [original extension plan](MCP-EXTENSION-PLAN.md) and [forward audit/reuse execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md). The original plan owns the product contract, phase gates and exact test procedures; the forward plan owns the next delivery order, source disposition and pending-check mapping. This document records implementation, deviations and user-facing results, not another execution order.

**Execution-order correction (1 October):** complete testing/auditing of the existing implementation before further platform improvements. New product features are paused. Harness coverage and fixes needed for reproduced failures are allowed; missing prerequisites do not count as completed tests. Then resume implementation block by block.

**Dependency decision (1 October):** the [plan's library ownership decision](MCP-EXTENSION-PLAN.md#library-ownership-decision--1-october-2026) selects the official Rust MCP SDK, existing HTTP/vault/schema libraries, official conformance testing, and `oauth2` for later native OAuth simplification. A short source check verified that Goose uses `rmcp` plus its own host adapters; OpenCode and Cline use the official TypeScript SDK with their own application integration. Grain keeps its Agent, native runtime and approval/account rules. No whole agent framework or gateway is adopted. This refines who maintains generic behavior; it does not reopen retired privileges. SDK/conformance work belongs to R2 and native OAuth reuse to R3, after the current foundation acceptance pass. The decision's historical ledger was 28 Pass / 25 Pending; B1a raised it to 30 Pass / 23 Pending; B1b raised it to 33 Pass / 20 Pending; B1c's first four procedures raised it to 37 Pass / 16 Pending; its final check 44 raises it to **38 Pass / 15 Pending**. All seven whole-phase gates remain open.

## The current position in plain language

**Current roadmap, 6 October:** we are implementing **E4 — publishing**, after the
tested foundation and provisional E1–E3 product blocks. Recent work is application
maintainer tooling and focused tests; it is not another Agent testing application.
Five stages still have work because E4 is a large delivery stage and its local
checkpoints do not close the hosted/protected release requirements.

| Step | What the user gets | Current state |
|---|---|---|
| Foundation | Native/MCP execution, exact permission requests, cancellation, lifecycle, account isolation and honest uncertain results | 52 baseline requirements accepted; live expiry 12 explicitly deferred. Controlled and necessary real-provider/model checks are recorded separately. |
| E1 — Contract | Native tools or remote MCP descriptors with a smaller tool-only author interface | Provisionally accepted. Extensions cannot use retired screen/context/prompt privileges. Exclusive obsolete paths may now be removed; old Grain contracts need no backward compatibility. |
| E2 — Connections | Direct remote JSON connections and signed-store MCPs use the same host-owned account/auth/runtime rules | Provisionally accepted through actual-app checkpoints. MCP SDK and native OAuth library reuse are integrated; provider/production certification remains separate. |
| E3 — SDK/CLI | Authors create, check, package and draft native/MCP submissions, with separate DESCRIPTION/media | Provisionally accepted. Public API freeze and official extension publishing are later. |
| **E4 — Publishing, current** | Checked source/build/provenance, guarded signing, a clean tool-only catalogue, current hosting/update/revocation and GitHub publication | Local boundaries audited/verified; GitHub hosting confirmed. Compatibility-only E4i/E4j machinery is removed; clean signed bootstrap is implemented. The first-publication gate is next. Current signing/key custody, remote execution/hosted coherence and applicable operational controls remain; old catalogue migration, historical routes and version reservations are no longer requirements. |
| E5 — Store and management UI | Clear native/MCP/custom connection, setup, login and lifecycle flows with defined listing content | Remaining. UI 2.0 branch isolation and user visual approval apply. |
| E6 — Agent recovery modules | Reviewed automatic-read rules, bounded metadata reuse/freshness, auth continuation and crash/orphan recovery | Remaining as separate implementation blocks. |
| E7 — Measurements and reference integrations | Mixed native/MCP/custom tools, larger catalogues and measured selection/model/resource quality | Remaining. Includes the official reference extensions after infrastructure acceptance. |
| E8 — Release certification | Final public contract, current security/platform/resource evidence and controlled release enablement | Remaining. No pre-release settings/extension upgrade acceptance is required. Passing local test counts never activates production. |

**What you need to test now:** no new manual batch for the current maintainer-only
work. Live token expiry remains deferred and permanent public OAuth hosting remains
pending. Future user batches are reserved for actual account consent, ordinary
current installation/account behavior and UI approval when those blocks are ready.

**Earlier foundation checkpoint notes (historical):** the dated entries below keep
their original counts and limits. Use the current roadmap above and latest audit
links for today's status.

**MCP client configuration and stale approval accepted (2 October):** [Focused audit](MCP-CLIENT-CONFIGURATION-AUDIT.md) accepts **11 and 13** together. Empty-secret public and actual confidential registrations, public ID changes, same-ID secret rotation/removal and unsupported missing-secret refusal use real settings/SDK/OS vault paths. Four old confirmations dispatch nothing; ten approved A/B reads and five actual restarts identify the correct account without new login. The full six-case OAuth regression retains account-switch and disable/re-enable evidence; final clean repeat, detected skipped-client-change fault, eight native-auth, two smoke and two short transport/catalog cases pass, cleanup Pass. A harness callback-owner leak was repaired by consuming each delivered URL once while preserving issuer codes for actual late-login refusal. **46 Pass / 7 Pending**, inventory **60 IDs / 38 self-tests**. No new manual batch or ordinary production auth/transport/dependency change. Compile once per stable unit and select affected regressions; real clocks and distinct failure stages remain intact.

**Previous authenticated MCP shutdown accepted (2 October):** [Focused audit](MCP-AUTH-SHUTDOWN-AUDIT.md) accepts **15**. Provider Disable and Developer Mode off across modern/legacy JSON/SSE return honest uncertainty for eight held approved reads, discard late responses, preserve login and refuse eight old confirmations without dispatch. Twenty-four fresh/restart A reads succeed without a new authorization exchange. Real tests exposed legacy-session cleanup trying to reload an invalidated account ticket; an operation-scoped zeroizing owner now permits one bounded DELETE for the exact original session, with no refresh or tool replay. Fourteen combined MCP cases, clean five-case OAuth repeat, eight native-auth, two smoke and two public live cases pass; skipped Disable fails immediately with cleanup Pass. Official tools pass; known initialization stays Blocked. **44 Pass / 9 Pending**, inventory **59 IDs / 36 self-tests**. No new manual batch. Check 13 has shutdown/stale-confirmation evidence but remains Pending for changed-client variants; check 17 still requires eligible live nested evidence.

**Previous authenticated MCP cancellation accepted (2 October):** [Focused audit](MCP-AUTH-CANCELLATION-AUDIT.md) accepts **31**. Four authenticated held-read closes and four pending closes preserve the actual enabled account and scoped grant; late responses never reach the model, and twelve fresh/restart A reads succeed without new authorization. All thirteen combined MCP cases, a clean four-case OAuth repeat, eight native-auth cases and two smoke cases pass; deliberate production disconnect fails the preservation oracle and cleans up. A reproduced 256-entry OAuth evidence-log limit was repaired with bounded capacity and an exhaustion self-test; production limits/clocks are unchanged. **43 Pass / 10 Pending**, inventory **58 IDs / 35 self-tests**. No new manual batch. B2 now needs only eligible live nested-read check 17; whole B2/B3 and release gates remain open.

**Previous MCP OAuth/late-login unit accepted (2 October):** [Focused audit](MCP-AUTH-FIXTURE-AUDIT.md) accepts check **10**. The real SDK signs in, stores an actual scoped grant, reads the correct A/B account through approved nested tools and restores it after restart. Cancel sign-in, Disable and Developer Mode off each refuse the issued old callback without exchange/grant/resurrection; fresh B reads/restarts succeed. All twelve combined MCP cases and a fresh three-case OAuth repeat pass. Both deliberate faults fail; the abandoned credential is independently deleted. Eight native-auth, two smoke and two public live cases pass; official tools pass and known init remains Blocked. Historical ledger **42 Pass / 11 Pending**, **57 IDs / 34 self-tests**. No new manual batch. The fixture was required for authenticated B2 work; closing this callback check did not skip B2 or certify whole B3. Check 31 was next.

**Previous B2 public live unit accepted (2 October):** [Live-read/HTTP-era audit](MCP-LIVE-READ-AUDIT.md) accepts 8/20. Real public documentation discovery/read, disabled stale/fresh refusal, re-enable/restart and large-read uncertainty followed by actual recovery passed twice in fresh profiles. The live run exposed a generic-ID HTTP 400 negotiation incompatibility; a narrow standards-based repair keeps request correlation, byte/deadline/account guards and no-action-replay behavior. All nine MCP, eight native-auth and two smoke regressions pass; a missing-live-evidence fault fails as intended, all cleanup Pass. Official legacy/modern tools still pass; invalid standalone initialization stays Blocked. **41 Pass / 12 Pending**, inventory **54 IDs / 31 self-tests**. No new manual batch.

### Why the pending count stayed at 14

The ledger counts complete numbered requirements, not individual harness scenarios. Controlled catalog, byte-limit, timeout and conformance subprocedures strengthened 8/17/20/31 without fulfilling their live/account portions. No new numbered requirement was added. Public-service acceptance closed **8 and 20**, reducing 14 to **12**; audited late-login ownership closed **10**, reducing 12 to **11**; authenticated Agent close closed **31**, reducing 11 to **10**; authenticated shutdown now closes **15**, reducing 10 to **9**; audited public/confidential setup and changed-client approvals now close **11/13**, reducing 9 to **7**; audited controlled and genuine-model staged workflows now close **46/47**, reducing 7 to **5**; audited interrupted workflows now close **48**, reducing 5 to **4**; audited independent MCP-provider accounts and fixed-port conflicts now close **14**, reducing 4 to **3**. Live Linear browser acceptance closes 9, reducing 3 to 2; live public nested-read acceptance closes 17, reducing 2 to 1. Partial OAuth coverage cannot mark incomplete objectives Pass. Historical snapshots retain their counts; this table and top snapshot are current.

### Deferred foundation requirement: 1

| Block / check | Already established | What still prevents acceptance |
| --- | --- | --- |
| B3 / 12 — Deferred by user, 3 October | Actual controlled SDK grant/restart, real issuer expiry, twice-rotated refresh and unavailable-refresh reconnect recovery | Actual supported live MCP expiry/refresh and user-assisted recovery evidence; resume later with a new explicitly owned grant. Not a blocker to authorized B5a/B5b, still a release prerequisite. |

Separately open: pinned official initialization, all seven release gates, retained input findings and production discarded-grant reconciliation. Check 9 is accepted from actual browser evidence; 17 from controlled typed/account and repeated public nested reads. B5a SDK compatibility is accepted; B5b native OAuth is next, and live check 12 resumes later. Issue writes, provider documentation and grant metadata cannot accept it. Physical obsolete-code removal is on hold.

**Planning audit (1 October):** the [forward plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md) inventories retained policy, library-replacement seams and physical-removal candidates. All 25 baseline pending acceptance objectives remain useful; zero are deleted because of delegation. Finish B1–B4 on the current implementation before dependency replacements, then repeat affected checks after each refactor. The original fixture loader refuses authentication; the separately guarded account fixture below now supplies that prerequisite. Source counts are review surfaces, not promised LOC savings. No fresh runtime/security certification or new Pass verdict follows from the planning audit itself.

**B1a accepted (1 October):** actual migration/restart preserves edited legacy content and refuses retired access; actual typed native results retain their declared values and invalidate changed-contract approvals. Terminal removal now clears only the removed owner's refusal, and failed uninstall retains the owner before cleanup. Both final cases passed twice, both deliberate corruptions were detected, and installation/registry/store/native-failure retests passed. [Audit and evidence](NATIVE-FOUNDATION-AUDIT.md). Eight B1 account checks remain. No new manual batch is assigned for B1a.

**Historical B1b fixture prerequisite (2 October, before numbered acceptance):** the real isolated app imports/reviews a narrowly scoped account extension, signs in through production OAuth/PKCE and the actual Windows vault, reads A/B through approved Agent tools, refuses an approval from the previous account and restores B after restart. Disconnect now immediately retires only the old account's worker generation; this repairs a real cleanup failure. Two final clean runs passed, wrong-account output failed, and deliberately abandoned login cleanup deleted one actual test credential with none remaining. [Focused audit, identities and exclusions](NATIVE-AUTH-FIXTURE-AUDIT.md). The maintained harness has **33 distinct scenarios**. No new manual batch or numbered-check Pass is assigned: next complete 41/45/53, then the five B1c schedules, using this fixture.

**Historical B1b full numbered unit (2 October, before B1c):** [Binding/owner audit](NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md) accepts checks 41/45/53 using real unbound/id-only credential refusal and reconnect, four reviewed declaration changes, actual CLI developer A/B builds, independent account/source results, reload/restart/restoration and exact parked-installation credential removal. Two final combined runs passed; deliberately forged source ownership and an unchanged declaration failed as expected. The inventory is now **35 distinct scenarios**. The ledger is **33 Pass / 20 Pending** (15 human / 18 reviewed automated). No new manual batch is needed. Three discarded developer grants required independent end-of-run vault deletion; production orphan reconciliation remains open, and these results do not certify whole authentication or a live provider. Next is B1c 42–44/50–51 and its audit, then MCP B2.

**Historical B1c partial acceptance (2 October, before check 44):** [Sign-in/switch/expiry audit](NATIVE-AUTH-SCHEDULE-AUDIT.md) accepts 42/43/50/51. Six production developer-reload/disable/disconnect schedules release old callbacks and preserve fresh login; explicit A/B switch refuses stale approval; cancelled/denied/failed switches retain A across actual reads/restarts; partial consent is refused, actual expiry without refresh requires reconnect, and supported refresh performs one real exchange without login. All seven native-auth cases passed, the four new cases repeated and both deliberate faults failed as required; all cleanup Pass. Inventory: **39 distinct scenarios**. Ledger: **37 Pass / 16 Pending**. No new manual batch. Check 44 and its two-provider/refresh/logout proof were next at this snapshot; live accounts, orphan reconciliation, retained input findings and all seven release gates remain open.

**B1c final numbered acceptance (2 October):** [Refresh/logout and provider-independence audit](NATIVE-AUTH-REFRESH-AUDIT.md) accepts 44. Actual approved read/held refresh/logout/reconnect refuses old read/replay; an awaited late auth future cannot republish a logged-out grant. Two fixed native identities in the same host retain independent grants; an Agent-approved peer read completes while the primary production refresh remains held. Both survive restart, and removing the primary preserves the peer. A fixed auth-only debug probe creates the concurrency and exports no credential or tool execution. All eight account cases, final fresh-profile repeat and ordinary smoke passed; early-release fault failed as required, all cleanup Pass. Inventory: **40 distinct scenarios**. Ledger: **38 Pass / 15 Pending**. No new manual batch. All ten B1 numbered requirements are accepted, with retained input/release findings separate. Next B2, then B3/B4.

**B2 first unit accepted (2 October):** [Catalog/transport audit](MCP-CATALOG-TRANSPORT-AUDIT.md) accepts 23 using the real app and a guarded unauthenticated HTTPS MCP peer. JSON/SSE modern discovery and legacy handshake, nested wire/result values, selected schemas, mixed/excluded/all-unsupported catalogs, cursor refusal, actual restart and stale disable/schema approvals pass. Both final cases pass twice in fresh profiles; both deliberate corruptions fail; all eight native-auth and both ordinary smoke cases pass, cleanup Pass. Inventory: **42 distinct scenarios**, including two MCP cases. Ledger: **39 Pass / 14 Pending**. Checks 8/17 have controlled supporting evidence and remain Pending for their live requirements; 20/31, conformance and whole B2 remain open. No new manual batch. The fixed peer stores no credential; account persistence/authentication are excluded. The audit retains all early TLS/fixture/oracle/bootstrap failures.

Grain extensions now have one job: provide tools that the agent can call. A tool might be implemented directly as a native Grain extension, or supplied by an MCP server. Both go through Grain's checks before execution.

Much of the foundation is implemented. Old extension privileges are refused, workers have clearer start/stop ownership, inputs and results have limits, sign-in has stronger account separation, and the agent can find selected tools and continue a task after approval. Installation and account changes also have stronger save-before-activation behavior.

There is now a maintained Agent test harness in `tests/agent-harness/`. It opens a separate real Grain instance, performs repeatable native-tool and Agent approval/cancellation checks, and saves readable results. Its disposable settings, extension state, credentials namespace and WebView caches are separate from the ordinary app. It can grow into MCP/account and deeper Agent suites. The initial scripted model makes tool decisions reproducible; it does not replace live-model or real-account acceptance.

The complete product is still unfinished. One public MCP documentation-read path now has reviewed live evidence; live expiry/refresh, the real application's memory baseline and the one-to-five-extension workflow ladder remain uncertified. **51 of 53 numbered acceptance checks have passed: 15 user-reported and 36 reviewed real-app automated results. 2 remain Pending.** Baseline B4 is accepted, including the focused audit and repeated controlled/genuine-model interruption tests; this does not close the separate R4 implementation gate. Block 1's tool-only boundary and Block 2A's stop/replacement behavior passed their real-app checks and focused audits. **Block 2B is accepted: focused audit, automated retests and the remaining user dictation/pill/Agent observations passed. Block 2C is accepted: focused source audit, automated retests and the user's ordinary-app oversized-reply/dictation/pill/greeting observations passed. Block 2D first-unit implementation, focused audit and automated retests are complete; the user's ordinary-app check 40 passed; the first 2D unit is accepted. The second 2D CLI packaging/legacy-ownership unit has completed its focused source audit and post-audit repeated tests; checks 28 and 39 are reviewed automated Pass. The third 2D signed-store unit has completed its focused audit and repeated final tests; checks 18 and 36 are reviewed automated Pass. The fourth 2D registry testing unit has completed its focused audit and final combined/repeated tests; check 49 is reviewed automated Pass. Native account binding/owner checks 41/45/53 are accepted against a controlled provider; native refresh/logout and two-provider check 44 is now accepted with its focused audit. Retained intermittent Escape observations remain open. All seven whole-phase acceptance gates remain open.** The separate R1 legacy-migration/upgrade gate and release security review are not implied by these smaller acceptances.

An earlier broad implementation sweep passed **447 automated tests**, with one separate live-store test ignored; focused checks for the current audit patch are recorded below. These tests exercise production logic and local protocol fixtures; they do not certify real accounts, the real OS credential vault or the complete real-model workflow.

## What has been implemented

### 1. Extensions have a much smaller permission boundary

Extensions can declare tools/functions and narrowly scoped supporting access: approved network hosts, their own storage and scoped authentication. They cannot obtain Grain's screen, OCR, selection, cursor, whole transcript, Space, prompt replacement, prompt packs or priorities, shortcuts, resident activation or other retired internal capabilities.

This is enforced at package validation and host dispatch, including attempts to use historical grants or call old APIs directly. Hiding a control in the interface is not the only protection. Mixed packages requesting a tool plus retired capabilities are refused rather than silently running with a different meaning.

**Physical-removal clarification:** the production worker interface and generated authoring API no longer expose capture/context/prompt/session services. However, `src-tauri/src/host_api.rs` still contains legacy implementation branches such as `capture.selection`, `capture.app`, `capture.screenText`, `capture.screenImage`, LLM and semantic services behind the tool-only preflight allowlist. Requests to these methods are refused before those branches run, including with historical or `All` grants. Their presence means extension access is retired, but physical backend removal is unfinished. Retired manifest fields/types also remain for compatibility, refusal and migration; that does not authorize a new extension to use them.

The agent's own context access remains a first-party responsibility. This work does not build new agent screen/OCR features. Ordinary dictation, user prompts, Snippets, Context and Agent settings must continue to work independently.

### 2. Existing installations have a preservation-oriented migration

Incompatible legacy extensions are quarantined and disabled. Obsolete grants and historical slot ownership are cleared, including parked installed copies underneath developer overrides.

Edited extension-owned prompts and custom bindings are archived as inert JSON before removal from active use. Repeated migration avoids duplicate archive entries. Existing package artifacts and user storage are preserved. The migration checkpoint is saved only after the preceding migration work succeeds, and startup still revalidates packages.

**Accepted B1a:** edited prompt/binding preservation, repeated actual startup, an interrupted durable archive checkpoint and failed-uninstall recovery have reviewed real-app evidence. **Still needed:** broader upgrade/rollback and crash-boundary certification; check 2 alone does not close the R1 phase.

### 3. Authoring and management have been reduced to tools

New CLI projects expose an explicit action through `grain.actions`; their generated API no longer advertises the retired host services. Import, developer loading and signed store downloads validate the reduced contract. Store filtering excludes retired integrations, and direct installation has independent checks so bypassing the catalog does not bypass policy.

Retired recommendation controls and extension contributions have been detached from the relevant management/core surfaces. An empty store is valid when its published catalog contains no eligible tool-only packages.

**Still needed:** historical example/documentation cleanup, remaining unreachable code and catalog registrations, and real-app visual confirmation. Necessary parsing/migration tombstones are not working legacy APIs.

### 4. Native workers have clearer ownership and cleanup

A cold call starts the needed native worker; a warm worker can be reused and is later reaped when idle. Unique generations distinguish each supervisor and worker lifetime. Late callbacks, socket closure, idle cleanup or memory-observer results belonging to an old worker cannot legitimately destroy its replacement.

Unload, disable and replacement invalidate queued or pending work. Cleanup rejects outstanding requests, removes listeners/callbacks and retires owned workers and resources. Calls are serialized per current worker with bounded admission. Changed source bytes cannot silently keep executing through an obsolete warm worker.

Native calls have a single 20-second operation budget covering validation, wake and reply, with a shorter readiness ceiling. Blocking filesystem or vault operations can still delay some cleanup; no hard storage deadline is certified.

**Still needed:** real-app cancellation/recovery evidence and measured memory/handle baselines. Enabling/listing an extension alone does not start its runtime; an ordinary recently used native worker can remain warm until idle cleanup.

### 5. Both adapter paths check the exact call

The host checks the enabled instance, current account/configuration identity, selected tool definition and arguments before execution. A confirmation prepared for an older account, package, developer project or schema is refused after that owner changes, even when the visible tool name stays the same.

Native parameter contracts are validated. MCP input validation uses an offline schema evaluator with bounded complexity, nested constraints and a documented supported profile. It does not fetch arbitrary remote schema references. Unsupported individual schemas can be excluded while supported tools remain usable; malformed or incomplete provider discovery still fails.

**Current policy:** native and MCP calls conservatively require confirmation, including reads. Automatic reads need a host-reviewed policy and restricted provider scopes; an extension's own claim that a tool is harmless is insufficient.

**Accepted B1a:** seven declared native typed/optional payloads round-trip through actual workers. A valid declaration edit without reload refuses the old approval before dispatch, then fresh approval succeeds. Check 22 is reviewed automated Pass; authenticated owner and MCP contract acceptance remain separate.

### 6. Results describe what actually happened

Execution distinguishes: nothing was sent, success, a tool-reported error, an unknown outcome after sending, and a received result that cannot be used. If the response disappears after a call was sent, Grain must not pretend the operation never happened or repeat it automatically.

Text/JSON previews retain useful formatting with explicit truncation or omission. Raw responses, catalog discovery, argument structures, decoded results and retained model previews have separate limits. Private error details are redacted rather than copied blindly into the model-facing result.

Actual SDK HTTP and legacy SSE fixtures cover lost responses, bounded discovery, pagination loops and cleanup. These are local protocol tests, not a universal transport/provider compatibility claim.

### 7. Sign-in belongs to Grain and credentials stay out of model context

Grain owns browser authorization, callback listeners, cancellation and credential storage. MCP uses the pinned official Rust SDK with additional bounded host checks. Public-client and confidential-client configuration are supported where the provider's registration permits them. Cancellation and the whole-flow timeout release the owned callback listener; an old browser completion must not reactivate a cancelled account.

Refresh ownership is scoped so one slow provider's network request does not impose a global provider-network lock. Configuration changes, logout and account replacement invalidate late work. The OS vault itself may serialize access.

**Still needed:** live provider/registration certification and complete verified issuer/resource/client/scope mapping. Authentication is not yet seamless across every MCP. Some registrations require a fixed callback port and cannot sign in concurrently on that port.

### 8. Native accounts are separated and validated more carefully

Native credentials are bound to the declared client, endpoints, scopes and allowed API hosts. Old unbound credentials require reconnect. Installed and development copies have separate account sessions; reloading the same developer directory preserves its session, while changing the project owner requires the appropriate separate grant.

Exchange/refresh responses are checked before publication: supported token type, credential grammar, declared scope coverage, expiry and bounded encoded storage. An expired credential without a usable refresh token reports that reconnect is needed. Tokens are not added to tool descriptions, model context or ordinary diagnostics.

A new native login stores its candidate credential first, then commits the selected account pointer. If pointer persistence fails, the prior selected account is retained and only the unreachable candidate is retired. Successful publication cleans up prior credentials afterward.

**Controlled native ownership verified:** the guarded `native-auth` suite uses actual permission review, OAuth/PKCE, OS vault, account publication, authenticated API reads and restart. It found and verified the prior disconnect-worker repair; full declaration/unbound/id-only refusal and installed/developer ownership checks 41/45/53 are now accepted. **Sign-in/switch/expiry verified:** B1c 42/43/50/51 are accepted by the [schedule audit](NATIVE-AUTH-SCHEDULE-AUDIT.md). **Refresh/logout and provider independence verified:** the check-44 audit accepts the controlled native schedules and independent approved peer read. **Still needed:** provider-specific scope equivalence, vault crash/orphan reconciliation and live account tests. Native API credentials and MCP grants remain separate. Preserving an old local token cannot guarantee its issuer has kept it valid.

### 9. The agent loads selected tools instead of every schema

The initial agent context contains a bounded directory and host-owned search/load tools. Search returns small metadata pages; loading requests exact tool IDs. A later load can add another tool from the same unchanged extension without forgetting the first. Unrelated schemas are not automatically offered.

The directory reports incomplete coverage beyond 100 extensions. Each explicit load selects one to eight tool IDs, and all offered definitions share a 32 KiB serialized budget. Fabricated tools and tools loaded too late for the current model response cannot bypass the offered-tool checks.

**Still needed:** a bounded metadata cache, more precise freshness rules and measured search relevance. Current selected discovery is fresh on each search/load, and unrelated catalog changes can conservatively invalidate a selection. This is selective loading, not a finished caching system.

### 10. Approval can continue the original task

The agent retains the original call IDs, messages, selected definitions and remaining budgets while approval is pending. Approving consumes the exact prepared call once, appends its actual result to the original call and resumes the remaining task. The user should not need to dictate the whole request again.

Calls after the first withheld call in a batch receive honest unexecuted results. Duplicate call IDs/approval tokens are refused. Declining or failing a tool blocks another attempt at that same tool within the task, while an independent verification tool may still run. Completed receipts survive a later model failure.

Pending approval expires after two minutes and releases retained state. Close, Stop, replacement and cancellation own the run's cleanup. Transports are not kept open while waiting for the user. Restart ends pending runs; there is no durable restart-resume promise.

**Still needed:** explicit continuation after an authentication interruption and combined real-model/native/MCP read → approved write → verification evidence. The existing continuation tests and adapter tests are separate evidence, not that complete live demonstration.

### 11. Saving now precedes exposing important activation changes

Registry loading refuses malformed, unreadable, oversized or future-version state while preserving the existing file. It does not silently overwrite damaged state with an empty installation list. Saves use bounded streamed JSON and an owned temporary file before replacement.

Enablement, permission grant-and-enable, enabled package replacement and developer publication/restoration keep tentative changes hidden until the save succeeds. Failure restores the edited owner rather than exposing an unsaved enabled extension. Same-version package replacements use content-owned artifacts, and pending store downloads cannot undo a later disable/removal choice.

Explicit disablement has different failure behavior: it still invalidates the live instance even if saving fails. That failed save must be reported; it cannot alone guarantee the disabled state survives restart. JSON registry state and OS-vault state do not form one complete crash-proof transaction.

**Still needed:** remaining refusal/cleanup mutation recovery, artifact/orphan reconciliation and real-app damaged-state recovery acceptance. There is no automatic repair UI, power-loss certification or multiple-process writer guarantee.

## Where execution changed from the plan

The initial broad extension concept was deliberately replaced by the user's tool-only scope correction. That is an approved product change. Separately, some implementation choices differ from the target, and some promised pieces simply remain unfinished. The table keeps those cases distinct.

| Change or difference | Classification | Current behavior and reason | Follow-up |
|---|---|---|---|
| Finish existing testing before further platform improvements | User-directed delivery-order correction, 1 October | Freeze new product features; complete the existing test matrix, essential failure fixes and focused audits first. | Resume improvements block by block only after the foundation testing pass is complete; keep account/desktop prerequisites explicit. |
| Generic standards maintenance delegated without replacing Grain's Agent | Approved library direction; detailed plan now recorded | Keep current `rmcp` integration and host policy; later upgrade/simplify SDK seams and use `oauth2` for standard native operations. | Complete all 25 outstanding objectives before replacements; recertify affected final paths. Conformance replaces duplicate wire fixtures only with mapped coverage, not host acceptance requirements. |
| Broad internal extension features removed | User-directed scope replacement | Screen/OCR/selection, Space, prompts, OS hooks and other retired integration features are refused. Context remains agent-owned. | Keep retirement enforced; do not put these capabilities back on the extension backlog. |
| R1–R4 work interleaved instead of closing each whole phase in sequence | Delivery-order deviation | Worker identity, account ownership, persistence and continuation depend on one another. Deterministic work continued while real-app acceptance was unavailable. | Close the outstanding gates explicitly. Do not use implemented slices to declare R1 finished or expand uncertified providers. |
| Every tool currently needs confirmation | Conservative interim policy | Host-reviewed automatic-read classification is unfinished. Provider annotations cannot self-authorize it. | Implement and certify the reviewed read policy before promising fewer approvals. |
| Fresh selected discovery; no descriptor cache | Incomplete planned optimization | It avoids retaining stale runtimes/descriptors but repeats selected discovery. Whole-catalog identity can invalidate a selection after an unrelated change. | Add bounded generation-aware metadata reuse and finer selected-tool freshness. |
| Approval resumes a task; authentication interruption does not yet have equivalent continuation | Unfinished planned capability | Original task/call identity survives approval. Explicit agent auth interruption handling remains open. | Add bounded auth continuation with the same account/replay guards. |
| Provider and schema support is a documented subset | Compatibility limitation | Curated remote HTTP and tested legacy paths, provider-supported registration modes and bounded offline schemas are the current scope. | Publish a dated tested matrix; complete needed registration/identity work. Do not claim universal MCP support. |
| Targeted JSON/vault publication instead of a new database or journal | Implementation choice with remaining recovery work | Important activation/account changes commit before exposure, using existing low-overhead storage. Cross-store crash recovery is incomplete. | Reconcile orphan credentials/artifacts and finish interrupted-mutation evidence without overstating durability. |
| Retired compatibility markers and historical examples remain | Unfinished planned cleanup | Some markers preserve migration/refusal explanations and user data; some unreachable implementations/examples still need review. | Remove truly dead code/examples after ownership checks, retaining only necessary migration tombstones. |
| Native workers retained browser network access | Focused audit bug repaired and retested | The Rust host refused ungranted `grain.net.fetch`, but Blob Web Workers could call browser networking directly. A supervisor-page CSP narrows their browser connections; the supervisor's Tauri capability is limited to listen/unlisten/emit events. The user observed blocked direct fetch and a successful greeting after the patch. | Keep residual IPC/local-socket risk in the release security review; this is not full process sandbox certification. |
| Short Windows locks interrupted registry publication | Focused reliability repair | The real-app replacement suite reproduced Windows error 5; a real handle denying delete sharing reproduces the same error. Only atomic publication receives bounded retries, retaining the same synced staging bytes. | Keep persistent-error preservation tests and real replacement evidence; do not infer the identity of the original locking process. |
| Delayed shortcut cleanup was not synchronized with registration | Focused lifecycle repair | Registration and eligible cleanup now share a small generation/ownership guard; cleanup scheduling remains nonblocking on the shortcut manager thread. | Keep deterministic race tests and the ten-pair real close/reopen/Escape scenario; deeper Quick Agent/offer paths remain separate acceptance work. |

Physical removal of obsolete backend dispatch implementations and exclusive wiring/helpers remains a later cleanup unit, as requested by the user. The current Block 1 gate is that retired calls remain unreachable, with negative access evidence; first-party context and the metadata needed for refusal/migration must survive eventual cleanup.

Native currently uses the existing embedded tool execution path. Arbitrary native executables, unrestricted local MCP commands and OS sandbox certification are outside the initial release; they are not completed capabilities. Agent context expansion is also outside this implementation effort.

## Remaining phase gates

| Phase | What the unfinished work means in ordinary language |
|---|---|
| R0 — Contract and baseline | Finish the versioned contract/inventory and measure resource/latency baselines. |
| R1 — Retirement and migration | Finish interrupted cleanup/refusal recovery and confirm that upgrades preserve user data without restoring old access. |
| R2 — Execution | Finish reviewed read policy and certify cancellation, outcomes and cleanup through both real adapter paths. |
| R3 — Authentication | Finish provider identity/grant verification, vault recovery and live native/MCP login/refresh/disconnect certification. |
| R4 — Agent workflow | Add metadata caching/freshness and auth continuation; measure retrieval and prove the complete real-model task flow. |
| R5 — Multiple extensions | Certify one native, one MCP, then two, three and five mixed extensions, including failure/partial-completion cases. |
| R6 — Release | Finish cleanup/examples/bindings, CI and compatibility reporting, upgrade acceptance and the user's real-app review. |

Some of this can continue with deterministic tests without the user. Real account compatibility, application appearance, actual model selection, upgrade behavior and measured real-app resource usage need their corresponding live acceptance evidence.

## Sequential acceptance track

**Forward order:** the [new audit/reuse plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#7-execution-blocks-and-acceptance-order) takes precedence over older handoffs. Finish native migration/inputs/account-owner prerequisites first (B1), current MCP transport/tools (B2), current MCP auth (B3), then current Agent workflows (B4). Library replacement and new product capabilities follow accepted baseline testing. The smaller accepted native units below retain their historical evidence.

**Agent harness, 1 October:** the user authorized real-application automation and clarified that this is an Agent harness, initially exercising extension/tool lifecycle. The [architecture and delivery record](AGENT-TEST-HARNESS.md) and [runner instructions](../../tests/agent-harness/README.md) cover the implemented isolated real Tauri/WebView2 adapter, versioned native fixture, scripted HTTP model, production developer-socket reload, build identity, cleanup and evidence reports. `AGENTS.md` now permits this acceptance automation. Runtime scenarios and production-logic suites remain distinct evidence classes. Check 37 is now Pass, combining recorded automated reload coverage and the user's remaining ordinary-app observations; the original 12 manual Pass results are preserved.

**Findings investigated in the focused audit:** replacement failed again in clean profile `run-L8HNtp`, now with the full Windows `Access is denied (os error 5)` cause. A real Windows handle denying delete sharing reproduced that failure in a regression test. Atomic publication now tolerates short locks through five bounded backoffs totaling 250 ms of added sleep, while persistent failures still preserve saved state and report failure. The source audit also found unsynchronized delayed shortcut cleanup; a small ownership guard now prevents old cleanup removing a new registration and protects queued panel creation. [The focused audit](NATIVE-LIFECYCLE-AUDIT.md) records evidence, references and limits. Earlier failed reports remain preserved. The process responsible for the original file lock and the cause of every earlier Escape observation are not established; passing reruns alone do not establish them. The runner still stops failures without scenario/tool-call retries.

**Harness corrections:** the real first-run profile does not initialize shortcuts through the ordinary settings page, and clearing its binding map makes the normal initializer fall back to default accelerators. Test startup now owns the existing production HandyKeys manager with no ordinary accelerators registered, retains explicit blank binding entries and marks initialization complete; the normal command is checked for idempotence. Only Agent's transient Escape is exercised. Follow up is opened before typed approval; its production React Enter handler is activated directly because CDP key delivery during native resize was intermittent (`run-jrIG7K`). These changes are confined to test setup/driving and do not fix or certify general production shortcut behavior. A separate Windows DLL lock prevented compiling another configuration while the runtime was open; build/backend compilation must precede real-app runs in the shared debug target.

**Historical block map (1 October; superseded by the current B1–B7 plan):** Block 2D consent, installation and persistence. The table retains its original handoff states. Current focus is B3 check 12; B1/B2/B4 baseline requirements are accepted. The [reuse execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md) governs the remaining expiry test, audits and later implementation order. A missing prerequisite is not a Pass.

| Block | Behavior to finish | Real-app acceptance to collect | Focused audit after user feedback | State |
|---|---|---|---|---|
| 1. Tool-only boundary and one native tool | Ordinary Grain features survive; retired extension capabilities are absent/unreachable; a newly authored tool loads and actually runs through Agent. | Checks 1, 3, 4, 7, 16; check 2 if a legacy installation exists. Confirm the real greeting tool result, not a model-written greeting. | Verify declaration, host preflight, worker network boundary, Agent offering and core-feature independence. Catalog dead handlers for later physical removal; fix and retest audit findings. | **Accepted 30 September: five checks and audit retest Pass** |
| 2. Native execution, consent and persistence | The same native tool survives ordinary reload/restart, stops safely and cannot execute an old approval against a new owner. | Checks 5, 6, 18–19, 21, 24–30, 32–40, 49, 52–53 as applicable; use disposable packages for installation cases. | Audit worker/resource ownership, timeout/result certainty, package publication, approval identity, registry recovery and teardown. Fix/retest. | **2A/2B/2C and first four 2D units accepted; native account-owner tests open; retain Escape regression observations** |
| 3. One MCP transport and tool contract | A controlled MCP provider is discovered, loaded, called and disconnected with honest results; native/account foundation precedes this. | B2: checks 8, 17, 20, 23 and 31 with a safe read and suitable shapes; missing prerequisites remain Blocked. | Audit actual SDK transport, pagination/schema limits, dispatch/outcome classification and cleanup. Fix/retest; no SDK upgrade during baseline. | Queued |
| 4. Authentication for one certified MCP provider | One real MCP account connects, survives restart/expiry and disconnects; stale sessions cannot act for a replacement. Native checks 41–45/50–51 and owner 53 belong to the preceding B1 foundation. | B3: MCP checks 9–15. Record registration mode, scopes and provider identity; do not request all six catalog logins. | Audit callback, issuer/resource/client/scope mapping, vault publication/refresh, cancellation and cleanup; fix/retest. A provider-specific compatibility matrix is required. | Queued |
| 5. Agent tool selection and full task | Agent finds selected schemas, confirms a disposable write once and resumes/verifies without re-dictation. Typed native inputs (22) belong to B1. | B4: checks 46–48 and relevant stale-call controls; use scripted and actual model calls with test objects. | Audit current selection/call identity, approval continuation, denial and receipt preservation; fix/retest. Future caching/auth continuation remains a separate implementation gate. | Baseline B4 (46–48) accepted; future R4 gate open |
| 6. Two complete extensions together | One native and one MCP tool both work in the same task, including name collisions, disablement and partial failure. | Run the R5 one-native, one-MCP and two-extension ladder with recorded real calls, account routing and cleanup. | Audit cross-extension data boundaries, isolation, lifetime and result attribution; fix/retest. | Queued |
| 7. Scale and release | Three- and five-extension workflows, compatibility, measured resources, upgrade and release behavior are documented and accepted. | Complete the remaining R5/R6 gates, including real Tauri visual review and any still-open numbered checks. | Final focused integration/security review, compatibility/CI matrix and resource measurements. | Queued |

The order deliberately puts **one working native tool before broad authentication work**. MCP transport is checked with a controlled safe provider before auth is called complete; if that provider itself requires login, its credential setup is a prerequisite, while the auth module remains unaccepted until Block 4. One certified provider is an honest release milestone; it is not a claim that all MCP servers work. The R5 two-extension demonstration is the first cross-extension acceptance milestone, with three/five tested afterward.

Block 2 is intentionally divided before its whole acceptance: **2A stop/replacement** (checks 6, 25, 27), then a focused audit; **2B repeated/close cleanup** (5, 29, 30, 35, 37), then an audit; **2C deadlines, results and input** (21, 24, 26, 32–34), then an audit; and **2D consent, installation and persistence** (18–19, 28, 36, 38–40, 49, 52–53), then an audit. Missing prerequisites are recorded rather than forcing test mutations into a real profile. Other checks stay in their numbered ledger and phase gates. Each audit unit can span several small user batches, but one unit must be accepted before the next starts.

### Current small-batch handoff

**Historical handoff retained below:** no new user test batch is assigned in this planning task. The next execution slice is [B1a migration/typed inputs and B1b's account fixture prerequisite](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#9-first-implementation-handoff-and-completion-definition). Older Step 2C instructions do not supersede it.

**Active handoff: testing the existing implementation only.** All B1 requirements and B2 checks 8/20/23 are accepted. Next obtain suitable nested/account-preserving MCP prerequisites for 17/31, including the scoped MCP-auth fixture, then complete B3 9–15 and B4 46–48 with tests → focused audit → final repeats per unit. Remaining **12** baseline requirements must finish before SDK/native-OAuth replacement, physical cleanup or new features. Resolve the official initialization fixture separately. No new human batch is assigned by the public-read unit. Retained input findings, production vault reconciliation and whole-release gates remain open; historical handoffs below do not override this order.

**Step 1A — observed, partial check 16.** The user's real-app screenshot shows **Load unpacked** and **Hosted MCP validation** with Linear, Notion, Atlassian, GitHub, Slack and Google Calendar rows. It does not show Recommendation Lab/Core 6/Stress 24 in the visible area. A red **“MCP protocol negotiation failed. Check the account and provider availability.”** banner is visible above Linear; its cause is unknown and is recorded for the later focused MCP block. No provider login, Test call, native tool execution or full-store review was performed. Check 16 therefore remains Pending rather than Pass.

**Step 1B — check 3 Pass.** The user's 30 September real-app screenshot shows a developer row for `com.example.tool-smoke` pointing to the prepared disposable folder; the user reports enabling it and then confirms the tool works in Agent. The generated fixture's inspected manifest declares the `Say hello` action and zero permissions/activation. No unexpected permission request was reported. The Developer list shows the extension ID rather than a separate tool button. The MCP negotiation banner is a separate unresolved observation; no provider action was requested.

**Step 1C — check 4 Pass.** The user reports that Agent's Tool Smoke tool works and ordinary dictation works in the real app, then explicitly confirms the greeting also worked both immediately and after about three minutes without tool activity. This completes the user-facing cold/warm/idle greeting procedure. Worker-reap log and measured resource evidence remain independent Block 2/phase-gate work. Do not connect, test, disable or reconfigure hosted MCP providers to make this native test pass.

**Step 1D — checks 1, 16 and 7 Pass.** The user reports that the ordinary settings/core-feature and restart check worked, old extension controls and old extensions are no longer visible, and the invalid retired-capability package was rejected. The exact rejection text and any change to the pre-existing MCP banner were not provided; neither is needed to claim provider compatibility. The original native greeting had already worked cold, warm and after idle. No hosted-provider action was required.

**Step 1E — focused audit completed for Block 1.** Source review confirmed package validation rejects retired declarations and host preflight refuses retired RPC methods even with historical grants. It also found that extension Web Workers retained standard browser `fetch`/XHR/WebSocket access because the hidden host had no Content Security Policy. The hidden `extension-host.html` now restricts browser connections to Tauri supervisor IPC and Grain's local authenticated tool socket; extension HTTP requests are intended to use the exact-host `grain.net.fetch` broker. The hidden host's Tauri capability was narrowed from `core:default` to only event listen/unlisten/emit. [MDN's worker guide](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Using_web_workers) describes worker networking and Blob-worker CSP inheritance; [MDN `connect-src`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src) documents the network restriction, and [Tauri's CSP guide](https://tauri.app/security/csp/) documents its IPC exception. The production build, focused worker/supervisor tests and extension contract/host tests pass. The user then observed direct browser fetch blocked while the native tool and dictation still worked. IPC/local-socket exposure remains a residual boundary for release security review, not a claim of complete process sandboxing.

**Step 1F — Pass, 30 September.** The user supplied actual Agent tool results: Tool Smoke returned `Hello from this tool.` and Network Boundary Probe returned `PASS: unbrokered browser fetch was blocked.` The user also reports normal dictation is working. The probe directly requested `http://localhost:1420/` without a network permission; that live dev-server endpoint independently answered HTTP 200 during the handoff. The disposable probe's CLI doctor reported zero findings. These post-patch observations close Block 1's focused audit. Unloading the probe was requested but not reported; it is a cleanup step at the start of Block 2. No new numbered check was added or marked Pass, so the ledger remains 5 Pass / 48 Pending. This does not test check 2's legacy-profile migration, all possible browser/IPC escape routes, or a live MCP provider.

**Step 2A — Block 2A user batch, checks 6, 25 and 27.** Unload Network Boundary Probe if it is still listed. Load and enable the CLI-validated disposable `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a` through Developer → Load unpacked. It declares no permissions and has a fast `Say lifecycle hello` tool and a fifteen-second `Say slow lifecycle hello` tool; both request confirmation. (1) For check 6, ask Agent to use the slow tool, approve it, then unload Lifecycle Smoke *before* it finishes. This first call starts cold. The old call must not show `Slow lifecycle hello completed.` after unloading; reload the same folder and confirm a new fast call returns `Lifecycle hello completed.` (2) For check 25, approve another slow call, disable it while running, immediately re-enable it, and make a fresh fast call. Expect no late success from the old call and one new `Lifecycle hello completed.` result. Repeat after a reload/cold start if the UI permits; report whether that subcase was reached. (3) For check 27, start a fast call but leave its confirmation unanswered; load the same folder again without editing it. The old confirmation must disappear or be refused if approved. A fresh confirmed call must return one `Lifecycle hello completed.` result. If the UI prevents navigating to Developer while a call or confirmation is open, report that exact obstruction as Blocked for that part; do not infer a result. If a slow call finishes before the disable/unload click, repeat that attempt. Report the three numbered outcomes together, with any exact error. No provider login or source editing is needed. After these three, audit and repair Block 2A before starting 2B.

**Step 2A — Pass and audited, 30 September.** The user reported all three sequences worked. The supplied results include an uncertain outcome for the interrupted slow call (with no automatic retry), fresh `Lifecycle hello completed.` tool results after replacement, and a refusal of the old confirmation after reload. The exact cold-start repeat for check 25 was not separately narrated, so this is recorded as the user's aggregate Pass rather than an independently timed trace. Source review followed the actual unload/disable commands, token-scoped worker removal, pending-reply drainage, registry generation fingerprint and Agent approval resume. Focused checks passed: **258 core unit + 4 integration, 48 native-host, 4 action-executor and 15 Agent tests**. A visible encoding defect in the stale-approval refusal (`â€”`) was corrected to readable punctuation; that copy-only correction does not change approval or dispatch behavior. Native call interruption still cannot prove an already-started handler had no effects. Block 2A is accepted for its tested stop/replacement behavior; release-wide lifecycle/resource certification remains open.

**Step 2B — completed user batch, check 29 and the pending-approval part of check 30.** Keep the disposable Lifecycle Smoke extension enabled; its slow tool waits fifteen seconds. (1) Ask Agent to use `Say slow lifecycle hello`, approve, then close Agent with its **×** while it is running. Reopen Agent and make a fresh `Say lifecycle hello` call; expect no late slow success, no replay and one fresh fast result. (2) Repeat the slow-call sequence, but close Agent with **Escape** while normal dictation is not recording. Reopen and verify the fast call again. (3) Repeat once more, answering the slow tool's confirmation by typing a clear `yes` instead of clicking Approve; then close Agent while it runs and verify a fresh fast call after reopening. These three modes together complete check 29 if they pass. (4) Leave a fast greeting confirmation pending, close Agent and reopen it; the old approval must be gone, and a new fast request must work. This starts check 30; its separate slow-model-response case remains for a later batch. If a call finishes before you close Agent, repeat that attempt. Report the four observations together with any exact error. No source edit or provider account is needed.

**Terminal retest before Step 2B — user-reported Pass, 1 October.** The user reproduced a `0xc000013a` exit by summoning Agent while the PowerShell terminal running Grain was focused. Agent's invisible selected-text capture synthesized Ctrl+C into that terminal, which Windows delivers as a console interrupt. The Windows capture path now skips synthetic copy when a recognized standalone terminal host is in front, leaving Agent's dictation, screen and other context paths available. The user reports trying both the PowerShell shortcut and a normal text-selection surface after the fix, with expected behavior. Embedded terminals inside editors are not yet classified and need separate validation. Windows `cargo check --lib`, the focused terminal guard test and all 16 scoped Agent tests passed; the Tauri tests used the repository's Common Controls manifest runner. This retest does not itself change a numbered manual check.

**Step 2B — observed, check 29 Pass; check 30 partial.** The user reports all four prior close/reopen modes behaved as expected: closing an executing slow Lifecycle Smoke call with ×, Escape, and after conversational `yes` stopped the old result while a fresh fast call worked; closing with a confirmation still pending removed the old approval and a new request worked. The report is aggregate rather than individual timestamps or logs. Check 29 is Pass. Check 30 remains Pending until its separate slow-model-response/replacement case is observed. A focused source review traced X through window destruction to `clear_pending_action`, Escape through `global_close`, and both through `AgentRunControl` cancellation and token discard; all 16 scoped Agent tests pass. This is a partial Block 2B audit, not acceptance of the whole block.

**Step 2B — completed user batch, checks 30, 5 and 35.** (1) For the remaining part of check 30, ask Agent a harmless question that makes it show its loading state; close it immediately while the model is still replying. Reopen at once and ask a *different* harmless question. The old answer or approval must not appear in the new session, and the new question must work. If the first answer finishes before closing, repeat that attempt. A brief “already running” refusal while cleanup finishes is acceptable; retry after it clears. (2) For check 5, use the disposable Tool Smoke folder: unload, reload, approve if prompted and call its greeting **ten times**, recording whether every cycle produces exactly one fresh `Hello from this tool.` and no old error breaks the replacement. (3) For check 35, call a fast native greeting, leave tools idle for more than **2 minutes 30 seconds**, then call it again. After another fast call, wait about **1 minute 50 seconds** and start the fifteen-second Lifecycle Smoke call, approving promptly. It should finish; an idle cleanup must not kill an executing call. Disable/re-enable or reload once afterward and verify a fresh fast call still works. Report these three numbered results together with exact errors, if any. Do not count check 35 Pass merely from the earlier three-minute Tool Smoke repeat: it also requires the near-boundary/in-flight and replacement cases.

**Step 2B — observed, checks 30, 5 and 35 Pass.** The user explicitly confirms all three full procedures worked, including ten unload/reload-and-greeting cycles and both timed idle cases. No visible error was reported. The report is aggregate, without per-cycle timestamps or worker/RAM measurements. Source review found idle removal rechecks the current token, activity time and pending calls under the queue-admission lock; cancellation/reload removes only the owned generation and drains its pending replies. All 48 `extension_host::tests::` tests pass, including idle selection, replacement, duplicate-socket and ten-generation cases. This is a focused partial Block 2B audit; its final shared-socket/reload check 37 remains.

**Step 2B — completed human batch, check 37 Pass.** The real-app harness covers repeated authenticated developer reload, in-flight replacement and disabled-state preservation. The user reports the remaining ordinary dictation/recording-pill observations worked and explicitly confirms the Agent greeting worked. These were the three post-audit observations, not three additional numbered checks. Combined with the recorded reload coverage, check 37 is Pass and Block 2B is accepted. The report is aggregate; exact ordinary-profile reload timing and the running build identity were not separately supplied.

**Completed-batch reproduction reference:** for the one ordinary-profile reload, keep Lifecycle Smoke enabled and run `C:\t\debug\grain-ext.exe dev` from `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-b71f42cba18b48a4afd1561c7a0b403a`, after putting `C:\Projects\Grain\grain\node_modules\.bin` on that PowerShell's `PATH`. Append one harmless comment to its `src/main.ts`; the watcher should report rebuild/reload. Then perform the three observations, stop the watcher and restore `src/main.ts` from `C:\Users\watrm\AppData\Local\Temp\grain-lifecycle-smoke-main-baseline-20261001.ts`. The [audit document](NATIVE-LIFECYCLE-AUDIT.md) includes copyable setup commands. This live-profile step checks the normal microphone/pill listener; the harness uses a separate port and skips the pill process. It is preserved for reproduction, not a request to repeat the accepted batch.

**Step 2D — first consent/installation unit accepted; wider store/account/recovery requirements remain.** Check 34 is Pass: the user supplied the expected oversized-reply warning, reports ordinary dictation/recording recovery, and supplied the actual fresh `Harness hello (one)` result. This closes 2C with the existing focused audit and automated retests. The maintained real-app harness covers cancel/Allow/restart, stale consent and call approvals, same-version imported artifacts, installed/developer owner restoration and isolated registry startup refusal. Signed-store schedules and native OAuth/account portions retain their separate prerequisites; this slice cannot certify them. All 24 real-app scenarios passed in `run-3fp5CX`; the five installation cases repeated in `run-DJ79qE`, both cleanup Pass. The intentional skipped-restart oracle failed as required with cleanup Pass. Reviewed checks 19, 38 and 52 are automated Pass. The user reports all three ordinary-app observations Pass, completing check 40 and accepting this first unit with its audit/retest evidence. The following packaging unit closes CLI/legacy ownership gaps; do not repeat checks 34/40 or connect hosted accounts now.

For each step, ask for **Pass / Fail / Blocked**, the visible result and any exact error. Do not require the user to run the whole 53-check list at once. Update the existing numbered ledger and the active block only after its full numbered procedure has evidence; if a failure appears, stop this block's acceptance, reproduce/audit/fix it and retest before moving to the next block.

**Step 2D — packaging unit accepted.** The [focused CLI packaging audit](NATIVE-PACKAGING-AUDIT.md) records real CLI build/doctor/pack, same-version replacement, obsolete flat/version-only files, exact installed-record preservation and original installed approval refusal through the complete override/restoration cycle. All 25 scenarios passed before that extra audit assertion; the final six installation scenarios passed twice afterward. The deliberate stale-package fault failed the replacement greeting and cleaned up. Checks 28 and 39 are reviewed automated Pass. The next signed-store unit below closes checks 18/36; wider 49 recovery and 53 account portions remain separate. No repeat of check 40 or microphone batch is requested.

**Step 2D — signed-store unit accepted.** The [focused store audit](NATIVE-STORE-AUDIT.md) records actual Store loading/close/reopen controls, signed fixture install and real consent/Agent results, offline Update refusal, and disable/removal during held downloads. The final mutation cases also assert state across actual restart, including developer override and its parked installed record. Checks 18/36 are reviewed automated Pass. The following registry testing unit closes 49; account-owner prerequisites in 53 remain separate and whole 2D stays open. Nothing new requires the user to test now.

**Step 2D — registry testing unit accepted.** The [focused registry audit](NATIVE-REGISTRY-AUDIT.md) records state/pointer preservation, actual quarantine/refusal/recovery, four damaged startup variants and real Windows failed-save behavior across restart. The final 30-case combined run and clean-profile registry repeat passed, with negative-oracle and cleanup evidence. Check 49 is reviewed automated Pass. The existing acceptance testing pass continues; native account-owner test 53 and retained intermittent Escape observations remain open. Product improvements remain paused.

## User testing: seven groups, all 53 checks

The numbered checks below are the same checks in the execution plan; **1–8 have not been renumbered**. This is a short result ledger, not a replacement for the plan's [full procedures and disposable Tool Smoke setup](MCP-EXTENSION-PLAN.md#combined-real-app-test-checklist-retirement-lifecycle-and-authentication-phases).

**Status vocabulary:** Pending = the complete numbered procedure is not yet verified; Pass = the full real-app procedure has reviewed evidence, labeled user-reported or automated; Fail = attempted and wrong; Blocked = a missing prerequisite prevented the check. There are **46 Pass / 7 Pending**, comprising 15 user-reported and 31 automated Pass results. Unit tests, partial scenario links and a run report alone do not certify a full row; automated acceptance is recorded explicitly after scope review and audit. The runner never rewrites this ledger. Introductory current snapshots agree with the numbered rows; historical snapshots retain their evidence counts.

Run the real app from the repository root:

```powershell
cd C:\Projects\Grain\grain
bun run dev:asr
```

Use existing ASR configuration and a tool-capable Agent model. Tool Smoke requires no external account; its build/load instructions are in the plan. A model merely writing a greeting does not prove the tool ran. MCP checks need an eligible configured provider; native account checks need a separate authenticated disposable native fixture and supported registration, which may not yet be exposed by the reduced UI.

Use disposable objects/packages for test writes. Do not damage the normal registry, change its permissions, edit vault contents or expose credentials to force failures. Deterministic fixtures cover those schedules. Where a catalog, provider, registration, isolated profile or observable timing is unavailable, record the specific prerequisite as Blocked.

### A. Everyday behavior and removal of old privileges — 6 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 1 | Dictation, Snippets, Context and Agent still work; core settings and user prompts survive restart. | Pass |
| 2 | Old privileged extensions stay disabled across restart; archived edited prompts/bindings survive without duplication. | Pass (reviewed real-app automation) |
| 3 | Load Tool Smoke with tool-only consent; no screen, OCR, prompt or transcript permissions requested. | Pass |
| 7 | A disposable package requesting retired permissions/startup activation is explicitly refused. | Pass |
| 16 | Store/developer controls offer tool-only capabilities; retired recommendation/lab/integration controls are absent. | Pass |
| 40 | Cancel consent leaves the tool off; one fresh Allow enables it and a real greeting works, with usable controls. | Pass |

### B. Native calls, stopping and recovery — 12 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 4 | The actual greeting works cold, warm and again after idle worker cleanup. | Pass |
| 5 | Ten unload/reload cycles work without duplicate replies or an old error killing the replacement. | Pass |
| 6 | Unload during a slow greeting stops/invalidates it; reload restores normal calls. | Pass |
| 24 | A 25-second greeting times out, does not replay or disable the tool, and a normal call recovers afterward. | Pass |
| 25 | Disable/reload during execution invalidates the old call without destroying the new worker. | Pass |
| 26 | Rebuild a warm tool without Reload; obsolete source is refused and a fresh request uses the changed source. | Pass |
| 27 | Even an unchanged reload invalidates an earlier pending confirmation. | Pass |
| 29 | Close Agent or use Escape during a native call; no late result/replay and a fresh session works. | Pass |
| 30 | Close with approval/model response pending; no old reply or approval appears in the replacement session. | Pass |
| 32 | Measure the native absolute timeout from invocation/approval; expect about 20 seconds under normal local I/O. | Pass |
| 35 | Idle cleanup and requests near its boundary preserve an executing/current replacement worker. | Pass |
| 37 | Repeated reloads preserve socket recovery, disabled state, recording and ordinary pill actions. | Pass |

### C. Installations, permission reviews and saving — 8 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 18 | Repeated store close/reopen does not resurrect old requests; offline cached browsing cannot install. | Pass (reviewed real-app automation) |
| 28 | Installed → development override → installed restoration preserves the installed owner and rejects old approvals. | Pass |
| 36 | A pending store download cannot reinstall/re-enable something subsequently removed or disabled. | Pass (reviewed real-app automation) |
| 38 | A permission review becomes invalid when its package/declaration changes; a fresh sheet matches the new tool. | Pass |
| 39 | Reimporting changed bytes with the same id/version uses the new package after restart and override restoration. | Pass |
| 49 | Ordinary restart preserves registry state; damaged-state refusal is tested only with an already isolated profile. | Pass (reviewed real-app automation; pointer serialization only) |
| 52 | Consent cancellation, enablement and disablement persist after ordinary successful saves/restarts. | Pass |
| 53 | Developer folders A/B and the restored installation retain the correct owner, output and account after publication. | Pass (reviewed real-app automation; controlled native provider) |

### D. Inputs, outputs and truthful failures — 7 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 17 | A supported nested MCP input produces a real harmless read result. | Pass — reviewed real-app automation, 3 October; controlled typed/account reads plus live Hugging Face nested read, approval, result and restart; [audit](MCP-NESTED-LIVE-READ-AUDIT.md) |
| 20 | Ordinary and safely large MCP reads complete or refuse within bounds; a fresh ordinary read recovers. | Pass (reviewed real-app automation; controlled bounds plus public live recovery) |
| 21 | Lost native replies, tool errors and malformed envelopes are classified honestly; private error markers stay hidden. | Pass |
| 22 | Native declared parameters preserve types/optional values; a changed contract invalidates an old approval. | Pass (reviewed real-app automation) |
| 23 | Supported tools remain usable in a mixed MCP schema catalog; excluded schemas are not offered or executed. | Pass (reviewed real-app automation; controlled MCP peer) |
| 33 | An oversized decoded native result is unusable rather than successful; a normal result still works afterward. | Pass |
| 34 | An oversized raw native message rejects/cleans up correctly, with no blind replay or shared-listener regression. | Pass |

### E. MCP connection and accounts — 9 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 8 | Test/discover a configured MCP, perform a real read, then disable it and verify refusal. | Pass (reviewed real-app automation; public live provider) |
| 9 | Denied or cancelled login finishes cleanly; the callback listener becomes reusable. | Pass — reviewed real-app automation with human browser participation, 3 October; actual Linear cancellation, zero grant, reusable callback and successful fresh consent/discovery; [audit](LINEAR-LIVE-AUTH-AUDIT.md) |
| 10 | Completing an old browser login after cancellation cannot resurrect the account; fresh login works. | Pass (reviewed real-app automation; actual SDK callback/vault, controlled issuer) |
| 11 | Supported public/confidential registration works; changing client configuration invalidates the old grant. | Pass — reviewed real-app automation, 2 October; actual public/confidential client setup, ID/secret rotation/removal, unsupported-secret refusal and fresh/restart reads; [audit](MCP-CLIENT-CONFIGURATION-AUDIT.md) |
| 12 | Real login survives restart and refresh after documented expiry; a stored-credentials badge alone is insufficient. | Deferred — user decision 3 October; live expiry/refresh remains unverified |
| 13 | An MCP approval from an old account/enablement/configuration cannot run under the replacement. | Pass — reviewed real-app automation, 2 October; actual account switch, disable/re-enable and four client/secret changes refuse old approvals with zero dispatch and fresh account recovery; [audit](MCP-CLIENT-CONFIGURATION-AUDIT.md) |
| 14 | Supported providers operate independently; fixed-port login conflicts are reported, not treated as success. | Pass — reviewed automated; [focused audit and repeated real-app evidence](MCP-PROVIDER-INDEPENDENCE-AUDIT.md). |
| 15 | Disable/provider-mode shutdown during MCP work cleans up and allows a fresh call after re-enable. | Pass — reviewed real-app automation, 2 October; authenticated provider Disable/Developer Mode off, JSON/SSE modern/legacy, exact uncertainty/no replay, retained grant and fresh/restart recovery; [audit](MCP-AUTH-SHUTDOWN-AUDIT.md) |
| 31 | Close Agent during a slow MCP read or pending approval; account remains intact and a fresh read works. | Pass — reviewed real-app automation, 2 October; authenticated JSON/SSE modern/legacy, actual grant and fresh/restart reads |

### F. Native sign-in and account isolation — 7 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 41 | Unbound or declaration-changed native credentials require reconnect; a supported fresh grant works across restart. | Pass (reviewed real-app automation; controlled native provider) |
| 42 | Reload/disable/disconnect during native sign-in prevents old callbacks selecting an account. | Pass (reviewed real-app automation; controlled native provider) |
| 43 | Switching native account A → B refuses A's pending approval; a new real read identifies B. | Pass (reviewed real-app automation; controlled native provider) |
| 44 | Logout defeats an old refresh; unrelated native providers are not blocked by its network request. | Pass (reviewed real-app automation) |
| 45 | Installed/developer copies have separate accounts; same-directory reload preserves the appropriate session. | Pass (reviewed real-app automation; controlled native provider) |
| 50 | Cancelled/failed account switching preserves the prior selected account; a successful switch selects the new one. | Pass (reviewed real-app automation; controlled native provider) |
| 51 | Partial consent is refused; expiry without refresh requires reconnect, while supported refresh recovers. | Pass (reviewed real-app automation; controlled native provider) |

### G. Tool selection and complete agent tasks — 4 checks

| Check | What to check / expected result | Status |
|---|---|---|
| 19 | A native confirmation becomes invalid after disable/re-enable or source replacement; a fresh call works. | Pass |
| 46 | Search/load offers only selected schemas; adding another tool preserves earlier selections and obeys limits. | Pass — reviewed real-app automation; controlled and configured genuine-model repeats, focused audit |
| 47 | Read → approve a disposable write → verify continues without re-dictation; duplicate/stale approvals do not execute. | Pass — reviewed real-app automation; controlled and configured genuine-model repeats, focused audit |
| 48 | Denial/expiry/Stop prevents dispatch; a later model failure preserves completed receipts and unfinished-step information. | Pass — reviewed automated; focused audit and clean repeats |

### Testing order

Follow the [all-25 baseline mapping and forward blocks](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#6-disposition-of-every-pending-acceptance-check). All B1 requirements, B2 checks 8/20/23/31, B3 checks 10/11/13/14/15 and baseline B4 checks 46–48 are accepted. Pending counts: B1 **0**, B2 **1**, B3 **2**, B4 **0**: **3** (9/12/17). All 25 original objectives remain; twenty-two were accepted, none removed. Next prepare the user's chosen Linear account in an isolated profile for actual browser denial/cancel, documented MCP expiry/refresh and supported nested harmless reads. Share eligible setup while retaining separate failure assertions and genuine recovery evidence. Human procedures are reserved for account consent or ordinary-app review; no manual retest is required for check 14. Finish baseline testing/audits before library replacements, then repeat affected checks.

## Evidence and maintenance

| Evidence | Recorded result | What it does not establish |
|---|---|---|
| Latest core suite | B1a: 262 unit + 4 integration tests passed, including 49 registry tests. | Live app appearance, real accounts or a measured RAM baseline. |
| Earlier affected backend suites | 185 tests passed across auth, worker host, Agent, executor, capability, MCP, imported updates, developer projects, host RPC, event socket and store. Historical combined core/backend count: 447; this is not a fresh combined run. | A single complete live-model/native/MCP workflow or real OS-vault acceptance. |
| Latest static checks | Core Clippy with warnings denied, backend check/Clippy and scoped formatting/diff checks passed; unrelated backend warnings remain. | Whole-release acceptance or unrelated warning cleanup. |
| Block 1 audit patch | Frontend production build and 9 focused supervisor/runtime tests pass; 105 SDK/checker/CLI, 16 host API and 4 developer-loader tests pass; backend library check passes. | Real WebView CSP enforcement, actual direct-network refusal or a complete worker/IPC sandbox. |
| Block 2A focused audit | 258 core unit + 4 integration, 48 native-host, 4 executor and 15 Agent tests passed. Token-scoped teardown, generation changes and stale approval refusal were reviewed against production calls. | Guaranteed rollback of an already-started tool, all cold-start races, measured RAM/handles or every future backend path. |
| Block 2C focused audit/retests | Checks 21, 24, 26, 32 and 33 automated Pass. All 19 real-app scenarios passed in `run-GIQ8JB`; six failure cases repeated in `run-vfq102`; intentional `run-EGhvlm` error-to-success fault failed with exit 1. All cleanup Pass. 49 native-host + 4 executor + 18 Agent normal-build tests, 260 core unit + 4 integration, 11 harness self-tests and 9 focused frontend tests passed. Harness type/build, backend Clippy and scoped formatting/diff checks passed; unrelated backend warnings remain. | The later user batch completed check 34; this earlier runtime run alone excludes ordinary microphone/pill recovery. No live account, model judgment, companion executable or measured memory/handle certification. Counts overlap earlier evidence and are not additive. |
| Block 2D signed-store audit/retests | All 28 real-app cases Pass in `run-4ihytH` before expanded restart/override assertions. All three final store cases Pass twice in `run-giIwNy`/`run-FCVjyS`; checks 18/36 reviewed automated Pass. `run-qAFd1y` detects deliberate unclosed Store, exit 1/cleanup Pass. Eight normal-build store tests, one feature marker test, 15 harness self-tests, build/type checks, Clippy and scoped formatting checks passed. | Test publishing anchor and loopback fixtures; no live publisher/root rotation, account/vault, power-loss, microphone or memory/handle certification. Counts overlap earlier evidence. |
| Block 2D registry testing/audit | All 30 real-app cases Pass in `run-I1qAql`; all three final registry cases repeated in `run-CfCnvH`; cleanup Pass. Check 49 reviewed automated Pass. Deliberate missing pointer detected in `run-SXOHUd`, exit 1/cleanup Pass. 47 normal-build registry tests and 16 harness self-tests Pass; JS/PowerShell/stamp/scoped whitespace checks Pass. Application sources unchanged. | No vault/account validity, power-loss, R1 upgrade, other-platform or memory/handle certificate. Earlier foreground/Escape failures remain recorded and unattributed, despite subsequent passes. |
| Earlier slices | SDK, CLI, checker and affected frontend type/build/test checks are recorded in the plan's dated evidence. | They were not all rerun in the latest backend slice; historical overlapping counts must not be added together. |
| B1a migration/typed-contract audit | Both final cases Pass in `run-qX8nKr` and post-audit `run-qF6TP9`; deliberate numeric coercion/archive loss detected in `run-7vtjTu`/`run-ghNMQp`. Six installation, three registry, three store and six native-failure cases retested Pass. All cleanup Pass. 18 runner self-tests, normal backend check, harness frontend/type/build, core Clippy and scoped formatting Pass. Checks 2/22 reviewed automated Pass. | Controlled Windows app/scripted-model evidence. No account/vault, normal-profile upgrade, power-loss, ASR/pill or resource certification; retained input investigation remains open. |
| B2 first MCP audit/retests | Check 23 accepted; two final real-app MCP cases pass twice, both deliberate faults fail, eight native-auth and two smoke retests pass; all cleanup Pass. 46 normal MCP tests, four guards and 21 self-tests pass. Inventory 42. [Audit and identities](MCP-CATALOG-TRANSPORT-AUDIT.md). | Controlled unauthenticated HTTPS peer. Checks 8/17/20/31, byte-limit/slow-call schedules, conformance, live accounts and all release gates remain open. |
| Manual/live checks | User-reported checks 1, 3, 4, 5, 6, 7, 16, 25, 27, 29, 30, 34, 35, 37 and 40 Pass. With 24 reviewed automated checks, the numbered ledger is 39 Pass / 14 Pending. Block 2A, 2B and 2C audits/acceptance passed. Check 37 combines automated reload coverage and the user's remaining ordinary-app batch. | No live provider certification or achieved workflow-completion percentage is claimed. Exact build/reload timing for the latest user batch was not supplied. |
| Initial Agent harness, Windows | All 12 real-app scenarios Pass in `run-q7NuS5`, cleanup Pass. Its eleven short cases also Pass from another clean profile in `run-L9Rzrk`. Normal-build focused suites: 16 Agent + 48 native-host + 46 MCP + 23 auth = 133 Pass. Harness assertions/reporting: 9 Pass; test-feature isolation marker: 1 Pass. | No live-model/account, OS-vault write, microphone/pill, measured RAM/handle trend or whole-block acceptance. Earlier intermittent findings remain open. |
| Block 2B focused audit and retests | Current executable: all 13 real-app cases Pass in `run-f3Vfd2`; all 12 short cases Pass again in `run-zqk29A`; both cleanup Pass. Core: 260 unit + 4 integration, Clippy with warnings denied Pass. Maintained logic: 47 registry + 18 Agent + 48 native-host + 46 MCP + 23 auth = 182 Pass; the 47 registry tests are a subset of the core total. Agent rerun after cancellation edit: 18 Pass. Harness self-tests: 9 Pass. The user subsequently reports the three remaining ordinary-app checks Pass. | The original locking process and every earlier Escape failure are not attributed; these Windows results are not a release/resource or other-platform certificate. |
| New Escape failure oracle | `run-w2P04k` deliberately withholds Escape: expected window-destruction Fail after about eight seconds, exit 1, cleanup Pass, no tool dispatch. | This verifies the new oracle rejects missing input; it is not a product Pass. |
| Harness failure detection | `run-NYtjox` intentionally returns an incorrect fixture greeting: expected assertion Fail, exit 1, cleanup Pass. Missing-binary self-test verifies Blocked/exit 2 rather than success. | A deliberately failed oracle check is not a passing product scenario. |
| Harness build checks | Application and Vite-config type checks, normal/harness frontend builds, separate-port assertions, scoped Rust/JS formatting and staged whitespace checks pass. Test hooks are feature-gated; the Handy tree is unchanged. | Existing unrelated generated-binding changes and backend warnings are outside this patch. |

Representative recent commits: `10d6990d` (native sign-in ownership/binding), `cbeccbb5` (native account isolation), `3232b53c` (selected tools/approval continuation), `4da6c745` (registry preservation/account publication), `ff015f94` (grant validation), and `10d777b1` (activation publication). Earlier implementation evidence and multi-source research remain in the execution plan.

For subsequent implementation work:

1. Update this document's date and implementation snapshot; revise the cumulative behavior/remaining limits instead of only appending a chat history.
2. Add or resolve a divergence row when delivery differs from the authoritative plan. Distinguish user-approved scope changes, temporary choices and unfinished requirements.
3. Keep full procedures and newly numbered checks in the plan. Add each new check exactly once to an appropriate group here, retaining existing numbers; update both totals.
4. Record actual user results by changing the row status and adding evidence below. Do not infer Pass from unit/protocol tests, partial scenario links, credential badges or a model's prose response. Reviewed full real-app automated acceptance must identify its evidence class and audit.
5. Close a phase only when its gate evidence is recorded. Record deterministic checks, live acceptance and measured resources separately.

### Reviewed automated registry acceptance

**Checks 2/22 — Pass, 1 October 2026, isolated actual Grain host/scripted model:** both B1a cases passed in `run-qX8nKr` and repeated after focused audit in `run-qF6TP9`; all cleanup Pass. Actual edited legacy prompts/bindings, inactive quarantine/grants, preserved user data and repeated/interrupted startup satisfy check 2. Seven actual typed results, exact approval/dispatch counts and changed-valid-declaration refusal satisfy check 22. Numeric coercion and archive-loss faults fail with exit 1/cleanup Pass. The reproduced stale terminal-owner quarantine and failed-uninstall publication paths were repaired and regression-tested. Fingerprints, development failures and scope limits are preserved in the [focused audit](NATIVE-FOUNDATION-AUDIT.md). No account fixture or whole-phase closure follows.

**Check 49 — Pass, 1 October 2026, isolated actual Grain host/scripted model:** `run-I1qAql` passed all 30 real-app scenarios, cleanup Pass. `run-CfCnvH` repeated all three final registry cases from a clean profile, cleanup Pass. Four damaged-state fixtures preserve bytes and refuse actual startup/operations; installed, disabled and quarantined state plus a nonsecret opaque account pointer survive actual restart. Real Windows publication locks verify failed enable rollback and live-but-unsaved disablement, then explicit recovery. This certifies pointer serialization, not a credential grant/account switch. Deliberate `run-SXOHUd` detects missing pointer, exit 1/cleanup Pass. Final runner fingerprint `418788e0532d393ba84f23ae7130bdd758c66f81b33d27ee35d3230012eced2f`; host SHA-256 `d073206e878b9b70fa34ef3665cc1645e26f595277a3122d4357c8626e6484f8`; source fingerprint `900fa95beb866e10bb13981a7280dde2e0c00dab7ce299a211d3c424d1214d70`; precommit base `ba3670cc`. [Focused audit and exclusions](NATIVE-REGISTRY-AUDIT.md). Check 53 remains Pending with its conditional account fixture portion Blocked; no platform feature implementation resumes from this unit alone.

### Current regression observations to retain

**AGENT-BUDGET-01 — open, 3 October:** ordinary Linear issue creation/assignment works according to the user, but repeated tasks sometimes end with the model requesting more tools after the task budget ends. The production eight-round/24-call stop refuses further dispatch, and the focused Agent logic suite passes. Record this as a task-efficiency/completion investigation, not an authentication failure or resolved bug. Capture exact redacted task/round/call/receipt evidence before altering limits; never replay completed writes. [Finding and next evidence](LINEAR-LIVE-AUTH-AUDIT.md#ordinary-agent-results-and-deferred-budget-finding).

During the registry testing sprint, combined `run-NGBF2o` stopped because native Escape could not establish owned foreground focus. Focused `run-HRfoJm` then sent the input pair but timed out waiting for window destruction. Both have cleanup Pass; later cases in the failed combined batch are Not run. The cause remains unattributed. The test adapter now refuses plain Escape while a modifier is held and offers a read-only modifier probe; that probe saw none at the time tested. `run-JrNBa3` subsequently passed all ten replacement/Escape pairs. These observations are recorded in the [registry audit](NATIVE-REGISTRY-AUDIT.md), not silently erased by reruns. Final combined `run-I1qAql` also passed those ten pairs, but does not establish the cause of earlier failures. Retain this intermittent case in the foundation-testing review; no production shortcut fix is claimed.

The [native-input investigation](NATIVE-INPUT-INVESTIGATION.md) preserves three clean ten-pair passes, a newly reproduced combined-run focus refusal with **zero Escape input**, and post-adjustment lifecycle/click-path evidence. `run-27m4Cu` reports a denied foreground request on its tenth pair; it is not an accepted Escape event failing to close Grain. The maintained adapter can now activate an unobstructed owned header through one guarded actual click and records both click/key counts. Seventeen self-tests pass. The explicit click case passes ten pairs and the withheld-Escape oracle still fails truthfully, cleanup Pass. This fixes a test setup limitation; it does not explain or close `run-HRfoJm`, certify production focus behavior or advance the 53-check ledger. The thirty-case combined run `run-CTjIOP` passed before the final coordinate/ownership safeguards; the audit distinguishes its earlier runner identity from the final explicit click run `run-Lu4QR6`. Current final input runner fingerprint: `41523f19c8fd5c676408fa8ee8cc48bf252d2be61513187da1e5b37eaeace4e8`.

### Reviewed automated signed-store acceptance

**Checks 18 and 36 — Pass, 1 October 2026, isolated actual Grain host/scripted model:** `run-giIwNy` and `run-FCVjyS` each passed all three final store cases after audit, with cleanup Pass. The real Store component cancels ten held loading attempts and releases its index, actual signed Install/consent/greeting works, offline cards disable Update and backend installation is refused. Held updates cannot undo disable/removal; choices survive actual restart, including developer override and parked-installed preservation/removal. Final executable SHA-256 `d073206e878b9b70fa34ef3665cc1645e26f595277a3122d4357c8626e6484f8`, source fingerprint `900fa95beb866e10bb13981a7280dde2e0c00dab7ce299a211d3c424d1214d70`, runner fingerprint `7b759e516f4e2596c61adc30e4ed38830e91517721747f9b4a5df2eacf71207a`, precommit base `c3be5770`. Earlier `run-4ihytH` passed all 28 cases before extra mutation assertions; final `run-qAFd1y` detects withheld Store closure with exit 1/cleanup Pass and zero tool events. The [focused audit](NATIVE-STORE-AUDIT.md) retains failed test-assumption runs and exclusions. This is reviewed automation, not a user report or production publishing-root/account certificate. Whole 2D remains open.

### Reviewed automated packaging acceptance

**Checks 28 and 39 — Pass, 1 October 2026, isolated actual Grain host and real CLI:** `run-NgmddX` and `run-FlNtip` each passed all six installation cases after the focused source audit, with cleanup Pass. Actual compiled greetings distinguish installed replacements and developer A/B. The original installed approval remains held through override/restoration and is refused with zero extra dispatch; the saved installed record is unchanged. Both old package lookup files remain preserved and cannot shadow new bytes across restart. `run-5gFNhK` detects deliberate obsolete import with exit 1/cleanup Pass. The fixture has default settings and no OAuth grant; nonempty settings, signed-store and native account/vault certification are not inferred. Check 53 remains Pending with its conditional account portion Blocked. This is reviewed real-app automation, not a user report.

### Manual result record

**Checks 1, 3, 4, 7 and 16 — Pass, 30 September 2026, Windows real app:** the prepared tool-only native project loaded/enabled without retired consent and its Agent greeting worked cold, immediately again and after about three idle minutes. The user then reported all three remaining Block 1 checks worked: normal core settings/features and restart, absence of old extension controls, and rejection of the separate package requesting `capture`. Exact refusal text, worker-reap logs and MCP banner status were not supplied; no live provider was certified. **Block 1 audit retest after `6e02748b` — Pass:** the user provided actual tool results for Tool Smoke (`Hello from this tool.`) and Network Boundary Probe (`PASS: unbrokered browser fetch was blocked.`), and reports that dictation works. Check 2 remains Pending because an actual legacy installed profile and archive/restart migration were not specifically exercised. For later reports record: **check number, date, tested commit, platform, provider/model when relevant, Pass/Fail/Blocked, observed result and next action**. For failures, include the exact visible error and redacted developer-log timestamps. Never include credentials or private prompt contents.

**Checks 6, 25 and 27 — Pass, 30 September 2026, Windows real app:** the user reports all three Lifecycle Smoke modes worked. Supplied tool output shows an interrupted slow call reported as uncertain rather than undone/retried, successful fresh fast calls on the current instance, and refusal of the old approval after an unchanged reload. The refusal included a malformed dash (`â€”`); its string literal was fixed during the focused audit. A separately timed cold-start repeat for check 25 was not described. No native write, installed-package rollback or measured worker memory claim follows from these observations.

The preceding manual result is **check 37 — Pass, 1 October 2026, Windows ordinary app**, following the `d4a6ca15` handoff. The user reports the first two ordinary dictation/recording-pill observations worked and explicitly confirms the third Agent greeting worked. This is aggregate human evidence combined with the previously recorded authenticated developer-reload scenarios; the user's exact build, provider/model and ordinary-profile reload timing were not separately supplied. The focused audit and post-audit retest close 2B; remaining 2C/2D and whole-phase gates are unaffected.

**Check 34 — Pass, 1 October 2026, Windows ordinary app after the `0884aca5` handoff:** the user supplied the actual unconfirmed-outcome warning, reports all three requested observations passed including ordinary dictation/pill recovery, and supplied the actual `Harness hello (one)` tool result. The exact build/model was not separately supplied. Together with the audited raw-budget/cleanup/recovery harness evidence, this closes 2C; it does not certify accounts, store or resource gates.

**Check 40 — Pass, 1 October 2026, Windows ordinary app after the `8ffcb79c` handoff:** the user explicitly confirms all three procedures passed: import/consent cancellation with disabled restart, one Allow and actual greeting across restart, and ordinary dictation/pill/control observations. The exact app build/model and tool transcript were not separately supplied. This completes the first 2D unit with the focused audit and automated retests; packaging/store/account requirements remain separate.

### Reviewed automated acceptance

**Check 10 — Pass, 2 October 2026, isolated real Grain host/locked SDK/controlled OAuth issuer:** `run-4JRUXP` passes all twelve combined MCP cases and `run-dTmIAr` repeats all three audited account cases in a fresh profile. Cancel sign-in, Disable and Developer Mode off each release the actual callback listener, refuse its issued old redirect, exchange/store nothing and preserve disconnected state across actual restart. A fresh B grant and actual approved read survive another restart. Exact account/nested result, zero stale dispatch and private-token exclusion are independently asserted. `run-EeGMP1` detects wrong account; `run-z9xkzX` independently deletes one abandoned actual grant. Eight native-auth, two smoke and two public live cases pass; official tool cases pass while standalone init stays Blocked. All named final cleanup passes. [Focused audit](MCP-AUTH-FIXTURE-AUDIT.md) records exact identities, stricter-oracle/journal repairs, the user's accidental cancellation and two policy-blocked interrupted scratch roots. External browser UX/whole MCP authentication remain separate. Ledger **42 Pass / 11 Pending**. Next authenticated B2 check 31; no new user batch.

**Checks 21, 24, 26, 32 and 33 — Pass, 1 October 2026, Windows real app/scripted model:** reviewed full procedure coverage in `run-GIQ8JB` and clean-profile repeat `run-vfq102`, after the focused source audit and warm-source repair. Both runs identify binary SHA-256 `0a8d6c0129d27ce259f3a26e5f76ee6d1171bb80ca6247e2f58f6fa06cc83121`, source fingerprint `b8025e0c753fdeab041fb7c1b0cf639df15c5ed9b2a69a5b90dad1b5ac5c8275`, runner fingerprint `6d737cb05b69db8d9d47589582ffbf366776cb9ff8ca5a113e64126c52959e60`, and a dirty working tree based on `5800818d`. The actual absolute deadline measured **20,006 ms** in both accepted runs. This automation evidence does not change the provenance of the original thirteen user results. Check 34 now combines that automated coverage with the user's ordinary-app batch above. [Audit, research and reproduction](NATIVE-FAILURE-AUDIT.md).

**Checks 19, 38 and 52 — Pass, 1 October 2026, Windows real app/scripted model:** full reviewed disable/re-enable/source replacement refusal, actual stale permission-sheet refusal/current disclosure, and cancel/allow/disable persistence procedures passed in `run-3fp5CX` and repeated from a clean profile in `run-DJ79qE`, after the focused audit and guarded shutdown repair. Each batch ran eleven owned host lifetimes with ten real restart transitions. Executable SHA-256 `354303346b7bc88510446518ab88be3f5deacb0dfaf9f953a102fffcb6b19c35`, source fingerprint `413bf55d818ca801e9a4977089bb5378b4b2947b8d9d833017846e588d84d8a4`, runner fingerprint `724bdcbdc00c2504b76948507f01fb16d52f0229e9684cf2aae81602f1e52f6b`, dirty precommit base `0884aca5`. Checks 28, 39, 40, 49 and 53 retain partial supporting coverage/Pending status; 18 and 36 have no real-app store acceptance from this unit. [Focused audit and exclusions](NATIVE-INSTALLATION-AUDIT.md).

### Progress log

- **29 September 2026:** Created this consolidated report through `10d777b1`; reconciled all 53 pending manual checks into seven groups and documented scope changes, implementation differences and the seven open phase gates. This update changes documentation only.
- **29 September 2026:** Verified the API-versus-implementation distinction against the production worker, generated authoring API and host preflight/dispatch: legacy backend branches remain unreachable behind refusal and still require physical removal. Recorded the next retirement unit and a minimum user acceptance checkpoint; no implementation or manual-test status changed.
- **29 September 2026:** Added sequential acceptance blocks and a single active real-app handoff. Block 1 starts with reduced extension controls, then a real native tool call; all 53 manual checks remain Pending until the user reports results.
- **29 September 2026:** Received the Developer-screen screenshot, recorded check 16 as partially observed and noted the MCP negotiation error without attributing a cause. Prepared disposable, bundled Tool Smoke locally; Step 1B is to load its folder. No provider connection or application-code change was made.
- **30 September 2026:** The user showed Tool Smoke loaded in the real Developer view and reports turning it on. Individual tools are not shown as separate buttons there; Step 1C batches cold, immediate repeat and idle-recovery Agent calls. Checks 3, 4 and 16 remain Pending until their complete outcomes are reported.
- **30 September 2026:** The user reports the tool and ordinary dictation work. Marked check 3 Pass (one completed manual check), recorded partial check 1 and 4 evidence, and requested clarification before closing the immediate/idle repeat procedure. No code, provider or credential change was made.
- **30 September 2026:** The user confirms both immediate and three-minute repeat tool calls succeeded. Marked check 4 Pass; 2 of 53 numbered manual checks are complete and 51 remain Pending.
- **30 September 2026:** Prepared a separate disposable negative package with a retired `capture` permission; the real CLI doctor reports only that intended error. The next user batch covers core settings/restart, retired management/store controls and refusal of this package while preserving the valid greeting. No app code or provider credentials changed.
- **30 September 2026:** The user reports all three remaining Block 1 checks worked. Marked checks 1, 7 and 16 Pass, bringing the ledger to 5 Pass / 48 Pending. The focused audit found direct browser networking available to native workers, added a hidden-host CSP and narrowed its Tauri capability to event operations. Automated build, contract, host and worker tests pass. Prepared a second disposable, CLI-validated tool that attempts a direct fetch of the dev server; real WebView retest remains before accepting Block 1. Physical deletion of unreachable retired backend handlers is deferred as a separate later cleanup unit.
- **30 September 2026:** The user supplied Tool Smoke and Network Boundary Probe tool results after the audit patch and confirms dictation works. Accepted Block 1; the numbered ledger stays at 5 Pass / 48 Pending because these were retests. Prepared a CLI-validated Lifecycle Smoke fixture with fast and slow confirmed tools for the first Block 2 batch (checks 6, 25 and 27). No code or hosted-provider change was made in this update.
- **30 September 2026:** The user reports all three Block 2A Lifecycle Smoke modes worked and supplied the interrupted/uncertain, stale-refusal and fresh-success tool results. Marked checks 6, 25 and 27 Pass (8 Pass / 45 Pending). Audited worker/Agent ownership and relevant tests, corrected the visible stale-refusal encoding, and accepted Block 2A. The next four observations exercise Agent close/Escape/conversational approval and pending-confirmation teardown in Block 2B.
- **1 October 2026:** Investigated the Agent summon failure reported as `STATUS_CONTROL_C_EXIT`. The user confirmed it occurs after using the Agent shortcut in the terminal, rather than at app startup. Guarded the Windows selection-copy path against terminal foreground windows; real-app terminal and nonterminal retests remain before resuming Block 2B. No numbered check was marked Pass.
- **1 October 2026:** The user confirms both terminal-fix retests and all four prior Block 2B close/reopen modes worked. Marked check 29 Pass (9 Pass / 44 Pending); check 30 has pending-approval evidence but still needs its slow-model-response case. Reviewed the X/Escape/window-destroy paths and reran all 16 Agent tests. Block 2B remains active; the next batch covers checks 30, 5 and 35.
- **1 October 2026:** The user explicitly confirms the remaining model-response close case, all ten Tool Smoke unload/reload cycles and both idle timing cases worked. Marked checks 30, 5 and 35 Pass (12 Pass / 41 Pending). Reviewed token-scoped reload and reaper removal; all 48 extension-host tests passed. Verified the disposable Lifecycle Smoke build and CLI doctor before handing off final Block 2B check 37. No application code changed.
- **1 October 2026:** Added the persistent acceptance-harness design and linked it to the execution plan. Verified that the developer socket offers reload only and that a temporary working directory/environment alone cannot establish application/vault isolation. Defined H0–H4 delivery, evidence classes and human sign-in handoffs; 12 Pass / 41 Pending remains unchanged. This update changes documentation only.
- **1 October 2026:** Implemented the persistent **Agent** harness and renamed its design companion to `AGENT-TEST-HARNESS.md`. Added the isolated feature-gated real host, scoped fixture controls, production worker observations, selective scripted model, actual Agent UI driver, authenticated production reload, real idle timing, stable scenarios, build/runner identities, bounded evidence and owned teardown. User authorization supersedes the obsolete automation rule in `AGENTS.md`.
- **1 October 2026:** Ran all twelve scenarios successfully with cleanup Pass, then repeated the eleven short cases from a clean profile. The real idle case retired after about 126.6 seconds and a slow call dispatched after another 110-second wait completed on the same worker; its whole case took about 253 seconds. The deliberately wrong greeting failed as required. Reports identify the precommit dirty working tree based on `55133ae5`, executable SHA-256 `33b68063383faaec7d78b3ec5f548dc81fdc26897cfc4b6e564f1243ec0e06cb`, source fingerprint and runner fingerprint. Local full reports live in ignored `tests/agent-harness/.runs/`; use the maintained commands to reproduce them on another checkout. Recorded the registry-save and earlier Escape findings instead of masking them with retries. Manual ledger remains 12 Pass / 41 Pending; check 37 and the focused whole-block audit remain open.
- **1 October 2026:** Completed the focused native close/reload source audit and documented it in `NATIVE-LIFECYCLE-AUDIT.md`, including multi-source primary/open-source references. Reproduced Windows registry error 5 in `run-L8HNtp` and a real sharing-lock regression; bounded retries now retain the same synced staging file and preserve persistent failures. Synchronized transient registration/deferred cleanup with a small ownership guard; cancellation also discards a queued first instruction. Added real ten-pair close/reopen/Escape coverage, a deliberate missing-Escape oracle and the maintained core registry test group.
- **1 October 2026:** All thirteen real-app scenarios passed in `run-f3Vfd2`; the twelve short cases repeated successfully in `run-zqk29A`, both cleanup Pass. Idle retirement was observed after 144,957 ms; its full case took 272,217 ms. The intentional `run-w2P04k` failure had exit 1, cleanup Pass and no tool dispatch. Current binary SHA-256 is `36a639d84334e57a2372466bfaf91ef9bf5ac60b093a666638167a2441b72cf8`, source fingerprint `ab8eedcd02f93ad39b68c4f9fbb4b14f212a9af366fb585324686548225128b2`, runner fingerprint `0ff0f12197bc28653fef68876f214c08ba01bde069a92282b3545cb64edd3070`; reports identify the dirty working tree based on `e42a3a4c`. Full core tests, 182 grouped logic tests (overlapping core registry subset), 9 harness self-tests, harness build/type checks and scoped static checks passed. Manual ledger stays 12 Pass / 41 Pending. Only the three ordinary-app recording/pill/greeting observations after one developer reload remain for this sub-block; next is 2C, not hosted-provider sign-in.
- **1 October 2026:** The user confirms the last batch worked, clarifying that the first two observations were ordinary dictation/recording-pill checks and explicitly confirming the Agent greeting. Recorded check 37 Pass using this aggregate report plus the already recorded developer-reload evidence; Block 2B is accepted after its focused audit and post-audit retest. Ledger is now 13 Pass / 40 Pending. Next is 2C harness coverage; this documentation update changes no application code and does not certify that next unit.
- **1 October 2026:** Completed native Block 2C implementation and focused source audit. Added six maintained real-app failure scenarios, five invalid-input cases, coarse host outcome/timing evidence and the normal-build executor logic group. Reproduced the first fresh call failing after source drift without Reload, repaired warm reuse to require current source identity, and added a regression. Preserved `run-Stew7J` (product failure) and `run-QWY4Zt` (oversized-input oracle assumption). All 19 current scenarios passed, six repeated from a clean profile, and the intentional error-to-success fault failed as required; all cleanup Pass. Recorded five reviewed automated checks Pass, making 18 Pass / 35 Pending. Only the ordinary-app portion of check 34 remains before closing 2C; 2D and all seven whole-phase gates remain open.

- **1 October 2026:** The user supplied the expected oversized-reply warning and actual fresh greeting, and confirms all three ordinary-app observations worked. Marked check 34 Pass; 2C is accepted after its focused audit and post-audit tests. Ledger: **19 Pass / 34 Pending** (14 human, 5 reviewed automated). Started 2D maintained harness coverage; its acceptance remains open.

- **1 October 2026:** Completed the first native 2D audit unit: real consent, same-version import, installed/developer A/B restoration, ten actual host restart transitions and preserved invalid-registry startup. Corrected harness onboarding/setup navigation, intentional shutdown ownership and route remount assumptions; preserved the failed runs. All 24 scenarios passed, the five new scenarios repeated, and deliberate skipped restart failed as required, all cleanup Pass. All eight normal-build logic groups passed 199 tests; 12 harness self-tests and builds/static checks passed. Recorded checks 19, 38 and 52 as reviewed automated Pass: **22 Pass / 31 Pending**. Ordinary check 40 and wider store/CLI/account portions remain; whole 2D and all seven phase gates remain open.

- **1 October 2026:** The user explicitly confirms all three normal-app consent/restart/dictation observations passed. Recorded check 40 Pass: **23 Pass / 30 Pending**. Accepted the first 2D unit after its source audit, repeated automated checks and this user batch. Next is real CLI packaging and legacy artifact ownership; no MCP/authentication block is started.

- **1 October 2026:** Accepted the second 2D audit unit: actual CLI build/doctor/pack and same-version artifact ownership, original approval refusal through restored ownership, saved installed-record preservation, legacy-file non-shadowing and A/B/restoration across actual restarts. All 25 scenarios passed before an added audit assertion; the final six installation cases passed twice afterward. Deliberate stale package was detected, exit 1, cleanup Pass. Thirteen harness self-tests, nine CLI tests and build/static checks passed. Recorded checks 28/39 as reviewed automated Pass: **25 Pass / 28 Pending** (15 human, 10 automated). Next is signed-store schedules; account/recovery requirements, whole 2D and seven phase gates remain open.

- **1 October 2026:** Accepted the third 2D audit unit: signed-store fixture installation, actual loading/close/reopen and offline controls, integrity/cancellation refusal, pending disable/removal and post-mutation restart/override ownership. All 28 scenarios passed before additional audit assertions; three final store cases passed twice afterward. Final withheld-close fault detected with exit 1/cleanup Pass. Eight normal-build store tests, one marker test and 15 harness self-tests passed, as did build/type/Clippy/scoped formatting checks. Recorded checks 18/36 as reviewed automated Pass: **27 Pass / 26 Pending** (15 human, 12 automated). Next is remaining 49/53 registry/account recovery; whole 2D and all seven phase gates remain open. No new human test batch is requested.

- **1 October 2026:** The user clarified testing must finish before further platform improvements. Paused feature work and completed registry test coverage/audit only: disabled/quarantined/opaque-pointer restart preservation, four invalid startup variants and real Windows failed-save schedules. Final all-30 run and clean-profile registry repeat passed; the missing-pointer fault was detected, cleanup Pass throughout. Recorded check 49 as reviewed automated Pass: **28 Pass / 25 Pending** (15 human, 13 automated). Retained earlier test-oracle errors and two intermittent native Escape failures; later Escape passes do not attribute their cause. Native account-owner test 53 and wider existing acceptance remain open. No product implementation resumes yet.

- **1 October 2026:** Continued existing-foundation testing only. Added coarse native input diagnostics and guarded owned-header focus setup; reproduced Windows foreground refusal with zero Escape insertion and retained the failed batch. Three initial clean ten-pair runs, twelve post-adjustment lifecycle cases and ten explicit click-path pairs passed; the final withheld-Escape fault failed as intended. Seventeen harness self-tests passed. No application feature/source or credentials changed. Earlier accepted-input timeout remains unattributed; native account-owner 53 needs a distinct guarded auth fixture. Ledger stays **28 Pass / 25 Pending**.

- **1 October 2026, final input audit/retest:** All thirty cases passed in `run-CTjIOP` before the final coordinate/ownership safeguards. After those test-only safeguards, ten explicit click/Escape pairs (`run-Lu4QR6`) and twelve lifecycle cases (`run-R5MkE6`) passed; seventeen self-tests passed. Final withheld-key `run-3Gtam7` detected the live panel after 8,085 ms, exit 1/cleanup Pass; its diagnostic collection made total scenario time 173,834 ms and remains a harness timing limitation. Final runner `41523f19c8fd5c676408fa8ee8cc48bf252d2be61513187da1e5b37eaeace4e8`; host/source unchanged. Checked zero owned host/event listeners afterward. Ledger and remaining whole-phase gates unchanged; no production focus fix or native account certification is claimed.

- **1 October 2026, library ownership decision:** Stopped implementation/testing at the user's request and compared upstream dependencies and host integration in Goose, OpenCode, Cline and Rig. Selected continued official `rmcp` use, a gated 3.5.0 upgrade candidate, official conformance against Grain's production adapter, existing HTTP/vault/schema libraries and later `oauth2` native OAuth reuse. Retained the current foundation-first acceptance order; no larger framework, dependency or product change was applied. Added the decision and bounded delivery slices to the authoritative plan. Existing test evidence, the 28/25 ledger and all seven open gates are unchanged.

- **1 October 2026, source audit and forward execution plan:** Queried the graph, inspected production adapter/auth/worker/manifest/harness ownership and reconciled all 25 pending checks with the library decision. Added `MCP-EXTENSION-REUSE-EXECUTION-PLAN.md` with measured source inventory, keep/edit/replace/remove dispositions, native-plus-MCP architecture, controlled/live evidence mapping and sequential acceptance/refactor/release gates. Clarified that the complete existing 25-check baseline precedes replacements; conformance can support the current SDK, and affected final paths must be recertified afterward. Retained unresolved input observations, live prerequisites and all historical verdicts. Documentation only; no app runs, dependency changes or phase closures. Ledger remains 28 Pass / 25 Pending.

- **1 October 2026, B1a implementation and acceptance:** resumed the approved testing-only plan; added maintained actual-app migration/typed-contract cases and fault oracles. Reproduced stale quarantine after terminal owner removal and made uninstall use the existing save-before-publication rollback helper. Two final cases passed twice, and all affected installation/registry/store/native-failure cases passed with cleanup. Focused graph/source audit, 262 core unit + 4 integration tests, 18 runner self-tests, Clippy and normal/harness build checks passed. Recorded checks 2/22 as reviewed automated Pass: **30 Pass / 23 Pending**. Next is B1b guarded native account fixture; eight B1 account checks, retained input observations and all seven phase gates remain open. No SDK/OAuth replacement or new manual batch.

- **2 October 2026, B1b native binding/owner acceptance:** added two maintained actual-app account scenarios with finite debug-only setup. Reviewed 41/45/53 procedures cover unbound/id-only grants, four declaration changes, installed/developer A/B accounts, CLI-built source ownership, reload/restart, stale approvals and independently counted parked uninstall. Two clean combined runs passed; wrong-owner and unchanged-declaration faults failed as required. The final formatting-only repeat and ordinary smoke/hot-reload retests also passed, all cleanup Pass. Focused source audit, 23 auth tests, three feature guards, 20 runner self-tests, normal/harness builds and static checks passed. Ledger is **33 Pass / 20 Pending**; next B1c 42–44/50–51. Three discarded developer vault grants required harness cleanup; production orphan reconciliation remains open in R3. [Evidence, source audit and exclusions](NATIVE-ACCOUNT-OWNERSHIP-AUDIT.md). No new manual batch, SDK/OAuth replacement or release-gate closure.

- **2 October 2026, faster B1c testing/partial acceptance:** reused the guarded native fixture/shared account assertions and one stamped host; added four cases plus the maintained `native-auth-schedules` subset. Actual observed barriers and two-second provider-issued grants replace guessing/waiting for long fixture lifetimes; production clocks/timeouts and required restarts remain unchanged. Corrected a mistaken unchanged-installed-reimport assumption to actual CLI/developer WebSocket reload; retained its failed run. Complete affected native-auth batch and final four-case repeat passed; skipped-cancellation/full-consent faults were detected. 23 normal auth tests, three feature guards, 20 self-tests and build/static checks passed. Accepted 42/43/50/51: **37 Pass / 16 Pending**, next 44, then B2. Final four cases took 44,956 ms of scenario time. Existing discarded developer-grant cleanup gap remains open. [Focused audit, identities and limits](NATIVE-AUTH-SCHEDULE-AUDIT.md). No test requirement dropped, manual batch added, library replacement or whole-phase closure.

- **2 October 2026, B1c final numbered acceptance:** added `native.auth-refresh-logout` and one fixed peer identity. Actual approved refresh/logout/reconnect, awaited stale-publication refusal and same-host provider independence passed. Fixed missing peer receipts, exact unknown-outcome recognition and failure observation retention; retained all three earlier failures. All eight account cases, final clean repeat and ordinary smoke passed; premature-release fault failed with cleanup Pass. New isolated case leaves no credential for final deletion; older full-suite procedures still leave four discarded developer grants for independent cleanup. 23 normal auth tests, four final feature guards, 20 self-tests and build/static checks passed. Accepted 44: **38 Pass / 15 Pending**, inventory 40. Final new case took 13,690 ms. [Focused audit, identities and limits](NATIVE-AUTH-REFRESH-AUDIT.md). No new manual batch, objective removed, library replacement or whole release gate closure. Next B2.

- **2 October 2026, B2 first acceptance unit:** added two maintained real-app MCP cases and a fixed debug-only HTTPS peer. Modern/legacy JSON/SSE, nested arguments/results, lazy selected schemas, mixed/unsupported catalogs, paging refusal, restart and stale approvals pass. Repaired disposable TLS and test bootstrap/oracle assumptions without changing ordinary MCP behavior; retained every early failed run. Final suite passed twice in about six seconds of scenario time; both deliberate faults failed, eight native-auth and both smoke retests passed, all cleanup Pass. 46 normal MCP tests, four harness guards, 21 self-tests and build/static checks pass. Accepted 23: **39 Pass / 14 Pending**, inventory 42. [Focused audit, identities and limits](MCP-CATALOG-TRANSPORT-AUDIT.md). B2 bounds/cancellation/conformance and live prerequisites remain next; no new manual batch, library replacement or release-gate closure.


### B2 controlled response bounds and cancellation - 2 October 2026

The [focused audit](MCP-BOUNDS-CANCELLATION-AUDIT.md) records three new real-app cases and their separate stage evidence: large JSON/SSE previews preserve UTF-8 and explicit omission; twelve lifecycle/overflow/drop combinations classify unknown with one approved call and fresh recovery; four approved held calls and two pending approvals close without replay or late-result contamination. The real host and locked production adapters are unchanged except for two finite debug-only submit instructions. The controlled peer has no account, so checks 20/31 remain Pending for their complete mapped procedures. **39 Pass / 14 Pending** remains accurate.

Inventory: **45 maintained scenarios**, five MCP cases, 22 runner self-tests. The [cleanup inventory](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) distinguishes retained regression infrastructure from disposable profiles/keys/processes and identifies test-seam retirement conditions. No unused future scaffold was added. Failures stop the batch, preserve their exact substage/model evidence and leave later cases Not run; accepted final runs and all early oracle/reporting failures are retained in the audit. Next is remaining B2 acceptance, not broader implementation. No new human batch is required now.

### B2 catalog budgets and genuine deadlines — 2 October 2026

The [focused audit](MCP-CATALOG-DEADLINE-AUDIT.md) records three more maintained real-app procedures. In plain language: incomplete or oversized tool lists are refused before the Agent can use them; a tool that never replies stops at the real HTTP deadline with an honest unknown result; and a server that keeps sending slow catalog pages cannot keep discovery alive indefinitely. Each variant checks that its old resources close and a new approved read works. Production clocks, libraries and ordinary application behavior are unchanged.

All eight MCP cases pass in `run-lsAhcb` and fresh-profile repeat `run-PfoAgA` with identical final fingerprints, 116 independent observations and 80 actual tool calls per run. Real HTTP measurements stay near 45 seconds and absolute discovery near 90 seconds. Deliberate `run-Lje41Z` supplies a complete catalog where refusal is required; its precise first assertion fails with zero calls. These final runs/fault have cleanup Pass, zero retained sessions/replies/timers and no disposable profile/TLS directories. The inventory is **48 scenarios**, eight MCP and 23 runner self-tests. No numbered check is removed or prematurely accepted: **39 Pass / 14 Pending** remains correct for outstanding live/account, conformance, B3/B4 and release requirements.

**Reporting deviation and retained exception:** initial `run-FCSQDD` lost its terminal session before final evidence; no acceptance is credited. Automatic approval review rejected deleting its disposable profile/TLS directories with "blocked by policy"; cleanup remains Pending and its exact root is in the [inventory](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory). This exposed a harness reporting gap, so reports now checkpoint before/after every case with `activeScenario` and cleanup Not run until teardown. The infrastructure stays bounded and has no background checkpoint engine. Existing production orphan reconciliation and input observations remain separate. No new manual batch; next is official production-wrapper conformance.

Final supporting verification: 46 normal production MCP tests, four harness guards, 23 runner self-tests and build/scoped static checks Pass. `run-HXkox0` passes all eight native-auth regression cases with zero remaining scoped credentials after independent cleanup; `run-wseoi8` passes both smoke cases. Both use the final MCP runner/binary/source fingerprints and cleanup Pass. The source/graph audit and fresh-profile repeat complete this controlled testing unit, with the interrupted-root exception retained. Whole B2 and all seven release gates are still open.

### B2 pinned official tool conformance — 2 October 2026

The [focused audit](MCP-CONFORMANCE-AUDIT.md) adds the official runner through Grain's real MCP adapter and Agent. In plain language: an independent official server offers a sum tool; Grain finds and loads only that tool, asks for real approval, calls it once and verifies that 5 + 3 returns 8. Both legacy and modern protocol cases pass, with official wire-schema validation, repeated fresh profiles and independently verified cleanup. A deliberate missing-official-check fault is detected after the real successful call.

The standalone official initialization case remains **Blocked**: its raw fixture sends an empty successful reply to Grain's actual discovery probe. Grain refuses it. The complete entry retains two Pass / one Blocked and exits 2; the supported tools subset exits 0. No production negotiation bypass, SDK upgrade or protocol response rewrite was used. This is a documented prerequisite, not another accepted numbered check.

Inventory: **51 scenario IDs** (48 self-contained plus three official, one Blocked), **28 runner self-tests**. Final `conformance-A5VZOH` / `conformance-3ZS7TL` agree; fresh tools `conformance-FVzpxr` pass; fault `conformance-KM0Sf7` fails as required, cleanup Pass. All eight MCP regressions (`run-TAS4pn`), eight native-auth regressions (`run-yp7aB8`) and both smoke cases (`run-Ufo4fZ`) pass on the same final host/source/runner identity. Scoped native credentials are independently removed; this does not close production orphan reconciliation.

Focused graph/source audit, 46 production MCP tests, four Rust harness guards, 28 self-tests and build/static checks complete this named tool unit. Temporary relay/cache/classifier retirement conditions are recorded in the [inventory](AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory); no new production engine is introduced. Previous policy-blocked interrupted-root cleanup remains Pending. **39 Pass / 14 Pending** and all seven open release gates are unchanged. No new manual batch; next remains eligible B2 live/provider/account evidence and resolution of initialization compatibility before B3/B4.
