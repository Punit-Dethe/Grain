# E4i: authenticated legacy catalogue history preservation

**7 October follow-up:** [E4j migration audit](EXTENSION-LEGACY-MIGRATION-AUDIT.md)
records the completed archive consumer/reservation implementation. Protected
production signing/approval, activation and hosted-client checks remain open.

7 October 2026. Bounded maintainer implementation and scoped audit; **not a
production migration, independent release approval or activation**.

## Outcome and scope

`grain-registry capture-legacy-history` preserves the actual earlier signed
four-document catalogue history through pinned public tip
`af6e24425d0eba1f667913f8a5403e9a6fb7ce76`. This closes the E4 prerequisite of
recovering old signed identities and their referenced bytes. A protected
migration consumer and a reviewed/signed current revocation baseline remain
required before this history can participate in active publication.

The real capture contains **18 proofs / 27 content variants / 8 conflicting
`(id, version)` identities / 37 addressed assets**. All **109 proof/asset files**
are compared byte-for-byte with exact source Git objects. Capture data including
the manifest is **1,326,780 bytes**; the receipt has its own 8 MiB bound.

Historical manifest:
`crates/grain-registry-tools/fixtures/legacy-history-af6e244.json`;
SHA256 `6cad006f21b54e7041921dd68db0f8acf30932c0a8617a82a3b671a04a1b56f3`.
Archive receipt:
`25f8e8cb9484a0301ec5bdb34bb9eaf4fbbc82a4f84bf1c196c2eed1338a7532`.
These are evidence pins, not release authority.

## Discoveries and deliberate adjustments

1. The first signed index references package
   `10046c61be5d612027adf58abdae14e8e5511f7b69826e4b913f03163880bc66` before its
   blob appears in the repository. Exact bytes are present in the next pinned
   catalogue commit `104351a27521d1a6d822b589239da03f0ace5439`. Recovery uses only
   the pinned history pool, verifies the original signed digest/size and records
   that actual source. The first proof records one recovered asset; no claim
   that the original tree was complete.
2. Earlier documents use retired presentation tier `builtin`, rejected by the
   current SDK. A narrowly documented publisher-signature-only helper reuses
   existing crypto to authenticate **raw** historical bytes first. Only a
   temporary archival parsing view maps `builtin` for shared asset enumeration.
   Raw signed documents stay unchanged; ordinary `verify_index` and the SDK
   still reject that tier. No old runtime, privilege or installation is enabled.
3. Historical contents changed under eight extension version identities. The
   archive preserves **all** artifact/listing/media variants and names conflicts;
   it does not silently choose a winner or import them into active metadata.
   A migration consumer must reserve these identities and reject their reuse.
4. No legacy signed revocation document exists. The format is explicitly
   four-document historical proof, with no manufactured signatures or invented
   empty historical revocation state. If signed revocation paths appear in this
   input history, capture refuses rather than discarding them.

These adjustments follow actual signed data rather than widening active client
admission. New-layout `capture-github-publication` continues to refuse an
incomplete legacy baseline.

## Contract and resource ownership

Strict pinned manifest: schema 1, actual full SHA1 tip, exact ordered unique
catalogue-changing commits, at most 128. Compare to bounded complete reachable
Git history of roots/index/revocation documents and signatures, including merges
and signature-only changes. No manually selected omissions or shallow history.
Completeness is relative to the chosen pinned tip and available standalone Git
repository; it is not remote branch protection, approval or coverage of all refs.

Reuse existing hardened local Git subprocess runner, standalone repository and
origin checks, allowlisted content-address paths, raw binary batch materializer,
app-pinned root verification and streaming asset hash/size checks. No author
execution, build, archive filters, fetch, credential access, signing or pushing.
Git replacement objects are disabled; grafts, alternates, shallow/promisor state,
includes/filters/redirecting configuration and unsafe file modes are rejected.

Bounds: 128 commits, 8,192 addressed assets and identity variants, 131,072
reservation references, 128 MiB signed document data, 1 GiB captured data plus
8 MiB receipt. Document/artifact bounds and existing Git 30-second/output limits
remain. Binary assets stream; bounded maps and document buffers exist only during
the maintainer command. Git children are waited/reaped; owned scratch closes and
only newly created incomplete output is removed. Existing directories, dirty
source files, branches, old artifacts and main publication remain untouched.

Output is structurally separate from serving: `proofs/`, `assets/`, `manifest.json`
and `legacy-history.json`; no selected pointer or active `v1/`. The unsigned
receipt labels historical evidence/no revocation proof. Recovery records source
commits; unused committed assets remain in Git and are not copied. Assets available
only in other refs or non-catalogue-changing trees require a separately reviewed
recovery design; this command does not scan arbitrary refs or rebuild missing bytes.
Same-account filesystem/Git executable integrity remains an operator assumption,
not a sandbox claim. Windows directory sync remains visibility rather than a
certified power-loss guarantee.

## Verification and focused audit

Windows: **80/80 full maintainer tests**, **9/9 existing core trust tests**, locked
production CLI build, strict package Clippy for all targets/no dependencies,
scoped formatting and **12/12 public CLI verdicts** Pass. New eleven Rust tests
use actual minisign signatures, bounded fixture Git histories and disposable test
anchors; no public CLI trust override or production key is created. Tests cover
expired/raw/withdrawn/conflicting proof, exact ancestry/manifest/pin, signature and
publisher mismatch, missing/corrupt assets, explicitly sourced recovery, unsafe
modes/revocation evidence, retired tier versus active rejection, address
deduplication, unknown formats/fields, output boundaries and shallow/origin guards.

The actual public CLI lane uses the real app-pinned legacy trust chain and
independent fixture manifest pin. It compares all 72 signed-document files and
37 asset files to their exact committed source objects; refuses malformed/mismatched
pins, wrong origin, unsafe destinations, omitted/reordered/duplicated history,
unknown manifest authority and a missing tip. Source HEAD/status and preserved
receipt stay exact; no `.key` files or hosted changes. The shared small Python
checkpoint lives beside the maintainer CLI and runs on both platforms, not in
the maintained Agent runner. It stays useful for supported legacy migration;
no new Agent scenario, general framework, desktop automation or manual batch.

Evidence: `tests/agent-harness/.runs/e4i-legacy-history-01/` (ignored), final
`cli-02/report.json`, component/core trust/Clippy logs. An initial CLI fixture
path typo left an empty nonaccepted `cli/` scratch directory; earlier missing
asset/tier refusals prompted the documented fixes. No failed attempt is counted
as an accepted verdict. Linux source-pinned checkpoint is recorded below after
successful execution and independent artifact/source verification.

### Accepted cross-platform checkpoint

Source implementation `223614dd274b6568435dd9e1bf43ec92b28acce5` is committed;
tested/pushed combined source `ae79f10a3af5ec823ec23726cafa44245faa39db` preserves
the concurrent remote README-only commit `69968d74` by normal merge, no force.
Registry workflow `a2436fd5ca4295aad56602bef93ff83e5724d780` pins that exact Grain
source and complete legacy tip; no GitHub write/OIDC/keys, credentials persisted,
author build or main activation in this read-only lane.

[Successful Linux run 37516026483](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37516026483)
passes **81/81 full maintainer tests / 9/9 core trust tests / 72 actual CLI
verdicts** (all 60 prior checks unchanged plus 12 legacy checks). Locked build
and each job step succeed. Independently downloaded artifact `11436209611`,
reported archive SHA256
`80588a5c3a7b5417e78b18fe842ec5d9b357fa5f457c78c6534209f3a2bb8f93`,
has seven-day retention. Downloaded contents are read, never executed.

Actual source pin, all previous 60 label/exit/oracle triples and new 12 triples
are independently compared with accepted Linux/Windows reports. Every one of
109 legacy files and the entire receipt are identical across platforms, with
all recorded SHA256 digests checked; one explicit recovery, all eight conflicts
and zero `.key` files verified. Linux evidence:
`tests/agent-harness/.runs/e4i-legacy-history-linux-01/`; independent verification
report `e4i-legacy-history-01/linux-verification.json`.

Windows CLI SHA256
`684ea5d050edc97343aaf507c9c286d6a31ab825452876b4884ff90ac7b0f5c1`;
Linux CLI SHA256
`c6018685b7c9c58799c406ea390fd4468d1a05d51df1b429ae10a42859f8b9e5`.
Independent final CIM snapshot checks **125 Windows CLI/Git PID references**:
zero remaining owned children. Earlier transient numeric PID matches are not
treated as owned processes or killed; final `process-check.json` has no matches.
User/generated bindings SHA256 remains
`2bd7fc3f77443d7317375fdff8441cac2733f228aa1c6aafe844ffd21809bac`;
nested held edits/deletions/untracked source remain exact. The draft PR is
updated without merge, administrative changes or physical cleanup.

The generated local/CI archive is reproducible evidence, not yet a protected
durable production archive or signed migration baseline. Its short-lived CI
artifact retention must not be mistaken for the production retention policy.

Graph-first minimal/query/impact/detect/review used. New archival files were
unindexed and graph test links were stale; zero impact is **not** an audit claim.
Direct review follows signatures before historical normalization, exact manifest
coverage, source object/path/mode bindings, each signed asset reference and size,
bounded shared deduplication, complete conflict recording, nonactivation layout
and owned failure/process cleanup. Existing active metadata admission is unchanged
and exercised by full maintainer/core regressions. This is a scoped self-audit;
independent protected release review is still required.

## Remaining E4 gates

Next: protected archive reauthentication and reservation enforcement in the
complete migration/hosting contract; genuinely reviewed/signed current roots,
index and revocation baseline; authorized conditional GitHub activation; actual
client snapshot coherence; effective review/protection/key custody and operational
renewal/revocation/root recovery. No fake history, capability reactivation, physical
obsolete removal, public contract freeze or production release. E4 remains open;
E5 management/store, E6 runtime/recovery, E7 tool measurements/reference
integrations and E8 release certification follow. Five stages retain work.

Foundation **52 Pass / 1 Deferred (12)**, forward **71**, full metadata criterion
**56 Pending**, Agent inventory **108 / 93 / 81** stay unchanged.

Design references: [Git raw-object/batch documentation](https://git-scm.com/docs/git-cat-file),
[Git history selection](https://git-scm.com/docs/git-rev-list), and
[TUF specification](https://theupdateframework.github.io/specification/latest/)
for authenticated metadata, address binding and separation of old proof from
fresh admission. This custom legacy archive is **not TUF conformance**.
