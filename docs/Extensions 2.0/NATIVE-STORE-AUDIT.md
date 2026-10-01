# Signed-store installation and download ownership — Block 2D third unit

Date: 1 October 2026. Companion to the [packaging audit](NATIVE-PACKAGING-AUDIT.md), [execution plan](MCP-EXTENSION-PLAN.md), [progress ledger](MCP-EXTENSION-PROGRESS.md) and [maintained runner](../../tests/agent-harness/README.md). This unit follows accepted native consent and CLI packaging. It covers numbered checks 18 and 36; it does not close the whole 2D block or begin MCP/authentication acceptance.

## Scope and implementation

Three maintained `store` scenarios run Grain's real debug-only isolated Tauri host, real React Store/Agent windows, production signature verifier, package installer, registry mutation and native extension worker. The existing local scripted model drives selective discovery and actual tool approval/result continuation. No account, microphone, paid model or publishing credential is used.

The runner owns an ephemeral loopback HTTP fixture with a fixed public test signing seed. It produces modern minisign `ED` signatures over Blake2b-512 prehashes, including the authenticated trusted comment. Signed catalogues name the actual SHA-256/length of a permission-free tool artifact. The current native installer accepts the fixture's JSON artifact; this is not evidence about archive extraction.

Only the compile-time `agent-harness` host can initialize the test publishing anchor. It requires the existing isolated application identifier/profile marker, exact data directory and an optional validated port. The endpoint is constructed as `http://127.0.0.1:{port}/`; the marker cannot provide an arbitrary URL or public key. Store/model ports cannot collide with each other, the harness event listener or the ordinary event listener on 7124. Normal builds continue using the original `StoreState::init`, production publishing roots and mirrors. The release-build prohibition and profile/vault guards remain in force.

Cached catalogue initialization still uses the real verifier and rollback floor with the test anchor. No roots rotation is bypassed and then certified: the fixture has no roots-chain or revocation document, and fetched roots still use production root verification. Bootstrap with this fixed test publishing anchor is an explicit test seam, not a demonstration of production publisher onboarding.

### Acceptance procedures

- **Check 18, `store.close-offline`:** ten actual loading-page/route-close/reopen cycles hold the real catalogue HTTP response, assert loading, trigger the production component's unmount cleanup, observe response release and no resident index, and then recover a fresh card. Actual Install leaves the verified package disabled with no worker. Real consent and Agent greeting survive a real host restart. A later cached catalogue remains visible offline with the existing notice and disabled Update button; a direct backend update is also refused. The already-installed greeting still works. Network restoration recovers the fresh card and closing releases its index.
- **Check 36, `store.pending-mutations`:** a held artifact update is superseded first by production disable, then by removal. Completion is refused and cannot overwrite the chosen disabled/removed state, including after actual restart. The original installed bytes remain usable after explicit re-enablement. A subsequent explicit fresh installation/consent selects the new greeting after restart. Both pending schedules then repeat with an active developer override: disable persists across restart without altering its parked installed record; removal persists across restart while the developer remains enabled and returns its own greeting. Unloading confirms that the removed installed copy cannot reappear.
- **Additional integrity coverage, `store.integrity-close`:** same-length corrupt bytes fail the artifact hash; closing cancels a held artifact request before publication; invalid catalogue signature falls back offline and cannot start a new artifact request. Corrected signed bytes install and recover the approved greeting.

Disable/remove scheduling uses the existing guarded fixed-fixture production controls. Store install/offline controls and route closure are exercised through the real application. Every greeting uses actual permission consent and Agent confirmation; there is no direct arbitrary action or tool-approval control.

## Focused audit and corrections

Graph minimal context, semantic lookup, change detection and review context guided direct inspection of store refresh/close cancellation, signed metadata and hash validation, registry revision ownership at installation publication, disabled/remove mutations, feature-gated initialization and harness teardown. Graph edges are incomplete and do not certify execution paths by themselves.

The review corrected the following test assumptions before acceptance:

1. The initial store case used backend commands without proving the actual Store component cleanup/offline controls. It now runs ten UI route cycles, actual Install, the offline notice and disabled Update.
2. The profile marker also needs to reject the ordinary event port 7124. Both model/store validation now refuse it; the feature-gated marker test covers invalid store ports and existing isolation constraints.
3. The fixture's initial legacy unhashed minisign form did not match the production verifier's modern-signature policy. Before its first application run, the signer was corrected to `ED`/Blake2b-512 with the global signature. A Node self-test independently verifies authentic versus modified bytes and checks that the fixed key is absent from production trust anchors.
4. Comparing the complete check 36 procedure with the initial scenario exposed missing restart assertions for the disabled/removed state and the conditional developer-override schedules. These were added before accepting the row; the combined regression predates this expansion.
5. The expanded override case initially expected a second declaration-consent sheet for an unchanged already-approved tool declaration (`run-P3Q08L`). The real production control correctly enabled it without a second sheet. The test now asserts existing declaration approval and uses production enable; every greeting still requires actual Agent call approval. A subsequent parked-record comparison initially read the active developer record rather than `dev.replaced` (`run-1QFIil`); the assertion now compares the correct serialized parked installed record. Both failed runs have cleanup Pass and remain failure evidence, not product Pass.

No production store defect was reproduced in this unit. Changes add scoped test initialization, coarse read-only observations and repeatable fixtures/assertions. They do not add production endpoints, replace production trust keys or change the ordinary Store interface.

Fixture servers, request journals and held sockets have bounded ownership. Teardown destroys held responses/connections, closes the listener and verifies endpoint release, including intentional failure. A failed assertion stops later cases and cannot silently count them Pass. Existing executable/source/runner stamps prevent accepting a changed definition during a run.

## Verification evidence

`run-4ihytH` passed all **28** real-app cases with cleanup Pass, including store and prior Agent/native/CLI cases. The production idle case took 272,041 ms and the absolute native deadline measured 20,009 ms. `run-uKrfcn` repeated the three store cases with cleanup Pass. These runs predate the added check 36 restart/override assertions; they are combined regression evidence rather than certification of those later assertions.

The three final store cases passed twice from independent clean profiles: **`run-giIwNy`** and **`run-FCVjyS`**, both cleanup Pass. Each records seven owned host lifetimes, including post-disable/post-removal and developer-override restarts. Final runner fingerprint: `7b759e516f4e2596c61adc30e4ed38830e91517721747f9b4a5df2eacf71207a`. The earlier combined run has runner fingerprint `ab2e963be52d768541edce691fa00768bee555f54098e2c37d93120fcddfa7ce`; executable/source identity is unchanged.

Earlier `run-wBWBTU` deliberately withheld actual Store closure, failed the cancelled-response deadline with exit 1 and cleanup Pass, and captured the real loading page with no worker or tool events. Final **`run-qAFd1y`** detected the same deliberate withheld-close fault after 20,040 ms, exit 1, cleanup Pass, with zero tool events/zero workers and the final runner fingerprint. Neither intentional negative run counts as a product Pass. Preliminary `run-JguMGk` and `run-hNWtNb` passed all three store cases with cleanup Pass but predate the final port-guard rebuild; these are retained as earlier evidence rather than relabeled final.

The final feature host built successfully, including frontend type/embedded-asset compilation. Eight normal-build store logic tests passed in `logic-X2gDA6`; the ignored live-network test remains ignored. One feature isolation-marker test passed. Fifteen runner/fixture self-tests passed. Backend Clippy succeeded with 74 warnings; warnings remain and no broad cleanup is inferred.

Build identity: host SHA-256 `d073206e878b9b70fa34ef3665cc1645e26f595277a3122d4357c8626e6484f8`; application-source fingerprint `900fa95beb866e10bb13981a7280dde2e0c00dab7ce299a211d3c424d1214d70`. CLI SHA-256 remains `10babd722118db5ff80d79aa549355c381d2da750585de49eeb72f8ed068dd4a`, restamped against that source. Reports identify precommit base `c3be5770` and dirty files separately. Full local reports/logs are ignored artifacts; checked-in commands reproduce the procedures.

## Research basis and limits

[VS Code's installation service](https://github.com/microsoft/vscode/blob/main/src/vs/platform/extensionManagement/node/extensionManagementService.ts) supplies a primary-source reference for cancellation checks and staged installation publication. [TUF overview](https://theupdateframework.io/docs/overview/) and [specification](https://theupdateframework.github.io/specification/v1.0.36/) inform separating authenticity, expiry/rollback and artifact hash/size checks. Grain is not claimed to implement the full TUF role/rotation protocol. The [actual minisign verifier source](https://github.com/jedisct1/rust-minisign-verify/blob/master/src/lib.rs) supplies the modern-signature compatibility reference; the fixture leaves that production verifier unchanged. These are multiple independently relevant primary references, not an unverified popularity ranking or borrowed certification.

Not certified here: live production catalogue/publisher/root rotation, production roots revocation, vault/account replacement, nonempty extension settings preservation, power-loss durability, R1 interrupted migration, memory/handle trends, physical click targeting or microphone/pill behavior. Existing default-build store tests provide separate deterministic expiry/revocation evidence; the three real-app cases do not expand that into live certification.

**Acceptance:** checks 18 and 36 are reviewed real-application automated Pass. Ledger: **27 Pass / 26 Pending** (15 human, 12 reviewed automated). The signed-store third unit is accepted after its focused audit and two final clean-profile runs.

The next 2D unit is remaining registry/quarantine/disabled ownership and account/vault prerequisites in checks 49/53. Keep their full rows Pending until complete procedure evidence exists. No new ordinary-app microphone batch or hosted OAuth login is needed for this permission-free store unit. Whole 2D and all seven phase gates remain open.
