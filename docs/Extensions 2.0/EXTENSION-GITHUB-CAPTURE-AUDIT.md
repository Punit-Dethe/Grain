# GitHub previous-publication capture and legacy migration boundary

Date: 6 October 2026. E4h. Branch `extensions/tool-only-retirement`.

## Decision and plan

Continue the confirmed GitHub host with a narrow prerequisite for protected
publication: capture authenticated previous history directly from a pinned Git
commit. Plan: graph/source and primary Git documentation → reuse existing
publication/hosting and bounded child helpers → implement exact raw-object capture
and safe inventory reconstruction → focused signed/Git cases plus full maintainer
regressions → actual app-pinned CLI acceptance/refusals → scoped audit → pinned
read-only Linux checkpoint → progress, documentation and scoped pushes.

Inspection found that the actual legacy registry at
`af6e24425d0eba1f667913f8a5403e9a6fb7ce76` has signed roots/index and addressed files,
but lacks signed revocations and the new `.registry-publication/` receipt/proofs.
The nested development branch retains that v1 layout. Do not invent missing
signatures, root approval, an empty historical revocation state or a receipt pin.
This unit delivers committed-history capture for the complete publication layout;
the first legacy migration itself is still Pending. All legacy bytes/working edits
and existing publishing workflow remain untouched. This is a scoped implementation
checkpoint, not certification of an activated GitHub release.

## Implementation contract

`capture-github-publication` accepts a standalone protected operator checkout,
expected GitHub OWNER/REPO, independently pinned full 40-character commit and
bundle receipt SHA256, and fresh output outside the checkout. It reuses the existing
origin/configuration/complete-ancestry checks. Capture can read a previous commit
while HEAD remains the candidate; ordinary publication still requires candidate
HEAD and its exact sole parent. Git's configured local origin is not proof that
a commit exists on GitHub: the future protected runner must verify actual remote
membership and its independent expected refs/policy.

The command inspects `v1/` and `.registry-publication/` only. Before creating output,
it requires a complete six-document signature set, pointer and receipt; unique
regular non-executable blob modes; safe exact metadata paths; SHA256-addressed
native/MCP/Markdown/WebP/GIF names; and the existing per-file and total-byte bounds.
Unsafe names, linked/executable/submodule inputs, excess files/bytes or missing
baseline documents refuse. The selected object must be an actual commit, not a tag
or branch spelling. No git archive/checkout/filter/attribute path is used.

One `git cat-file --batch` command reads exact recorded object IDs and sizes. A
bounded owned temporary file carries the combined binary stream; reconstruction
reads bounded headers, exactly each declared length and its separator. Newlines,
NULs and header-like strings inside artifact bytes are data, not control records.
Files are created only at previously validated relative destinations. Git omits
empty directories, so capture creates only the known blob/media/history folders.
The original raw receipt/pointer, metadata/signatures, historical proof and assets
are preserved byte-for-byte in `FRESH_CAPTURE/bundle/`.

Existing hosting verification authenticates that entire capture against Grain's
actual pinned trust chain and independent receipt pin. Historical/previous selected
proof may be expired; capture does not activate it or claim freshness. The public
hosting verifier and new publication candidate check still require freshness.
Version identity and complete history/pooled asset checks remain. Existing raw Git
blob comparison additionally binds reconstructed files to the pinned commit.
Output's unsigned `capture.json` records repository, commit, bundle receipt digest,
selected snapshot, file/byte counts and an explicit not-release-approval class.
The command prints its SHA256; no source approval, signing or GitHub write follows.

The existing Git runner now exposes its already bounded/reaped temporary output
for binary streaming. Old text inspection retains its existing output/deadline
rules. Publication/capture share checkout helpers; includes/redirects/filters are
checked before other local config consumers, and no publication HEAD requirement
was relaxed. Inherited GIT variables, system/global configuration, lazy fetch and
interactive prompts stay disabled. Each child has the existing 30-second deadline
and is killed/reaped on budget/error paths. Temporary handles close after use.

There is no new dependency, server, background thread, cache, Agent runner scenario
or idle resource. Capture uses bounded metadata/path buffers and streaming file
copy, not whole-artifact RAM buffers. Temporary disk can approach a full bundle
size in addition to final capture; the existing 1 GiB profile plus at most 8 MiB
Git framing bounds the batch. Slow large local reads refuse within the unchanged
deadline. Protected immutable operator Git objects/outputs and a trusted Git
executable are assumed; this is not a same-account adversarial filesystem sandbox.
Only newly owned partial output is cleaned on failure; pre-existing outputs,
repository files and held obsolete assets are not removed/restored.

## Evidence and focused audit

Seven added component tests use the existing actual Minisign/Git publication
fixtures, with private test-only roots and no production CLI root override:

1. Capture both previous and selected commits despite a dirty checkout; feed both
   captures through the publication gate and preserve HEAD/dirty bytes.
2. Reconstruct all known empty directories; committed export-ignore attributes
   do not drop metadata, and normal hosting verification accepts the capture.
3. Preserve a real binary native artifact with NULs/newlines and Git-header-like
   bytes exactly, with authentic new version/hash metadata and retained old assets.
4. Corrupt signed metadata, wrong receipt and missing withdrawn MCP file each
   refuse with complete owned-output cleanup and unchanged source HEAD.
5. Unknown committed paths, executable modes, oversized roots and missing legacy
   baseline signatures/receipt refuse before output creation.
6. Authentic expired previous proof captures successfully but remains refused by
   fresh hosting verification. Expiry is modeled by signed past dates, not an
   actual-clock deployment observation.
7. Invalid commit/pins, wrong origin and existing/contained outputs refuse;
   pre-existing sentinels remain intact.

Windows **69/69 full maintainer Rust tests**, locked production CLI build, strict
package Clippy/all targets and scoped formatting Pass. Actual app-pinned CLI
evidence has **12/12 precise verdicts**: seed initialization/export, successful
committed capture while checkout metadata is dirty, recovered fresh-bundle
verification, seven specific identity/receipt/containment/existing-output/missing
object refusals, and refusal of the actual committed legacy registry. The positive
CLI fixture uses Grain's already signed public bootstrap seed and a disposable
local Git repository, not generated production private keys or an alternate root.
It proves local committed capture/trust/byte behavior, not actual GitHub approval
or protected branch activation. Its source identity exists only in the disposable
fixture; no live registry commit is written.

Evidence is `tests/agent-harness/.runs/e4h-commit-capture-01/`; Windows CLI SHA256
`ab085c80833184e715a165054ae79144896c464e20b9df3c41ae7e59cf13ba49`.
All eight captured seed files match the original hosting bundle; empty folders,
receipt source pins and preserved dirty metadata are asserted. Independent CIM
inspection finds zero remaining processes for 21 recorded CLI/fixture-Git PIDs.
The public CLI probe creates no private keys and no GitHub/hosting changes.
Component tests use ephemeral signing keys as before; their count is separate.

Graph minimal context/exact query/impact and detect→review were consulted. New
capture/test files are not indexed; generic review also includes the unrelated
generated bindings. Scoped direct source/diff inspection covers these files and
the shared runner/config checks. Full regressions cover existing preparation,
source review/signing, publication lease, renewal, serving/history/expiry paths.
This is a focused implementation audit, not an independent reviewer or final
release audit. Unrelated bindings retain SHA256
`2bd7fc3f77443d7317375fdff8441cac2733f228aa1c6aafe844ffd21809bac`.
Disposable probes remain ignored evidence, not maintained Agent harness machinery.
Linux acceptance is Pending until the exact source-pinned lane completes.

## References and next delivery

[Git cat-file](https://git-scm.com/docs/git-cat-file) defines raw object/batch
content and distinguishes filter conversion from normal object reads.
[Git ls-tree](https://git-scm.com/docs/git-ls-tree) exposes exact modes, object IDs,
sizes and NUL-separated tree paths. Grain applies its existing authenticated
hosting bounds/inventory and commit-binding rules around those primitives.

Next E4: protected migration of authentic legacy source/history, reviewed complete
baseline signing, pinned remote capture/authorization and scoped conditional
GitHub activation; then actual hosted client coherence and operational controls.
Before any migration writes, inventory earlier signed catalogue commits and reserve
their extension-version identities/assets without re-enabling legacy capabilities.
Four-document old proof must remain explicit historical evidence; a missing past
revocation document must not be retroactively claimed signed or empty. Any new
current revocation state needs real publishing authority and review. The actual
legacy publisher is unchanged, including its separately recorded missing-helper
problem. Release protections, independently approved source review/key custody,
scheduled renewal/revocation/root recovery remain separate Pending gates.

No user manual batch. Foundation **52 Pass / 1 Deferred (12)**, forward **71**, full
OAuth **56 Pending**, Agent inventory **108 / 93 / 81**, parked optional website,
physical-removal hold and five E4–E8 stages retaining work remain unchanged.
