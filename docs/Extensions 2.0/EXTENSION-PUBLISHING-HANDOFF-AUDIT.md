# Prepared-artifact receiving boundary — E4b1

**5 October 2026: forward 70 accepted for local byte/contract verification only.**
E4 publishing remains in progress; five E4–E8 delivery stages retain work.
Companions: [producer audit](EXTENSION-PUBLISHING-PREPARATION-AUDIT.md),
[maintainer commands](../../crates/grain-registry-tools/README.md) and
[execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md).

## Product change and authority boundary

The existing unprivileged producer can prepare native or MCP bundles. New
`grain-registry verify-prepared` checks received bytes against a separately reviewed
submission, an independent receipt digest and trusted producer-executable digest.
This is the receiving prerequisite for E4b; catalogue migration and trusted CI
verification/signing are not completed by this slice.

The shared schema-1 receipt is now deserializable with unknown fields refused;
its existing wire representation remains compatible. Verification compares the
raw receipt digest before parsing, schema/evidence class/current producer version,
producer digest, complete validated structured submission and its digest. It
requires the artifact's fixed basename, nonzero bounded size, exact hash, native
tools-only or actual host MCP admission, exact id/version/API and canonical
serialization. Extra root entries and links refuse. Shared listing validation
checks DESCRIPTION and actual decoded media against reviewed hashes/sizes/order.

The CLI is read-only and starts no build, author tool, extension, server, network
request or signing operation. Artifact and description bytes are retained for the
immediate verified call, then dropped; media metadata are retained without keeping
all decoded images alive. Receipt reads cap at 64 KiB, native artifact at 8 MiB,
MCP descriptor at 8 KiB. Existing shared listing limits cap description at 64 KiB,
six images at 4 MiB each and 16 MiB total listing bytes. Image decoding retains its
existing 2048px/16 MiB requested limits and first-frame scope; this slice does not
certify all animation frames or Markdown rendering.

Caller policy must provide independent pins. Self-derived fixture pins exercise
contract checks and cannot prove CI identity, human review, remote ownership or
reproducibility. Exit zero grants neither `verified` nor `core` trust. Owned input
must remain immutable: filesystem reads are not an OS snapshot, hard-link/race
defence or a transferable whole-bundle approval ticket. The subsequent publisher
must hash media while copying and consume checked artifact/description bytes in
the same bounded operation, rather than trusting paths from an earlier exit zero.

## Scoped audit and verification

Graph-first context/impact and change/review guidance preceded source inspection.
Graph linkage for the new module/test cases is incomplete; graph output is not
execution evidence. The scoped audit inspected input budgets, fixed paths,
strict receipt parsing, structured source binding, canonical artifact admission,
shared listing/image limits, resource lifetime and separation from signing.
No remaining finding blocks this limited local receiving boundary.

- **139 locked Rust tests Pass:** CLI 13, checker 28, registry 15, SDK 83.
  Six new receiving matrices cover both kinds/actual media, independent-pin and
  reviewed-source mismatch, altered receipt/artifact/description/media,
  self-consistent invalid identity/retired capability, unknown receipt fields,
  path injection/extra files and bounded oversized reads. Existing producer
  checks remain passing; unit fixture reuse adds no testing framework.
- Actual final `grain-registry` binary runs **12/12 expected verdicts** on copies
  of the earlier E4a bundles: both kinds accept, and each rejects wrong receipt,
  wrong producer, changed artifact, changed DESCRIPTION and changed receipt.
  Valid calls preserve input hashes. This also checks prior receipt compatibility.
  Unit cases cover media; executable fixtures have no media. No CI provenance,
  signing, remote source ownership or desktop acceptance is inferred.
- Locked registry build, Clippy, Rustfmt and scoped whitespace checks Pass.
  Clippy retains two existing CLI conversion warnings and one existing registry
  documentation indentation warning; no new warning or dependency is introduced.
  An initial broad command used the nonexistent package name `grain-cli` and
  failed before running tests; the corrected `grain-ext-cli` command completed.
- Windows execution does not run existing Unix-only filesystem-link cases.
  Link refusal is inspected in the shared and receiving code; no cross-platform
  filesystem certification is claimed here.

Final binary SHA256:
`2d7cbcae010eac17c95b9ae45c807c1e11104aefc9669ff427ce858caf9ce68d`.
Prior producer policy pin from accepted local E4a evidence:
`38219b8ae008f1c9d326d93902034a524ba2d0df356f9bc9ac08e85b31f77d95`.
Evidence is based on dirty precommit `a3022c8f2deb5c3fea2e7881bdb7ef57d3a6e468`,
Windows x64. Ignored logs are `.runs/e4b-receive-{tests,all-tests,build,clippy}.log`;
executable evidence is `.runs/e4b-receive-78d6a570/report.json`.

The copied marked nonsecret scope is explicitly retained as disposable evidence.
All 12 recorded synchronous registry children exited; no account, credential,
profile, listener or background service was created. Retention is not file-cleanup
Pass. The one-off PowerShell commands are not maintained harness infrastructure.
No application, SDK contract, harness source/scenario or nested registry files
changed. No unaffected desktop or 81 Node suite rerun is claimed. Preserve the
unrelated `src/app/bindings.ts` edit and existing nested registry changes.

## Remaining E4b and downstream work

Use maintained cryptographic provenance verification before handing artifacts to
a trusted signing-only job. [GitHub CLI attestation verification](https://cli.github.com/manual/gh_attestation_verify)
supports repository, source digest and signer-workflow restrictions; these require
a Grain-owned policy and trusted workflow context. [GitHub artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
require verification rather than trusting generation alone. [Reusable-workflow
separation](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
informs keeping untrusted builds outside publishing authority. This block adds
no custom cryptographic trust service and activates none of those workflows.

Next E4b2 must align DESCRIPTION/media catalogue production and host consumption,
both artifact kinds and supported compatibility, then implement the independently
verified CI/review handoff and signing boundary. Keep the legacy publisher guard
until the new complete path is accepted. Use focused components while developing
and existing signed-store real-app cases at the catalogue/host checkpoint.
Separate registry workflow/serving/protection/key-custody/update/revocation work
remains part of E4. Nested changes need their own appropriate branch and scoped
commits preserving the user's existing edits.

Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory
**108 scenarios / 93 self-contained / 81 Node**, public-hosting/live acceptance,
release gates and physical obsolete-code removal hold are unchanged. There is no
new user test batch, production publication, public SDK freeze or release claim.
