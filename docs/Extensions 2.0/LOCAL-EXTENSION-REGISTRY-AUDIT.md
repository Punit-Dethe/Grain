# Local extension registry: source and publishing audit

**Current first-publication/workflow block, 7 October:**
[Audit](EXTENSION-FIRST-PUBLICATION-AUDIT.md) records the clean initial Git gate,
manual current-contract source build and already-signed publication workflows.
The initial route preserves current seed trust/policy, requires fresh empty
metadata with next versions, verifies exact committed bytes, omits fabricated
previous proof and refuses an already-established publication base. The publisher
runs pinned trusted code, never author builds/signing keys, and uses a single
expected-base conditional push with remote confirmation and no ambiguous retry.
Windows **76 Rust / 4 publisher / 9 core trust / 12 real CLI checks** pass,
alongside locked build, scoped strict Clippy, rustfmt and scoped audit. Linux
checkpoint evidence is tracked in the audit. No live catalogue/main activation,
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

**Superseded scope, 7 October:** the user confirms Grain has no deployed users
and is before pre-alpha. Legacy compatibility, archive carry-forward, version
reservations and old settings/extension migrations are no longer requirements.
The implementation/evidence below is historical; its exclusive machinery is
scheduled for removal under the [clean-break execution policy](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md#current-policy-unreleased-platform-clean-break--7-october).
Current security, signing and hosting integrity remain required.

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

**E4i historical evidence, 7 October:** [Archive audit](EXTENSION-LEGACY-HISTORY-AUDIT.md)
captures all 18 signed catalogue-changing ancestors through pinned `af6e244...`
without using the dirty worktree. Real data has 27 content variants for 13 version
identities, eight conflicts, 37 addressed assets and one package recovered from
a later pinned catalogue tree. The old `builtin` tier is raw historical data,
not new active SDK admission. All 109 proof/asset files are compared with exact
Git source bytes. Signed revocation evidence is still absent; protected complete
migration/publication remains gated. Existing dirty source/deleted-file state,
legacy publisher, main, trust anchors and physical-removal hold are retained.

**E4h boundary rechecked, 6 October:** [Committed capture audit](EXTENSION-GITHUB-CAPTURE-AUDIT.md)
records actual production-CLI refusal of the pinned legacy `af6e244...` catalogue:
its signed roots/index and addressed files do not include signed revocations or
the new publication receipt/history layout. The current development branch's v1
still shares that legacy layout. No signature, empty past revocation state or
release approval was invented. New raw-Git capture supports a complete authenticated
publication baseline; protected first migration remains Pending. The working edits
and two historical deletions below are preserved exactly; no registry artifacts,
current signatures, main publisher or production hosting changed. The original
dated audit below remains historical source evidence.

**Date:** 3 October 2026. **Application baseline:** `f305a745`, branch `extensions/tool-only-retirement`. **Registry:** `C:/Projects/Grain/grain/grain-extensions`, separate Git repository on `main`, HEAD `af6e24425d0eba1f667913f8a5403e9a6fb7ce76` (7 September). Its actual remote is [Punit-Dethe/Grain-Extention](https://github.com/Punit-Dethe/Grain-Extention). GitHub's current `main` matches that HEAD at inspection time.

**Scope:** source/metadata audit and release planning. No registry files, GitHub settings, workflows, credentials, signatures or artifacts were changed. No source build, extension execution, signing or publication was attempted. Companion: [ecosystem audit](EXTENSION-ECOSYSTEM-BLAST-RADIUS-AUDIT.md) and [forward execution order](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md).

## 1. Limitation resolved, with boundaries

The user's local-folder clarification resolves the previous source-access limitation. The configured `Punit-Dethe/grain-extensions` bootstrap returned 404 because it names a different repository from this checkout's remote. The correct remote is accessible and public. This establishes repository identity; it does not fix the app's bootstrap or certify a functioning release pipeline.

The separate registry working tree already contains edits to `CONTRIBUTING.md` and four `core/grain.voice-actions` files, deletions of one historical blob/Markdown asset, and an untracked `extensions/com.grain.tasks-example/` directory. These are pre-existing state. Inspect current files as a dirty snapshot, distinguish them from committed examples, and preserve them exactly. The outer Grain repository ignores this nested checkout. Do not accidentally stage it, commit on its `main`, restore its deleted assets or push its user edits while updating the application plan.

The graph returned no useful registry nodes, so source was read directly. No nested `AGENTS.md` was found. The parent repository's boundaries and the user's physical-removal hold remain applicable.

## 2. What exists

| Component | Current contents | Meaning |
|---|---|---|
| `extensions/` | Community source-pointer submission example plus the untracked Tasks native-tool draft | Retain the submission role; examples need current tools-only replacement and real authoring acceptance. |
| `core/` | Voice Actions, App Modes, Starter Prompts and Agent Center Layout; source manifests, packs, README and images | Historical first-party catalog/reference material, not current official tools-only integrations. Archive/migrate deliberately later; do not delete now. |
| `.github/workflows/` | PR build/check and main-branch publish YAML | A useful intended separation, with incomplete execution and release controls below. |
| `v1/` | Signed roots/index, blobs and content-addressed Markdown/WebP media | Reuse static signed distribution. Signature presence alone does not certify provenance, eligibility, file completeness or deployment. |
| `site/index.html` | Generated static shop window | Not the app's trust authority. Its old catalog content and repository links need coordinated migration. |
| README/contribution/review/labels documents | Pinned-source and human-review intent, broad retired capabilities, README listing convention | Preserve useful ownership/review concepts; rewrite the public author guidance in E3/E4. |

Catalog metadata at this snapshot: **spec 1, version 39, expiry `2026-10-07T12:30:05Z`, four entries**. Read-only checks found all four referenced local artifact files with matching SHA-256 and their referenced README files present. No signature or app acceptance verdict follows from those checks. Older unreferenced assets are not automatically eligible for deletion.

| Current catalog ID | Legacy behavior | Compatibility disposition |
|---|---|---|
| `grain.voice-actions` | Transcript transforms, app/URL opening, application capture and contributed settings | Retired permissions/host surface; current tool-only catalog/pack validation refuses it. |
| `grain.app-modes` | Transcript/context transforms, application capture, LLM and contributed settings | Retired permissions/host surface; refused. |
| `grain.agent-center-layout` | Pack tier and `agent.reply-surface` | Retired built-in ID/tier/visual contribution; refused. |
| `grain.starter-prompts` | Pack tier and `dictation.prompts` | Retired pack/prompt contribution; refused. |

All four catalog rows have empty `source_commit` and `reviewed_commit`. Community `com.example.hello` demonstrates the retired transcript-transform API and has an all-zero commit placeholder. It is not a current installable tools example. The Tasks draft is closer to the tools API, but is untracked, not submitted/published and has not been validated here. In particular, its own-storage usage needs permission/doctor/result-contract review; do not label it certified based on its directory name.

## 3. Live read-only GitHub metadata

The following were queried from the correct repository on 3 October. They are snapshots, not assertions about future configuration or secret values.

| Read-only endpoint | Observed result |
|---|---|
| Repository and `commits/main` | Public repository; default branch `main`; HEAD equals local `af6e244…`. |
| `actions/runs` | Three recorded runs, all `publish` on push, all completed with failure. |
| [Latest recorded publish run](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/30170348285), `…/jobs` | Failure at step 3, **Read submission.toml**. No signing/publication success is evidenced. Source shows the helper absent; raw logs were not inspected. |
| `releases` and `releases/tags/v1` | Release list empty; `v1` release lookup returns 404. Committed `v1/` files are not a published GitHub Release. |
| `branches/main/protection` | GitHub explicitly reports **Branch not protected**. |
| `rules/branches/main` | Zero effective rules returned. This checks rulesets as well as the preceding classic-protection response. |
| `environments` | `publish` exists, with empty protection rules and no deployment branch restriction. Its YAML name alone provides no reviewer gate. |

No secret values, signing-key files or account grants were read. Secret availability/custody, collaborator ownership, organization policies and a real successful deployment remain separate verification requirements. No GitHub administration setting was changed.

## 4. Concrete delivery findings

Findings below are source/API observations. Proposed repairs need their own implementation, tests and focused audit. This audit does not prove an active exploit or claim the incomplete workflow has ever signed an unreviewed artifact.

| ID | Evidence | Required change and acceptance |
|---|---|---|
| REG-01 — Bootstrap and serving paths | App `crates/grain-core/src/trust.rs` targets `Punit-Dethe/grain-extensions/releases/download/v1/`. Registry remote is `Grain-Extention`; signed roots name `raw.githubusercontent.com/Punit-Dethe/Grain-Extention/main/v1/`; no releases exist. | Choose and test one served layout with the full root → index/revocations → blob/media path. Update signed root metadata/bootstrap/rollout together. A repository-name substitution alone cannot certify the route. |
| REG-02 — Missing submission helper | Both workflows execute `./ci/read-submission.sh`; no tracked/local `ci/` helper exists. Latest recorded publication failed at this step. | Implement one bounded structured changed-submission resolver with pinned-source verification and distinct create/update/remove/multiple-change cases. No arbitrary output-to-shell interpolation. |
| REG-03 — CI/CLI invocation drift | Build workflow uses `grain-ext doctor --project source`; current CLI rejects any doctor argument and uses its working directory. It passes `grain-registry check-submission --dir extensions`, but that command appends `extensions/` to its argument. | Align working directories/options with actual CLI tests, including the expected registry root. Install/build pinned Grain tools and a specified JS toolchain; the YAML currently provides no reproducible setup for them. Test generated project through these exact commands. |
| REG-04 — Review-to-artifact evidence absent | PR checks upload only `ci-out/`, with no shown writer/build digest manifest. Publish checks out source again and packs it, but has no comparison with a reviewed PR artifact/digest and no explicit reviewed-source receipt. | Bind accepted source/manifest/lockfile, build identity and artifact digest to review. Use a no-secret build followed by independently verified handoff/signing. Recheck if source changes; never trust an artifact's name or a YAML comment as provenance. |
| REG-05 — Governance not enforced | Human review is documented; effective `main` protection/rules are absent; `publish` has no protection rules. Workflows use mutable action tags. | Protect required checks/source/workflow changes and the publishing environment, with a practical documented owner/approval policy. Pin vetted actions to full SHAs and minimize token scopes. Test denied/bypassed/changed-source paths before enabling publication. |
| REG-06 — Maintainer pack helper differs from author packaging | `grain-registry::build_pack` directly joins the manifest's `entry` to `src`, reads it, constructs empty payloads and validates. It lacks the explicit project-entry containment check and canonical icon embedding used by `grain-ext pack`; it does not run an author's build. | Reuse canonical validated packaging rather than a second looser implementation. Check traversal/absolute paths/symlinks, built-entry presence, icon/asset bounds and byte parity. Untrusted source processing must not share signing authority or expose checkout credentials. No traversal exploit was executed here. |
| REG-07 — Trigger and concurrency gaps | Both YAML path filters cover only `extensions/**`; `core/**`, tools/workflow changes and catalog expiry/revocation have no corresponding delivery path. Publish has no concurrency guard. | Decide supported source kinds and explicit renewal/revocation/manual release procedures. Serialize catalog version/sign/upload mutations; handle changed/deleted/multiple entries and failure/retry without rollback or overwriting newer metadata. No idle app polling is required. |
| REG-08 — Revocation and release completeness | Local `v1/revocations.json` and its signature are absent, though publish uploads them; there is no shown generation step. App paths expect `blob/` and `media/`, while release file uploads alone do not establish that URL hierarchy or update raw-branch files. | Produce and validate every required signed document and referenced asset; serve them at the verified URLs. Exercise empty revocations, actual revoke, malformed/missing file, offline/freeze and interruption. Never hand-edit signed files as a repair. |
| REG-09 — Description/media delivery incomplete | Publish passes no `--media-src`; its release list omits `v1/media/*`; both docs and producer still use README for listing text. | Migrate DESCRIPTION/summary/asset schema across CLI, submission, producer, index, app and public site. Verify content hashes and all served paths, not merely local file presence. |
| REG-10 — Provenance and trust metadata | All current rows lack source/review commit. Producer stores passed source commit but initializes `reviewed_commit` empty. YAML always publishes `verified`, while docs describe a separate automatic `core` path not implemented there. | Require exact reviewed identity/digest and controlled trust assignment for native packages and MCP descriptors. Keep first-party provenance separate from self-declared reserved IDs. A reviewed descriptor does not certify immutable remote code/tools. |
| REG-11 — Submission/public documentation mismatch | Current checker mostly checks nonempty pointer fields, ID/category vocabulary and typosquat; it does not prove tag/commit ownership/build identity. Examples/docs still permit prompt packs/context/OS access and say `submit` opens a PR although the current CLI writes submission files/instructions. | Rewrite supported types and examples, reject legacy/placeholder submissions, enforce source/tag/commit/version/license/namespace rules at authoritative boundaries, and describe what commands actually do. Do not promise zero CI round trips or publication from doctor alone. |
| REG-12 — Signing cleanup and expression inputs | Key file creation and removal are in one sign step; removal follows the publishing command and can be skipped on failure. Source metadata is embedded into the shell through expression substitution. | Use scoped secret-file permissions and unconditional owned cleanup, bounded/validated inputs and structured arguments/intermediate environment variables. Test injected/invalid metadata and sign/upload failure with disposable keys; never use production keys for harness experiments. |

These are requirements for **E3/E4/E8**, not a new broad rewrite of the accepted runtime. REG-06 merits a focused shared-packaging audit before any producer is allowed to sign current third-party projects. The missing helper and failed live runs already prevent a production-readiness claim; repairing only that first error would expose later unresolved stages.

## 5. Primary security references and modest reuse

The existing [pinned PR workflow](https://github.com/Punit-Dethe/Grain-Extention/blob/af6e24425d0eba1f667913f8a5403e9a6fb7ce76/.github/workflows/build-and-check.yml) and [pinned publisher](https://github.com/Punit-Dethe/Grain-Extention/blob/af6e24425d0eba1f667913f8a5403e9a6fb7ce76/.github/workflows/publish.yml) establish intended separation, not working enforcement. The application tools above establish the actual invocation/packaging contract.

[GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use) supports least-privilege tokens, full-SHA action pinning, careful treatment of untrusted checkouts/artifacts, workflow ownership and avoiding direct context-to-shell interpolation. A trusted parser still needs containment when consuming author-controlled paths. Egress filtering alone does not prove that code cannot tamper with a later signing stage.

[GitHub environment documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments) distinguishes naming an environment from configuring required reviewers/branch restrictions. The read-only environment response supplies the actual missing settings here; no additional approval platform is needed merely to enforce the chosen release review.

[Artifact attestation concepts](https://docs.github.com/en/actions/concepts/security/artifact-attestations) and [usage](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations) provide an established optional provenance mechanism. Start with exact source/run/artifact-digest validation; adopt attestations if useful rather than inventing a parallel trust service. Attestation does not replace a Grain catalog signature, source review or current MCP/account checks.

The broader [multi-host comparison](EXTENSION-ECOSYSTEM-BLAST-RADIUS-AUDIT.md#4-reference-check-and-what-to-borrow) still supports native/MCP adapters plus custom configuration. No new gateway, agent framework, registry server or permanent background engine is selected.

## 6. Keep, change, hold

**Keep:** repository history; reviewed-source-pointer role; signed static distribution/client verification; trust separate from manifests; existing packaging/installation/store/registry harness coverage; useful labels and explicit incident/review ownership.

**Change in E3/E4:** public examples/docs, native/MCP descriptor submissions, structured source validation, canonical pack generation, build/review/sign handoff, protections/permissions, metadata provenance, DESCRIPTION/assets, bootstrap/serving and renewal/revocation/concurrency procedures. Coordinate producer and app format migration. Do not switch URLs silently before owned compatibility tests pass.

**Hold:** physical obsolete-file/artifact/repository deletion, publishing production catalogs, root rotation, deployment settings mutation and promotion of old examples. Preserve the user's existing edits. Future registry implementation uses a separate appropriate branch in that repository; this task authorizes inspection/documentation only.

## 7. Acceptance order and tests

1. **E1/E3 contract and source boundary:** settle small manifests/descriptor/config identity; accept generated examples and exact CLI/checker behavior. Add submission parsing/quoting/ownership and pack entry/asset containment tests. Explicitly reject all legacy examples as new submissions; keep them as inert migration fixtures where needed.
2. **E4 unprivileged build:** disposable valid/invalid native and MCP-descriptor projects, pinned toolchain, wrong commit/tag, two submissions, changed source after review, dependency failure and denied path traversal. Record exact metadata/digest and bounded reports. No production signing secrets or personal service accounts.
3. **E4 signing and hosted-path fixture:** an isolated publishing context with disposable test keys; refuse missing/mismatched review/build/digest; validate signing failure cleanup, serialized versions and complete blob/media/document paths. Load/install/update/revoke through the maintained real Grain store/account boundary with existing test anchors. Do not create a production GitHub Release to run this fixture.
4. **E8 administration and final deployment:** verify enforced review/check/branch/environment ownership, secure key custody and actual successful controlled release/serving. Production publication/administration changes require their own concrete reviewed task. Repeat applicable offline/rollback/freeze/update/revocation and real-app resource/UI evidence.

Reuse `tests/agent-harness/packaging.mjs`, `installation.mjs`, `registry.mjs`, `store.mjs` and their production-logic checks. Add narrowly scoped pipeline fixtures only when implementing this lane; no speculative runner is created now. A passing client signed-store fixture is not a passing external publishing workflow.

**Ledger unchanged:** 52 baseline Pass / 1 Deferred (12), forward 54 Pass; 89 scenario IDs / 74 self-contained / 70 runner self-tests. No new human test batch. Existing auth/cleanup/conformance findings and R0–R6 gates remain as recorded. Immediate runtime continuation remains the separate authentication recovery/status unit; the E4 source-access prerequisite is now resolved and concrete publishing work is mapped, not accepted.

## 8. Reproducible snapshot

Read-only evidence used: `git status --short --branch`, `git log -1`, tracked-file inventory, the two workflow files, submission/author/review documents, current catalog/roots and app CLI/producer/consumer source; GitHub repository/commit/runs/jobs/releases/branch-protection/effective-rules/environment metadata. No raw workflow logs or secret values were collected.

SHA-256 fingerprints of inspected public files, including current dirty contribution guidance:

| File (registry-relative) | SHA-256 |
|---|---|
| `.github/workflows/build-and-check.yml` | `a20f7162678b0e13665e4f50d0e4cc821ad134887b03934f479a54abfa2ac79c` |
| `.github/workflows/publish.yml` | `278defce64d9e642e9369952edaa3453b9b816faa691fce84a323ee09d15ca2a` |
| `v1/index.json` | `ee585292aa8aec4a15f2c546ef9de6cefe32cb5401d6d98b56de02e16875601b` |
| `v1/roots.json` | `5b1f7cf03f47eb0a63945279933715802b1104d89c7df7e991a416ee608563d0` |
| `CONTRIBUTING.md` | `f1b0ac4fd8b0d3ddb93c543d195f285567ebe316e392304ffb156239cfb544d6` |

These fingerprints establish inspection identity, not a new signed release or an immutable record of GitHub administration settings. Refresh the relevant observations before implementing or certifying E4/E8.
