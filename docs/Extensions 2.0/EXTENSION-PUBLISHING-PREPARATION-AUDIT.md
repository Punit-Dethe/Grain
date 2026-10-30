# Publishing preparation — E4a

**5 October 2026: forward 69 accepted for local no-signing preparation only.**
E4 remains in progress; five delivery stages E4–E8 still retain work. Companion:
[registry findings](LOCAL-EXTENSION-REGISTRY-AUDIT.md), [E3b contract](EXTENSION-SUBMISSION-LISTING-AUDIT.md)
and [maintainer commands](../../crates/grain-registry-tools/README.md).

## Implemented production tools

The active registry `build-pack` previously joined an author entry path loosely,
omitted canonical icon payloads and serialized a different shape from the author
CLI. It now reuses `grain-extension-checks::build_pack` and the author CLI's
compact serialization. Compilation is a separate unprivileged-job responsibility.
It writes a new file outside source through synced no-clobber staging; invalid
source, existing output or source-overwrite attempts preserve prior bytes.
This repairs the active producer; held retired-capability files and old private
submission/README publishing code are not deleted.

New `prepare-artifact` supports native JavaScript tools and remote MCP descriptors.
It consumes a bounded shared submission plus an already available standalone
source checkout, verifies local origin/HEAD/exact tag, project identity/API and
DESCRIPTION/media snapshots, packages canonical bytes and writes a new owned
output directory. It includes artifact, DESCRIPTION and exact media bytes;
receipt.json is written last. Failed preparation has a scoped best-effort guard
for only its newly created directory. Existing outputs refuse; no retry or
publication occurs. No build script, Node install, fetch, OAuth, signing key,
root rotation, catalogue mutation, upload or GitHub setting is involved.

The unsigned receipt records validated source submission/digest, exact artifact
size/hash, listing hashes and running producer executable identity/version.
Its evidence class explicitly says **unsigned-not-reviewed**. Local Git refs
and a locally supplied origin are not independent remote ownership. A receipt
cannot grant `verified`/`core` trust. The subsequent trusted handoff must verify
approved source/CI/run/toolchain/artifact independently; the old publisher still
refuses schema-1 source submissions.

## Source checks, limits and audit

Preparation requires clean tracked/index state and no untracked inputs, with
definition, DESCRIPTION and submitted media tracked. Ignored generated native
build output is permitted and bound to artifact bytes; this does not prove a
reproducible build. Local Git config includes/filters/repository extensions and
promisor settings, symlinked config, linked worktrees, index symlinks/submodules
and hidden index flags refuse. Origin must be a single exact canonical submitted
URL; fully qualified tag peeling and HEAD must equal the full submitted SHA1.

Git uses fixed process arguments without a shell; inherited Git overrides,
system/global config, replacement objects, optional locks, fsmonitor/untracked
cache, prompts and lazy fetching are disabled. Each command is checked against
a 30-second deadline and a 256 KiB temporary-output monitor/bounded memory read;
temporary disk output can exceed the threshold between polls. Only that child
is killed/reaped on error; temporary handles/files expire immediately. Windows
children are hidden. Error messages do not echo rejected private origin/config
contents. This is maintainer-only synchronous work with no background service
or application dependency.

Source and submission are checked again before the receipt. An immutable owned
CI checkout remains required: this is not a filesystem snapshot, hostile Git
object-database integrity proof or operating-system sandbox. The receipt is
preparation evidence and must stay out of a signing authority decision until
independent trusted-job verification exists. Output cleanup is best effort on
filesystem failure and cannot be certified after hard process loss.

Graph-first source/impact and change/review guidance preceded scoped inspection.
The nested workflow has no indexed nodes; its source was read directly. Graph
reported no downstream flows but imperfect/unindexed new-test linkage (risk .35);
that is not test execution evidence. The focused source audit covered process
arguments/environment/config execution, tag ambiguity, output ownership,
byte/hash/identity matching, cleanup, serialization, producer isolation and
legacy signing refusal. Audit hardened repository-extension configuration,
which could otherwise change the config read path; the regression refuses it.
The audit also extended legacy publishing refusal to prepared receipt folders,
including when `--media-src` is omitted, before key reads or output writes.
New Clippy findings were fixed. No remaining finding blocks this limited slice.

References checked against current primary documentation:

- [Git rev-parse](https://git-scm.com/docs/git-rev-parse) documents exact commit peeling and end-of-options; the tool uses a qualified tag rather than ambiguous revision text.
- [Git status](https://git-scm.com/docs/git-status) and [ls-files](https://git-scm.com/docs/git-ls-files) inform stable status/index inspection; [Git environment controls](https://git-scm.com/docs/git) inform config/replace/lazy-fetch suppression. These do not independently establish repository ownership.
- [GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use) supports keeping untrusted builds away from signing authority. [Artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations) remain an established candidate for the later CI handoff rather than a new custom trust service. No workflow/attestation is activated by this block.

## Final verification and evidence

- **133 locked Rust tests Pass:** CLI 13, checker 28, registry 9, SDK 83. Seven
  new registry tests cover both kinds and media, real annotated-tag resolution,
  wrong origin/HEAD/tag, dirty/untracked/hidden index, mismatched identity/listing,
  includes/filters/repository extensions, submodule/symlink index modes,
  oversized actual Git output, unbuilt native source, embedded icon/entry bounds,
  existing output preservation and source overwrite refusal.
- Native fixture's build script deliberately exits 93; preparation still succeeds
  because it packages the previously built entry without executing the script.
- Final locked CLI/registry builds, Rustfmt/scoped whitespace and Clippy Pass.
  Clippy retains two pre-existing CLI conversion warnings and the pre-existing
  registry doc indentation warning. No new package version is added: existing
  tempfile moves into maintainer runtime dependencies; CLI/image are dev-only
  fixture reuse. The application/SDK/host transport/harness source is unchanged.
- Final binary evidence `e4a-executable-01a4b452` prepares both local source
  repositories, checks producer/artifact identities, compares MCP output with
  actual `grain-ext pack` and native output with actual shared `build-pack`,
  and accepts both through actual `check-submission`. Both overwrite attempts
  exit 1 with receipt bytes preserved; both changed-source attempts exit 1 with
  no output. Both attempted legacy publishes refuse with E4 guidance before
  reading the absent key or creating any catalogue output. Native uses an
  existing compiled nonsecret author fixture. No remote
  repo fetch, human review, signing, publishing or application acceptance is
  claimed by these tests.
- Initial executable attempt `e4a-executable-79a84730` stopped before submission:
  the author guide/example used unsupported `--summary`. The command actually
  derives summary from project description/name. Corrected the guide and reran
  in a fresh scope; the failed attempt has no acceptance credit.

Final identities and resource inspection are recorded in ignored
`.runs/e4a-executable-01a4b452/report.json` and `.runs/e4a-inspection.json`:
producer SHA256 `38219b8ae008f1c9d326d93902034a524ba2d0df356f9bc9ac08e85b31f77d95`
and CLI SHA256 `96a6d41536accbdddd95d8e01ea05c87f9220e8dac0881782f3dd36eaa77661c`.
Tests are based on dirty precommit `9cffabb40447421afd3c2de0a31f501bf03c3cd7`,
Windows x64, Git 2.52.0.windows.1. Desktop acceptance is deliberately not repeated
for a maintainer-only path: no host catalogue parsing/runtime/UI changes occurred.
The E4 catalogue migration will require the existing signed-store real-app
checkpoint. Existing 108 scenarios / 93 self-contained / 81 Node inventory and
earlier real-app verdicts stay unchanged; no new harness case/framework.

## Retention and next work

Keep the product preparation command, shared-packaging path, focused tests and
maintainer/author documentation. Ignored logs and the three marked owned nonsecret
source/registry/artifact scopes are disposable evidence, intentionally retained;
no profile/account/secret/listener is created. One-off verification script is
evidence support, not a maintained runner/framework or production dependency.
Independent read-only inspection of all three scope paths finds zero owned
processes. The earlier successful `e4a-executable-da847868` predates the receipt
publication guard; the final repeat above certifies the final binary. Files are
retained intentionally, so no physical scratch-file cleanup Pass is inferred.
Existing nested registry edits/deletions/untracked Tasks draft and main branch
were inspected and left untouched. No changes are committed there.

REG-06's active loose packaging path is repaired; REG-03 gains an executable
shared command path; REG-04/09/10 gain unsigned byte/identity preparation only.
They are not fully closed. Next **E4b trusted artifact/review handoff and
DESCRIPTION/media catalogue producer/consumer migration**, followed by registry
workflow/serving/update/revocation/governance acceptance. Refresh relevant remote
administration observations when implementing those settings, not as a substitute
for code checks. Any future nested repository edits need their own appropriate
branch and scoped commits, preserving current user changes.

Baseline remains **52 Pass / 1 Deferred (12)**; permanent public OAuth client
hosting/live acceptance **56 Pending**. No new manual test batch, physical
obsolete-file deletion, E5 UI, public SDK freeze or production publication.
