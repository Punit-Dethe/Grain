# Genuine registry CI provenance checkpoint — E4b3

**Historical schema-1 fixture checkpoint.** The [6 October remote source/review
audit](EXTENSION-SOURCE-REVIEW-BUILD-AUDIT.md) supersedes the current signer policy:
schema 2 requires actual merged GitHub source/latest independent approval. Earlier
two positive fixture signings below remain valid evidence for their recorded
version, not live positive acceptance of the new review gate. Real remote source/
build and both attestation paths now pass; protected production review remains Pending.

**6 October 2026: controlled native/MCP CI-to-signing cryptographic checkpoint
passes. Protected production human review, real author builds and release
governance remain Pending.** This closes the previous missing genuine positive
attestation evidence within fixture scope; it does not certify production
publication or complete E4. Forward 71 remains the latest numbered product
checkpoint. Five E4–E8 delivery stages retain work.

## Implemented and run

The correct separate registry repository now has branch
`registry/trusted-provenance`, HEAD `d3468cea04eaf83a6f7a74a59710c5302d2768f2`:

- `candidate-provenance.yml`: a read-only preparation job without OIDC or
  publishing credentials, followed by a separate attestation job with only
  contents-read, OIDC and attestation-write authority. No release, environment or
  repository secret is used. Checkout credentials are not persisted.
- Every external action is pinned to an immutable commit. Preparation builds
  Grain `1be8a2c0b6f657461aebe5291a20ec379d23135b` with Rust 1.96.0 and locked
  dependencies. GitHub's current maintained `actions/attest` v4.2.2 generates
  standard build provenance; no custom predicate or cryptographic implementation.
- The small fixed data generator uses actual `grain-ext init`/`submit` and
  `grain-registry prepare-artifact`/`prepare-catalogue` for both kinds. It runs no
  extension/build script; the native fixture's deliberately failing build script
  is never invoked. Temporary source Git repositories are disposed after output.
  Example origins are not claims of remote source ownership or human review.
- The attestation job downloads candidate data and executes no artifact code.
  The genuine bundle covers both candidate JSON files and producer metadata.
  Public CI artifacts are retained for seven days.

[Actual successful run 37412601265](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37412601265)
completed both prepare/attest jobs. Artifact `attested-candidates` ID
`11389975876` is 15,878 bytes; `checked-candidates` ID `11389099761` is 8,494 bytes.
Producer executable SHA256:
`17822d7bcb9cdcd96b505a1cca14068a91be8aa7cb2a4ab0b4d213a6021fc33f`.

The Windows signer now accepts ordinary scoped branch refs while rejecting empty,
traversal/hidden/lock/trailing-dot components, invalid characters, wrong prefixes
and excessive length. Paths are still fixed verifier arguments, never shell code.
This repairs the earlier single-component policy restriction without relaxing
the exact certificate source-ref comparison.

## Verification and focused audit

**24/24 affected registry Rust tests Pass**, including the new scoped-ref matrix
and existing real minisign, policy, byte, prior-index and owned-process tests.
Locked build, Clippy, formatting and scoped diff checks pass; three existing
Clippy warnings remain, with no new warning. No app/SDK runtime, frontend or
harness source changes, so no redundant desktop/Agent batch or unaffected broad
suite was added. Local fixture generation passes through the real CLIs.

**13 actual local command verdicts Pass:** independent official-GitHub verification
of the attested producer metadata, disposable key generation/prior-index signing,
two positive full `sign-reviewed-candidate` invocations and eight identity
refusals. Both native and MCP use the genuine downloaded CI bundle. Wrong source
commit, signer commit, branch and workflow each fail cryptographic verification
before a deliberately missing key or any output. Producer metadata pins are used
only after its independent exact-workflow/source/commit/ref verification.

Positive commands create verified signed index version 8 with the expected source
commit, fixture submitter and exact artifact/DESCRIPTION hashes. The product's real
index verifier checks signatures inside the signer; the previous index stays
unchanged. Keys/public policy here belong exclusively to disposable fixtures, not
the app's deployed trust root. No generated output is installed or uploaded.

Graph-first exploration and scoped detect/review report risk 0.60/low and no
indexed outside flows. Their coverage gaps are not an audit verdict. Direct
workflow/helper/diff review checked job permissions, action/toolchain/source pins,
absence of secret/key/environment/release authority, no execution of downloaded
data, exact subject identity, bounded command lifetime and source/output ownership.
The first CI run [37412493466](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37412493466)
failed workflow validation because runner context was unavailable in job-level
environment expressions. It gets no acceptance credit. The path now resolves in
the build step; the corrected run passed. No bypass or fake signature was added.

Ignored retained evidence lives under
`tests/agent-harness/.runs/e4b3-real-provenance-01/`: received bundle/data, public
policies/signatures, logs, report and cleanup inspection. `check.mjs` is a one-off
disposable evidence script, not maintained harness infrastructure. Disposable
key files are removed in `finally`; independent Win32 inspection of all 13 command
PIDs/remaining children finds zero owned processes. No app profile, account or
listener is created. Retained fixture/report files are not file-cleanup Pass.
The local generator smoke output `.runs/e4b3-ci-local-fixtures-01/` is also disposable.

Final Windows registry executable SHA256:
`b78f71f9982ca68760bfedcadb1ba1288ed8a23d40722dcd02b68549d70d5a88`.
Official `gh` 2.100.0 executable SHA256:
`2ae2b350c227a618f2d8965b1900aeee13446ff42e17ef0bd5a0b6405c593cfb`.

## Scope, remaining work and cleanup

The workflow runs fixed controlled data, not arbitrary author projects. Fixture
reviewer/submitter policy is deliberately not production approval. Next implement
actual pinned author build/source handoff and protected, independently derived
review policy, then complete-path serving/promotion/renewal/revocation acceptance.
The prior unsigned receipt still cannot prove remote source ownership by itself.
Do not promote a name in a JSON policy into evidence of a real approved review.

Read-only GitHub inspection on 6 October still finds zero effective `main` rules
and an unprotected `publish` environment. The legacy publisher is **active in
GitHub**, although incomplete; it was not triggered by these isolated branch
pushes. Its files, main, settings, production keys and release state are unchanged.
This checkpoint does not disable it or establish protected publishing governance.

Maintenance/removal ledger: `ci/provenance/fixtures.py` is a narrow temporary
checkpoint producer, not a new Agent harness. Retire its branch-push trigger and
obsolete fixture preparation when real reviewed author builds replace this lane;
keep only a useful small manual provenance regression. Deliberately update the
pinned Grain revision at a maintainer contract checkpoint. Track the fixtures as
temporary infrastructure rather than letting them become a general testing app.
See [registry CI documentation](../../grain-extensions/ci/provenance/README.md).

Pre-existing nested registry edits/deletions/untracked Tasks draft are preserved
and excluded from both registry commits. `src/app/bindings.ts` stays unstaged and
byte-identical (SHA256 `2bd7fc3f77443d7317375fdff8441cac2733f228aa1c6aafe844ffd21809bac`).
No obsolete capabilities/assets are physically removed. Baseline **52 Pass / 1
Deferred (12)**, full **56 Pending**, Agent inventory **108 / 93 / 81**, public
OAuth hosting/release gates and no-new-user-test-batch status remain unchanged.

Primary references: [maintained attestation action](https://github.com/actions/attest/tree/1e69f48acb82d1966a394da916b4c1698aa569d6),
[GitHub attestation verification](https://cli.github.com/manual/gh_attestation_verify)
and [trusted builder isolation](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating).
