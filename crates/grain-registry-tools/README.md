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
verification and signing authority remain outside this local byte verifier. That handoff, catalogue
DESCRIPTION/media migration, complete serving paths, protections and key custody
are the remaining E4 work. New schema-1 source submissions still refuse the
legacy README-based publisher. Prepared receipt folders also refuse that publisher,
even without `--media-src`, before key reads or output writes. No new publication
command is enabled here.

Existing legacy key/sign/catalogue commands remain development tools under the
physical-removal hold. They are not a production deployment procedure. Do not
interpret the existence of a signature, local receipt or passing doctor as
release acceptance.
