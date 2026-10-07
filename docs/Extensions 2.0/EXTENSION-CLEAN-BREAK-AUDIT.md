# Unreleased extension platform: clean break and catalogue bootstrap

7 October 2026. Implements the user's decision that Grain has no deployed
users, is before pre-alpha and needs no old extension/settings/registry migration.

## Implemented

- Removed `legacy.rs`, `migration.rs`, their exclusive test modules, the pinned
  old-history manifest and the archival Python CLI checkpoint.
- Removed `capture-legacy-history`, `verify-legacy-history`,
  `sign-legacy-migration`, `prepare-github-migration` and the retained-archive CLI
  option. They are absent commands, not blocked implementations.
- Removed signed archive binding, archive storage/export/capture, inherited old
  version reservations and initial migration handoff enforcement from shared
  serving/hosting/publication paths. No old registry is imported or consulted.
- Added `bootstrap-serving-tree`: validates embedded **current app trust seed**,
  keeps its genuine roots/current revocation rules, signs fresh empty catalogue
  and revocations with an explicit publisher key and 1–30 day expiry, and advances
  versions above that current seed (currently 2/2). No historical retirements,
  recovered routes, old settings or archive outputs are generated.
- Removed CI's second legacy-repository checkout and 25-command archival batch.
  The maintained current-contract checkpoint now checks command removal and
  clean bootstrap alongside existing source/trust/Git/process boundaries.

This removal is deliberately bounded to the compatibility machinery added by
E4i/E4j. Other exclusive obsolete runtime/capability/old producer paths remain
eligible for later coherent removal; no remaining old-code hold is implied.
First-party Agent context, dictation/audio, native/MCP execution/authentication,
vault ownership and unrelated local working edits were not changed.

## Preserved current correctness

Source review/build/provenance, signature validation, root ownership, bounded
files, unknown output refusal, owned cleanup, normal signed updates, metadata
renewal, complete hosting verification, raw committed-byte checks and explicit
expected-base publication leases remain. Ordinary current-contract version
checks protect current signed updates; they no longer depend on old registry
identities or an archive. Hosting's empty-pool byte guard is retained.

Bootstrap validates an empty fresh seed and checked version/size bounds before
reading the key. Wrong/missing keys fail, signatures must verify against the
current pinned publisher, and preexisting outputs are not overwritten. The
existing signer accepts its unencrypted minisign format; no production key was
accessed or created by the public CLI checkpoint. Positive component tests use
real disposable signatures, including store initialization, renewal, promotion
and hosting export through existing code. No new background service, Agent
harness framework or visual substitute was added.

Graph context/refactoring/impact and diff review were used. Its derived call/test
coverage was incomplete; direct source review, compilation and full retained
tests establish the actual caller boundary. Removal does not touch shared
publisher-signature verification in `grain-core`.

## Verification

Windows: **72/72 full maintainer tests** (69 retained current-contract regressions
plus three clean bootstrap regressions), **9/9 core trust tests**, **8/8 public
CLI checks**, locked build, strict Clippy and scoped format/diff checks pass.
The lower count is intentional: 26 obsolete compatibility tests were deleted,
not failed, waived or converted into product acceptance credit. The component
suite now finishes in about ten seconds on this machine; this is an observed
checkpoint duration, not a general performance guarantee.

Evidence: `tests/agent-harness/.runs/e4-clean-break-01/` component/core-trust,
build/Clippy logs and `cli-report.json`. The public executable verifies current
app trust, rejects the four removed commands and refuses invalid lifetime or
missing explicit bootstrap key. Nothing is hosted or activated. Linux
source-pinned checkpoint remains Pending until its evidence is verified.

## Next implementation

Prepare the **clean first-publication GitHub gate** against exact expected base
and candidate commits, with current signed bootstrap/bundle and no previous
legacy bundle. The ordinary publication gate intentionally requires a previous
complete new-contract publication; do not invent one or import old history to
satisfy it. Then align the repository's new source/build publishing workflow,
current app/hosted trust metadata, installation/read verification and applicable
signer/remote-write controls. Old helpers that only publish the obsolete contract
should be retired as those replacements become complete.

E4 is current. E5 management UI, E6 Agent modules, E7 measured integration and
E8 eventual release readiness follow. Five delivery stages retain work, with
old migration and upgrade preservation removed from required scope. No new
manual batch; foundation/forward ledgers remain historical facts, unchanged by
maintainer test counts. Deferred live expiry stays Deferred. Experimental
website and upstream/frontend branch boundaries remain as previously agreed.
