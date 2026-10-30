# Native CLI packaging and artifact ownership — Block 2D second unit

Date: 1 October 2026. Companion to the [first installation audit](NATIVE-INSTALLATION-AUDIT.md), [plan](MCP-EXTENSION-PLAN.md) and [progress ledger](MCP-EXTENSION-PROGRESS.md). The user confirms all three first-unit ordinary-app procedures passed after `8ffcb79c`; check 40 and that unit are accepted. This second unit covers native packaging and ownership, before moving to MCP/authentication.

## Scope and implementation

`native.cli-package-ownership` is the sixth maintained installation scenario. It creates fixed, disposable projects with one confirmed greeting, no permissions/authentication, distinct source revisions and the existing local 512-pixel icon. The actual `grain-ext pack` command runs the fixed npm build script with the repository's installed esbuild, then the actual `doctor` validates the built project. No fixture dependency installation, external account or development watcher is needed.

The builder compiles the CLI with `--locked` and locates it through versioned Cargo metadata. A separate CLI stamp binds its executable hash and application-source fingerprint. The packaging scenario checks that stamp before starting the host, records the CLI identity and command exits, and checks its executable hash again afterward. Other suites do not require a CLI stamp. Reports retain package/icon hashes and command metadata instead of extension source or account secrets.

The package import seam uses the existing guarded, fixed-path production import command. Installation approval uses the real permission sheet, and every actual greeting still uses the real Agent approval/executor/worker path. The fixture setup bypasses the operating-system file picker; the user's accepted check 40 supplies ordinary-app import/consent/control evidence separately.

The scenario verifies:

- Real compiled package replacement with unchanged id/version, distinct actual greeting results, stale approval refusal with zero extra dispatch, and current bytes after actual process restart.
- An original installed approval kept open through developer override and installed restoration; it remains unusable even when the visible installed owner returns. The serialized installed record must match its pre-override value, including enablement, grants, artifact identity and this fixture's default settings.
- CLI-built developer folders A/B, old-call refusal at successful owner changes, selected developer ownership after restart and restored installed results after unloading/restarting.
- Both historical flat and version-only package files containing obsolete source, seeded only after the owned host exits. Neither can shadow the current hash-owned package; their bytes remain unchanged rather than being silently deleted.

## Focused audit

Graph minimal-context/semantic exploration located the actual CLI pack pipeline and legacy pack path. Change detection and review context were followed by direct inspection of the changed runner/stamp/build code, production CLI build/doctor/pack, SDK package validation, imported activation and native artifact lookup. Graph flow/test edges are incomplete; they are navigation guidance, not verification evidence.

The audit found a coverage gap in the initial packaging case: checking refusals separately at each owner change did not explicitly keep the original installed approval through the complete override-and-restore cycle. That procedure was added, including a comparison of the serialized installed record. This is a test-oracle correction, not a newly established product defect.

The existing production loader reads the recorded hash-owned artifact first. A corrupt current artifact fails rather than falling back; historical lookup candidates are still bound to the recorded hash/id/version. The test checks visible source selection with conflicting legacy bytes; it does not modify the active registry or corrupt a live artifact to force success.

CLI operations have a 30-second completion deadline and a 32,768-character output ceiling. Spawned commands are hidden, use fixed arguments and owned project/output paths, and kill only their own process tree on failed completion/cancellation. Fixed build scripts finish normally without retained workers/watchers. Host restarts retain the earlier exit-zero, endpoint-release and new-session requirements. Final cleanup remains mandatory; no ordinary profile, microphone, credential namespace or Handy source is modified.

## Research references

- [VS Code's packaging guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension) separates packaging from publishing. This motivates testing Grain's actual CLI artifact rather than treating a handcrafted imported JSON fixture as CLI/store evidence.
- [VS Code's extension-management implementation](https://github.com/microsoft/vscode/blob/main/src/vs/platform/extensionManagement/node/extensionManagementService.ts) supplies an implementation reference for installation and replacement ownership. Grain retains its existing tool-only contract and hash-owned storage rather than copying broad editor capabilities.
- [VS Code's real extension test utility](https://github.com/microsoft/vscode-test) supports testing installation in the actual host. Grain uses its existing isolated Tauri/WebView2 runner, with separate scripted-model and human evidence.
- [Chrome's unpacked-extension/reload procedure](https://developer.chrome.com/docs/extensions/get-started/tutorial/hello-world) distinguishes changed source from loading/reloading the running extension. Grain additionally tests exact call approval across restored ownership; this stronger local requirement is not a Chrome protocol claim.
- [Cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html) documents the versioned JSON format and target directory, used to locate the built CLI without hard-coding a developer's Cargo path.

These are source and workflow references, not claims that another project's implementation certifies Grain or that popularity ranks were measured.

## Verification and remaining boundaries

**Focused source audit and post-audit repeat runs complete. This packaging unit is accepted.** Checks **28 and 39** are explicitly recorded as reviewed real-app automated Pass for the disposable tool-only fixture. Check **53** retains Pending status: its CLI/owner/result/restart portion passes, while its conditional OAuth/account portion is Blocked because no authenticated native fixture was used. Check **49** retains its wider quarantine/disabled/account recovery requirements. Store checks **18 and 36**, the whole 2D block and all seven whole-phase gates remain open. The ledger is **25 Pass / 28 Pending**, comprising **15 user-reported and 10 reviewed automated** results.

| Evidence | Result |
|---|---|
| `run-ZaXydS` | Initial CLI packaging scenario Pass, cleanup Pass. |
| `run-cwb37u` | All **25** real-app scenarios Pass, cleanup Pass, **14** owned host lifetimes. Production idle retirement observed after **145,468 ms**; full idle case **272,290 ms**. Absolute native deadline **20,004 ms** from invocation. This run precedes the added original-approval round trip. |
| `run-NgmddX`, `run-FlNtip` | After the focused audit correction, all **six** installation cases Pass twice from separate clean profiles; both cleanup Pass, fourteen host lifetimes each. Exact saved installed record and both legacy files preserved; all eight real CLI command exits zero in each packaging case. |
| `run-5gFNhK` | Deliberate `stale-package` fault fails the expected replacement greeting, exit **1**, cleanup Pass. Failure screenshot shows actual `Harness hello (packaged-one)` instead of `packaged-two`. This is a detected oracle failure, not product acceptance. |
| Self/CLI/static/build | **13** harness self-tests and **9** CLI library tests Pass. Frontend type/embedded build, separate host/CLI builds and scoped JS formatting/whitespace checks Pass. The feature host build has twelve existing warnings. The earlier 199 normal-build logic tests were not rerun or added to these counts; application sources are unchanged. |

Runtime reports identify the dirty precommit tree based on **`8ffcb79c`**, host executable SHA-256 **`ce84f598614dd43933aa8c1d6349fbb28f12722f220a4f872bedd3884030c4dc`**, CLI executable SHA-256 **`10babd722118db5ff80d79aa549355c381d2da750585de49eeb72f8ed068dd4a`** and unchanged application-source fingerprint **`413bf55d818ca801e9a4977089bb5378b4b2947b8d9d833017846e588d84d8a4`**. The initial single/full runs use runner fingerprint **`6058231e967ccf34a81f9bf1977518b5fd703bd5bf0a2b49349dd5c301ed30aa`**. Final repeated suites and deliberate fault use **`2e6bd1ecd6a70625f796345dee3dbd73f3630f93c4fb7a32ee9fa79726b95d61`**; the unchanged twenty-four other scenarios are supplemented by these post-audit installation retests, not relabeled as another full-matrix run.

The initial package SHA-256 is **`770ee17508ac959b4c4046e915178206a46ae899aaebb002b1a0e3eb93934de5`**; same-version replacement is **`5f5a0d557061e7a119ef82d1d78cc6b15ff352ce6a2a242e3f2834d6c579797a`**. The recorded icon SHA-256 is **`531bd7a7205401d02ac31d386c6619a352702467eead8b1475bdbcebe972d5b1`**. Unrelated pre-existing generated bindings remain unstaged. This unit changes harness tooling/documentation only; it adds no production engine, permission or extension API.

Reproduce from the repository root in PowerShell:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
npm run test:agent -- --suite native-installation
npm run test:agent -- --suite all
npm run test:agent -- --scenario native.cli-package-ownership --fault stale-package
npm run test:agent:harness
cargo test --locked -p grain-ext-cli
```

The deliberate fault must fail replacement-result verification, exit 1 and still clean up. A detected fault is oracle evidence, not a product Pass. Reports remain in ignored `tests/agent-harness/.runs/`; their recorded precommit identities must not be relabeled.

This permission-free fixture has no nonempty extension settings or OAuth grant. Native account/vault ownership, quarantine/disabled-record recovery, signed-store catalog/offline/download schedules, abrupt power loss and multiple registry writers remain separate gates. It does not certify live-model judgment, physical input targeting or a universal extension/provider contract. No new ordinary-app microphone batch is requested for this unit; the user's accepted procedures are retained for reproduction.
