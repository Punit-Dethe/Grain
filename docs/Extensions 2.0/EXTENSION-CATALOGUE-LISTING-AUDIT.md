# DESCRIPTION catalogue producer/consumer — E4b2

**5 October 2026: forward 71 accepted for unsigned catalogue staging and
the scoped DESCRIPTION consumer checkpoint.** E4 publishing remains open;
five E4–E8 delivery stages retain work. See the [receiving audit](EXTENSION-PUBLISHING-HANDOFF-AUDIT.md),
[maintainer commands](../../crates/grain-registry-tools/README.md) and
[execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md).

## Implemented boundary

Signed `IndexEntry` now supports optional `listing: {sha256, size}` for a separate
user-facing DESCRIPTION. Existing `media` hashes, kinds, sizes and order carry
the pictures. Both native and MCP admission validate this new profile with shared
submission limits. New listings reject nonempty legacy README, bad hashes/kinds,
zero/oversized documents or images, excess images and total-budget overflow.
Absent listing preserves existing catalogue serialization and legacy behaviour.
The small new listing struct rejects unknown fields; catalogue envelopes retain
their established forward-compatible fields.

New `prepare-catalogue` reuses receiving validation and independently supplied
pins, stages exactly the checked artifact/description bytes, and rehashes each
bounded image while copying. It creates a new owned directory with content-addressed
native/MCP artifact and Markdown/image blobs, followed by `candidate.json`.
The candidate explicitly declares unsigned/not-reviewed, has **dev** trust,
empty review/author fields and no index envelope or signature. Source identity,
reviewed summary/listing/order and numeric required API minimum are retained.
Existing-output and input-overwrite refusals preserve bytes. Existing scoped
output cleanup is reused; no dependency, background service or testing engine is
added. Cleanup is best effort after ordinary failure, not a crash-safe transaction.

The store projects the new DESCRIPTION hash through the existing `readme` frontend
wire field and `store_readme` command. There is no new UI design or visual approval
assignment. Signed DESCRIPTION/image sizes, when available in the resident or
verified cached catalogue, bound disk/network reads and exact-length checks.
Hashes remain mandatory. Conflicting sizes for one content address refuse.
Closed-store detail loads drop their temporary parsed catalogue; no new resident
cache, thread or timer is retained. Legacy/hash-only media requests retain their
existing 16 MiB limit when no explicit signed listing reference is available;
this is not a new hash-membership access-control system. MCP card projection and
management remain intentionally deferred to E5, while MCP listing admission and
detail-document consumption are checked now.

## Audit and evidence

Graph-first context/search/impact preceded edits; change/review guidance preceded
the scoped source audit. Graph reports incomplete test linkage (risk .60 and
broad gaps including unchanged functions), not observed test failures. Audit
covered candidate authority, serialization compatibility, numeric API derivation,
fixed content-addressed paths, immutable input assumptions, copied-byte/hash/size
identity, filesystem ownership/cleanup, listing budgets, media cache/network reads
and catalogue lifetime. It tightened metadata string checks before cloning and
detects conflicting sizes across every matching media reference. No remaining
finding blocks this limited staging/consumer slice.

- **184 focused Rust tests Pass:** author CLI 13, checker 28, registry 17,
  SDK 85, core install 21, core trust 9 and normal store 11. The store's separate
  live-network test remains ignored. New tests exercise both-kind candidate
  bytes/media/authority, refusal/no-clobber, legacy/new listing round-trip,
  invalid/bounded metadata and consumer projection/size policy. Final SDK/registry
  and normal store repeats pass after audit changes.
- **81 Node checks Pass:** existing runner 78 and client-metadata 3. No scenario,
  runner framework, model instruction, bridge or fault mode was added. Three
  existing store fixture/handler files gained the explicit document and exact
  read/hash-refusal assertions.
- Actual registry command: **6/6 expected verdicts**, native/MCP staging plus
  both existing-output and wrong-pin refusals. Independently checked artifact
  and DESCRIPTION hashes/sizes, dev/empty review metadata, numeric minimum,
  absence of index/signature and refusal preservation. Unit fixtures cover
  actual images; executable fixtures have none.
- Real application: **3/3 MCP store** (`run-ueUUKF`) and **3/3 native store**
  (`run-UJYHlc`) pass. MCP management rejects a corrupted DESCRIPTION with no
  prior cached copy, then reads the correct document; native projection/read
  checks pass alongside existing install/update/revocation/close/offline/restart
  procedures. These are controlled signed fixtures/scripted model, not live CI
  publication or provider authentication. All six existing cases retain their
  separate verdicts and normal deadlines.
- Locked tool/host build, frontend type check/production asset build, tool and
  normal backend Clippy, SDK/maintainer Rustfmt, fixture Prettier and scoped whitespace checks pass.
  Existing compiler/lint warnings remain; no new warning is accepted. No public
  SDK freeze, dependency replacement, production signing or deployment occurred.

The first backend Clippy invocation used root working-directory/environment
instead of the established short-target backend setup and failed in the unrelated
transcribe CMake build script with Windows error 267. Its log is retained without
acceptance credit; the corrected backend working-directory/environment run is
the lint evidence. No CMake/dependency or application source fix was needed.

Evidence is based on dirty precommit `68777d4eb07ff4f51294453de286fcbd4e66497d`,
Windows x64. Host SHA256:
`8fd7170434e5b0f088ee4c039c25dfefb40a612074786e503581f83d7ba26d5c`;
source fingerprint `3dee1212c45cc1aeadb9fa3cf3241c0873f581590904e387355b843b527b4b6f`.
Registry SHA256 `c6f71ec1c60317ad0e52c015b2cb650826186ae9f0494b037f7f1d781202a9f3`.
Author CLI SHA256 `9b1ad5f70aefc280c7cd03d3dfee64d319e8136981bb50f7bcb34bf1e56b65e4`.
Ignored `.runs/e4b2-*` logs and `e4b2-catalogue-8faa2e7d/report.json` retain
component/executable evidence; normal final store report is `logic-7EV4dy`.
Both desktop scopes report cleanup Pass. Independent `.runs/e4b2-inspection.json`
checks **12 host / 3 author CLI PID references**, zero remaining matching/scoped
processes, zero listeners on the three recorded fixture-port references and no
data/webview directories. All six synchronous registry children exited as well.
The marked nonsecret candidate scope is explicitly retained, not file-cleanup Pass.
No source repository, ordinary account or extension setting is modified by staging.
The pre-existing bindings edit is preserved byte-for-byte; nested registry edits
and physical-removal hold remain untouched.

## Next handoff

E4b3 must establish independently verified CI/source/workflow/review policy and
the signing-only boundary, then validate and consume the complete candidate/blob
set while assigning legitimate review/trust and catalogue envelope metadata.
Use maintained attestation verification as described in the receiving audit;
receipt/pin/candidate fields cannot certify themselves. Keep the old publication
guard until that full path is accepted. Separate registry workflow/serving,
key custody/protections and update/revocation acceptance remain E4 work, preserving
that repository's current edits on a separate appropriate branch.

Baseline **52 Pass / 1 Deferred (12)**, full **56 Pending**, inventory
**108 scenarios / 93 self-contained / 81 Node**, public-hosting/live acceptance
and all release gates are unchanged. No new human batch. This closes a scoped
implementation checkpoint, not E4 publishing or production readiness.
