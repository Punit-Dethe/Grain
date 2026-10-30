# Historical version reservation and selected snapshot export — E4d

**6 October follow-up:** [E4e hosting-bundle audit](EXTENSION-HOSTING-BUNDLE-AUDIT.md)
adds a separate archive-complete producer/verifier for retained native/MCP assets
and signed historical proofs. The single-snapshot export documented below stays
unchanged. Actual hosted activation and protected release governance remain gated.

6 October 2026. This follows the [E4c local serving audit](EXTENSION-SERVING-PROMOTION-AUDIT.md).
The historical reservation/export primitive is accepted within the local
operator-owned store boundary. **E4 and production hosted publication remain
open.** No desktop, UI, Agent runtime, auth, SDK or harness runner changed.

## What was delivered

After withdrawal, the immediate current catalogue no longer contains the old
version. Promotion now checks retained signed snapshot catalogues before allowing
that version to return. Different package/descriptor, artifact type, description
or ordered media hashes refuse; exact restoration or a new version is allowed.
The current pointer stays unchanged on refusal. No second mutable history
database, background process or additional dependency was introduced.

`export-serving-snapshot --store PATH --out FRESH_PATH
--expected-current-sha256 DIGEST` captures the independently pinned selected
snapshot under the same OS lock. It validates strictly fresh metadata and complete
addressed bytes, copies them using existing streaming bounds, rechecks the result
and records an unsigned `snapshot.json` receipt. The output's `v1/` is independently
verifiable by the existing CLI. A later promotion leaves old exports unchanged.

Source: Grain `0abc271736ec8ff959b1d5e39dfeb31e9f98347d`, registry
`a952e43a6c24b3fe2b61944d867215a4bde902e6`, on the existing dedicated branches.
The source and CI are pinned to these exact revisions. The original shared
current-snapshot verification is now one helper used by promotion and export;
existing schema-1 pointer bytes remain compatible. Snapshot staging is dropped
before its parent directory is synchronized.

## Security, resources and deliberate policy limits

Promotion holds the existing nonblocking exclusive local OS lock while it reads
the current pointer, validates its base, authenticates history and installs/selects
the next snapshot. Historical roots/index/revocations use the actual app-pinned
chain. Old metadata may be expired only as historical identity evidence; it is not
exported or selected as fresh publication. The complete metadata/reference
inventory hash must match its content-addressed directory name.

Historical identity checks read one bounded catalogue at a time and do not reread
old packages or retain a cumulative cache. Limits are 4,096 installed snapshots
and 128 MiB combined signed metadata inspected in one promotion. Count exhaustion
refuses before installing a further snapshot. An audited future archival method
must preserve version identities; silently deleting snapshots or bootstrapping a
fresh store is not a valid workaround. These are explicit maintainer limits, not
an unbounded production-lifetime claim.

Valid installed but unselected snapshots conservatively reserve their identities
after an interrupted operation. That does not select them or confer review
authority. Unsigned `.tmp*` staging is ignored, retained and never used as history.
Links, unexpected names, invalid historical signatures and identity mismatches
refuse. The local store must remain append-only and operator-protected: an
arbitrary same-account writer deleting or renaming history is outside this trust
boundary. Bootstrap validates its supplied catalogue, not missing history from
before that store was initialized. Existing full release-history migration stays
a protected deployment requirement.

Export requires a fresh external output and the independent entire-pointer digest.
Stale/malformed pointers, lock contention, expired metadata and changed selected
bytes refuse. In-progress output uses the existing RAII cleanup; no input or
previous export is deleted. `snapshot.json` is operational evidence, never approval
or signature authority. No network, keys, author execution or model is used.
Export does not mutate the current pointer or deploy hosting.

## Verification and scoped audit

Windows affected suite: **15/15 Rust Pass** (the ten earlier serving tests plus
five new historical/export tests). Locked production build, workspace format
check and all-target registry Clippy with `-D warnings` pass. New tests verify:

1. Actual install → withdrawal → changed-code restoration refusal, unchanged
   pointer, exact-byte restoration and old snapshot retention.
2. Historical description/media/type reservation, including an installed orphan;
   a new version remains admissible. These tests isolate signed metadata identity
   admission, not semantic validation of hypothetical package bytes.
3. Invalid historical signature/directory identity and unexpected path refusal;
   unsigned abandoned staging stays inert and retained.
4. Complete exact export, unsigned receipt/pointer binding, contention/stale/
   overwrite/containment refusal and stable old export after actual promotion.
5. Corrupt or aged selected snapshot refusal without output or pointer mutation.

Actual Windows public CLI: **25/25 separate expected verdicts**, retained in
`.runs/e4d-history-export-01/`. Sixteen existing assembly/store/refusal checks are
reused, with nine new export/verification/refusal verdicts. Exact six signed
metadata files and receipt state are also compared. Independent Python OS locking
blocks export as well as promotion; stale/invalid digests, existing output,
inside-store output, corrupt selection and traversal all refuse with no unfinished
export. CLI bootstrap uses genuine app-pinned empty seed signatures. Changed
nonempty promotion/history uses real component signatures with disposable roots;
neither substitutes for protected positive live publication.

Windows executable SHA256:
`b948c6ace97366818cc093236cef4c4e62435dc21f7e076d34efcb5baf8ebbae`.
Independent inspection of 25 recorded command PID references finds zero remaining
owned CLI processes and zero keys in the evidence directory. Test-only signing
keys remain RAII-owned temporary fixtures. Nonsecret scripts/reports/snapshots are
retained disposable evidence, not file-deletion Pass.

Linux [CI 37418989531](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37418989531)
is **successful**: **16/16 affected Rust tests**, including Unix symlinks, locked
production CLI build and **25/25 actual CLI verdicts** with independent
`fcntl.flock` contention. Every required job step passed at the recorded exact
source pins. The existing branch-only workflow was extended, not a new
runner/framework: pinned Rust 1.96.0, exact application source and locked dependencies.
Permission remains read-only contents; no author builds, OIDC, secrets or hosting.

Linux executable SHA256:
`297bffe0c75d30df52afdfdc0cb55b58041ca21b622ed9d7fe668b47cf8613a2`.
Evidence artifact `11392139645`, SHA256
`94d17900fc6625d978642550bb5036a705fefd48ef042e5692dc58b78e568d97`,
has seven-day retention. `.runs/e4d-history-export-linux-01/` retains its reports,
logs and nonsecret snapshots, inspected as data without executing downloads.

Graph-first minimal context, file summary, impact, detected changes and review
context were used before direct source review. Graph test-gap hints are not
acceptance evidence: actual Rust tests and public CLI invocations cover the changed
helper/transition/dispatch paths. Scoped audit inspected withdrawal/orphan policy,
signature/naming bindings, history budgets, no-cache streaming, lock lifetime,
pointer compatibility, output cleanup and unsigned receipt boundaries. No failed
test or production defect was encountered in this block; audit refinements received
the final affected verification. Existing root bindings and nested registry edits
are preserved.

## Plan, deviations, cleanup and next work

This implements the historical identity safeguard identified in E4c and a bounded
local export for a future hosting adapter. It keeps the existing CLI/static signed
distribution architecture. The deliberate refinement is conservative reservation
of signed installed orphans and bounded metadata scanning rather than building a
new history service. Exact restoration is allowed under Grain's policy; the
[npm unpublish policy](https://docs.npmjs.com/policies/unpublish/) illustrates a
stricter immutable-version registry precedent. Immutable snapshot capture follows
the consistency principle documented by [TUF](https://theupdateframework.github.io/specification/);
this code remains Grain's existing signing format and is not TUF certification.

Remaining E4 work: hosting-specific conditional activation, coherent metadata/key
rotation and stable historical content-hash availability; protected migration of
prior registry history; actual independent protected review-to-signing, enforced
branch/environment/workflow ownership and production key custody; operational
renewal/revocation. Export includes only the selected snapshot, not an aggregate
archive of withdrawn assets. A deployment adapter must retain that archive and
must not rewrite signed base URLs or treat the local file lock as distributed
coordination. No deployment identity is invented or activated here.

The existing draft PR remains unmerged; the active incomplete legacy publisher and
missing-helper legacy check are untouched. No main/admin/key change, obsolete-code
deletion, public contract freeze or new human batch. Existing baseline
**52 Pass / 1 Deferred (12)**, full **56 Pending**, latest numbered forward **71**
and inventory **108 / 93 / 81** stay unchanged. **Five E4–E8 stages retain work.**

Keep product tests. `.runs/e4d-history-export-01/` and downloaded Linux evidence are
disposable nonsecret retained files; delete only as a separately scoped cleanup.
The branch-only Linux wrapper retires when its assertions enter supported
maintainer CI. Existing signed-store suites will run at actual host/serving
integration, not for this unchanged desktop. Earlier audit entries are historical.
