# Source submission and listing contract — E3b

**5 October 2026: forward 68 accepted within provisional author-tool scope.**
Together with [E3a](EXTENSION-AUTHORING-CLI-AUDIT.md), this closes E3's current
SDK/CLI lane. Next is E4 publishing; five delivery stages E4–E8 retain work.
This is not a public SDK freeze, production publishing or a release certificate.

## Delivered behavior

Native tools and remote MCP descriptors now use the same typed schema-1 source
submission. The CLI drafts metadata locally and the maintainer checker reads
the same shared parser. Neither runs the source, clones repositories, starts an
MCP server, authenticates, signs, uploads or publishes. The source profile is
deliberately bounded: canonical HTTPS GitHub owner/repository, a literal ASCII
tag subset and a full nonzero lowercase 40-character commit. Categories must be
exactly `tools`. License/contact remain bounded opaque text, not SPDX or identity
verification. Source/tag/commit ownership is E4 work.

`DESCRIPTION.md` is the mandatory user-listing snapshot, distinct from developer
README. It must contain visible UTF-8 Markdown within 64 KiB. Optional `media/`
is flat, at most six lowercase WebP/GIF files, each 4 MiB maximum, with a combined
16 MiB budget including the description. Symlinks, directories, invalid images,
unsafe/duplicate basenames and multiple cover assets refuse. Images have strict
2048-pixel width/height limits and a requested best-effort 16 MiB allocation
limit. Only the first frame is decoded. Rendering, full animation validation
and a process-wide memory cap are not certified. Decoding is sequential; only
description and asset metadata/hashes are retained in the returned snapshot.

Structured TOML serialization replaces manual interpolation. Required fields,
unknown fields, duplicate singleton flags, exact project/folder identity, bounded
bytes and SHA256 snapshots are checked. Validation precedes registry mutation.
The existing registry root must be real; a new submission folder is created
without overwriting existing submissions. DESCRIPTION is written first and
submission metadata is atomically published last as a completion marker. Failure
cleanup owns only that newly created folder. Author media stays in pinned source;
the submission records its exact hash/size and deterministic cover-first order.

The maintainer checker verifies submission shape and DESCRIPTION byte identity
through the shared reader. It does not independently obtain/check pinned media
or repository ownership. New source submissions refuse the old README-based
publisher before reading package/key inputs or creating output. The legacy
producer remains physically present under the user's deletion hold; this guard
does not certify the entire old publishing pipeline.

## Scope, dependencies and audit

Production changes are limited to `grain-sdk`, `grain-extension-checks`,
`grain-ext-cli`, `grain-registry-tools` and the root lockfile. SDK holds pure
metadata/validation; checker holds bounded filesystem/decoder work; CLI and
maintainer tools share it. Existing TOML 0.8, SHA2 0.10 and image 0.25.10 are
reused. WebP/GIF features add five locked decoder transitives: color_quant 1.1.0,
gif 0.14.2, image-webp 0.2.4, quick-error 2.0.1 and weezl 0.1.12. No new app
engine/dependency or Tauri lockfile change is introduced by this block.

Graph-first inspection and change/review guidance were used before targeted
reads. Review impact was broad/truncated (500 nodes/37 files) with imperfect
test linkage; graph guidance is not execution evidence. Scoped source review
covered serialization, flags, source references, project kind/API identity,
bounded reads, symlink/containment guards, hash snapshots, image limits,
mutation ordering, artifact cleanup, legacy publisher refusal and dependencies.
Audit repairs preserve DESCRIPTION UTF-8/content checks at the maintainer
boundary and propagate registry directory-read failures rather than silently
skipping them. No remaining finding blocks this bounded authoring unit.

Primary references informed the retained implementations and limits:

- Structured escaping/serialization follows the [TOML 1.0 specification](https://toml.io/en/v1.0.0).
- Literal tags use a stricter bounded subset of [Git reference-name rules](https://git-scm.com/docs/git-check-ref-format), without revision expressions or shell execution.
- The distinction between strict dimensions and best-effort allocation is explicit in [image 0.25.10 Limits](https://docs.rs/image/0.25.10/image/struct.Limits.html). First-frame decode is intentionally narrower than full animation/render acceptance.

## Verification

| Check | Final result |
|---|---|
| Locked Rust all-targets tests | **126 Pass:** SDK 83, CLI 13, checker 28, registry 2. |
| Node runner/client-metadata checks | **81 Pass**; no new harness scenarios or source changes. |
| Generated native contract, `author-CA5qGq` | **4/4 Pass**, 15 expected type refusals, compiler negative control and actual hello bundle. |
| Actual CLI/maintainer, `e3b-source-20261005` | Both native/MCP submits accepted; shared registry accepts two; changed DESCRIPTION produces expected exit 1; restored bytes accept again. Quoted/backslash contact round-trips. |
| Real app, `run-QBV7Mp` | **12/12 extension-contract Pass**: native API/results/compatibility/migration/packaging/ownership/auth, store integrity, MCP transport/provider independence and smoke. |
| Real app, `run-pY7paE` | **3/3 store-mcp Pass**: actual CLI descriptor generation/packaging, signed acquisition/update/removal, SDK accounts and revocation. |
| Fresh real app, `run-40RZxc` | **1/1 store.mcp-management Pass**, 8,212 ms scenario time, same final executable. |
| Build/static checks | Actual embedded real-app build/types and ordinary Vite production build Pass (7.66 s); Clippy Pass with two existing CLI conversion warnings and one existing registry doc indentation warning. Host build retains twelve existing feature warnings. Scoped format/whitespace checks Pass. |

Listing tests exercise actual WebP/GIF encoding/decoding, first-frame dimension
refusal, malformed/oversized/empty text, nested/unsupported/excess media,
hash/order and submission mismatch. The Unix-only symlink fixture was not run
on Windows; source guards were reviewed. Runtime store cases are controlled
Windows x64 Grain/Wry/WebView2 154.0.4258.53, not external browser/live-provider
or genuine-model effectiveness certificates.

Exact final identities (dirty precommit tree; docs added afterward):

- Base: `a108256f96261d9209bda61a7eb11cd6cebb1369`.
- App/CLI source fingerprint: `d6a7a4124268422ad05a2e25942891a8bf227f6a2fcc9629d7bb95cb2db57c0f`.
- App SHA256: `4eddab6d30142ac5c614dc0bac3a85da7d8432f7547e77eb29b7aadd74b2c536`.
- CLI SHA256: `a6105fda4d73d83085d1b4b1a0b73a876c5032009f908774de30460e48d81aeb`.
- Maintainer SHA256: `377ecf8eb344f17c48a03c03aa301658cf96b9c9e4656c9e9f9c1c9b91e4f546`.
- Runner fingerprint: `691b526b3738def98e06c89a6ff1d6a55ecca12e057f7a4c0a03a4871b437707`.

All three desktop scopes report cleanup Pass. Independent read-only inspection
checks **25 host PID references**, executable/root ownership and all eleven
recorded marker ports: no owned processes/listeners or data/fixture/TLS/author
scratch folders remain. Scoped native/MCP/configured grant inventories report
zero remaining credentials; MCP client-secret/registration inventories are
zero. PID reuse alone is not considered a surviving owner; the inspector kills
nothing. Ordinary accounts/profiles remain untouched.

## Retention, differences and next block

Keep the shared contracts/checkers and focused unit tests as product authoring
infrastructure. Reuse the existing native author-contract and real-app suites;
no additional runner case, instruction, fault mode or framework was needed.
Inventory remains **108 scenarios / 93 self-contained / 81 Node tests**.
Ignored `.runs/e3b-*` logs/inspection reports and generated author folders are
disposable nonsecret evidence. The actual source/registry fixture is deliberately
retained, marked owned/nonsecret, with no process/listener/credential. Its pointers
are synthetic; it proves parser/CLI integration, not actual Git source provenance.

An optional combined executable-test/recursive-cleanup command was rejected by
automatic approval review with only “blocked by policy” before execution. No
deletion occurred. The same substantive test was completed safely without
deletion; the generated source fixture is retained explicitly rather than
claiming its file cleanup passed. Prior interrupted-root exceptions remain
separate and unchanged.

The plan is refined at the producer boundary: local `submit` now supports both
kinds, while publication remains explicitly gated. E4 must migrate the separate
registry and catalog producer/consumer together; verify source/tag/full commit,
build packaging, media hashes and DESCRIPTION; protect signing/key custody;
and exercise update/revocation/serving provenance. The old private submission
producer type, README publisher and physical retired-capability branches remain
held until equivalent final paths and confidence permit removal. Updating an
existing source submission through this CLI is not yet supported. E5 owns
store/connection UI and safe description rendering. E6 runtime/recovery, E7
measurements and E8 platform/release certification remain.

Baseline **52 Pass / 1 Deferred (live expiry 12)** is unchanged; permanent OAuth
client hosting/eligible live acceptance **56 Pending** remains. No new human
test batch, UI redesign, public freeze, external repository edit or release.
See the [author guide](../../crates/grain-ext-cli/README.md) for commands.
