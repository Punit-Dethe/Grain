# Native consent, ownership and restart — Block 2D first unit

Date: 1 October 2026. Block 2C is accepted after the user's three ordinary-app observations. This first 2D unit covers native package consent, owner replacement and registry persistence in the maintained real-app harness. **It does not close all of 2D:** signed-store/offline/download schedules, native OAuth/account ownership and wider registry recovery retain separate requirements. First-unit automation excluded file-picker/CLI packaging and ordinary input; the user subsequently completed check 40, and the second packaging audit below closes CLI-specific gaps.

## Implemented coverage

The `native-installation` suite adds five scenarios to the actual isolated Grain host:

| Scenario | Verified behavior | Numbered checks supported |
|---|---|---|
| `native.consent-persistence` | Cancel the real permission dialog; restart disabled/unapproved. Allow once; greet; restart enabled; greet again. Disable with a pending call; refuse it; restart disabled; enable with the saved review and greet. | 40, 52 |
| `native.stale-review` | Keep the real permission sheet open while replacing the package/declaration. Old Allow fails without activation/worker creation. Fresh sheet shows the revised name/tool and enables the current greeting. | 38 |
| `native.owner-restoration` | Same-id/version import changes the actual greeting and refuses the old approval. Installed → developer A → developer B → installed restoration uses distinct actual results, with old-approval refusals and real restarts after package replacement, developer replacement and restoration. | 28, 39, 53 |
| `native.enablement-approval` | Pending call becomes invalid after disable/re-enable, and again after source replacement. Fresh requests each recover once. | 19 |
| `native.registry-refusal` | Malformed and future-migration-version files are written only after the owned host exits. Actual startup preserves their exact bytes, logs registry load refusal, admits no registry/worker and refuses import. Restoring the original file after exit restores enabled ownership and actual calls. | 49 |

Imported packs are maintained JSON fixtures with only the confirmed greeting, no permissions, authentication, native companions or external accounts. They are checked by the real SDK and production import command. Tests select fixed paths/IDs under their owned root. The package seam skips the operating-system file picker and CLI submission packaging; it cannot establish those UX/contracts. Developer loading preapproves declarations by existing policy and leaves them disabled; tests enable through the production command, rather than expecting a permission dialog that policy does not show.

Restart actually exits the owned host with code zero, disconnects its WebViews, verifies both old endpoints close, then launches the same stamped executable with the same isolated marker/profile. The new session must have no worker, pending approval or active Agent. Evidence records every host PID, session-tagged events and bounded coarse owner/enablement/review observations. Startup has a fresh events buffer; the runner preserves earlier sessions with an aggregate 4096-event ceiling. Cleanup removes only verified children of that unique run and never operates on an ordinary profile.

## Findings, corrections and focused audit

The first management runs stopped before acceptance because a fresh profile has no ASR model, and the embedded production assets omit the developer navigation entry. The test build now resolves onboarding to Done behind `agent-harness` only; its existing startup guards still require a distinct app ID, owned profile and debug build. Tests navigate the existing management route and use its real components/handlers. They do not download a model, activate a microphone, change the normal navigation, provide a Tauri mock or render an alternate UI. The failed runs remain preserved.

`run-aVZDnU` reproduced the explicit harness shutdown being vetoed by Grain's normal tray keep-alive policy. Previous final cleanup could fall back to killing the owned process; a restart test must not silently count that as an ordinary exit. The guarded harness shutdown now sets the same `INTENTIONAL_QUIT` flag as tray Quit before requesting exit. This repairs the test seam, not normal tray behavior. Subsequent restart assertions require exit zero and endpoint release; forced cleanup remains a failure-recovery fallback, not a successful restart.

`run-z68ixw` passed consent persistence but found the next scenario using a stale mounted management card after fixture-only backend setup. Ordinary import handlers refresh their cards themselves; fixture commands do not drive those React handlers. The runner remounts the normal route after setup, while deliberately keeping an old permission sheet mounted for the stale-review test. No product UI change or automatic test retry was introduced.

Graph entry/change/review/impact tools were consulted, followed by direct source review of guarded test commands, production import/grant/enable/unload/uninstall, the registry startup path and runner teardown. Graph output reported incomplete flow/test edges; its risk scores are not acceptance evidence. The focused review verified:

- Controls remain feature-gated and require the isolated main window/app/profile. They accept a finite operation enum with fixed fixture paths and ID; imported bytes are bounded before decoding, validated by the SDK, and forbidden from declaring permissions or authentication. The real production import revalidates before publishing.
- Installed consent uses the real sheet's Cancel/Allow handlers, exact reviewed digest and save-before-enable path. Imported changes cannot inherit an unreviewed declaration; a stale review cannot widen or enable it.
- Call approval is separate from installation approval. Every greeting still needs the Agent's real confirmation. No test command executes a tool or approves an individual call, and old approvals are checked for zero additional dispatch.
- Source/package/developer owner changes go through production publication/replacement logic. Actual distinct greeting results, preserved enabled state and restart ownership supplement generation/status assertions; no registry mutation is used to force a desired owner.
- Damaged-state setup runs only between owned process lifetimes. Original valid bytes are saved/restored in that disposable profile; invalid inputs are compared byte-for-byte after startup. No recovery UI, interrupted power-loss transaction, multiple-writer or account/vault certification is claimed.
- The normal build retains normal onboarding/tray behavior. The Handy-derived tree and production UI design were not changed. No additional resident service, timer, worker engine or production retained source cache was introduced.

## Research references

- [Chrome permission-update guidance](https://developer.chrome.com/docs/extensions/develop/concepts/permission-warnings) explains disabling an extension until a newly required permission is accepted. Grain additionally binds review to exact declarations and ownership; a version label or old sheet alone does not authorize replacement tools.
- [VS Code's real extension smoke tests](https://github.com/microsoft/vscode/blob/main/test/smoke/src/areas/extensions/extensions.test.ts) and [installation/restart acceptance checklist](https://github.com/microsoft/vscode/wiki/Smoke-Test) support testing real installation/state behavior across application lifetimes. Grain retains its stricter tool-only boundary rather than inheriting VS Code's broad extension capabilities.
- [Electron desktop testing guidance](https://www.electronjs.org/docs/latest/tutorial/automated-testing) provides a reference for driving a real desktop application's existing WebViews and lifecycle. Grain is Tauri/WebView2 and uses CDP only on its owned test host; no Electron runtime or browser replica was added.
- [Rust rename semantics](https://doc.rust-lang.org/std/fs/fn.rename.html) and the [Windows rename issue](https://github.com/rust-lang/rust/issues/123985) explain why persistent state deserves real Windows/restart evidence alongside deterministic sharing-lock tests. Existing atomic publication/bounded retry behavior was retained; these tests do not certify power-loss durability.

## Verification and acceptance

**Focused implementation, source audit and automated retests complete. The user confirms all three ordinary-app check 40 procedures passed after `8ffcb79c`; the first 2D unit is accepted. The whole 2D block remains open.** Checks **19, 38 and 52** have complete reviewed real-app procedure evidence and are explicitly recorded as automated Pass. Check **40** is now user-reported Pass. At this first-unit snapshot, checks **28, 39, 49 and 53** retained partial supporting coverage/Pending status: CLI packaging, legacy artifact shadowing, disabled/quarantined/account preservation and conditional native-account observations were not all certified. The user confirms ordinary file-picker/consent/control and input observations; these are distinct from packaging and account certification. Signed-store checks **18 and 36** have no real-app acceptance evidence in this unit. At automated completion the ledger was **22 Pass / 31 Pending**; after the user's check 40 result it is **23 Pass / 30 Pending** (15 human, 8 reviewed automated).

| Evidence | Result |
|---|---|
| `run-3fp5CX` | All **24** real-app scenarios Pass; cleanup Pass. **11** owned host lifetimes, with ten actual restart transitions. The existing native idle case took **270,293 ms**, with retirement after **143,540 ms**; the absolute tool deadline measured **20,009 ms**. |
| `run-DJ79qE` | All **five** installation scenarios Pass from another clean profile; cleanup Pass. Again eleven host lifetimes/ten restart transitions. |
| `run-QnxWlK` | Deliberate `skip-restart` fault failed the session assertion (`1 !== 2`), exit **1**, cleanup Pass, zero tool dispatch. This verifies failure detection; it is not a passing product scenario. |
| `logic-RpxOf2` | All eight normal-build production groups Pass: **47 registry + 8 imported-update + 4 developer + 18 Agent + 49 native + 4 execution + 46 MCP + 23 auth = 199** tests. Counts overlap earlier evidence and are not cumulative coverage. |
| Self/static/build | **12** harness self-tests Pass; frontend type/build and separate host build Pass; scoped Rust/JS formatting/diff checks Pass. Normal backend Clippy exits successfully with **68 existing warnings**, and the feature host has its expected startup warnings. |
| Manual package helper | Windows PowerShell produced valid UTF-8 JSON with one confirmed tool, no permissions/authentication and the expected fixed greeting source. No ordinary profile/account was changed. Store/CLI-submission packaging is excluded. |

Both accepted runtime batches use executable SHA-256 **`354303346b7bc88510446518ab88be3f5deacb0dfaf9f953a102fffcb6b19c35`**, source fingerprint **`413bf55d818ca801e9a4977089bb5378b4b2947b8d9d833017846e588d84d8a4`**, and runner fingerprint **`724bdcbdc00c2504b76948507f01fb16d52f0229e9684cf2aae81602f1e52f6b`**. Reports identify a dirty precommit tree based on `0884aca5`; they are not relabeled as postcommit runs. Full reports/screenshots live in ignored `tests/agent-harness/.runs/`. Reproduce with the versioned commands below. Earlier setup/shutdown/route failures remain preserved; none is counted as acceptance.

Reproduce after `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1`:

```powershell
npm run test:agent -- --suite native-installation
npm run test:agent -- --suite all
npm run test:agent -- --scenario native.consent-persistence --fault skip-restart
npm run test:agent:logic -- --group all
```

The deliberate `skip-restart` run must fail the new-host-session assertion, exit 1 and still clean up. A failed oracle is not a product Pass.

## Ordinary-app consent/persistence handoff

Use the normal app and its existing model/microphone. Rebuild/start with `bun run dev:asr` from the repository if necessary. This is a native fixture, not a hosted-provider sign-in.

```powershell
cd C:\Projects\Grain\grain
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/prepare-installation.ps1
```

The command prints one disposable `.grainpack`; it requests no permissions or external account. It intentionally lacks a submission icon and is not a CLI/store-submission certificate.

1. **Cancel stays off:** Extensions → Import pack → select the printed file. Enable **Native Installation Smoke**, inspect the single **Say installation hello** tool, then Cancel. Fully quit Grain through tray Quit and reopen it. Expect the extension to remain Disabled.
2. **One Allow survives restart:** Enable again → Allow and enable once. Ask Agent: **“Use Native Installation Smoke's Say installation hello tool.”** Approve the actual call; expect **`Harness hello (installed-one)`**. Fully quit/reopen and request/approve it again. Expect the same real result, without another installation-permission sheet.
3. **Ordinary input and controls:** Close Agent, dictate into a scratch field, then start/cancel another recording through the pill. Expect one paste from the completed recording and none from cancellation. Confirm the permission sheet and enable/disable controls remain usable. Uninstall Native Installation Smoke afterward.

The user reports all three observations Pass; this procedure remains for reproduction. Do not edit registry files or change folder permissions in the ordinary profile. This batch closes the ordinary-app portion of check 40 and validates the real file picker; it does not unblock signed-store/account portions of 2D by implication.

## Subsequent packaging unit

The [second 2D audit](NATIVE-PACKAGING-AUDIT.md) closes CLI packaging, original installed-approval refusal through full override/restoration and legacy-file non-shadowing gaps. The suite now has six cases. Checks 28/39 are reviewed automated Pass; check 40 is user-reported Pass. Current ledger: **25 Pass / 28 Pending**. The historical first-unit run counts/identities above are preserved. Signed-store and wider registry/account requirements remain open.
