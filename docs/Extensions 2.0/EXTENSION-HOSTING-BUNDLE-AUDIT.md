# Archive-complete static hosting handoff — E4e

6 October 2026. The bounded local hosting-bundle producer and independent verifier
are implemented and accepted within maintainer scope. **No host was selected,
configured or activated; E4 and production certification remain open.** This
follows [E4d historical reservation/export](EXTENSION-SERVING-HISTORY-EXPORT-AUDIT.md).

## Delivered behavior

The earlier single-snapshot export omitted withdrawn artifacts. A static hosting
handoff now contains fresh selected metadata plus all addressed files retained in
the operator-owned snapshot store. Signed historical proofs authenticate those
files without making historical extension listings active. Old clients can retain
their addressed references once the future host preserves this route layout.

Two product maintainer commands were added:

```powershell
grain-registry export-hosting-bundle --store STORE --out FRESH_BUNDLE --expected-current-sha256 CURRENT_POINTER_DIGEST
grain-registry verify-hosting-bundle --bundle BUNDLE --expected-receipt-sha256 INDEPENDENT_RECEIPT_DIGEST
```

The producer prints the receipt SHA256. A protected handoff captures that pin;
merely hashing an untrusted receipt is not an independent pin. Commands verify
the application's actual pinned root chain and never grant review authority.
There is no custom-anchor CLI, signing/key access, networking, deployment, author
execution, model call or new service/dependency. Existing desktop/Agent/auth/SDK
paths are unchanged.

Application source `7eed2f1f6a6f1048fc4b3e11bc66e2714ac6fe84`, registry CI source
`4bdb747e703ca30e9fcf888bf6d34cc23659ddc3`, on the existing dedicated branches.
The Linux workflow pins exact application source, Rust 1.96.0, locked dependencies
and immutable actions; it extends the existing checkpoint, not the Agent runner.

## Layout, bindings and selected versus historical data

Bundle layout:

```text
bundle.json                  bounded unsigned receipt
current.json                 exact captured local pointer bytes
v1/
  roots/index/revocations JSON and detached signatures (selected)
  blob/                      union of native/MCP addressed artifacts
  media/                     union of addressed documents/images
history/<snapshot-digest>/   six original signed metadata/signature files
```

The selected index alone defines active listings. History includes all retained
installed snapshots other than selected, including conservatively reserved signed
orphans; unsigned `.tmp*` staging remains inert. Existing `verify-serving-tree`
still verifies a single snapshot and correctly refuses unreferenced archive files.
Use the archive-aware verifier for this bundle layout.

The independent receipt digest binds exact selected state, captured pointer digest
and sorted, unique, safe history membership. Exact `current.json` bytes must hash
to that digest and parse to the selected state. Fresh selected metadata must match
that state. Each old proof has its own app-pinned signed roots and publishing key;
its complete metadata/reference inventory identity must match the directory name.
Raw JSON/signature bytes and future fields are preserved. Old signed proof expiry
is historical evidence only; active expiry remains strict.

The verifier reconstructs the full addressed union, rejects conflicting explicit
sizes and streams every required asset to verify its signed hash/size. Duplicate
paths copy once. Unsafe/duplicate history IDs, links, missing/tampered/extra assets,
proofs, directories, pointer, receipt or selected metadata refuse. A receipt cannot
override signed selection, freshness or trust. Consistency also checks historical
version identities against one another, including versions absent from today's
catalogue: contradictory old artifact/listing/type identities refuse.

## Resource lifetime and supported limits

The same nonblocking exclusive store lock spans pointer capture, signed history
inspection, streaming copy and final independent verification. Shared history
traversal was extracted from the existing promotion guard; there is no duplicate
history database or new background engine. One bounded catalogue and one 64 KiB
file buffer are processed at a time. Only bounded path/hash and version-identity
maps remain for the operation; old package bodies are not cached.

Maintainer profile limits: 8,192 unique assets and extension-version identities,
1 GiB total including proofs/receipt/pointer, 4,096 installed snapshots including
selected, 128 MiB aggregate signed metadata and a 384 KiB receipt. Existing per-file
limits remain. Exhaustion refuses; long-lived larger archives require a separately
audited archival/deployment design preserving identity and old references. This is
not an unbounded registry-lifetime claim or a reason to delete old snapshots.

Output must be fresh and outside the store. Incomplete output uses existing RAII
cleanup. Files and Unix directories synchronize before completion; the E4c
Windows power-loss limitation remains. Inputs, pointer and old exports are never
changed by this producer. Locks/temp resources drop on completion or failure.
No runtime listener/account/model/profile was created.

## Tests, executable evidence and audit

Windows **21/21 affected Rust tests Pass**: fifteen prior serving tests and six
new bundle tests using real signatures and ordinary filesystems. New coverage:

1. Actual native/MCP install → withdrawal → archive export; exact active metadata,
   all withdrawn bytes and signed proof preservation; stable old bundle after a
   later promotion and renewed bundle with the entire addressed history.
2. Missing/changed archive asset, pointer, selected/history signatures, removed
   proof and extra root/asset/proof inputs refuse.
3. Wrong/stale/modified receipt pins and re-pinned malformed/forged selection,
   traversal or duplicate history refuse independently of signature validation.
4. Corrupt historical source removes only owned unfinished output; contention,
   stale/inside/overwrite refusals preserve the current pointer and prior outputs.
5. Conflicting signed legacy/imported histories refuse even when both affected
   versions are currently withdrawn. This fixture models inconsistent history,
   not an admitted promotion or a production import.
6. Rotated current publishing key with expired historical proofs succeeds; an
   actually signed but expired active catalogue refuses even with a new receipt.

Locked Windows build, workspace format and all-target registry Clippy with
`-D warnings` pass. Actual Windows CLI **39/39 separate expected verdicts** reuses
the previous twenty-five checks and adds fourteen hosting/verifier cases, with
independent Python-held OS lock contention. Exact pointer/receipt state and
unchanged current bytes are checked. Public CLI uses genuine signed app seed
bootstrap; nonempty archive/rotation/history uses real component signatures and
disposable roots. Neither is approved positive live publication.

Windows executable SHA256:
`1034157275a41b3d48fed03c9de543f5f6d451659d4df702e94ff09f1bfa9bf7`.
`.runs/e4e-hosting-bundle-01/` retains nonsecret scripts/logs/snapshots/reports.
Independent inspection finds zero remaining owned CLI processes across 39 recorded
command PID references and zero keys in this directory. Rust fixture keypairs are
owned temporary test data. Retention is not file-deletion Pass.

Linux [CI 37420673705](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37420673705)
is **successful**: **22/22 affected Rust tests**, including Unix symlinks, exact
locked production CLI build and **39/39 actual CLI verdicts** with independent
`fcntl.flock`. All required steps passed at the recorded exact source pins. No OIDC,
publishing credential, author build or hosting action exists in this lane.

Linux executable SHA256:
`344c56065a62e666b7e7492a1d0c6cb942bb81ff3d1b60331acc48a68602abb0`.
Evidence artifact `11393460042`, SHA256
`bcbedde90ad6f0914409bda42d968b53b0f7495bd841245c7e81f258a32c3fbb`,
has seven-day retention. `.runs/e4e-hosting-bundle-linux-01/` retains downloaded
logs/reports/nonsecret snapshots, inspected only as data and never executed.

Graph-first context/file summary and change/review context preceded direct review
of new unindexed files. Scoped audit checked pointer/receipt provenance, actual
root/signature bindings, archive membership, cross-history identities, deduplication,
expiry/rotation separation, resource bounds, lock lifetime and cleanup. Audit
refinements added exact original pointer-byte binding and cross-history conflict
refusal before final tests. No failed test attempt is counted as acceptance. Existing
root bindings/nested registry edits and the Handy tree remain untouched.

## Remaining E4 work, deployment contract and cleanup ledger

The historical-content preparation gap is closed locally; actual hosted retention
and metadata activation are not accepted by an offline bundle. Future hosting must
preserve the signed base URL's `blob/` and `media/` routes, retain immutable hashes,
activate metadata coherently and use its own conditional activation mechanism.
No signed base URL/bootstrap was changed. Flattening these files into GitHub
Release assets does not implement the client's nested path contract.

Remaining: canonical host selection/route verification and conditional deployment,
prior protected history migration, actual independently approved review-to-signing,
enforced branch/environment/workflow protections and production key custody,
operational renewal/revocation. A local filesystem lock is not distributed
coordination. Do not merge the draft into the active incomplete legacy publisher;
the missing-helper legacy check remains a separate failed gate. No main/admin/key
change, production activation, public contract freeze or obsolete-code deletion.

No new user batch or Agent harness scenario/runner/bridge/model source. Baseline
**52 Pass / 1 Deferred (12)**, full **56 Pending**, latest numbered forward **71**
and inventory **108 / 93 / 81** remain. **Five E4–E8 stages retain work.** Use existing
real-store acceptance when actual host integration changes. Later E5 UI, E6 runtime
modules, E7 measurements/reference integrations and E8 certification retain order.

Cleanup ledger: `.runs/e4e-hosting-bundle-01/` and downloaded Linux evidence are
disposable nonsecret retained data; no new general harness. Keep product tests.
Retire the branch-only checkpoint wrapper when supported maintainer CI owns these
assertions. Earlier source/fixture/physical-removal holds remain unchanged.

Primary references: [TUF consistent-addressing specification](https://theupdateframework.github.io/specification/)
informs immutable asset retention; this remains Grain's existing signing format.
[S3 conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html)
illustrate a host-provided concurrency gate for future deployment, not a selected
hosting provider, new cloud dependency or completed remote-write implementation.
