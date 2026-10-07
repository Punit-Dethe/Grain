# Hosted metadata generation binding

Date: 8 October 2026. Stage: E4. This unit connects the publisher and app
contract; it does not activate the public catalogue or approve production keys.

## Product behavior

`index.json` now carries two hashes inside its existing signed envelope:

```json
"generation": {
  "roots_sha256": "SHA-256 of exact roots.json bytes",
  "revocations_sha256": "SHA-256 of exact revocations.json bytes"
}
```

The catalogue's signature authorizes those exact companions. All three role
signatures, version identity/floors, size limits and expiration checks remain
required. Hashes cover JSON bytes, not detached signature comments. Semantically
identical JSON with different formatting is a different document. A companion
change requires a newly bound catalogue with an incremented catalogue version.

This is a compact extension to the existing contract, not a new framework:
no fourth metadata file, signing role, worker, database, timer or resident idle
catalogue. Both native and MCP entries use the same publication envelope; the
author-facing native tools/MCP descriptor contract is unchanged.

## Connected producer and consumer paths

- SDK describes `MetadataGeneration`; core constructs/verifies exact hashes.
  Missing binding is representable solely to read the embedded bootstrap seed;
  the acquisition verifier has no seed exemption.
- Bootstrap first constructs the new revocation bytes, then binds/signs the
  catalogue. Renewal rebuilds that binding after advancing both versions and
  expirations, preserving entry/policy/future envelope data. Existing byte
  limits are rechecked after adding the binding and before reading the key.
- Reviewed source signing requires a bound prior catalogue and preserves its
  binding when companion bytes remain unchanged. Complete serving verification
  checks that proof later; a hash string is never itself publication approval.
  The pre-existing general maintainer mutations bind authenticated fresh
  companion bytes before signing their catalogue.
- Assembly, immutable local snapshots, promotion, hosting receipts/history,
  committed Git capture and initial/update publication gates share the complete
  metadata verifier. An unsigned receipt cannot override signed companions.
- The initial gate compares the new binding against exact seed-derived roots
  and policy in addition to the previous policy/version/lifetime checks.
- App refresh verifies the generation against the exact root bytes it would
  persist and the selected authenticated revocation bytes, before fresh cache
  commit and acquisition approval. A stronger selected policy cannot be
  replaced by an older response just to satisfy an index's binding.
- A current-key authenticated kill switch may still advance negative policy on
  mismatch. Its policy-only cache commit retains the prior catalogue/floor and
  remains offline. This intentionally need not be an installable generation.
  Restart keeps independently valid policy/floors; the next acquisition still
  requires a matching fresh publication. Bad signatures never gain authority.

The exact embedded six-piece seed is allowed only by private bootstrap and
initial-gate input paths. Normal serving verification, initialization, assembly,
renewal, hosting/export/capture and app network acquisition refuse it as an
unbound publication. This is a clean break, not a legacy compatibility route.

## Verification and audit

Graph-first exploration/impact and scoped change/review calls were followed by
direct caller/diff inspection: the graph is overinclusive and does not reliably
identify these tests. Review covered seed exception reachability, binding before
key read/signature, exact output bytes, root rotation, negative-policy handling,
durable selection before app authority and resource cleanup.

Windows product checks:

- **300 core**, **85 SDK**, **77 maintainer** tests pass. New coverage checks
  missing/malformed binding, exact byte identity, genuinely signed companions
  from different publications and unchanged selection/output on refusal.
  Existing signed bootstrap/renewal, root-key rotation, native/MCP catalogue,
  hosting/history, committed capture and conditional Git race checks pass.
- **11 normal backend store tests pass / 1 existing live-network test ignored**:
  `tests/agent-harness/.runs/logic-JtXtvy/evidence/report.json`. The signed refresh
  matrix has **14 Windows modes**, adding unbound-index and independently
  signed mixed-policy cases. Kill switches apply and installations stay off;
  rollback, clock/expiry, cancellation and actual Windows sharing-lock failure
  checks remain. Public test signatures are refused by normal production trust.
- **13 actual public CLI checks**:
  `tests/agent-harness/.runs/e4-generation-cli-02/report.json`. They verify input
  boundaries and refusal of unbound seed initialization/assembly/renewal, plus
  explicit bootstrap key/lifetime admission. No output, key or remote write.
  The old signature-only `verify` CLI now delegates to complete serving
  verification; its exclusive display helper is removed. Signed fixture files
  are byte-exact in Git on every platform.
- **4 existing Python publisher tests** pass, including actual disposable
  conditional push, competing writers and no retry on unknown outcome.
- **78 existing runner self-tests** pass. The existing store fixture now binds
  its fixed public-test companion bytes; revocation changes advance the index
  and binding. An existing self-test checks the exact served revocation hash.
  No new scenario, acceptance ID, general harness framework or manual batch.
- Strict core/maintainer all-target Clippy and SDK production-library Clippy
  pass. SDK all-target strict lint separately finds two existing unchanged
  `manifest.rs` test warnings (needless borrow and empty doc-comment lines);
  they were not suppressed or represented as a passing full-SDK lint check.
- Locked real-app build, TypeScript, Vite and scoped Rust/JS formatting pass.
  The build retains the existing 12 backend warnings.

Both existing real-app suites pass **3/3 each** on final source commit
`9c6e15df47932fddc688ad503dbb8173b6b36f8e`: native/store
`run-VMfX26` and store/MCP `run-wDUY1i` under
`tests/agent-harness/.runs/`, with distinct case verdicts and cleanup Pass.
Independent inspection finds no remaining owned host among their 12 PID
references and no listener on either owned store port. Ordinary profiles,
settings and the unrelated `src/app/bindings.ts` bytes remain untouched.
Both reports use host SHA-256
`615000e2380af9efaf29866a71558bd5f487a5407567c05d04ca4709e36473f6`
and source fingerprint
`970d8b82563fe3a3b826fed432b43b7b9f1e283589912099590c5126a1ef7de5`.
Earlier `run-AVIaCm` / `run-wnB2Ha` also passed; these final runs follow the
exclusive old verifier cleanup and record the committed source explicitly.
Verified [Linux checkpoint 37681876972](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37681876972)
uses workflow commit `11049e5a1c21ee24bb34c09910b46f85a32593bf` and exact Grain
source `9c6e15df47932fddc688ad503dbb8173b6b36f8e`. Downloaded evidence under
`tests/agent-harness/.runs/e4-generation-linux-01/` confirms **78 maintainer /
12 core trust / 5 cache / 4 publisher / 13 public CLI** checks. All 13 CLI
label/exit/expected verdicts match Windows. Checkout logs record the actual
source SHA; `verification.json` records the independent comparison.
Linux tool SHA-256 is
`5951415995ee1c7dc01fe22c5cfd5a5b3aebe116016aa0a99dffc46842811944`.
Artifact ID is `11509850623`; the service-reported archive digest is
`f1223ae3c67538d08ada2755338d74e037ffa15b52668bf19a83726077112708`.
This certifies product/CLI behavior on Linux, not Linux desktop GUI acceptance.

The draft registry branch's publication verifier and read-only checkpoint are
pinned to this source; actionlint passes. The isolated source builder remains
on its independently verified `ed55bf1df86dcd994fb43cecdb8f1abf033de8d7` producer
pin because source preparation/build/attestation contracts are unchanged. It
does not sign or publish catalogues. No publisher job, key, main merge,
protection/environment change or public activation was performed.

Earlier failed development runs remain recorded. They found missing new fields
in synthetic test constructors, stale signed fixtures after companion mutation,
one missing Node test import and changed offline test-anchor expectations.
Those fixtures/assertions were corrected without weakening generation refusal.

## Test maintenance and remaining limits

The old Linux checkpoint's 67 seed-based CLI cases and six former first-publish
CLI cases depended on serving an **unbound** empty seed. They
are historical, not current acceptance. Their redundant seed/no-op shell lane
is retired; the existing shared public-CLI script now has 13 relevant boundary
verdicts. Genuine signed positive flows remain covered in product regressions,
including the Linux-specific filesystem guard. This removes obsolete testing
code instead of adding a replacement application or a public test trust bypass.

The small branch-only checkpoint wrapper remains cleanup debt: retire it when
normal maintained product CI owns these regressions. The pre-existing general
maintainer commands and other old-contract-only paths still need their own
scoped cleanup; this unit does not extend extension compatibility or migrations.

The design adopts authenticated companion-hash binding from [TUF's snapshot
rules](https://theupdateframework.github.io/specification/v1.0.36/). It is **not
TUF compliance**: no threshold/delegation framework or new freshness role is
claimed. Existing [minisign verification](https://docs.rs/minisign-verify/0.2.5/minisign_verify/)
is reused; no crypto implementation/dependency or new signing key is added.

Public operator signing/custody, repository protections, actual signed HTTP/app
activation and permanent OAuth client identity remain separate release gates.
GitHub raw remains the catalogue host; the experimental website is parked.
Foundation **52 Pass / 1 Deferred** (live token expiry), forward **71**, identity
criterion **56 Pending** and inventory **108 / 93 / 81** remain unchanged.
E4 remains current; E5 management UI, E6 Agent modules, E7 measured integration
and E8 release readiness follow. No new user testing is required for this unit.
