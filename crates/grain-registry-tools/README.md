# Grain registry tools — provisional maintainer workflow

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

Existing legacy key/sign/catalogue commands remain development tools under the
physical-removal hold. They are not a production deployment procedure. Do not
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

The separate registry branch now has a pinned preparation/attestation checkpoint
for controlled native/MCP data. Next: real pinned author-source build isolation and
protected review-derived signing policy. Author builds must have no publishing credentials
or privileged attestation step. Follow [GitHub's trusted-builder guidance](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
and [secure workflow guidance](https://docs.github.com/en/actions/reference/security/secure-use).
Do not activate the current nested registry's incomplete legacy publisher.
