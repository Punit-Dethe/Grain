# Reviewed catalogue signing boundary — E4b3

**Historical local boundary.** The [6 October remote source/review audit](EXTENSION-SOURCE-REVIEW-BUILD-AUDIT.md)
adds mandatory schema-2 policy fields and actual GitHub merged-source/latest-review
reads after provenance and before keys. Schema-1 policies described below are no
longer accepted by the current signer. Historical local/component evidence remains;
live positive protected review-to-signing is still Pending.

**6 October follow-up:** [Genuine CI checkpoint](EXTENSION-CI-PROVENANCE-AUDIT.md)
now passes both-kind positive signing and eight identity refusals with the actual
registry's GitHub bundle. Scoped branch names are supported. Protected production
review and real author-build/release governance remain gated. The 5 October local
evidence and limitations below are historical; its missing genuine positive
attestation requirement is closed within controlled-fixture scope.

**5 October 2026: local maintainer signing boundary implemented and audited.
Positive trusted registry CI/review-to-signing acceptance is still Pending.**
Forward 71 remains the latest accepted product checkpoint; E4 is not complete.
Five E4–E8 delivery stages retain work. No production upload, workflow activation,
repository settings/key mutation, physical removal or new manual test batch.

## What changed

Added `grain-registry sign-reviewed-candidate`, reusing the existing shared
submission/receipt/artifact/listing checks and catalogue staging. A protected,
independently digest-pinned policy binds source submission, receipt, producer,
candidate, verifier executable, exact registry/workflow identities, reviewer,
submitter, approval window, previous index and publishing public key.
The command regenerates an owned candidate snapshot and compares its complete
file inventory with received bytes; it does not accept author-supplied trust.

Official `gh attestation verify` handles Sigstore/GitHub cryptography with fixed
identity/commit/ref/predicate/issuer policy. No second cryptographic implementation,
transport/auth engine, application dependency or testing framework was introduced.
The verifier child has bounded output/deadline and isolated home/config/environment;
its own process is killed/reaped on monitoring failure. Signing keys are opened
only after public-route policy/source/candidate/prior-index/provenance gates.

The new fragment contains content-addressed blobs and a version-incremented signed
index. Trust becomes verified, not core. Existing versions cannot be reissued.
The real Grain verifier validates the signature with the independently approved
public key before output creation. The previous catalogue is never overwritten.
Full hosting/promotion, old referenced blobs, roots/revocations and deployed key
alignment remain separate E4 work. See [maintainer usage and policy contract](../../crates/grain-registry-tools/README.md#reviewed-candidate-signing-e4b3-local-boundary).

## Audit and corrections

Graph-first minimal context/search/impact preceded raw reads. Scoped
`detect_changes` and `get_review_context` report risk 0.40/low and no indexed
external flow. Their graph does not cover the new signing module completely;
test-gap counts are not a correctness verdict. A direct scoped source/diff audit
checked policy authority, candidate reconstruction, read ordering, key access,
bounded file/process lifetime, signature verification, immutable versions and
previous-output preservation.

The audit found that typed reserialization would strip unknown forward-compatible
fields from previously signed catalogue entries/envelopes. Corrected the signer
to retain their original JSON values when appending the new entry; native and MCP
regression checks include unknown envelope and entry metadata. Review dates are
normalized to UTC, and submitter attribution comes from protected review policy
rather than guessing it from repository ownership. Snapshot staging consumes the
same checked submission whose full digest was approved, avoiding an unnecessary
second submission read. Policy expiry is checked again before key access.

Protected policy/verifier/key locations and their parents must be immutable and
operator-owned. This tool does not provide an OS sandbox or authenticate a human
review merely because a policy contains their name. The next trusted workflow
must derive policy from real protected review evidence and reproduce the pinned
author checkout/build, not merely attest an uploaded author receipt. The signer
profile currently permits a single-component registry branch and same-repository
workflow. Production key custody, trust-root deployment, serialized promotion and
positive workflow provenance are not accepted by these local checks.

## Verification and evidence

- Locked component suite: **149 Rust tests** across CLI 13, shared checker 28,
  registry 23 and SDK 85. The first broad run had registry 22; the added process
  cleanup test and audit corrections passed the final 23-test affected repeat.
- Real disposable minisign keys and actual `grain_core::trust::verify_index`
  exercise both native/MCP signed updates, review metadata, prior-entry/unknown-field
  retention, monotonic version, wrong key, invalid prior signature/version and
  refusal of repeated published versions without output.
- Policy matrices refuse bad schema/digests/identities/key/approval windows and
  unknown fields. Exact-inventory tests refuse changed bytes, extra files and
  nested trees; executable/file admission is bounded. Fixed verifier arguments and
  environment are checked. Actual owned test-binary children are killed/reaped on
  deadline/output-budget failures; they are not fake provenance evidence.
- Final actual CLI run uses **18 commands / 14 expected signing refusals** across
  native/MCP: invalid GitHub bundle, wrong policy pin, expiry, wrong verifier pin,
  wrong approved submission, extra candidate file and forged trust. Both real
  installed `gh` calls refuse before a deliberately nonexistent signing key;
  no output is produced and the prior index remains unchanged. Positive GitHub
  attestation acceptance is **not** claimed by component signing tests.
- Locked build, formatting, scoped diff checks and Clippy pass. Clippy retains
  three pre-existing diagnostics (two CLI conversions, one registry doc indent);
  no new warning is introduced. No host/frontend contract changed, so no redundant
  desktop rebuild, visual automation or Agent model run was added.

Retained ignored evidence: `tests/agent-harness/.runs/e4b3-signing-local-04/`
contains report/logs, policy and nonsecret copied fixtures. The one-off script
lives in `.runs/e4b3-signing-local/check.mjs`; it is disposable evidence, not a
maintained runner. Initial fixture run stopped on the correctly enforced extension
folder-ID invariant and gets no acceptance credit. `-02`/`-03` are earlier successful
local runs before the final audit/submitter schema; `-04` binds final executable
bytes. Disposable fixture key files are removed in `finally`; retained public
policy/index signatures are not account credentials. Reports list command PIDs;
completed CLI children have exited. Independent Win32 process inspection of all
18 recorded command PIDs and their remaining children finds zero owned processes.
Final registry executable SHA256 is
`33e0bf19b0695dc05d852d0d608febd976982e63bd78c6d1981c28c48354e012`;
the verified official CLI executable SHA256 is
`2ae2b350c227a618f2d8965b1900aeee13446ff42e17ef0bd5a0b6405c593cfb`.
No app profile/account/listener is created.
Retaining scratch reports/fixtures is not a file-cleanup Pass.

Unrelated `src/app/bindings.ts` remains byte-identical (SHA256
`2bd7fc3f77443d7317375fdff8441cac2733f228aa1c6aafe844ffd21809bac`), unstaged.
The nested `grain-extensions/` repository/workflows and its existing dirty files
are untouched. Agent harness source/scenarios/inventory remain **108 / 93 / 81**.
Baseline **52 Pass / 1 Deferred (12)** and full **56 Pending** remain unchanged.

## Next coherent checkpoint

Finish E4b3 with the correct nested registry repository: pinned actions/toolchain,
unprivileged author build separated from trusted candidate/attestation steps,
protected human review and signing environment, independently supplied policy/
verifier pins and a genuine positive workflow attestation for both artifact kinds.
Verify negative workflow/source/review identities and owned cleanup there before
granting full CI-to-signing acceptance. Then complete E4 serving/promotion,
renewal/revocation and author submission enablement; E5 owns management/store UI.
Existing legacy publisher remains excluded for new submissions, with physical
deletion held. Public OAuth hosting and deferred live Linear expiry stay separate.

Primary references used: [GitHub CLI attestation policy](https://cli.github.com/manual/gh_attestation_verify),
[isolated trusted builders](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
and [workflow security](https://docs.github.com/en/actions/reference/security/secure-use).
