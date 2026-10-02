# Live nested MCP read and Linear contract audit

**Date:** 3 October 2026. **Branch:** `extensions/tool-only-retirement`.
**Disposition:** check **17** accepted after scoped source review, real-app repeats, detected negative oracle and affected regressions. Baseline **52 Pass / 1 Pending (12)**: 15 user-reported and 37 reviewed automated results. This accepts the remaining B2 baseline requirement, not the release transport/authentication matrix or all seven release gates.

## Findings and choice

The existing plan still requires accepted baseline tests before B5 library replacement. The current production MCP path already uses official `rmcp` 3.1.4; B5a's SDK upgrade/wrapper review and B5b's native `oauth2` 5.0 replacement have not started. SDK MCP authentication and native-extension authentication are separate paths.

The user approved one isolated Linear browser consent. Actual run `run-I9d4DB` passed in **15,892 ms**: explicit read-only grant, production discovery, structural inspection of all **38 supported tools**, actual host restart, unchanged grant and repeated identical contract discovery. **None has nested object inputs.** Linear therefore cannot provide the missing live nested-read prerequisite for this observed catalog. No account tool was called. Its grant/profile were removed on completion; the 86,100-second lifetime and refresh availability are metadata, not proof of refresh.

Use the official public [Hugging Face MCP](https://github.com/huggingface/hf-mcp-server) for the missing shape. Its [`hf_fs` implementation](https://github.com/huggingface/hf-mcp-server/blob/main/packages/mcp/src/hf-fs.ts) supports nested operations and public text reads; the [provider documentation](https://huggingface.co/docs/hub/agents-mcp) identifies the canonical service and built-in tool. Inspecting live discovery through a bounded bare HTTP research probe established availability only; it received **no acceptance credit**. The following acceptance runs use Grain's real production adapter.

## Maintained implementation

- `mcp-linear-contracts --linear-sign-in`: one human consent, no tool execution, actual supported production schemas projected to bounded structural metadata. Retains tool/property names, primitive types, composition, required fields and schema digest; drops descriptions, defaults, enum/const values and other literals. `constraintsOmitted` and `executableSchema: false` prevent representing the projection as an executable contract. Full production validation remains authoritative.
- Projection limits: 128 tools, 64 KiB retained report, 8,192 traversed nodes, depth 24 and bounded property/required/composition lists. The JavaScript verifier independently validates the structure/classification and compares the actual catalog after restart. This observer is debug-feature-only; do not reuse it as production schema validation.
- `mcp-hf-live`: explicit opt-in excluded from `all`, fixed anonymous HTTPS endpoint, no credentials, one selected schema and actual approval. The transport guard permits only ordinary handshake/discovery methods and the exact nested read below. Other tools, commands, targets, batches and extra arguments are refused. The isolated marker cannot mix Hugging Face with Linear, DeepWiki, store, native account or local MCP fixtures.
- Both stages execute through the actual Agent React panel, production discovery/selection, approval, SDK transport, result bounds and continuation. The local model scripts decisions; it does **not** certify a genuine model's judgment or arbitrary provider compatibility. Existing configured-model workflow evidence remains separate.

```json
{
  "operations": [{
    "cmd": "cat",
    "args": [
      "hf://models/google-bert/bert-base-uncased/README.md",
      "--max-bytes",
      "2048"
    ]
  }]
}
```

The oracle verifies the actual nested schema, initial absence of action schemas, exactly one selected action, fixed typed input, zero dispatch before approval, one client dispatch, the untrusted-result prefix, UTF-8/16 KiB bounds and specific contents of the public BERT document. The actual model preview was **4,586 bytes**, including text and structured metadata. A real restart repeats discovery, approval and the read without importing any account. Client dispatch counts are not independent server-side execution receipts.

## Review and verification

Graph-first impact/diff review followed by manual tracing covered marker admission, restart settings, fixed endpoint/client, both HTTP posting paths, private Linear observer, model journal, fault admission and cleanup. Graph missing-test edges are review leads, not evidence that the actual tests were absent.

**69 Node harness self-tests and 16 Rust harness/isolation tests pass.** Negative unit cases reject private literal leakage, false nesting, duplicate/count/depth/node/byte overflow, changed contracts, flat schemas, wrong result document/content, replay and missing evidence. The maintained real-app build passes TypeScript, frontend/CLI and debug host compilation. Clippy succeeds with 74 existing warnings; a focused diagnostic scan finds none in the changed harness modules. Formatting and scoped diff checks pass; unrelated generated binding edits remain excluded.

The first Rust invocation omitted the maintained Windows DLL test launcher and exited `0xC0000139`; rerunning through `scripts/run-rust-test.ps1` passes. This was an invocation error, not a product repair. The first public negative-run command was refused by the older DeepWiki-only fault allowlist before starting a host; admission now permits exactly the two isolated public scenarios and rejects other uses. No product behavior, timeout or scenario retry was added.

| Evidence | Result |
|---|---|
| `run-I9d4DB` | Actual Linear contract/restart inspection, 38 supported / 0 nested, cleanup Pass |
| `run-dnyJcq`, `run-L0zbZS` | Initial independent public nested runs: 7,632 / 7,000 ms, each two approved reads across restart, cleanup Pass |
| `run-sN7Q2A`, `run-a3CV6f` | Final independent public nested repeats: 6,811 / 7,004 ms, same exact receipts, cleanup Pass |
| `run-xZkxIe`, `run-TNRHL5` | Missing-result-evidence fault detected: Fail/exit 1 at 3,072 / 3,060 ms; cleanup Pass; no retry |
| `run-pqR487` | Both ordinary native smoke cases Pass |
| `run-gSkmE7` | Actual Linear Disable/Disconnect cancellation and restart cases Pass |
| `run-nMkavh` | Controlled MCP transport/nested contract regression Pass |
| `run-EZk4my` | Both existing public DeepWiki read/disable/restart/result-bound regressions Pass |
| `run-g8OpKU` | Controlled SDK OAuth/account/nested-read/restart regression Pass |

All reports retain actual cleanup results. Independent inspection verifies removed owned scratch directories, no retained owned host/listener and zero scoped Linear grant/client-secret inventory. Historical policy-blocked scratch from other units remains separately open and was not touched.

Public host SHA-256: `fbec6a27fd2df4aae0e45cd3b79b92c4e43a64554a34283094d8766e82c42a2e`; source fingerprint `cf303d24e46ceedc3eb6c1820232b2782233aee64d6d6560f1545acc2915bc61`. Final runner fingerprint: `fc898b88c711067a343b18f9c421fd082617e5735d10602404198ebd486c1d0f`. Linear inspection used the earlier host `5ae01ef49845576301d9ea5d84b77503055e4dd7215a6e8eef4c5e0af1411d32`, source `07e481c42167e987d315c50cfb0d748e97a61fdb2726d808b534ec341a9b7661`, runner `4b15027359d18313a836c9989ef22e8accf4f95aa7067ba8fc149bf06286f409`. Build base is `ee8674b7` plus recorded precommit dirty sources; reports are not relabeled as postcommit execution. Earlier public/negative runner fingerprints remain in their actual reports.

## Reproduce and remaining work

```powershell
powershell.exe -NoProfile -File tests/agent-harness/build.ps1
node --test tests/agent-harness/runner.test.mjs
node tests/agent-harness/run.mjs --suite mcp-linear-contracts --linear-sign-in
node tests/agent-harness/run.mjs --suite mcp-hf-live
node tests/agent-harness/run.mjs --scenario mcp.hf-nested-read --fault missing-live-evidence
```

The Linear command requires the account owner in an interactive terminal; the public command does not. Reports stay in ignored `.runs/`, not in Git. No new manual batch is needed for accepted check 17.

**Only baseline check 12 remains.** Implement its finite owned live-expiry procedure next: one fresh read-only Linear grant; validate the actual `get_user` contract (`query: "me"`); approved identifying read and real restart; record actual issued expiry; close hosts/listeners during the waiting interval; preserve only an explicitly owned, resumable test grant/marker; after genuine expiry, verify actual SDK refresh, the same account/read, persisted replacement across another restart and zero new login; finally disconnect and independently inventory/delete only this run's credentials. A bounded preparation/resume/abort interface and lost-process cleanup must be tested before leaving a grant behind. **That interface is not implemented by this unit.** Neither changing a clock nor waiting after the already-deleted grant proves refresh.

After 53/53 baseline acceptance and this authentication audit, proceed B5a SDK → B5b native OAuth → B5c physical retirement, each with affected recertification. The original release gates, standalone conformance initialization limitation, production orphan reconciliation and `AGENT-BUDGET-01` remain open.
