# Native migration and typed arguments — B1a acceptance audit

Date: 1 October 2026 (UTC evidence). Branch: `extensions/tool-only-retirement`. Companion to the [forward execution plan](MCP-EXTENSION-REUSE-EXECUTION-PLAN.md), [numbered procedures](MCP-EXTENSION-PLAN.md), [progress ledger](MCP-EXTENSION-PROGRESS.md) and [runner instructions](../../tests/agent-harness/README.md).

## Scope and conclusion

**B1a is accepted for checks 2 and 22**, after focused source review, failure fixes, two clean real-application runs and deliberately failing oracles. The ledger becomes **30 Pass / 23 Pending**: 15 human and 15 reviewed automated results. This closes one small unit, not B1, whole 2D or any R0–R6 phase. Eight native account checks remain in B1; the separately guarded authenticated fixture is next. No account login, SDK upgrade, OAuth replacement or physical retirement is claimed here.

The maintained `native-foundation` suite exercises actual isolated Grain startup, production registry/migration, Agent discovery and approval, and WebView2 native workers. The scripted model checks the actual returned worker payload, rather than copying its own request. No browser-only replica, normal profile mutation or approval bypass is used. Authenticated fixtures remain refused by the existing permission-free fixture gate.

## Full procedure mapping

**Check 22 — `native.typed-contract`:** seven payload cases cover the native contract's text, number and entity types. Unicode, quotes and multiline text, decimal/negative/zero numbers, entity identifiers, omitted optional text/number, explicit optional null and empty optional text are compared against actual worker JSON. Null optionals must be omitted; numbers must remain numbers. Two tools respect the existing four-parameter declaration limit. Every valid call requires real approval, has zero dispatch before approval, exactly one dispatch afterward, a successful outcome and exactly one verified model continuation.

While an approval is pending, the test changes only the manifest declaration: optional `note` becomes required with a valid utterance placeholder. No reload occurs; the owner and warm worker identity remain unchanged. The old approval must refuse with zero additional dispatch. Explicit reload and a fresh call using the new contract succeed. The original declaration is restored in `finally`. This directly tests a stricter boundary than relying on a reload generation to invalidate approval; the existing source/reload suite independently covers that path.

**Check 2 — `native.legacy-migration`:** first install/consent/greet through actual host commands, then seed only the stopped, disposable profile with a legacy capture grant, edited extension prompt and custom binding. Actual startup must disable/quarantine it, clear old grants and declaration approval, archive exact edits, remove their active contribution, select a valid first-party prompt, preserve ordinary prompts/shortcut bindings/artifact bytes/user-data sentinel and start zero workers. An explicit enable attempt must refuse. Repeat startup must retain identical archive bytes.

An interrupted durable checkpoint is then seeded: archives contain both user-edited prompt versions while active legacy settings and migration version remain old. Startup must finish without duplicating or losing either edit; another startup preserves the bytes. This simulates a persisted interrupted checkpoint, not a power-loss/crash certification at every write boundary.

A real Windows handle denying registry replacement makes actual uninstall fail. Both the in-memory owner and disk registry must remain intact; restart preserves the migration state/archives. Successful uninstall followed by valid developer load/greeting proves recovery. Finally a developer-only legacy declaration is quarantined at actual startup; terminal unload, restoring the valid declaration and fresh load/greeting recover. There are **six restart transitions, seven owned host lifetimes** per migration scenario. No retired action is dispatched.

## Reproduced bug and repair

`run-go2Bag` completed archive/restart checks but failed the fresh valid tool after successful legacy uninstall: the removed installation's quarantine reason remained keyed by its ID. That stale refusal also affected terminal developer unload.

`crates/grain-core/src/extensions.rs` now removes an identity's quarantine only when its last record is actually removed. Removing a parked installed copy retains the effective developer record and its refusal. Restoring a parked owner retains the existing quarantine behavior. A later valid package still passes normal validation/approval; this repair grants no retired capability.

Uninstall now follows the existing `save_gate` → registry write-lock order and uses the existing small `RecordEditSnapshot`/`persist_record_edit` path. Failure restores only that owner's record, quarantine, affected slots and scalar revision counters before readers regain access. This avoids losing the in-memory owner when registry publication fails. No all-registry copy, service, journal or new dependency is added.

Two regression functions cover terminal installed/developer removal, parked-copy preservation, unrelated quarantine isolation, and failed uninstall in plain/quarantined/parked states. The real-app lock test reproduces failed publication through the actual host. Source review of `grain_commands::extension_uninstall` confirms its `?` returns before credential/artifact cleanup when registry removal fails; this is source evidence, not real vault acceptance.

## Focused audit

Graph exploration preceded file reads. Change detection identified registry, unload, model and submission boundaries as review priorities. Graph review reported medium risk and missing test edges; its zero-flow output is not proof of isolation. Direct inspection covered snapshot restoration, save lock ordering, terminal versus parked ownership, startup quarantine, actual uninstall cleanup order, fixed instruction admission, worker result verification, runner fault restrictions and owned restart/lock-helper cleanup.

The Rust harness extension adds seven fixed typed instruction variants only. Existing debug/profile/window checks remain; ordinary builds do not compile those commands. Fixtures remain permission-free, and migration mutations are restricted to the disposable root. Tests restore edited manifests or remove the owned profile after failure. Reports retain stages/counts and a verification flag rather than raw typed arguments/results. Existing real-app close/escape limitations remain unchanged.

Research comparisons use primary sources: [Goose's extension manager](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/extension_manager/mod.rs) couples extension removal with tool-cache invalidation; [VS Code connection ownership](https://github.com/microsoft/vscode/blob/a460613c57b4c1eb2bc8edc97be694e05ae286b2/src/vs/workbench/contrib/mcp/common/mcpServerConnection.ts) guards asynchronous work with disposal ownership. These inform the owner-lifetime review. Grain's quarantine and save rollback are local requirements implemented with its existing transaction helper, not behavior borrowed wholesale from either project. The broader multi-project library decision remains unchanged.

## Evidence and reproduction

Build once, then run one suite at a time from the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
npm run test:agent -- --suite native-foundation
npm run test:agent -- --scenario native.typed-contract --fault stringified-number
npm run test:agent -- --scenario native.legacy-migration --fault lost-migration-archive
```

Both fault commands must report **Fail, exit 1 and cleanup Pass**. They mutate only actual owned fixture output/archive, never production source or the user's state. A successful exit from an injected fault is an oracle failure, not acceptance.

| Evidence | Result |
|---|---|
| `run-9Ii84Z` | Both final B1a cases Pass; cleanup Pass. |
| `run-5MUlVS`, after audit | Both cases repeated in a clean profile, Pass; cleanup Pass. |
| `run-qX8nKr` / `run-qF6TP9`, final assertion | Source review added exact first-party binding preservation. Both final cases Pass twice, cleanup Pass. Last typed case 4,467 ms, migration 7,276 ms; scenario timings, not cold/warm performance baselines. |
| `run-sQMF6j` | Actual number converted to text; typed result oracle fails, exit 1, cleanup Pass. |
| `run-STeXBe` | Actual migration prompt archive removed; preservation oracle fails, exit 1, cleanup Pass. |
| `run-7vtjTu` / `run-ghNMQp`, final oracles | Both corruptions repeated against final runner; Fail, exit 1, cleanup Pass. |
| `run-tpUtaf` | All six installation cases Pass; cleanup Pass. |
| `run-H1Y8cG` | All three registry recovery cases Pass; cleanup Pass. |
| `run-giFzXN` | All three store cases Pass; cleanup Pass. |
| `run-Cpo3je` | All six native-failure cases Pass, including invalid inputs, actual 20-second deadline and result/wire bounds; cleanup Pass. |
| Production logic | 262 core unit + 4 integration tests, including 49 registry tests, Pass. Core Clippy with warnings denied Pass. |
| Harness/static/build | 18 runner self-tests Pass; ordinary backend library check, scoped formatting/whitespace and real harness build with embedded frontend type/build checks Pass. Existing unrelated backend warnings remain. |

Final B1a runs bind runner/fixture fingerprint `543946c305949f482d04ea30d0ec5ccafe051d213a79ae22cd295183d729d4c8`; earlier B1a and affected installation/registry/store/failure retests used `4a0fb72cbdfbc55de0bdc04a028e15c7f24a2eff3417c7d5738babf868c5699f` before the binding-only assertion. Both share host SHA-256 `cac5d0e1b5033e2c6e7064ed6b5d7c9f5d54a0c26b29bca2b8e43b14556804ff`; application source fingerprint `8aaeb678326c85424be7c0ce65a3aec473639e883eaa8b3d84c4983dbf437a99`. Build base is `b0afe340`; reports record the actual precommit dirty sources independently. Reports remain in ignored `.runs/<run>/evidence/`; documentation stores the durable summary, not private artifacts. The unrelated generated `src/app/bindings.ts` edit is excluded from the commit and harness source generation.

## Earlier failures and limits

All these development runs retained failure and cleanup Pass: `run-M7iasR` exceeded the four-parameter declaration limit; `run-7qFiB1` lacked fixed instruction admission; `run-l1gy6w` made a required parameter invalid in the fixture utterance; `run-zv1h8z` wrongly expected first-party default prompts to disappear; `run-go2Bag` reproduced the product quarantine bug; `run-9BeDkJ` expected an inner OS error instead of the public persistence context. Fixture/oracle assumptions were corrected without weakening product policy. Earlier partial typed passes were not credited as whole B1a acceptance.

No ordinary-profile upgrade review, real provider/vault, microphone/pill, live-model judgment, other-platform, measured RAM/handle or power-loss certificate follows. The earlier accepted-input Escape timeout remains separately open in [the input investigation](NATIVE-INPUT-INVESTIGATION.md); clean B1a runs do not resolve it. Physical legacy code removal and all seven release phase gates remain open. No additional human test is needed for this controlled B1a slice; later real-account batches remain required where specified.
