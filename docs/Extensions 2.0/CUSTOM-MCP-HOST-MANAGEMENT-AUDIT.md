# E2c: custom MCP host management — 4 October 2026

Forward **62 Pass**, within metadata management scope. A custom server can now be imported, listed, edited and removed through the real application's guarded Tauri commands. It remains **inactive**. This does not certify custom networking, authentication, Agent tools, store acquisition or public UI.

## Implementation and ownership

`src-tauri/src/grain_mcp_connections.rs` is part of the existing `grain_mcp` module. Four commands are registered in the composition root:

| Command | Input | Result |
| --- | --- | --- |
| `mcp_connections_list` | None | Saved views |
| `mcp_connection_import` | `definitionJson` string | New inactive view |
| `mcp_connection_replace` | `id`, `expectedRevision` string, `definitionJson` | Committed view |
| `mcp_connection_remove` | `id`, `expectedRevision` string | Unit |

Every command requires the real `main` window and Extension Developer Mode. Mode is rechecked after scheduling and after acquiring the mutation lock. Disk I/O uses the existing Tokio blocking pool; there is no dedicated worker, process, listener, HTTP request or vault operation. A lazy app-owned registry retains at most 32 bounded nonsecret definitions. It opens no persistent file handles. Its publication locks/files are released by the existing core registry.

Example import:

```json
{"name":"My server","url":"https://service.example.com/mcp","authentication":{"type":"oauth"}}
```

The input remains the accepted strict 4 KiB `RemoteMcpConnection` format. It cannot contain enablement, commands, source/account IDs, headers, tokens or client credentials. Endpoint syntax validation is not network consent. No extension package is required.

Views expose only `id`, `revision`, `name`, `url`, `authentication` (`oauth`/`none`) and `state` (`inactive`). Vault account IDs remain internal. Revisions are canonical decimal strings, including values above JavaScript's exact-number range. Caller revisions resolve to an exact core lease; stale and noncanonical values cannot overwrite a newer record. A short host mutation lock covers both publication and the returned view, preventing a concurrent command from substituting a later result. Core cross-process file locking/digest checks remain authoritative.

Identical saves preserve bytes and revision. Label edits preserve the account binding and advance revision. URL/authentication edits rotate account identity. Removal/re-add creates fresh identities. No configured credential writer/runtime exists at this checkpoint, so there is no grant to migrate or dispose. **Before activation is implemented, replace/remove must gain current-operation invalidation and explicit disposal of returned retired account records.** Their current comments record this integration gate; these are not finished lifecycle commands for an authenticated connection.

## Source audit and research

Graph-first exploration used module summaries and impact analysis, followed by `detect_changes` and focused review context. The graph reported command/state wiring gaps and omitted relationships/new-module coverage; zero impacted flows was not treated as evidence of safety. Manual review traced composition-root state, command registration, main-window/developer guards, blocking-task ownership, strict parsing, revision handling, core publication and every maintained scenario/oracle.

The host mutation/result lock was added before acceptance to avoid a replace-then-read race returning another command's view. Actual concurrent-edit acceptance confirms one winner and one exact conflict. No new warning was introduced by this module; normal Clippy completed with the existing 68 backend warnings. Corrupt registry bytes are preserved. Cached metadata after an external edit is a snapshot: mutation refuses overwrite, and restart reloads the file. There is no automatic merge or active connection to continue on stale metadata.

Current primary references: [OpenCode's separate server configuration and authentication management](https://opencode.ai/docs/mcp-servers/) and [MCP security guidance](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices). Grain retains the already selected official SDK and host/vault boundaries. We use configuration/management separation as an architectural reference; importing OpenCode's plaintext credential/configuration modes or automatic tool exposure is not part of this unit.

## Verification and evidence

Normal-build MCP logic: **69 Pass**, `logic-NHTGf2`, including canonical revision parsing. Real filesystem registry tests: **13 Pass**. Maintained Node self-tests: **76 Pass**. Harness build includes TypeScript checking, Vite production assets, locked CLI and actual Tauri/WebView2 host. Normal locked backend Clippy, scoped Rust formatting and whitespace checks pass.

Three separately reported `mcp-configured` cases:

- **Ownership:** independent identical imports, inactive/no-account view, catalogue runtime refusal, unchanged bytes, label change, stale/noncanonical/oversized revisions, concurrent edits, URL/auth account rotation, removal/re-add and actual restart.
- **Access:** eleven rejected JSON shapes/destinations through import and replace; all four commands refuse with Developer Mode off and from an actual Agent window. No alternate UI/Tauri implementation is used.
- **Storage:** external disk edits refuse all mutation; malformed bytes survive load/import refusal; actual stopped-host repair/restart; exact revisions `9007199254740993` and `18446744073709551615`; exhaustion preserves disk; 32 saved entries, 33rd refusal and explicit removal of all records.

| Scope | Result | Scenario time |
| --- | --- | --- |
| `run-oyuB6c` | New three-case suite 3/3 Pass | 1,882 / 550 / 4,400 ms |
| `run-vep8Zp` | Catalogue transport contract Pass | 4,410 ms |
| `run-vmEsjm` | Catalogue OAuth/approved account/restart Pass | 5,616 ms |
| `run-AvrgXT` | Fresh three-case suite 3/3 Pass | 1,794 / 527 / 4,350 ms |
| `run-oayhXB` | Native cold/warm and denial 2/2 Pass | 1,158 / 944 ms |
| `run-0JPjcv` | Final runner three-case suite 3/3 Pass | 1,813 / 543 / 4,456 ms |

Negative oracle `missing-configured-cleanup` skips only owned fixture deletion. `run-IsxHKY` failed as expected with 32 remaining entries. Its attribution was improved without changing production or positive-path requirements: final `run-CI4lSs` exits 1 at **`Configured MCP bounded-import-count: Configured cleanup left a saved connection`**, with stage Fail. Both failure scopes are retained; neither receives acceptance credit. The final clean suite follows that attribution change.

Independent read-only inspector `.runs/inspect-e2c-management.ps1` verifies all eight scopes / **42 host PID references**, zero owned processes/listeners/scratch directories. The OAuth scope additionally has zero grants/client secrets/registration records. A broader inventory attempt on metadata-only markers was refused by the existing account-fixture guard; the inspector respects that guard instead of altering markers. Metadata-only scopes never acquire credentials. Exact inspection evidence: `.runs/e2c-management-cleanup.json`.

Stable application identity:

- Source: `3d84ed8773953c703156454f7a9a52a7f1a348f05fdf193652ed296ffea7f46f`
- Host: `441e33676627be934fb83f1bc8a8aca2b177fcd5608db851e7a61db81f8ca613`
- CLI: `7e8ec04b71e8fef19d6eff5d27f59d357046645baae241984758ba1e29d3cf50`
- Runner used by final named acceptance scopes (before a help-text correction): `768798fc13b51430d4de9694ba716c89fce93b0e0e66a29826ee9b5090c8d33d`. The help now lists `mcp-configured` and corrects the unchanged foundation count to 23; all 76 self-tests pass afterward. It does not alter scenario behavior or application identity.
- Reports identify precommit base `a40e12f32baad10ab986c02c304b0f5f97653dad` plus these source/binary/runner fingerprints, including the preserved unrelated bindings changes.

The shared SDK HTTP/auth/transport/deadline paths were not edited. Targeted catalogue/auth/native regressions accompany the management cases; unchanged 45/90-second deadline suites were not rerun as if this were a transport change. They remain required when shared transport changes and at release certification.

## Next and retained limits

Next E2 unit resolves these host records into the **same** MCP runtime, binds dispatch and consent to current revision/account ownership, implements enablement/authentication/status/cancellation and retired-account cleanup, then certifies configured real-server calls. Do not create a second executor or permit activation before that integration. Store descriptor acquisition follows through the same boundary. E3 author tooling, E4 publishing, E5 real management/store UI, E6 remaining runtime policy, E7 measured mixed sources and E8 release gates remain.

Baseline stays **52 Pass / 1 Deferred (12)**; full public OAuth identity check **56 Pending**. Forward 62 only accepts the scoped host management behavior above. Inventory **97 IDs / 82 self-contained / 76 Node self-tests**. No new manual batch, public freeze, production release or physical obsolete-code removal.

Maintain the command module, real-app scenarios and revision tests as regression coverage. Generated `.runs/` profiles/logs/reports/read-only inspectors are disposable evidence; the runner removes only its owned scratch. No temporary feature framework or replacement visual path was introduced. When runtime integration lands, update these commands and their tests rather than retaining a competing metadata-only management implementation.
