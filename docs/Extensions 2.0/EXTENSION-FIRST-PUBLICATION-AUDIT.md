# Clean first publication and current registry workflows

Date: 7 October 2026. Delivery stage: E4. This is implementation and scoped
verification, not live catalogue activation or release approval.

## Completed connected block

- Added `prepare-initial-github-publication`, using the same exact Git/origin,
  sole-parent, allowed-path, raw committed-byte and conditional lease checks as
  ordinary publication. No previous bundle, fake proof or legacy archive exists.
- Initial activation requires the empty, fresh bootstrap derived from the
  current app seed: unchanged genuine roots/signature and seed policy, next
  index/revocation versions, shared expiry within thirty days, no inherited
  history/assets. Public verification has no fixture-root override or seed expiry
  exemption. A base already containing publication proof must use the update
  gate, so this route cannot reset an established catalogue.
- Shared seed materialization stays maintainer-only and uses an owned temporary
  directory. It neither holds a service alive nor changes the app trust seed.
  The distinct initial handoff omits previous-receipt data; normal updates still
  emit their actual previous receipt and retain their current security checks.
- Replaced the registry's broken `build-and-check` entry point (missing helper,
  missing tools and duplicate author build) with manual dispatch into the existing
  reusable source builder. Native author execution, fresh preparation and
  data-only OIDC are separate jobs; MCP preparation skips native execution.
  No publisher key or publication permission enters those jobs. No privileged
  fork/merge-triggered attestation has been introduced.
- Replaced the old automatic publisher (missing helper, author build alongside
  signing access and obsolete release-upload route) with explicit dispatch of an
  already-signed publication commit. Trusted Grain code is pinned by full SHA;
  no candidate script is executed. No signing key or OIDC is configured in the
  publisher. The only write credential is the scoped workflow token for Git.
- Added the small maintained `ci/publish_github.py` runner beside maintainer tools.
  It captures/verifies signed Git bytes with the actual CLI, validates the full
  handoff against independently supplied inputs, checks remote main, conditionally
  pushes once with an explicit expected-base lease, then confirms main. Tokens
  are removed from verifier children and reach only final Git children via an
  owned askpass helper that stores no token bytes. No shell evaluates inputs.
  Timeout/race/uncertain response never triggers a replay; owned temporary outputs
  are cleaned. The Linux workflow is the only activation execution profile.
- Updated registry README/review policy to the current tool-only contract,
  DESCRIPTION.md and raw GitHub hosting. No extension README/store copy, former
  capability flags, fake security address or launch SLA is asserted as current.
  Preexisting CONTRIBUTING/core/example/v1 working changes remain untouched.

## Scoped audit and verification

Graph overview/impact followed by change/review context was used. Derived graph
coverage is incomplete/stale, includes unrelated local bindings and does not
index all new Python/test code. Direct scoped source/diff review plus compilation,
real signatures, disposable Git objects/remotes and actual CLI checks establish
this boundary; graph warnings are not treated as authoritative test coverage.

Windows: **76/76 Rust maintainer tests**, including four first-publication groups;
**4/4 publisher boundary tests**; **9/9 existing core trust tests**; **12/12 actual
public CLI checks**. Locked build, scoped strict Clippy (`--no-deps`), rustfmt and
scoped diff checks pass. The all-dependency Clippy attempt found two existing
useless `.into()` conversions in unchanged `grain-ext-cli/src/lib.rs`; this block
does not modify them or claim that broader dependency lint passed.

Positive first publication uses real disposable Minisign signatures and actual
Git commits, including deletion of experimental data while preserving unrelated
files. Real bare-remote checks cover both preflight and between-check-and-push
races. Failure groups cover wrong pins, metadata/roots/policy/expiry, nonempty
catalogues/history, parent/unrelated committed edits, output reuse, Git redirection,
hand-off mutation, injected Git environment and ambiguous/time-out outcomes.
The actual public executable verifies app-pinned seed/capture and rejects an
attempt to publish the long-lived development seed; no production publisher key
is opened and no public CLI trust override is added. The runner's verification
refusal path is also exercised end to end with that executable, without network
or remote writes. The positive runner's Git transport is the same code against
an owned bare remote; this is not evidence of configured GitHub authorization.

Windows evidence: `tests/agent-harness/.runs/e4-first-publication-01/`, including component/publisher
logs and `windows-initial-cli/report.json`. Focused shared CLI checks are maintained
under registry tooling, not added to the Agent harness. No browser replica,
Agent test engine, additional manual batch or compatibility machinery is built.
Linux [checkpoint 37662540187](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37662540187)
passes **77 Rust / 4 publisher / 9 core trust / 79 CLI checks** (67 retained plus
12 shared new verdicts). Downloaded evidence independently matches Grain source
`ed55bf1df86dcd994fb43cecdb8f1abf033de8d7` and registry workflow source
`66e23eef7c1a3b364367f2067c01e74b291b1e55`. Every previous 67 CLI verdict and all
12 Windows/Linux shared verdicts match; no hosting/key activation occurred.
Linux's extra Rust test covers Unix guards. Comparison record is
`tests/agent-harness/.runs/e4-first-publication-01/linux-verification.json`.
Artifact ID `11500638272`; GitHub-reported archive digest
`8b182522e619c1983fa10fd796f099e140ea664070f9a4c4a5510606e3226f84`.
Windows CLI SHA256
`907afc95668494433e808ef21faf1d9ef4c5641c2ca9ce0c9218eec998dd140e`;
Linux CLI SHA256
`fcbc36729919bf42ded55af6e80e2ebde3b030d077529fdf084477817d63084f`.
The native/MCP [source-build checkpoint 37662540836](https://github.com/Punit-Dethe/Grain-Extention/actions/runs/37662540836)
also passes: both downloaded producer documents match the exact Grain commit and
Linux executable digest, native author build stays isolated, MCP skips it, and
both enter fresh preparation/data-only attestation. Artifacts are inspected as
data, never executed. Four candidate/producer attestations were independently
verified with the official GitHub CLI against the exact reusable signer workflow
and registry commit, with self-hosted runners denied. JSON verification evidence
is retained beside the comparison record. This does not approve source or
authorize publication. Workflow/action pins are full SHAs; actionlint
1.7.12 was checksum-verified from its official release. Workflow schema lint
passes; shellcheck/pyflakes are not installed in this local lint invocation.

## Reference checks

The implementation follows [GitHub workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use) for full-SHA action pins, input handling and separating untrusted author execution from privileged work. The conditional update uses Git's [explicit expected-value lease](https://git-scm.com/docs/git-push), rather than relying on mutable remote-tracking state. [Actionlint](https://github.com/rhysd/actionlint) verifies workflow syntax; its successful lint does not certify external environment approval or branch protection.

## Operator boundary and next work

`publish` is a named GitHub environment, not proof of configured reviewers or
branch protection. The operator must review its approval/key custody/token rules;
this block neither reads administrative configuration nor changes/bypasses it.
The candidate must already exist in the expected repository; Actions checks out
that exact full commit, and Git uses the fixed expected GitHub URL. Manual
execution requires an authorized workflow dispatcher plus normal repository
controls. A failed protected-branch push remains a failure; do not weaken rules
or retry blindly. Workflow changes are a draft; this turn does not run `publish`,
merge main, create/sign a real catalogue, configure an environment or deploy.

Next E4 acceptance: establish the genuinely signed clean candidate under reviewed
publisher custody/authorization, then verify HTTP metadata/assets and real-app
catalogue/install coherence in an isolated fresh profile. Git main confirmation
is deliberately not credited as HTTP/app acceptance. The experimental website
remains parked: registry hosting uses the existing GitHub raw URL and does not
require a permanent website. Permanent OAuth client metadata identity remains a
separate unresolved release criterion; this block does not change authentication.

E4 is current; E5 management UI, E6 Agent modules, E7 measured integration and E8
release readiness remain. Foundation **52 Pass / 1 Deferred** (live expiry),
forward **71**, identity criterion **56 Pending** and inventory **108 / 93 / 81**
remain unchanged. Maintainer tests are not product acceptance credit. No new user
test batch is required by this block. Unreleased old settings/extensions need no
migration; preserve current security and unrelated working edits.
