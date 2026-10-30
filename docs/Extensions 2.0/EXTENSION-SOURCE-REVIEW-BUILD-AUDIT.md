# Pinned source/build handoff and fresh GitHub review — E4b3

6 October 2026. The remote native/MCP source-to-candidate slice and actual
unapproved-signing refusal are verified. **Production review-to-signing approval,
complete serving/promotion and full E4 acceptance remain Pending.** The latest
numbered product acceptance remains 71; five E4–E8 delivery stages retain work.
This audit supersedes the current signer's schema-1 review-policy description;
earlier fixture signing successes remain historical evidence.

## Product changes

`grain-registry` now requires strict **schema-2** protected policies. New mandatory
fields are `review_pull_request`, `review_head` and `review_id`. The policy must
name different reviewer/submitter accounts and independently pin the official
GitHub CLI. Schema-1 policies cannot reach the current signer.

After byte, prior-index and cryptographic CI validation, immediately before the
delayed key gate, the signer performs bounded read-only GitHub queries. It checks:

- A closed, merged registry PR whose base repository/branch, merge commit,
  reviewed head and submitting account match the protected policy. Merge SHA and
  PR head are separate pins; forks are supported without claiming their head
  repository is the registry.
- The actual merged `submission.toml` equals the complete shared source request;
  merged DESCRIPTION bytes match its signed size/hash. Candidate/source/media
  checks continue through existing shared preparation/receiving contracts.
- The independently designated human reviewer's latest effective review still
  approves that exact head at the pinned review ID. Self-approval, bot reviewers,
  dismissed/changes-requested/superseded reviews, unknown states, missing approval
  and future timestamps refuse. Comments alone do not revoke approval.

Review pagination is capped at 100 records/page and ten pages; a full final page
refuses instead of accepting incomplete history. Every GET is fixed to github.com
through the pinned executable, without a shell. The child environment restores
only CLI authentication/configuration and required OS paths; arbitrary inherited
secrets/overrides are stripped. Use read-only contents/pull-requests credentials,
never publishing credentials as a GitHub token. Each owned child has bounded
output/time and kill/wait cleanup. No raw API response or token is printed.
GitHub failure refuses; no offline review bypass exists. `verify-source-review`
exercises this same gate without a key; `inspect-submission` emits validated CI
request data with the existing no-overwrite/outside-input writer.

The SDK, author submission schema, application runtime and Agent harness contract
do not change. This remains maintainer tooling outside the shipped app.

## Real remote source/build CI

The separate registry branch contains `build-source-candidate.yml` and the small
`prepare-source.py` adapter. Its four job boundaries are:

1. **Trusted request:** build Grain tools from full commit
   `6d75c5a100cad9f2d543ffc91b08cf0667db3b4a`, Rust 1.96.0 and locked dependencies;
   validate the shared submission and emit a producer executable digest.
2. **Untrusted native author build:** checkout the exact source SHA, install from
   the committed npm lock with install scripts disabled, and explicitly run the
   build using Node 24.11.1. Read-only contents permission, no OIDC, environment
   secrets, publishing key, credential persistence or shared dependency cache.
   MCP descriptors skip this job; no MCP server is launched/authenticated.
3. **Fresh trusted preparation:** checkout the source SHA again with tags, verify
   the downloaded maintainer executable against the trusted request-job digest
   **before executing it**, copy only bounded native output as data, and run actual
   preparation/catalogue commands. Neither author scripts nor output execute.
4. **Data-only attestation:** download checked candidate/producer data and invoke
   the immutable GitHub attestation action with OIDC. Preparation must succeed;
   intentional MCP build skips are handled explicitly, cancellation is respected.

All external actions and Grain/toolchain/Node inputs are pinned. Debug symbols are
disabled for the CI maintainer executable to meet the unchanged 128 MiB guard and
reduce transfer/RAM. This is privilege separation, **not an egress sandbox** or
byte-for-byte build reproducibility certification. Current native CI requires a
public root GitHub project, npm lock, ignored/untracked `dist` and `dist/main.js`.
Custom output paths/private-source credentials are not silently accepted.

Actual remote fixture source pointers in `Punit-Dethe/Grain-Extention`:

| Kind | Isolated branch / tag | Exact author commit |
| --- | --- | --- |
| Native | `fixtures/native-tools` / `fixtures-native-v0.1.0` | `be97b2733766965c4a77fbe3965899cecc4c9389` |
| MCP | `fixtures/mcp-tools` / `fixtures-mcp-v0.1.0` | `e8b9863f3b20839f1efea73d015501d686f7cfdb` |

These controlled root projects exercise real remote checkout/origin/tag and build
inputs without widening the contract with fixture-only subdirectory options.
They do not certify author identity or human approval. No new repository was
created. Ordinary registry edits, main, keys and administrative settings survive.

## Acceptance and retained failures

Final registry source is `b4d5d71979d8d9b4dec517a6689012a45eeda8b1` on
`registry/trusted-provenance`. [Actual source CI run 37415224367](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37415224367)
passes both request/preparation/attestation paths and the real native build; only
the MCP author build is intentionally skipped. Acceptance checks the required
jobs individually, not merely the run's green badge.

- **26 focused Rust tests pass**, with final affected policy/review/minisign and
  approval matrix repeats. Cases cover merge/head/repository/branch/submitter,
  revoked/stale/missing/bot approval, schema/self-review and existing exact-byte,
  signature/index/key timing/process budget boundaries.
- **20 final actual CLI verdicts pass**: strict native/MCP request inspection and
  no-overwrite, actual draft-PR refusals, old-policy/self-review refusals, four
  independent genuine producer/candidate provenance verifications, two wrong CI
  identity refusals, two received remote-source validations and two full
  provenance-valid signer refusals at the GitHub approval gate. No key is created
  or accessed and no signing output is produced in this batch.
- Final formatting, focused build and scoped all-target Clippy with `--no-deps
  -- -D warnings` pass. The initial unscoped Clippy attempt exposed existing
  `grain-ext-cli` conversion warnings; those unrelated source lines were preserved.
- Independent inspection of 20 recorded command PIDs/children finds no remaining
  owned registry/gh/git runtime. Windows reused a PID for the current unrelated
  inspection shell; it was classified, never killed. Retained files are not a
  claim of file deletion/cleanup. No app profiles/accounts/listeners were created.

Nonaccepted attempts remain recorded:

- [Run 37414892609](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37414892609)
  built both request jobs/native output but refused the 140,885,848-byte debug tool
  at the 128 MiB guard. The fix removes debug symbols, without raising the bound.
- [Run 37415082623](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37415082623)
  prepared both kinds and attested native, but transitively skipped MCP attestation
  despite successful preparation. No full acceptance credit; explicit success-only
  attestation conditions fix this in the final run.
- The initial local signer oracle placed output under its protected policy parent;
  the existing containment guard correctly refused it. Evidence was retained and
  the corrected fresh attempt uses separate protected-policy/output paths.
- Draft PR triggered the existing `build-and-check.yml`, which fails because
  `ci/read-submission.sh` is absent (exit 127). This is the previously documented
  incomplete legacy workflow, not a passing production check or repaired publisher.

Linux producer pin:
`b453445f60d47bf9e527c7bbdf8248f7ff0ff636c8f5ceedfd836481aedc7966`.
Windows registry executable:
`c5ea8ebfe97319b7fe449d3d77bc906d24005ba866591cb645eb86e45290c5a8`.
Official GitHub CLI pin:
`2ae2b350c227a618f2d8965b1900aeee13446ff42e17ef0bd5a0b6405c593cfb`.
Final attested native/MCP artifact IDs are 11389864325 / 11390853028; seven-day
retention. Ignored `.runs/e4b3-source-build-01/` retains exact producer/receipt/
bundle/CLI/API/job/cleanup reports and failed-oracle evidence.

## Audit conclusions and next boundary

Scoped source review covers mandatory signer wiring, authority provenance,
downloaded executable admission, cancellation/skip behavior, exact-source/listing
bindings, child/environment/output lifetime and no-key refusal. The shared byte
and actual minisign tests remain independent of GitHub JSON fixture tests. Graph
context was used first, then change/review context; new/unindexed maintainer/CI
code was inspected directly. No new production dependency or background service.

[Draft registry PR 1](https://github.com/Punit-Dethe/Grain-Extention/pull/1) is a
concrete review artifact, **not approval**. It remains unmerged; main/admin settings
are unchanged. Current policy selection/reviewer eligibility must come from
protected operator infrastructure. Live positive independent merged review-to-
signing acceptance is still Pending and cannot be fabricated with self-review.
Mutable GitHub review state is checked last; this is not an atomic lock against
external changes after the query. Protected review/branch/environment governance
and serialized promotion remain required.

Next coherent E4 work is a complete signed serving-tree assembly and serialized
promotion boundary: exact current-index conflict refusal, all referenced asset/
root/revocation paths, deployed key relationship, renewal/revocation and registry
workflow/protection/key custody closure. Do not merge the draft into the active
incomplete legacy publisher or call these fixture keys/app roots production trust.
No new user test batch now. Baseline **52 Pass / 1 Deferred (12)**, public hosting
**56 Pending**, inventory **108 / 93 / 81**, physical-removal and release holds stay.

Cleanup ledger: fixture branches/tags, two `com.example.source-*` submissions,
branch-push checkpoint wrapper and ignored local scripts/source/node_modules/
downloaded tools are disposable evidence. Retire when reviewed real author builds
replace them; retain the reusable source lane and focused maintainer tests. No
Agent harness runner/scenario/bridge or alternate visual path was added.

Primary references: [GitHub pull-request review API](https://docs.github.com/en/rest/pulls/reviews),
[trusted builder guidance](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating),
[workflow security](https://docs.github.com/en/actions/reference/security/secure-use),
[job dependency conditions](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds).
