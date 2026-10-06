# E4j: archive-bound empty migration and continuing publication

7 October 2026. Maintainer implementation and scoped audit. Production signing,
authorization, activation and hosted-client acceptance remain open.

## What this completes

The saved legacy history is now consumed by the publishing path. The maintainer
verifier authenticates every original signed proof and referenced file, then
recomputes version reservations and conflicts. An independently pinned receipt
cannot authorize forged unsigned reservations; signatures/hashes still govern.
Git commit/source labels are verified by recapturing original committed history
at the initial publication gate, not inferred from an offline receipt.

An explicit publisher signer creates a fresh **empty active catalogue**, keeping
the original signed roots. It advances above the highest historical index and
preserves/advances the app's genuine signed revocation seed. All historical
versions receive current version-specific retirements. For the actual pinned
history: index 40, revocations 2, 13 retired version identities. The private
production publisher key was not accessed and these production documents have
not been signed; successful signing uses disposable real keys/signatures only.

Both signed documents bind `legacy_history_sha256`. Initialization copies one
bounded verified archive into the store. Normal promotion, renewal, snapshot and
hosting export, hosting verification, raw Git capture and continuing publication
retain it. Reusing any old `(id, version)`, removing/changing the binding, dropping
retirements, altering original proof/assets or adding unreviewed archive files
refuses. Future tool-only versions remain possible through normal source review.

The hosting bundle preserves original proofs and all historical addressed routes
under `v1/blob/` and `v1/media/` without listing old extensions. These bytes remain
historical; active SDK/runtime policy still rejects retired capabilities. The app
does not gain an archive-fetching service. Artifact copies stream; hosting's
existing 1 GiB aggregate budget includes the archive and public retained copies.
Input/output and Git process ownership/cleanup remain bounded.

The initial `prepare-github-migration` gate requires the archive's exact legacy
tip as the candidate's sole parent, unchanged roots, empty next-version metadata,
no invented earlier six-document history, complete recaptured legacy ancestry
and exact committed candidate bytes. It emits a distinct expected-base lease
handoff without authenticating, pushing or granting release approval. Later
publication uses the ordinary gate with complete previous/current proof.

## Audit findings resolved in this block

1. Grain ships signed revocations version 1, although the old public registry
   lacks that document. Initial migration must preserve seeded rules and advance
   to version 2; replacing content under version 1 would be inconsistent.
2. Signed index and revocations must bind the same archive, and every retained
   snapshot must preserve that binding. Unsigned archive admission is rejected.
3. The copying path validates manifest commit labels before constructing output
   paths, reauthenticates copied bytes, and cleans only unfinished owned output.
4. Hosting budget checks run even when the addressed asset pool is empty.
5. Windows debug CLI command construction overflowed the default main-thread
   stack after commands were added. Separating five argument builders with
   Clap `Args` and flattening migration commands preserves public names without
   increasing stack allocation or adding a thread. Real `--help` is now an
   explicit executable regression. A proposed newer derive `defer` attribute
   was unsupported by the locked derive version and was discarded.

Graph change/review queries were used; their new-module/test coverage was
incomplete. Acceptance therefore uses direct scoped source/diff review and real
executable tests. No whole-platform audit or production-ready claim is made.

## Verification and evidence

Windows: **95/95 full maintainer tests**, including **15 connected migration
regressions**; **9/9 active core trust tests**; **25/25 public CLI checks** against
the real independently pinned legacy archive. Locked build, strict Clippy and
scoped format/diff checks pass. Signing/store/update/renewal/hosting/capture and
initial/continuing Git handoffs are exercised with real disposable signatures
and Git repositories; malformed inputs preserve selected pointers and inputs.

The public checkpoint creates no key and signs no production document. It
rechecks all **109 actual proof/asset files** against Git. The original receipt
remains `25f8e8cb9484a0301ec5bdb34bb9eaf4fbbc82a4f84bf1c196c2eed1338a7532`;
manifest remains `6cad006f21b54e7041921dd68db0f8acf30932c0a8617a82a3b671a04a1b56f3`.
The prior 12 public verdicts are preserved, with 13 additional CLI checks.
Windows executable SHA256:
`123fd200d48426af7a4aae50e7b9d1e6cfeb68517b23b1597e6755fce6bb1a20`.

Local evidence: `tests/agent-harness/.runs/e4j-legacy-migration-01/`, final
`cli-03/report.json`, full-tests/core-trust/clippy/build logs. Earlier `cli-01`
and `cli-02` failures are retained as diagnostic evidence, not accepted results.
[Read-only Linux checkpoint 37525774777](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37525774777)
passes **96/96 maintainer / 9/9 core trust / 85/85 public CLI checks**. Source
`8b2d40c19b4a902d3bf9282dea5c96eab9bd5249`, workflow commit
`4a07f8e6e3e0794a1e3a51dfa1c3119208750060`, and actual legacy tip
`af6e24425d0eba1f667913f8a5403e9a6fb7ce76` are independently checked. All 60
previous Linux CLI verdicts and all 12 previous archival verdicts are unchanged;
all 25 extended verdicts equal Windows. The complete receipt and all 109 archived
proof/asset bytes are identical. Linux's extra test covers Unix file guards.
Downloaded evidence is inspected, never executed. No keys occur in the artifact.
Linux executable SHA256:
`e78d3b1b3e162c0573241d5d02666130e142d20c0254d36592fdeddfae176fe7`.

Artifact `serving-tree-evidence` ID `11441364464`; GitHub-reported archive digest
`c9dcc01dd0c19523175cfed54b0f1f5683452ad332eabec52b41ff29e91037c3`;
seven-day retention. It is test evidence, not durable production archival storage.
Local cross-platform comparison is `e4j-legacy-migration-01/verify_linux.py` and
`linux-verification.json`; downloaded evidence is `e4j-legacy-migration-linux-01/`.

## Operator order and what remains

Follow the maintained commands/layout in
[maintainer README](../../crates/grain-registry-tools/README.md). Obtain a protected
reviewed archive pin; approve/custody the actual publisher key; sign/verify the
empty baseline; initialize/export the bundle; commit only complete serving and
publication-proof bytes; verify the migration handoff against exact old/new Git
commits; authorize conditional remote execution; verify actual hosted bytes and
client cached/fresh/revoked behavior. Ordinary updates then use the existing
reviewed signer and continuing publication path.

This implementation does not supply a production signing authorization workflow,
secret custody, remote branch protection or independent release review. The
existing signer accepts its existing unencrypted key format; an approved
protected runner must supply it. Missing authority is not synthesized. Keep the
incomplete old publisher inactive; do not remove its files yet. The unrelated
existing registry PR failure for missing `ci/read-submission.sh` is still open.

E4 remains open for protected production baseline/signing, conditional GitHub
activation, operational controls and actual hosted-client coherence. E5–E8
remain in the plan; **five delivery stages retain work**. No new manual batch,
Agent harness source/scenario, website/client hosting change, main merge or
physical obsolete-code removal. Foundation **52 Pass / 1 Deferred (12)**,
forward **71**, criterion **56 Pending**, inventory **108 / 93 / 81** unchanged.

## Plan additions and maintenance

The signed archive binding and distinct first-migration handoff refine E4's
already planned history retention; they are not another delivery stage. Seed
version compatibility and the Windows parser fix are findings resolved at this
checkpoint. The 15 migration regressions and extended small Python CLI script
are permanent regressions while migration/history compatibility is supported.
Only ignored local evidence is disposable; no temporary runtime engine, fake
visual surface or another testing application was added.

Primary references: [Clap derive flattening and Args](https://docs.rs/clap/4.6.4/clap/_derive/index.html),
[Git's explicit expected-value lease](https://git-scm.com/docs/git-push), and
[TUF specification](https://theupdateframework.github.io/specification/latest/)
for signed metadata version/expiry consistency principles. Grain's custom
minisign protocol is not claimed to conform to TUF.
