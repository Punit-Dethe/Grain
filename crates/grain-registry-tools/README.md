# Grain registry tools â€” provisional maintainer workflow

These tools do not ship in the application. E4 publishing is in progress;
production signing/deployment and public SDK freeze are not certified.

Build the tools from the Grain checkout:

```powershell
cargo build --locked -p grain-ext-cli -p grain-registry-tools --bins
cargo test --locked -p grain-registry-tools --all-targets
```

Use `cargo metadata --format-version 1 --no-deps` to locate the configured target
directory. The binaries are `debug/grain-ext` and `debug/grain-registry` (`.exe`
on Windows). Authors use the [CLI guide](../grain-ext-cli/README.md).

## Source preparation, without signing

The unprivileged build job obtains a fresh standalone source checkout at the
submitted commit with its exact tag available. The origin must match the
canonical submitted HTTPS GitHub URL. Native JavaScript tools need their build
to finish first, in a disposable job without publishing keys, repository write
credentials or personal service accounts. MCP descriptor projects need no Node
build. `prepare-artifact` does not fetch, checkout, install, build, sign or upload.

Given a registry submission at `registry/extensions/com.example.tools` and its
built source at `source`, run from a directory containing both:

```powershell
grain-registry check-submission --dir registry
grain-registry prepare-artifact --submission registry/extensions/com.example.tools --src source --out prepared-tools
```

Output must be a new directory outside source/submission, with an existing
parent. Existing output is never overwritten. Native projects produce
`artifact.grainpack`; MCP projects produce `artifact.mcp.json`. Both include
the exact `DESCRIPTION.md`, submitted media and a final `receipt.json` marker.
Native packaging uses the same bounded checker, icon embedding and compact
serialization as author packaging. `build-pack --src source --out artifact.grainpack`
also uses that shared boundary, requires a prebuilt native entry and a new
output file outside source; it does not execute build scripts.

Preparation checks local origin, HEAD and exact `refs/tags/<tag>` commit,
project kind/id/version/API, clean tracked/index state, untracked inputs and
listing hashes/sizes. Description, project definition and submitted media must
be tracked. Ignored generated output such as `dist/` is permitted and bound by
its artifact digest, not claimed to be committed source. Assumed-unchanged or
skip-worktree files, symlink/submodule index modes, linked worktrees, local Git
includes/filters/repository extensions/promisor settings and symlinked Git config
refuse. This profile requires a complete standalone SHA1 checkout; unsupported
repository forms are explicit refusals.

Local Git inspection uses fixed argument arrays, no shell, a bounded 256 KiB
read/output monitor and 30-second deadline per command. It disables global/system
Git config, inherited Git overrides, replacement objects, optional locks,
fsmonitor/untracked cache, prompts and lazy fetching. Its child is reaped and
temporary output dropped on success or failure. Source/submission are checked
again before the completion marker. Keep the owned checkout immutable: these
checks are not an OS snapshot, build sandbox or Git database integrity audit.

## Receiving prepared bytes, without signing

`verify-prepared` reads an immutable received bundle against a separately reviewed
submission. Its two lowercase SHA256 pins are required caller policy: obtain the
receipt digest from independently verified build/review evidence and the producer
executable digest from trusted build policy. Copying either value from the received
receipt does not establish that evidence. The local command does not verify a
GitHub attestation or decide whether a workflow/reviewer is trusted.

```powershell
# Set these from independent trusted evidence, not the downloaded receipt.
$reviewedReceiptSha256 = '<trusted receipt SHA256>'
$trustedBuilderSha256 = '<trusted builder executable SHA256>'
grain-registry verify-prepared --submission registry/extensions/com.example.tools --prepared prepared-tools --receipt-sha256 $reviewedReceiptSha256 --producer-sha256 $trustedBuilderSha256
```

The verifier checks the raw receipt digest before strict parsing, schema/evidence
class/current producer-version profile, producer identity and the complete
structured submission. It rechecks artifact size/digest, native tools-only or actual
host MCP validation, exact project id/version/API and canonical artifact bytes.
DESCRIPTION and decoded media are checked with the shared listing validator against
the reviewed hashes, sizes and order. Unexpected root files and symbolic links
refuse; artifact paths must be the fixed basename for their kind. Receipt reads
are capped at 64 KiB, native artifacts at 8 MiB and MCP descriptors at 8 KiB;
listing limits remain the shared submission contract.

Success prints identity/hash/byte counts and **bytes only**. The command writes no
files, starts no extension/build/server, uses no signing key and grants no trust.
Artifact/description bytes are retained only for the immediate checked call;
media remain metadata. This is not an OS snapshot or a transferable approval
ticket. The future publisher must use immutable owned input and verify media bytes
again while copying; a successful earlier check cannot authorize changed files.

## Unsigned catalogue candidates

After receiving validation, stage content-addressed catalogue inputs with the
same independent pins and a new output directory outside both input trees:

```powershell
grain-registry prepare-catalogue --submission registry/extensions/com.example.tools --prepared prepared-tools --receipt-sha256 $reviewedReceiptSha256 --producer-sha256 $trustedBuilderSha256 --out catalogue-candidate
```

This reuses `verify-prepared`, uses the checked artifact/description bytes and
rehashes each bounded image while copying. Output contains `blob/<hash>.grainpack`
or `blob/<hash>.mcp.json`, `media/<hash>.md`, image blobs and a final `candidate.json`.
The candidate declares `unsigned-catalogue-candidate/not-reviewed`, uses `dev`
trust, and leaves author/review dates/commit empty. Source commit and reviewed
listing metadata are retained; the numeric catalogue API minimum is derived from
the admitted artifact requirement. There is no `index.json`, signature, upload,
key access or extension execution. Existing output refuses without overwrite;
failed staging removes only its newly created directory on a best-effort basis.
Keep owned inputs and output immutable; this is not an OS snapshot or crash-proof
transaction. Retained files are candidates, not publishable approval evidence.

New signed catalogue entries carry `listing: { sha256, size }` for DESCRIPTION
and existing sized `media` references in presentation order. `readme` must be
empty in this profile; absent `listing` preserves legacy catalogue compatibility.
The application uses DESCRIPTION through its existing detail-document command
and enforces its signed byte size when that metadata is available. The frontend
wire name remains `readme` until E5; there is no README fallback for explicit
listings. MCP cards/management UI remain E5 work.

The later trusted signer must independently authorize review/CI/source and bind
the complete candidate plus all copied blob bytes before setting trust/review
fields and signing the catalogue. Neither this command nor its pins implement
cryptographic CI provenance verification. The legacy publication guard remains.

## Receipt is evidence, not approval

The schema-1 receipt declares `local-preparation/unsigned-not-reviewed`. It
records the validated submission, SHA256 of its compact structured JSON,
artifact basename/size/SHA256 and running producer executable SHA256/version.
Listing hashes/sizes come from the submission and are checked against copied
source bytes. Exact received artifacts can be compared to these records.

A local origin/tag and self-authored receipt cannot prove remote ownership,
human review, trusted CI identity or reproducibility. A trusted signing job
must independently bind the approved submission/source, trusted workflow/run,
toolchain/build and exact artifact/listing digests. It must not run author code
or promote a receipt merely because its JSON parses. Cryptographic CI/review
verification and signing authority remain outside these local byte/staging tools.
Signed catalogue deployment, complete serving paths, protections and key custody
are the remaining E4 work. New schema-1 source submissions still refuse the
legacy README-based publisher. Prepared receipt folders also refuse that publisher,
even without `--media-src`, before key reads or output writes. No new publication
command is activated here. The controlled signer below is a separate boundary.

Old catalogue writers remain outside the new publishing path and are eligible
for removal; there is no physical-removal hold. They are not a deployment procedure. Do not
interpret the existence of a signature, local receipt or passing doctor as
release acceptance.

## Reviewed candidate signing (E4b3 local boundary)

`sign-reviewed-candidate` produces a **signed catalogue update fragment**, without
uploading, executing author code, modifying the previous catalogue or activating
the old registry workflow. This command is maintainer-only and never ships in Grain.
The [controlled registry CI checkpoint](../../docs/Extensions%202.0/EXTENSION-CI-PROVENANCE-AUDIT.md)
now verifies genuine provenance through this command; production human review and
governance remain unaccepted.

```powershell
grain-registry sign-reviewed-candidate --submission registry/extensions/com.example.tools --prepared prepared-tools --candidate catalogue-candidate --policy protected-review/approved.json --policy-sha256 '<independent policy SHA256>' --gh 'C:\Program Files\GitHub CLI\gh.exe' --attestation candidate-attestation.jsonl --previous previous-v1 --key protected-keys/publishing.key --out signed-update
```

The strict policy has these required fields. All digests are lowercase hexadecimal;
SHA256 fields have 64 characters and commit fields have 40. `submission_sha256`
is the SHA256 of `serde_json::to_vec(SourceSubmission)` in contract field order,
not TOML bytes or arbitrarily reformatted JSON.

| Policy field | Trusted meaning |
| --- | --- |
| `schema` | Integer `2`; schema-1 policies are refused by the current signer |
| `candidate_sha256`, `submission_sha256`, `receipt_sha256` | Exact approved candidate, complete structured source submission and preparation receipt |
| `producer_sha256`, `verifier_sha256` | Independently approved registry builder and official `gh` executable bytes |
| `registry_repo` | GitHub `owner/repo` whose CI provenance must verify |
| `registry_commit`, `registry_ref` | Exact registry commit and `refs/heads/<branch>`; scoped branch names are allowed, ambiguous/invalid components refuse |
| `signer_workflow`, `signer_commit` | Same-repository `owner/repo/.github/workflows/<file>.yml` or `.yaml` and immutable workflow commit; reusable workflow identity is the signer |
| `reviewer`, `submitter` | Protected review record's reviewer and submitting GitHub account; only submitter becomes catalogue `author` |
| `review_pull_request`, `review_head`, `review_id` | Nonzero merged registry PR number, exact full reviewed PR head and nonzero latest effective approving GitHub review ID |
| `approved_at`, `expires_at` | RFC3339 approval window: not future-dated, still valid and at most seven days; rechecked before opening the key |
| `publishing_public_key` | Independently approved base64 minisign publishing public key |
| `previous_index_sha256`, `previous_index_version` | Exact currently signed index bytes and monotonic version to extend |

Keep the policy, its expected digest, verifier executable and publishing key in
immutable operator-owned locations outside author/build workspaces. A digest
copied from an untrusted policy does **not** establish review. The current signer
also queries GitHub through the pinned official CLI before any key access. The PR
must be merged into the approved repository/branch at `registry_commit`, with the
exact `review_head` and submitter. The merged submission and DESCRIPTION must
match the prepared request. The protected reviewer must be a different account,
and their latest effective review must still approve this head at `review_id`.
Dismissed, superseded, changes-requested and future-dated reviews refuse. Comments
do not revoke an approval. Review pages are bounded (100/page, at most ten); an
exhausted history refuses rather than trusting a partial result. GitHub availability
is required; there is no offline approval bypass. Production branch/environment
protections and reviewer eligibility remain separate deployment requirements.
Do not generate this policy in an author-execution job.

`verify-source-review` takes `--submission`, `--policy`, `--policy-sha256` and `--gh`
to check this same gate without a key. `inspect-submission --submission <ID-dir>
--out <fresh-file>` emits the shared strict structured source request for CI; it
confers no approval. For review reads only, the child inherits the operator's
GitHub CLI authentication/configuration or a read-only `GH_TOKEN`/`GITHUB_TOKEN`
with contents/pull-requests access. Never place publishing credentials in those
variables. Requests are fixed GETs to github.com, no shell or author arguments;
each child is bounded to 60 seconds and 1 MiB or a smaller content limit, and is
killed/reaped on monitor failure. Raw API responses and credentials are not logged.

The signer reuses received-byte validation, binds the complete approved submission,
and stages one owned snapshot. It compares every candidate file against that
snapshot, including exact candidate JSON, artifact, DESCRIPTION and pictures;
extra files, ignored JSON fields, altered trust and symlinks refuse. Existing output
or output inside an input/policy tree refuses. The previous index must match its
approved digest/version, verify against the approved public key and be fresh.

Cryptographic provenance uses the maintained [GitHub attestation verifier](https://cli.github.com/manual/gh_attestation_verify)
(checked with `gh` 2.100.0), with fixed repository/workflow/commit/ref/certificate
issuer/predicate flags and self-hosted runners refused. The local bundle is copied
into scratch space; standard GitHub/Sigstore trust roots are used, with no custom
root or bypass switch. Trust-root retrieval may need network access. No user GitHub
login, publishing secrets or inherited verifier overrides enter the child; temporary
home/config paths are owned by this call. Fixed arguments use no shell. Verifier
stdout is capped at 1 MiB, runtime at 60 seconds, and owned children are killed/reaped
on monitoring failure. Raw attestation output/errors are not printed.

Only after these gates does the command read the bounded key file and use the
existing minisign library. It grants **verified**, never **core**, attaches reviewed
source/date and submitter metadata, refuses reissuing an existing `(id, version)`,
increments the index version with overflow checks, and sets a 30-day expiry.
Previously signed entries and forward-compatible fields survive. The actual Grain
index verifier checks the new signature against the approved public key before
output is created; a wrong private key leaves no output.

Output contains the new artifact/listing blobs plus `index.json` and
`index.json.minisig`, written last. It is **not** a complete serving tree: prior
referenced assets, signed roots and revocations are not copied. Promotion must be
serialized against the exact current index and verify all serving paths and the
deployed root/key relationship. A local policy key is not evidence of app-pinned
root deployment. Crash leftovers are not publishable merely because files exist;
future promotion must verify the whole fragment. Key custody still follows the
existing development-key arrangement, not a production KMS claim.

The separate registry branch now has a real pinned public author-source/build lane,
with native execution isolated from fresh preparation and data-only attestation.
[Source/review audit](../../docs/Extensions%202.0/EXTENSION-SOURCE-REVIEW-BUILD-AUDIT.md)
records both kinds' genuine CI success and unapproved-signing refusals. Complete
local serving/promotion is implemented below; protected positive review, hosted
delivery and production governance remain Pending. Author builds have no
publishing credentials or privileged attestation
step. Follow [GitHub's trusted-builder guidance](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
and [secure workflow guidance](https://docs.github.com/en/actions/reference/security/secure-use).
Do not activate the current nested registry's incomplete legacy publisher.

## Complete serving assembly and local promotion (E4c)

These commands **never sign, open a key, execute author code or deploy hosting**.
They verify roots against the actual keys pinned in `grain-core`, then index and
revocations against the root-authorized publishing key. No custom-anchor CLI flag
exists. `verify-serving-tree` additionally checks every referenced native/MCP blob,
DESCRIPTION/legacy README and media file, and refuses missing, altered, linked or
extra inputs. Package semantics remain the existing preparation/receiving/host
responsibility. Old signed catalogues can retain retired entries while the host's
eligibility rules continue to reject them; this is not capability reactivation.

```powershell
grain-registry assemble-serving-tree --base current-v1 --update reviewed-signed-fragment --out complete-assembly
grain-registry verify-serving-tree --v1 complete-assembly/v1
grain-registry initialize-serving-store --v1 current-v1 --out local-serving-store
grain-registry promote-serving-tree --assembly complete-assembly --store local-serving-store --expected-current-sha256 '<independently captured current.json SHA256>'
```

Assembly writes `v1/` with all six signed metadata/signature files, `blob/`, `media/`
and only assets referenced by the authenticated next index. Signed bytes and
forward-compatible fields are preserved. Optional signed roots/revocation updates
in the fragment replace base documents; unchanged documents/assets come from the
base. Output must be fresh and outside inputs. Its bounded `promotion.json` pins
the exact base index/roots/revocation JSON digests. This unsigned parent record
prevents a mismatched handoff; it **does not establish review or signature trust**.

Snapshots have 8,192 unique asset and 1 GiB total budgets, with existing package /
descriptor bounds, 64 KiB documents and 4 MiB pictures. Assets are streamed in
64 KiB buffers, verified while copying and synchronized before pointer selection.
Shared addressed files cannot disagree on signed size. Unsupported specs, duplicate
extension versions, dev/experimental trust, malformed references and expired new
metadata refuse. There is no seed/clock-skew exemption for a new serving snapshot.
Authenticated expired history can be used as the base for a fresh renewal.

The local store has `snapshots/<complete-file-inventory SHA256>/`, `current.json`
and a persistent empty `promotion.lock`. Promotion obtains a nonblocking exclusive
standard OS file lock, compares the caller's expected **entire current pointer**
digest, revalidates its selected snapshot and matches the assembly's exact base.
It refuses metadata rollback or different JSON at the same version, changed
artifact/listing bytes for an existing extension version, weaker/erased revocation
rules and no-op repetition. A root-authorized publishing-key rotation requires new
documents' signatures to verify with that key. No key rotation/signing is performed
by these commands.

A full snapshot is copied into owned staging, verified again and renamed into
the content-addressed directory before an atomic temporary-file replacement of
`current.json`. Lock handles/temporary state drop at completion/failure; **never
unlink the lock file** to force progress. Other writers must honor this same lock
and leave installed snapshots untouched. Paths are operator-owned immutable inputs;
this is not a sandbox against another same-account writer. Old snapshots are kept,
including withdrawn assets. Incomplete/crash leftovers cannot become current simply
because a directory exists; a reused snapshot is reverified. There is no automatic
retry or garbage collector.

Files are synchronized and Unix directories are synchronized. Windows provides
the atomic pointer replacement tested here, but this is not a cross-platform
power-loss durability certification. A failure after pointer commit explicitly
requires inspecting `current.json` before retrying. Readers/hosting adapters must
capture one pointer, verify it, and serve its immutable snapshot. Independent
machines/network filesystems need a hosting-specific conditional promotion gate;
this local lock is not distributed coordination.

Production hosting still needs coherent metadata/root rotation, stable availability
of historical content hashes, verified base URLs/root deployment, actual protected
review/key custody and renewal/revocation operations. Do not substitute this local
store for those gates or merge the incomplete legacy publisher. The empty app seed
is used only as a real pinned-root CLI fixture; disposable signing anchors in Rust
component tests never reach the public CLI or the app's trust configuration.

[Scoped serving audit](../../docs/Extensions%202.0/EXTENSION-SERVING-PROMOTION-AUDIT.md)
records the original Windows/Linux acceptance and operational limits. The
historical reservation/export follow-up below strengthens that checkpoint.

## Historical version reservation and selected snapshot export (E4d)

Promotion also authenticates retained snapshot metadata under the same store lock.
A withdrawn extension cannot return with changed artifact kind, package/descriptor,
description or media under its old `(id, version)`. Identical restoration and a new
version are allowed. Fully verified installed snapshots reserve identities even
if a crash prevented selecting them. Unsigned `.tmp*` staging is ignored, never
selected or deleted. Invalid signed history, a mismatched content-addressed
directory, links or unexpected names refuse promotion.

One bounded historical catalogue is inspected at a time; old package bytes are
not reread for this identity check. Bounds are 4,096 installed snapshots and
128 MiB combined signed metadata per operation. Exhaustion refuses and requires
an audited archival design that preserves identities; deleting old snapshots or
initializing a replacement store is not a safe way to bypass it. The store must
stay append-only and operator-protected. Bootstrap checks only the supplied signed
catalogue; it cannot reconstruct missing history from another deployment. No new
database, service, signing authority or public contract was introduced.

```powershell
grain-registry export-serving-snapshot --store local-serving-store --out fresh-export --expected-current-sha256 '<independently captured current.json SHA256>'
grain-registry verify-serving-tree --v1 fresh-export/v1
```

Export locks the same store, verifies the independently captured pointer and its
strictly fresh selected snapshot, then copies and rechecks exact signed metadata
and all addressed files using existing streaming bounds. Output must be fresh and
outside the store. `snapshot.json` records the selected state and current-pointer
digest; it is an unsigned operational receipt, not review/deployment authority.
Failure removes only owned unfinished output; old exports remain stable after a
later promotion. Export does not select a snapshot, sign, access keys or publish.

The export is one complete `v1/` snapshot. It does not aggregate old withdrawn
assets, solve distributed deployment, rewrite signed base URLs or activate hosting.
A future hosting adapter must preserve historical addressed assets and support
conditional activation/coherent roots, with protected approval/key custody and
operational renewal/revocation. [History/export audit](../../docs/Extensions%202.0/EXTENSION-SERVING-HISTORY-EXPORT-AUDIT.md)
records verification and the remaining gates.

## Static hosting bundle with retained assets (E4e)

```powershell
grain-registry export-hosting-bundle --store local-serving-store --out fresh-hosting-bundle --expected-current-sha256 '<independently captured current.json SHA256>'
grain-registry verify-hosting-bundle --bundle fresh-hosting-bundle --expected-receipt-sha256 '<independently captured bundle.json SHA256>'
```

The exporter prints the receipt digest. Capture it through the protected handoff;
do not establish independent trust by hashing an untrusted received receipt.
These commands use the actual app-pinned trust chain, never a CLI override key.
They do not sign, deploy, alter the pointer, execute author code or grant approval.

The bundle contains exact `current.json`, bounded unsigned `bundle.json`, `v1/`
and `history/<snapshot-digest>/` signed metadata proofs. `v1/` has the fresh selected
six metadata/signature files and the union of retained addressed `blob/` and
`media/` files. Only the selected index is active; historical catalogues are proof,
not active listings. Shared addressed files are copied once, with agreeing explicit
sizes. All retained installed snapshots are included; unsigned temporary staging
is inert. Conflicting historical `(id, version)` artifact/listing identities refuse
even when neither version remains selected.

The existing `verify-serving-tree` intentionally checks a single snapshot; use
`verify-hosting-bundle` for a bundle's archive-aware layout. Its independently
pinned receipt fixes the selected state and sorted exact history membership.
Verification binds exact pointer bytes to that state, authenticates all proofs
and their content-addressed identities, reconstructs the addressed file union,
and streams every actual file. Missing, changed, linked, duplicate, unsafe or
unexpected files/proofs/pointer/receipt fields refuse. Strict expiry applies to
active metadata; old signed proofs can be expired without activating them. Old
proofs retain their own signed roots when the current publishing key rotates.

This maintainer profile bounds the union to 8,192 unique assets and version
identities, 1 GiB total, 4,096 snapshots including selected, 128 MiB signed metadata
and a 384 KiB receipt. Individual existing file bounds still apply. One catalogue
and one 64 KiB file buffer are processed at a time; only bounded path/hash maps
are retained. There is no permanent resource/cache or new registry server.
Exhaustion refuses. During unreleased development, use a clean bootstrap rather
than adding a historical archival/migration system.
Owned incomplete output is removed on failure; previous inputs/exports stay intact.

This prepares an offline static-host handoff, not an activated hosting adapter.
Signed base URLs and bootstrap remain unchanged. Future deployment must preserve
the `blob/`/`media/` route structure, retain immutable hashes, activate metadata
coherently and apply the host's own conditional-write/protected release policy.
Do not flatten paths into legacy GitHub Release assets or assume a local lock
coordinates independent machines. Current hosting activation, review,
key custody and renewal/revocation remain gated. [Hosting bundle audit](../../docs/Extensions%202.0/EXTENSION-HOSTING-BUNDLE-AUDIT.md)
records evidence and limitations.

## Metadata-only renewal (E4f)

```powershell
grain-registry renew-serving-metadata --v1 SELECTED_SNAPSHOT --expected-snapshot-sha256 '<independently protected snapshot digest>' --key OPERATOR_PUBLISHING_KEY --expires-days 30 --out FRESH_RENEWAL
grain-registry verify-serving-tree --v1 FRESH_RENEWAL/v1
grain-registry promote-serving-tree --assembly FRESH_RENEWAL --store PROTECTED_STORE --expected-current-sha256 '<independently protected current.json digest>'
```

Renewal preserves every extension, revocation rule, future JSON field and addressed
file. Only index/revocation versions and expirations change; roots remain exact.
Lifetimes are 1â€“30 days and must extend both expirations. Authentic expired index/
revocations can recover, but expired roots need root-authorized recovery. The
2099 bootstrap seed deliberately refuses shortening. Input identity/signatures,
complete assets, counters and output containment are checked before the bounded
key read; both new signatures must verify under existing roots before output.

The result is a fresh complete `v1/` plus the previous parent for existing locked
promotion. It does not access GitHub, deploy, schedule work, reapprove sources,
alter revocation states or delete files. Key custody and protected release
authorization are external prerequisites; the command uses the existing
unencrypted maintainer key format and does not certify secure-memory erasure.
Use the protected selected snapshot pin, not a hash supplied by an author. Keep
normal source publication on `sign-reviewed-candidate`. GitHub remains the
confirmed registry host; optional OAuth website preparation is parked.
[Renewal audit](../../docs/Extensions%202.0/EXTENSION-METADATA-RENEWAL-AUDIT.md)
records signed regression evidence, command-line admission checks and remaining
operational/release gates.

## Commit-bound GitHub publication handoff (E4g)

```powershell
grain-registry prepare-github-publication --checkout OPERATOR_CHECKOUT --repository OWNER/REPO --branch RELEASE_BRANCH --expected-base-commit INDEPENDENT_BASE_COMMIT --candidate-commit INDEPENDENT_CANDIDATE_COMMIT --previous PROTECTED_PREVIOUS_BUNDLE --previous-receipt-sha256 PREVIOUS_RECEIPT_PIN --bundle PROTECTED_CANDIDATE_BUNDLE --expected-receipt-sha256 CANDIDATE_RECEIPT_PIN --out FRESH_HANDOFF
```

This is an offline publication gate, not a deploy command. Place the hosting
bundle's `v1/` at the repository's `v1/`; place `bundle.json`, `current.json` and
`history/` under `.registry-publication/`. Both the expected base and candidate
must already contain that layout. The first publication of the new catalogue
requires a separate protected procedure; a missing baseline is never bypassed.
Git does not track empty directories; reconstruct the known empty bundle folders
when capturing a bundle from a Git commit.

Supply independently protected full 40-character Git SHA1 commits and receipt
SHA256 pins. The candidate must be HEAD, have the base as its sole actual parent,
and change only the two publication directories. Both committed inventories and
raw blob identities must match the verified bundles, with no filters or newline
conversion. Dirty worktree bytes are not publication authority. All previous
signed history, including the previous selection and withdrawn addressed files,
must remain; existing monotonicity, immutable version and revocation checks apply.
An authentic expired previous publication can recover; a candidate must be fresh.

Use a complete standalone, protected operator checkout and immutable operator
bundle captures. Includes, filters, URL redirects, alternate/promisor objects,
shallow ancestry and grafts refuse. This is not a same-account filesystem sandbox.
The new directory contains unsigned `publication.json` and its printed digest;
it records the exact candidate, expected base, bundle pins, environment isolation
and fixed GitHub push argument vector. No shell command is executed. Its explicit
`--force-with-lease=refs/heads/BRANCH:EXPECTED_BASE` binds the remote update to that
base, and the verified single parent makes the intended update a fast-forward.
No broad force, plus refspec, implicit tracking-ref lease, tag or submodule push
is used. A disposable bare-remote test exercises success and stale-writer refusal.

The unsigned handoff grants no source approval, signing or GitHub authorization.
Do not execute an untrusted received argument vector. The later protected runner
must regenerate/verify the handoff and freshness immediately before activation,
bind its own fixed repository/branch/policy, provide scoped noninteractive GitHub
authentication and preserve server-side release protections. Existing global Git
credential configuration is intentionally disabled by the recorded environment;
the protected runner must provide its own credential path. There is no credential
helper, secret, network operation, persistent Git configuration change, deletion
or new dependency in this command. Production branch/protection behavior and
coherent multi-request client reads remain unverified here.
[Publication audit](../../docs/Extensions%202.0/EXTENSION-GITHUB-PUBLICATION-AUDIT.md)
records evidence and the next protected activation checkpoint.

## Capture previous publication from pinned Git objects (E4h)

```powershell
grain-registry capture-github-publication --checkout PROTECTED_STANDALONE_CHECKOUT --repository OWNER/REPO --expected-commit INDEPENDENT_FULL_COMMIT --expected-receipt-sha256 INDEPENDENT_BUNDLE_RECEIPT_PIN --out FRESH_CAPTURE
grain-registry verify-hosting-bundle --bundle FRESH_CAPTURE/bundle --expected-receipt-sha256 INDEPENDENT_BUNDLE_RECEIPT_PIN
```

Capture reconstructs the committed `v1/` and `.registry-publication/` layout into
`bundle/`, including known empty blob/media/history directories. It reads raw Git
objects in one bounded binary batch, never checkout/index bytes, attributes,
checkout filters, author programs or credentials. Full commits and independent
receipt pins are required; local origin is checked against the expected GitHub
repository. This local check does not prove remote commit membership or approval.
The selected commit can precede HEAD, so a candidate checkout can capture its
previous baseline without a checkout/reset. Capture changes neither Git nor input
files. Each file's safe destination, mode, size and total inventory are checked
before output; existing signed bundle/history verification and raw committed-byte
comparison run before a successful handoff is emitted.

Capture authenticates previous publication proof, so expired signed metadata can
be preserved. It does not make that metadata active/fresh or authorize a release;
the separate verifier above requires freshness, and publication independently
checks the new candidate. Incomplete publication proof is refused; initialize the new catalogue directly
rather than importing old registry data.

Output includes unsigned `capture.json` with source commit, receipt pin, snapshot
and captured file/byte counts; its printed digest is operational evidence only.
Use `FRESH_CAPTURE/bundle` in the existing `prepare-github-publication` handoff.
The protected runner must independently pin the repository/commit/receipt and
verify remote membership, review/authorization and freshness before activation.

The maintainer-only path streams artifact bytes through an owned temporary file
and disk capture; it does not retain artifacts in RAM or add a service, dependency,
cache or Agent harness. Temporary disk can approach one bundle size in addition
to the capture, bounded by the 1 GiB profile plus Git framing. Git's existing
30-second child deadline remains; oversized/slow/malformed inputs refuse. Partial
owned output is cleaned, while pre-existing inputs/outputs stay intact.
[Capture audit](../../docs/Extensions%202.0/EXTENSION-GITHUB-CAPTURE-AUDIT.md)
records historical verification. The clean first-publication/activation work is
tracked in the clean-break audit below; old registry migration is not required.
## Clean catalogue bootstrap (unreleased platform)

Grain has no deployed users; there is no old-registry migration or archive lane.
The four legacy capture/migration commands and their exclusive implementations,
fixtures and CLI checkpoint have been removed.

```text
grain-registry bootstrap-serving-tree --key EXPLICIT_PUBLISHER_KEY \
  --expires-days 30 --out NEW_BOOTSTRAP
grain-registry verify-serving-tree --v1 NEW_BOOTSTRAP/v1
grain-registry initialize-serving-store --v1 NEW_BOOTSTRAP/v1 --out NEW_STORE
grain-registry export-hosting-bundle --store NEW_STORE \
  --expected-current-sha256 INDEPENDENT_CURRENT_POINTER_SHA256 --out NEW_BUNDLE
```

Bootstrap authenticates the **embedded current app seed**, preserves its genuine
roots/current revocation rules, and signs a fresh empty index and revocations
with versions advanced above that seed (currently 2/2). Lifetime is 1–30 days.
This uses the current signer's unencrypted minisign format and an explicitly
supplied key; missing/wrong keys refuse. Root trust, seed freshness, empty
catalogue, version/size bounds and output separation are checked before signing.
There is no old checkout, legacy identity reservation, archive binding, recovered
file route, invented historical revocation or settings migration. Existing
operator output is never overwritten and unfinished owned output is cleaned.

A local bootstrap is preparation, not release approval or GitHub activation.
The existing ordinary Git publication gate requires a complete previous
new-contract publication. The separate `prepare-initial-github-publication` gate handles that first
publication; do not fake a previous bundle to pass the ordinary gate. Source review,
key ownership, current hosting integrity and app/hosted metadata coherence still
apply. Routine signed updates/renewal use the existing current-contract path.
See [clean-break audit](../../docs/Extensions%202.0/EXTENSION-CLEAN-BREAK-AUDIT.md).


## Clean first publication and manual GitHub activation

```text
grain-registry prepare-initial-github-publication --checkout STANDALONE_CHECKOUT \
  --repository OWNER/REPO --branch main --expected-base-commit FULL_BASE_SHA \
  --candidate-commit FULL_CANDIDATE_SHA --bundle VERIFIED_CAPTURE/bundle \
  --expected-receipt-sha256 INDEPENDENT_RECEIPT_SHA256 --out NEW_HANDOFF
```

The candidate must be a single publication-only commit directly above the
independently reviewed current `main` commit. That base must not already contain
`.registry-publication/` proof: after initialization, use the ordinary update
gate. Experimental `v1/` data can be replaced without migration; unrelated
repository paths cannot change. Committed bytes must exactly match the signed,
receipt-pinned bundle, irrespective of a dirty checkout. Origin, ancestry,
config, file modes, output separation and existing budgets remain checked.

Initial metadata must preserve exact embedded seed roots and seed policy, use
the next index/revocation versions (currently 2/2), contain no extensions/assets
or inherited history, and share a fresh expiry at most thirty days ahead.
The 2099 development seed is not a publishable bootstrap. No key is read by the
gate. The unsigned handoff has a distinct initial evidence class and omits the
previous receipt instead of fabricating one. It remains preparation, not approval.

The registry's `build-and-check` manual workflow now reuses the isolated source
builder. Native author execution has no OIDC/publishing key; trusted preparation
and data-only attestation run in separate jobs. MCP descriptors skip native
execution. A trusted registry branch and existing source review/producer policy
are required; merge/fork events do not automatically attest or publish.

The registry's `publish` manual workflow builds a full-SHA-pinned Grain verifier,
captures candidate bytes directly from Git, and uses the initial or ordinary
gate as selected. Its runner is `ci/publish_github.py` in that pinned Grain tree;
no candidate scripts execute and no publisher signing key is configured there.
The `publish` environment and repository rules remain external operator setup.
The job needs only `contents: write`, no OIDC or signing secret. Inputs reach
Python as environment values and argv, never executable shell fragments.

Only final Git children receive the scoped workflow token through an owned
askpass helper; no token is written into the helper, URL, argv or catalogue.
The runner checks the complete handoff against the independent tuple, verifies
remote `main`, pushes once with the explicit expected-base lease, and checks the
resulting reference. A race, timeout or uncertain postcondition never causes an
automatic retry. Temporary captures/helper files are cleaned on every outcome.
Repository protections are never changed or bypassed. Remote confirmation does
not certify raw HTTP delivery or real-app install coherence; those are the next
acceptance boundary. Landing the workflows does not activate the catalogue.

Focused checkpoint commands (no GitHub writes):

```powershell
cargo test --locked -p grain-registry-tools -- --test-threads=2
cargo clippy --locked -p grain-registry-tools --all-targets --no-deps -- -D warnings
python -m unittest discover -s crates/grain-registry-tools/ci -p test_publish_github.py -v
python crates/grain-registry-tools/ci/check_initial_cli.py --tool C:/t/debug/grain-registry.exe --evidence NEW_EVIDENCE_DIRECTORY
```

The public CLI checkpoint verifies actual app-pinned trust and refusal behavior;
positive first-publication signatures use disposable component keys, never a
public CLI trust override. See the [first-publication audit](../../docs/Extensions%202.0/EXTENSION-FIRST-PUBLICATION-AUDIT.md)
for verification, operational prerequisites and remaining work.

## Signed metadata generation

Every hosted `index.json` now includes `generation.roots_sha256` and
`generation.revocations_sha256`: SHA-256 of the exact companion **JSON bytes**,
covered by the index's existing minisign signature. The roots and revocation
signatures are still independently required. Detached signature comments are
not publication identity; formatting changes to JSON require a new binding and
index version. No fourth signing role, network file or service is added.

Bootstrap creates the revocation bytes first and binds them before index
signing; renewal rebuilds the binding after both versions/expirations advance.
Reviewed source publication preserves the authenticated prior binding when
companions are unchanged. Root/policy changes must be bound by a new signed
catalogue. Assembly, hosting verification, committed Git capture and both
publication gates share this check. They refuse mixed generations, including
when each component has a valid signature.

The exact embedded empty seed is accepted only as bootstrap/initial-gate input.
It cannot initialize a serving store, renew, assemble, export or authorize app
network installs without binding. The CLI checkpoint's former unbound-seed
publication examples are retired; genuine signed component tests cover positive
bootstrap/renewal/hosting/capture/conditional publication and public CLI checks
cover refusal boundaries. There is no public CLI trust override.

On a mismatched refresh, Grain stays offline for acquisitions. Authenticated
current-publisher kill switches can still strengthen cached negative policy;
that safety update never grants a mixed publication installation authority.
Offline browsing preserves the catalogue/version floor until a matching fresh
generation is accepted. See the [generation audit](../../docs/Extensions%202.0/EXTENSION-METADATA-GENERATION-AUDIT.md).
