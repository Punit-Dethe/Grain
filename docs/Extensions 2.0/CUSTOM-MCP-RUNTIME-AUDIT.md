# E2d: directly configured anonymous MCP runtime — 4 October 2026

Forward **63 Pass**, after focused source audit, a stable 31-case actual-app checkpoint, exact negative control, fresh repeat and independent cleanup. This unit concerns anonymous remote servers. Configured OAuth and store acquisition remain subsequent E2 work.

## Implementation

A directly configured connection now resolves into the existing MCP discovery/execution path. `Provider<'a>` borrows either the fixed catalogue descriptor or a host-owned configured record. Existing catalogue IDs, endpoints, vault namespaces and authentication mechanisms remain unchanged. There is no second executor, new framework or new dependency.

Import remains inactive. `mcp_connection_set_enabled(id, expectedRevision, enabled)` requires the real main window, Developer Mode and the current canonical string revision. Only anonymous definitions can be enabled in this unit. OAuth definitions refuse activation with explicit account-integration guidance. JSON cannot provide enablement, credentials, account keys or trust. Enablement is saved under the exact host-generated connection/account namespace; destination or authentication changes rotate that namespace, so old enablement cannot activate a replacement server. Labels retain enablement while advancing revision and invalidating earlier work.

The Agent directory includes enabled anonymous records as `mcp.configured-{host UUID}`. Normal lazy tool selection and explicit approval use the same pipeline as catalogue MCPs. The shared bounded HTTP client, public HTTPS/DNS destination policy, official SDK, stateless transport and restricted legacy fallback remain authoritative. The anonymous branch constructs no OAuth manager and accesses no credential vault. A configuration import or enable operation starts no network connection.

Ownership is captured under the registry mutation lock: a record lease and its cancellation ticket refer to the same revision. Approval digests additionally bind to the host account and revision, preventing identical servers, replacement destinations or later revisions from accepting an earlier approval. Immediately before dispatch, the existing generation guard rechecks developer consent, exact enablement and current disk ownership. The operation queue and original deadlines remain shared. Edit, disable, removal and developer-mode shutdown invalidate current operations before publication. An unchanged save preserves pending and active work; removing a different instance does not cancel the approved instance.

Cancellation before dispatch refuses without a tool call. Once dispatch may have occurred, cancellation reports an unknown outcome and never automatically repeats the action. Existing SDK/service closure disposes connections and held replies. Removal also prunes enablement and drops the record's control. At most 32 small controls remain for saved records; they retain generation history but no SDK client, process, listener, watcher or background task. `RuntimeProvider` boxes the configured owner to keep catalogue operations from carrying its larger stack variant.

## Source audit and references

Graph-first context/module exploration, change detection and focused review context were followed by manual tracing because the graph omitted shared flows and reported incomplete test relationships. Review covered descriptor borrowing, fixed catalogue compatibility, composition-root commands/state, lock order, current-generation capture, digest binding, settings publication, registry conflicts, anonymous destination policy, cancellation classification and harness assertions. A generation-lock callback never re-enters the registry's host mutation lock. No upstream Handy feature code was changed.

The first actual-app run, `run-t1VyuG`, failed at **restart-and-host-enable-isolation**: the isolated harness startup filter intentionally retained only fixed provider IDs and removed the new configured enablement. Cleanup passed; later scenarios were Not run. The filter now preserves only exact anonymous records for the owned reserved peer after strict registry loading; it does not retain arbitrary prefixed settings or unrelated endpoints. This is a harness startup fix, not a relaxed production filter. The combined checkpoint `run-DNwnV9` then exposed strict startup loading aborting during the intentional corrupt-file test; the report attributes a prerequisites failure with later cases Not run, cleanup Pass. Damaged metadata now retains no custom enablement and does not abort unrelated startup, leaving the preserved file visible to guarded command refusal/recovery. Both failures remain as evidence. The first unboxed variant also added a Clippy large-enum warning; boxing removes it. Final normal Clippy returns to the existing 68 warnings.

Primary references checked for this integration: [OpenCode MCP configuration and authentication management](https://opencode.ai/docs/mcp-servers/) and [MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization). The already accepted [destination audit](MCP-DESTINATION-BOUNDARY-AUDIT.md) documents shared networking/security references. The architectural inference is that configured server identity and host-owned authorization must remain separate from tool discovery. These references do not imply Grain should copy plaintext credential settings, eager tool exposure or add another runtime.

## Maintained verification

`mcp-configured-runtime` contains three separately reported actual Tauri/WebView2 cases. Its exact reserved descriptor maps only in `agent-harness` builds to the existing marker-validated owned HTTPS peer, with its scoped CA and no proxy. Normal custom URLs use production public DNS/TLS. The model selects configured IDs from the actual Agent directory; no alternate Tauri implementation, browser-only replica or bypass of approval is introduced.

| Case | Coverage |
| --- | --- |
| `mcp.configured-execution` | Inactive import; exact enable; real modern/legacy JSON/SSE approved reads; restart persistence; disable/re-enable; label edit; stale command refusal; destination/auth rotation; OAuth activation refusal; removal |
| `mcp.configured-approval-ownership` | Eight named pending-approval transitions; identical save and peer removal preserve the owner; label/disable/remove/destination/auth/developer-off refuse with zero dispatch |
| `mcp.configured-active-cancellation` | Four actual held calls over modern/legacy JSON/SSE; unchanged saves preserve active work; mutations cancel, report unknown, dispose replies/sessions and discard late results; fresh approved reads recover |

All five management commands are also checked with developer mode off and from an actual Agent window. Two normal-build owner tests verify identical-server separation, reused control invalidation, stale generations/stamps, copied endpoint ownership and rotated enablement keys. Existing 13 real-filesystem registry tests remain relevant.

For efficiency, `mcp-connection-checkpoint` selects the three metadata cases, three configured-runtime cases, all 23 catalogue transport/OAuth cases and both native smoke cases in one stamped host: **31 distinct cases**. Original suite identities, stage assertions and genuine 45/90-second deadlines are preserved. Failure stops the batch and leaves remaining cases Not run. It does not combine verdicts or replace broader release coverage. Run backend/build checks before application tests because Windows target DLLs are shared.

```powershell
node tests/agent-harness/production-tests.mjs --group mcp
cargo test --locked -p grain-core mcp_connections::
node --test tests/agent-harness/runner.test.mjs tests/agent-harness/client-metadata.test.mjs
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp-connection-checkpoint
node tests/agent-harness/run.mjs --scenario mcp.configured-execution --fault wrong-configured-owner
node tests/agent-harness/run.mjs --suite mcp-configured-runtime
```

The negative control alters only the model's owner receipt. A real owned approved call must occur, then the exact owner assertion must fail with exit 1 and cleanup Pass. It cannot bypass host permission or modify production routing.

## Evidence and acceptance

Initial post-repair suite `run-MQ2vA0`: **3/3 Pass**, 3,874 / 3,587 / 3,948 ms; cleanup Pass. After the corrupt-startup repair, `run-vz8ECw` passed all 31 scenarios and cleanup, but its final **harness.identity** check failed because the runner's help text was edited during execution. It is retained as a failed run and receives no full-checkpoint acceptance. The identity guard was not weakened. Stable final `run-8nrntG` passes **31/31**, exit 0, cleanup Pass, with the complete unchanged-deadline/auth/transport and native smoke coverage above. Its four HTTP variants take **183,678 ms**; two absolute discovery variants take **181,790 ms**. No production clock or requirement was shortened.

Final wrong-owner control `run-KHxsJb` exits 1 at **Configured runtime stateless-json-actual-approved-read: Configured owner receipt changed**, after exactly one owned `fixture_read`; cleanup Pass. Fresh final three-case repeat `run-Px0XKe`: **3/3 Pass**, 3,672 / 3,614 / 3,925 ms; cleanup Pass. Normal MCP logic has **71 Pass** (`logic-B7v1Nz` after the enum-size/harness-startup repair); all **13** core registry cases and **78** final Node self-tests pass. Build includes locked CLI/backend, frontend type checks and real embedded assets. Scoped Rust/Prettier/whitespace checks pass. Final evidence identities distinguish source, host and runner revisions.

Independent read-only `.runs/inspect-e2d-runtime.ps1` checks all seven completed success/failure scopes above: **126 recorded host PID references**, zero owned processes, listeners or scratch directories. Account-enabled markers additionally have zero grants, client secrets and registration records. Anonymous-only markers create no account. Exact inventory: `.runs/e2d-runtime-cleanup.json`. The three older interrupted/policy-blocked roots were not inspected as new accepted scopes or altered.

Stable final identities:

- Source: `6079b77d36a505b949dbd98156af3e110a9a3daff971de1abfca9ff1bb3b16a4`
- Actual host: `701af87f1c45ca16fbb19b5daca0816c92863711e56d15b8639370b61dbbc46a`
- Runner (negative control, fresh three-case repeat and final full checkpoint): `10041c470c0a898b35462eacfdefda70833fe1501d32da4a8d85cd47cf14e770`
- Reports identify precommit base `deb177bb` with these fingerprints and the preserved unrelated `src/app/bindings.ts` diff. Earlier repaired candidate runs retain their own identities; they are not substituted for the final stable checkpoint.

## Next and retention

Next E2 work is configured OAuth: use the same official SDK with exact host-generated vault account ownership, current consent/status and cancellation, registered client/issuer binding, refresh/logout and explicit disposal of retired account records. OAuth stays inactive until that complete path is verified. Returned retired records currently have no configured credential writer to clean up; activating authenticated records requires adding disposal first. Store descriptor acquisition then uses the same boundary.

E3 SDK/CLI packaging, E4 publishing, E5 real connection/store UI, E6 remaining policy/reconciliation, E7 measured mixed-source selection and E8 release certification remain: seven E2–E8 stages still retain work. Baseline **52 Pass / 1 Deferred (12)** is unchanged; forward **54/55/57/58/59/60/61/62/63 Pass**, public OAuth identity **56 Pending** remains dependent on an official hosting identity. Inventory **100 IDs / 85 self-contained / 78 Node self-tests**. No new manual batch, UI approval, public freeze, release or physical obsolete-code deletion is claimed.

Keep the runtime ownership tests, finite feature-only instructions, configured cases, fault oracle and checkpoint selector as maintained regression infrastructure. They use existing fixtures rather than new testing services. Generated profiles, TLS keys, logs and read-only inspectors under ignored `.runs/` are disposable evidence, with owned scratch removed by the runner. Prior three interrupted/policy-blocked roots remain separate and untouched. This unit introduces no temporary production scaffold or obsolete alternative execution path.
