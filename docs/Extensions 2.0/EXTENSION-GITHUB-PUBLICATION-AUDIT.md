# Commit-bound GitHub registry publication handoff

Date: 6 October 2026. Branch: `extensions/tool-only-retirement`. E4g.

## Scope and execution plan

Continue the confirmed GitHub registry host. The optional OAuth website stays
parked. Plan: graph/source review and primary Git documentation → reuse existing
hosting verification and bounded Git inspection → bind signed release/history to
exact committed bytes → exercise conditional publication against a disposable
bare Git remote → full maintainer checks and scoped audit → pinned read-only Linux
checkpoint → documentation, scoped commits and pushes. No Agent harness engine,
scenario, app runtime, SDK, UI, production host, trust-key or author contract change.

The planned GitHub publication boundary is delivered as `prepare-github-publication`.
The gate emits a conditional push handoff instead of itself authenticating or
deploying. This deliberate split keeps no-secret verification separate from
protected write authorization. Protected remote execution and the first history
migration remain the next E4 checkpoints, not completion claims for this unit.

## Contract and implementation

The command accepts a protected standalone operator checkout, independently
expected GitHub `OWNER/REPO` and branch, full base/candidate Git SHA1 commits,
previous/candidate hosting bundles and their independent receipt SHA256 pins,
and a fresh output directory outside all inputs. It never fetches, checks out,
stages, commits, signs, authenticates, edits Git configuration or pushes.

Each Git commit must contain the bundle's `v1/` at `v1/`, and its receipt, pointer
and historical signed metadata under `.registry-publication/`. The candidate must
be HEAD and its actual commit header must contain exactly the expected base as
its sole parent. Changes outside those two directories refuse. Both base and
candidate are compared to independently verified hosting bundles: exact path
sets, regular mode 100644 blobs, then exact batch Git object hashes of the raw
verified files with filters/newline conversion disabled. The working index and
dirty worktree do not authorize published bytes. Git's normal immutable object
integrity is assumed; a compromised Git executable/database is outside this gate.

The previous bundle can be authentically expired; only this internal baseline
inspection permits it. Public `verify-hosting-bundle`, new candidate verification
and exports still require freshness. Signatures, roots and file bounds are never
waived. Existing transition checks reject metadata rollback/same-version
replacement, changed extension-version identity and weakening/erasing revocations.
The candidate must retain every previous historical proof plus the previous
selected snapshot, so old withdrawn native/MCP artifacts and listing/media remain
in the verified pooled asset inventory. Receipt pins identify inputs, not source
review or publication authority.

The checkout must be complete and standalone. Local includes, URL rewriting,
filters, extensions, promisor/push-URL configuration, alternate objects, shallow
history and grafts refuse. Git inspection strips inherited `GIT_*`, disables
system/global configuration, lazy fetching and interactive prompts, and reuses
the existing bounded child runner. Publication inventory/hash input/output are
bounded to 8 MiB per operation and the existing 30-second deadline; temporary
stdin avoids a blocking pipe write. Each child is reaped on success/failure;
owned temporary files close afterward. Old preparation operations keep their
256 KiB output limit and do not create a new input file unless needed.

Hosting's existing asset/history/total-byte limits remain unchanged. Inventory
and hashing are batched, not one subprocess per file; artifact buffers are not
loaded wholesale. There is no background service, thread, idle state, cache or
dependency. Windows canonical extended paths are converted to drive/UNC spellings
for Git's batch path parser. Paths are quoted as data; controls/non-UTF8 operator
paths refuse. Known history traversal depth and file-count bounds apply. Bundles
are reverified after hashing; protected immutable operator captures are required.
This is not a sandbox against hostile concurrent mutation by the same OS account.

Fresh output contains unsigned `publication.json`; a printed SHA256 permits a
protected handoff. It records exact commits, bundle pins, selected snapshot,
environment isolation and a fixed Git argument vector. The update uses explicit
`--force-with-lease=refs/heads/BRANCH:EXPECTED_BASE`, not tracking-ref heuristics,
plus refspecs or broad force. The verified single parent makes its intended update
a fast-forward; tags/submodule recursion and local pre-push hooks are disabled.
No argument vector or shell is executed by this verifier. The later protected
runner must regenerate/validate the handoff, bind its own fixed destination and
release policy, provide scoped noninteractive authentication and recheck freshness.
The recorded global-config isolation means ordinary global credential helpers are
not silently inherited. Unknown push outcomes must not trigger an automatic retry.

## Evidence and focused audit

Eight added tests use actual Minisign signatures, Git commits and files. Private
test anchors remain test-only; the production CLI has no root override.

1. Verified native/MCP withdrawal history binds both commits exactly, preserves
   old addressed objects and leaves dirty worktree/repository files untouched.
2. The generated explicit-lease argument vector advances a disposable bare remote
   from its exact base. An independent later writer advances it; replay of the
   stale handoff fails, preserves the winner and retains withdrawn MCP files.
   Only the destination is substituted with a test-only local bare path. No actual
   GitHub endpoint/protection/authentication is exercised by this case.
3. Committed tamper, extra/missing files and executable blob modes each refuse at
   their own inventory/byte/mode assertion without creating output.
4. Unrelated source changes, wrong expected parent, wrong origin and local URL
   redirection refuse independently.
5. An otherwise authentic replacement bundle that erases earlier history refuses.
6. Bad repository/branch/commits/receipt pins and existing/contained outputs refuse;
   pre-existing sentinels and pinned checkout state remain intact.
7. Authentic expired previous metadata verifies only in baseline mode; fresh
   candidate/public verification refuses it, and corrupt old signatures still fail.
   This models signed past dates; it is not a real-clock deployment observation.
8. Missing committed publication proof and grafted ancestry refuse. The initial
   legacy migration is not silently treated as an ordinary release.

Windows final full maintainer suite: **62/62 Pass**, including all existing
preparation/review/signing/serving/history/renewal regressions. Four production CLI
malformed repository/candidate/unchanged-commit/branch cases refuse with exact
messages before nonexistent input access/output creation. Strict package Clippy
(`--all-targets --no-deps -- -D warnings`), locked build and scoped format pass.
Evidence: `tests/agent-harness/.runs/e4g-git-publication-01/`; CLI SHA256
`05a90e69874cb19f03af1486095b7c8cf2c4bb6ea174e93fcb6a2a9368019097`.

Initial candidate failures exposed Windows extended-path spellings and test
staging that relied on CopyFile-preserved size/mtime. Git's byte verifier correctly
refused mismatched committed pointers; fixture staging now explicitly rehashes
tracked bytes with `git add --renormalize`. An initial expiry test expected a
later error text, but the existing Fresh-status check refused earlier; its precise
assertion now matches that actual boundary. No guard was weakened to pass tests.

Graph-first exploration and detect→review identified the existing hosting and
preparation dependencies; graph coverage is stale for newly added files and
includes the unrelated generated bindings change. Direct scoped source/diff
inspection covers the new files. This is a focused implementation audit, not an
independent reviewer or final release certification. Full regressions cover the
shared helper/freshness refactors, and the unrelated bindings hash is unchanged.
No maintained Agent runner/source scenario was added; disposable command probes
are ignored evidence only.

Pinned read-only [Linux checkpoint 37450479331](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37450479331)
passes **63/63 full maintainer tests** (one additional Unix symlink guard), locked
CLI build and **50/50 actual CLI verdicts**: the earlier 46 serving/promotion/
history/bundle/renewal checks plus four new publication admission refusals. The
workflow has contents-read permission only; it accesses no release key, OIDC,
author build or GitHub write. Source is exactly
`7557c0d722e8ab1a23afe62c2710c65b3b5a298e`; registry workflow is exactly
`c4d0d2cb30cc4937ef877a246f863cba95d28e0c`. Linux CLI SHA256:
`f924649ce84ba3ba9b18ff35b16ce540e38aa16977d10743cda866d754ed8e5d`.
Artifact `11406841507` has reported archive SHA256
`12a68b791c416425b8e1426974831fddc43883d28b429bd30d7f20c444dc5ca6`
and seven-day retention. Downloaded report/log/public fixture evidence is in
`tests/agent-harness/.runs/e4g-git-publication-linux-01/`; it was read, not executed.
Inspection checks exact source, test/count totals, all 46 prior label/exit/oracle
triples preserved, four specific new nonzero refusals, unchanged selection and
no retained private-key files. Four recorded Windows CLI PIDs have zero matching
remaining processes under independent CIM inspection. Component private keys
are disposable TempDir fixtures; zero-key counters describe the public CLI lane,
not a claim that real-signature component tests avoided key generation.

## Primary references and remaining gates

[Git push](https://git-scm.com/docs/git-push) specifies explicit expected-ref leases
and the danger of relying on tracking references changed by background fetches.
[Git hash-object](https://git-scm.com/docs/git-hash-object) specifies batched path
input and hashing raw bytes without attribute/newline filters. Grain adds signed
metadata/history validation and exact base/candidate binding; this is not a new
MCP library, registry service or claim of distributed protocol certification.

The registry PR remains draft/unmerged. The actual legacy repository does not yet
contain the new protected baseline layout; its existing publisher is unchanged.
Do not deploy by bypassing the required previous bundle. Git cannot track empty
directories, so a protected Git-to-bundle capture must recreate the known empty
`blob/`, `media/` and `history/` folders. Protected migration, independent source
review/signing, GitHub release protections, scoped writer/key custody, scheduled
renewal/emergency revocation/root rotation remain open. One atomic Git branch
update does not make the application's separate mutable HTTP requests coherent;
the actual hosted client-read checkpoint remains necessary.

No manual user batch. Baseline **52 Pass / 1 Deferred (12)**, forward **71**, full
OAuth criterion **56 Pending**, Agent inventory **108 / 93 / 81**, physical deletion
hold and five E4–E8 stages with remaining work are unchanged. E5 management/store
UI, E6 recovery, E7 measurements/reference integrations and E8 certification follow.
