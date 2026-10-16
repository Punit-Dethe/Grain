# Extension store operations

Current operator handoff, 16 October 2026. The active scope/status lives in
[the six-block plan](PLATFORM-STABILIZATION.md); implementation evidence lives
in [progress](MCP-EXTENSION-PROGRESS.md). This runbook uses the existing CLI and
GitHub host. It introduces no service, scheduler or second publisher.

## Responsibility and credentials

The current tester maintainer is Punit-Dethe. The adopted ownership policy is
outside source/build trees at `%LOCALAPPDATA%/GrainPublishing/policy/ownership.json`,
SHA256 `d471cf0c145dd8a83b131c2e669284f026f78659e0c649f095cd8935423771c3`.
It authorizes exact tester approvals for GitHub/Notion; it is not blanket community
approval. Keep source approval, account consent and Agent execution policy distinct.

- **Root key:** authorizes publisher/trust-root changes. Keep offline and separate
  from routine publishing. A suspected root compromise requires a separately
  reviewed trust recovery; publisher-signed metadata cannot repair root trust.
- **Publishing key:** signs reviewed releases, renewal and revocation. Use explicit
  password unlocking; never put it in an author checkout, CI build artifact or chat.
  The app updater key is a different identity and must not be reused.
- **GitHub publisher access:** activates only the verified commit/bundle through
  the existing conditional workflow. Failed source build/review must never acquire
  signing credentials. A deployment environment's name alone provides no protection.
- **Provider registrations:** GitHub App and other pre-registered OAuth identities
  are maintained by their registration owner. Third-party authors document their
  registration/support prerequisites. Packages carry no client secrets.

Before public community release, the operator must confirm who holds each key,
who may approve/deploy, and a private backup plus tested restore procedure. Store
encrypted backups/password recovery separately with restricted access; prove
restoration with a harmless offline signature checked against the pinned public
key, never by publishing an unreviewed extension. This document does **not** claim
the current tester keys were rotated, backed up or restore-tested.

## Routine publication and renewal

Use the [CLI operator instructions](../../crates/grain-registry-tools/README.md)
for exact commands and argument contracts. Authors use `grain-ext submit`; operators
select trusted CI evidence and approve the exact source/candidate before
`stage-reviewed-release`. Retain its candidate/snapshot/bundle and final GitHub
publication receipt. Do not choose pins from an author's own claimed receipt.

Check selected signed metadata before expiry. `renew-serving-metadata` updates
index/revocation counters and expiry only; it does not approve releases or deploy.
Use its fresh assembly with existing promotion, export and commit-bound GitHub
publication. Confirm remote Git **and** anonymous hosted-file verification. There
is no automatic renewal job: assigning/scheduling an operator is an activation
decision, not implemented background work. Expired index/revocations can be renewed
if authentic; expired roots require root-authorized recovery.

## Broken release or abuse

1. Identify the exact published ID/version from the protected current snapshot;
   record a short public reason without private incident details.
2. Use `revoke-serving-release` with that independent snapshot digest and the
   publisher key. This prepares a complete assembly; it does not change hosting.
3. Verify, promote against the unchanged selected pointer, export, then follow
   the same commit-bound publication and anonymous checks as a release.
4. Confirm the app enforces the new revocation on refresh. Offline clients do
   not receive immediate push revocation. Retain immutable addressed assets and
   signed rules; do not edit metadata on the host by hand.
5. Review/publish a corrected new version through the normal author workflow.
   The old revocation stays in place. Re-enabling the bad version is not recovery.

The exact-version command covers native and MCP packages. It refuses unknown or
already revoked targets and never weakens an earlier rule. Widespread publisher
compromise/root rotation is a separate authority operation, not something this
single-release command claims to solve. Legacy `retire` is not a coherent-store
revocation procedure.

## Failed publication and provider outage

| Failure | Response |
| --- | --- |
| Source/review/signing refused | Fix that named cause; preserve the working store. Do not bypass approval or reclassify it as success. |
| Snapshot/current pointer or Git base changed | Inspect the newer trusted state and prepare again against it; never force-push over another publisher. |
| Local promotion succeeded, export failed | Reuse the verified promoted snapshot and export/verify again; do not sign or repeat provider actions unnecessarily. |
| Push succeeded, delivery verification failed | Read remote Git and verify the exact intended bundle before retrying the delivery check. Do not create another release merely because reporting failed. |
| Provider down or OAuth registration changed | Maintainer checks the upstream provider and registration. Preserve accounts/installation; report needs-attention/reconnect when required. Never silently redirect credentials to another endpoint. |
| Community package abandoned/abusive | Registry maintainer records the case, contacts the author where appropriate, and reviews restriction/revocation. A first-party listing does not make Grain the maintainer of the provider's MCP server. |

## Remaining activation gates

- **GitHub controls:** read-only preflight found no publish-environment protection
  rules/branch restriction, no rulesets and no main branch protection. Prepare
  concrete controls compatible with the current conditional writer for operator
  review; do not turn on a rule that silently prevents the publisher from working.
  Generic continuation/tester source approvals do not authorize changing these.
- **Key custody/restore:** the private operator assignments and restore check above
  remain unconfirmed. No secrets need to enter documentation.
- **Alpha application exposure:** source-aware access is implemented. The dedicated
  alpha identity/debug builds allow signed-store accounts/tools without Developer
  Mode; custom MCPs require a debug build and Developer Mode. Normal tests and the
  affected user real-app checks pass. Final packaged-alpha verification remains.
- **Contributor cleanup and release scope:** identify obsolete harness procedures
  in the existing inventory, keep normal tests as the default, and run only affected
  final application/packaging checks. Advertise only verified platforms/providers.

These are explicit Block 6 gates. No new installer, live publication, permission
change, key movement or new provider account is needed for the offline revocation
implementation and its ordinary signed-fixture tests.
