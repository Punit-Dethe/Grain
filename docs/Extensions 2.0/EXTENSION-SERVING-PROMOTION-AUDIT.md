# Complete serving snapshots and serialized local promotion — E4c

6 October 2026. Local assembly/promotion is implemented and verified. **This is
not production hosting, approved publication or completion of E4.** Latest numbered
forward acceptance remains 71; five E4–E8 delivery stages retain work. Existing
52 Pass / 1 Deferred (12), full public-hosting 56 Pending and inventory
108 scenario IDs / 93 self-contained / 81 Node self-tests remain unchanged.

## What changed in plain language

Previously, signing produced the new extension files and updated catalogue, but
not everything a registry needs to serve. The new maintainer commands build a
complete snapshot containing both the new files and all files still referenced
by the signed catalogue. They check the actual application's trust chain and
every addressed file before accepting that snapshot.

A local store retains complete snapshots and selects one using a small pointer.
Only one cooperating publisher can change that pointer at a time. If the current
snapshot changed since an operator captured it, an older update refuses rather
than overwriting newer work. Failed verification leaves the old selection intact.
This code is product maintainer tooling, not more Agent harness infrastructure.

## Implementation and trust boundaries

Application source: `af28a2434506181336d00978be9fa7d2397e08a8` on
`extensions/tool-only-retirement`. Registry checkpoint source:
`4b3a6fd3d5bc02f12d2e7190f8b107f9fb5d86b1` on `registry/trusted-provenance`.
The checkpoint pins the former full commit, Rust 1.96.0, locked dependencies and
immutable checkout/upload action revisions. No new production dependency,
background service, server, desktop command, UI or Agent runner was introduced.

Four `grain-registry` commands reuse existing core/SDK verification:

- `verify-serving-tree --v1 PATH` validates a complete snapshot.
- `assemble-serving-tree --base PATH --update PATH --out FRESH_PATH` merges a
  signed fragment with its signed base and verifies the complete result.
- `initialize-serving-store --v1 PATH --out FRESH_PATH` bootstraps a local store.
- `promote-serving-tree --assembly PATH --store PATH
  --expected-current-sha256 DIGEST` performs serialized conditional selection.

Production commands verify roots against Grain's actual pinned root keys; roots
authorize the publishing key used for index and revocations. There is no CLI
anchor override. Unsigned `promotion.json` binds the exact base metadata JSON
digests; it never grants review approval or signature trust. These commands never
open a key, sign data, run author code, invoke a model or change hosting.

Complete `v1/` contains roots/index/revocations JSON and signatures, plus real
`blob/` and `media/` directories. Native packages, MCP descriptors, separate
DESCRIPTION/legacy README and pictures must match authenticated hash/size
references. Missing, changed, linked, unexpected, oversized or unsafe files,
duplicate extension versions, unsupported specifications and unpublishable
dev/experimental trust refuse. Raw signed JSON and future fields survive exactly.
Existing package/descriptor semantics stay with preparation/receiving/host;
serving does not introduce a second semantic validator or reactivate old entries.

Bounds: 8,192 unique addressed assets, 1 GiB total, existing package/descriptor
limits, 64 KiB detail documents, 4 MiB pictures, 4 MiB index/revocation documents,
64 KiB roots and 8 KiB signatures. Assets use one 64 KiB streaming buffer; the
same bytes are hashed and copied, then synchronized. Shared paths cannot carry
conflicting explicit sizes. No entire asset cache is kept in memory.

New metadata must be strictly fresh, without seed or clock-skew exemption.
Authenticated expired history is permitted only as a base for fresh renewal.
Versions cannot roll back or replace different metadata JSON at the same version.
Versions present in both catalogues cannot change artifact/listing hashes or media.
Existing revocation rules cannot disappear or weaken, including all-version rules.
Key rotation needs root-authorized new key metadata and matching new signatures.

## Atomic selection, resource lifetime and limits

Store layout is `snapshots/<complete-inventory-sha256>/`, `current.json` and an
empty persistent `promotion.lock`. The snapshot identity includes all six exact
metadata/signature files and authenticated asset hashes. Promotion takes a
nonblocking exclusive standard OS file lock and, under that lock, verifies the
independent digest of the entire current pointer, the selected snapshot and the
assembly's exact parent before enforcing transition rules.

A full snapshot enters owned temporary staging, is verified again and renamed
before a synchronized temporary file atomically replaces `current.json`. An
existing destination snapshot is reverified; an incomplete crash leftover cannot
be selected simply because it exists. No-op repeats refuse. Old snapshots and
withdrawn assets remain; no garbage collector or automatic retry was added.

Lock handles and temporary staging drop on completion/failure. Never unlink the
lock file to force progress: cooperating callers must lock the same inode. Inputs
and installed snapshots must remain operator-owned and immutable; this is not a
sandbox against arbitrary same-account mutation. The lock coordinates one local
filesystem, not distributed writers or network storage. Unix directories are
synchronized. Windows atomic replacement and locking are verified, but power-loss
durability on every filesystem is not certified. A directory-sync error after
pointer commit explicitly requires inspecting the pointer before retry.

## Verification and retained evidence

Windows: locked full registry suite **36 Pass** before the final transition-map
optimization; **10/10 affected serving tests Pass** after it. Final locked build,
workspace format check and all-target registry Clippy with `-D warnings` pass.
The new tests use real minisign signatures with disposable test-only roots and
ordinary filesystem operations; they cover:

- Native/MCP complete assembly and exact signed-byte/future-field preservation.
- Missing/altered/extra/oversized/traversal/size/duplicate refusal.
- Signature/spec/strict expiry checks and expired-history renewal.
- Actual pointer promotion, stale/forged parent refusal and retained old snapshot.
- Lock contention/recovery and valid or corrupted unreferenced crash snapshots.
- Metadata rollback/reissue refusal and retained all-version revocation strength.
- Root-authorized publishing-key rotation and withdrawn-asset retention.
- Unix-only symlinked asset and directory refusal.

Final Windows executable SHA256:
`4b0d95b5be004ed7a62d035b14ebbfbf6b24b172ebadb9e9ff767bd4c3d4077e`.
The actual public CLI passes **16/16 separate expected verdicts** in retained
`.runs/e4c-serving-02/`: genuine pinned-root seed verification, assembly and store
initialization; fresh-output/input-containment/stale/no-op refusals; an independent
Python-held Windows lock and release recovery; three signed-document tamper
refusals; extra-asset and pointer-traversal refusals. The selected pointer remains
byte-identical after refusals. Earlier `.runs/e4c-serving-01/` records the same
checks against the pre-optimization binary and is historical evidence.

Linux checkpoint [37417629606](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37417629606)
is **successful**: **11/11 affected Rust tests**, including Unix symlinks, exact
production CLI build and **16/16 actual CLI verdicts** with independent Python
`fcntl.flock` contention. Reports record source,
executable hash, exit codes and command PIDs; nonsecret evidence is retained seven
days. Its permission is read-only contents, with no OIDC, publishing secret,
deployment or author build. CLI positive bootstrap uses the genuine signed empty
application seed. Nonempty changed promotion uses real component signatures;
neither is a positive protected live publication.

Linux executable SHA256:
`7076c2ee442424041f201000c296bfce33293801035e12e8040c98ae3836cc77`.
Evidence artifact `11391872964`, SHA256
`c8b82ed9e925f484c3671112e39b98a80a54746ed0e3311e5cd092aa2a5a9f3d`,
is retained seven days. Downloaded reports/logs/snapshots are preserved under
`.runs/e4c-serving-linux-01/` and were inspected as data, never executed. Every
required CI step completed successfully against the recorded exact source pins.

Initial two fixture failures were correct same-version refusal: separately created
fixtures had different expiry bytes. Deterministic timestamps corrected the
fixtures without weakening the guard. Clippy's needless-borrow correction and
the bounded-map transition optimization received affected repeats. No failing
fixture attempt is counted as acceptance.

Independent inspection found zero remaining owned CLI processes across 32 recorded
Windows command PID references, and no keys in either evidence directory. Unit
test keypairs live only in RAII temporary directories. Retained scripts/logs/local
snapshots are nonsecret disposable evidence, **not file-deletion Pass**. No account,
listener, model run or desktop profile was created for this block.

## Scoped audit, remaining production gates and next work

Graph-first context and change/review tools were used; new/unindexed serving/CI
files were inspected directly. Scoped review covered exact trust roots and bytes,
bounded metadata/streaming, path containment, signed transition semantics, pointer
CAS and lock lifetime, crash leftovers, temporary cleanup, source-pinned CI and
honest separation from review/deployment authority. Existing bindings and nested
registry edits remain unchanged. The Handy backend, UI and obsolete-code hold
were not touched.

Local acceptance closes the missing complete-tree/conditional-selection primitive,
not REG/E4 production governance. The next coherent E4 unit must resolve:

1. Hosting adapter: capture/verify one selected snapshot, coherent metadata and
   root rotation, stable availability of historical content hashes, independently
   conditional deployment and verified canonical base URLs.
2. Historical version policy: the present transition compares the immediate base
   and next catalogue. A withdrawn then reintroduced `(id, version)` needs a
   protected historical identity policy before public publication; retained
   snapshots alone are not that policy. No global-history guarantee is claimed.
3. Actual independent protected review-to-signing, branch/environment protections,
   pinned trusted workflow rollout, production key custody and operational
   renewal/revocation procedures. These checks require real protected authority,
   not self-review or generated fixture approval.

[Registry PR 1](https://github.com/Punit-Dethe/Grain-Extention/pull/1) remains draft
and unmerged. The legacy `publish.yml` remains active but incomplete and untouched;
the existing `build-and-check` still fails on missing `ci/read-submission.sh`.
Passing this isolated lane does not make all PR checks green or authorize merging
into that publisher. Production hosting identity is still undecided. Live provider
expiry test 12 stays explicitly Deferred. E5 management/store UI, E6 remaining
runtime modules, E7 discovery measurements and E8 certification remain later.

No new user testing batch. Reuse existing signed-store acceptance when actual
host/serving integration changes; focused CLI/component tests suffice here.
Cleanup ledger: `.runs/e4c-serving-01/` and `-02/` are disposable retained evidence.
The branch-only `serving-tree-checkpoint.yml` wrapper is temporary verification;
move its checks into the supported maintainer CI before retiring the wrapper.
Retain the focused product tests. Never treat retention or deferred release gates
as new completed application tests.

Primary references: [Rust standard file locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock),
[tempfile atomic persistence](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist).
