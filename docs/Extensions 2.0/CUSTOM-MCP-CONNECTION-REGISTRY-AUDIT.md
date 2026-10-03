# E2a: direct MCP definitions and host-owned persistence

3 October 2026. Branch `extensions/tool-only-retirement`. E1 is complete provisionally; **E2 is still in progress**. This is its storage/admission foundation, not a functioning custom-MCP management/import/authentication route. No UI redesign, ordinary account operation, new transport or physical obsolete-code removal.

## 1. Implemented boundary

`grain-sdk::mcp::RemoteMcpConnection` defines a direct connection without an extension package:

```json
{
  "name": "My service",
  "url": "https://mcp.example.com/mcp",
  "authentication": { "type": "oauth" }
}
```

This is a provisional input shape for the upcoming host import command, **not a file users can configure in the application today**. `none` is the other explicit authentication mode. Unknown/duplicate fields, raw credentials, headers, environment/commands, caller-defined IDs, trust, enablement and source claims are rejected. Input is limited to 4 KiB. Host admission reuses the descriptor's bounded display/HTTPS endpoint rules; a valid-looking DNS name is not proof that network access is safe.

`grain-core::mcp_connections::ConnectionRegistry` owns a fixed `mcp-connections.json` under the trusted app/profile data directory. It supports load/list/insert/lease/replace/remove. UUID v4 IDs for the connection, account and registry incarnation are created by Grain. An account ID is only a prospective vault namespace; there is no credential or consent attached to it. Configured source is constructed by the host, not accepted from JSON or persisted as a mutable source label. Catalog keys and existing settings are unchanged.

| Boundary | Rule |
|---|---|
| Public direct input | Strict name/URL/authentication only; no package ID, version, script or extension directory required |
| Persistent metadata | Schema 1, at most 32 custom connections, file at most 256 KiB; no token, client secret, enabled bit, tool schema or active session |
| Persistent identity | Canonical host UUID v4 IDs, distinct accounts, unique connections, positive revision; malformed/future/duplicate state refused without rewriting |
| Destination/auth edit | Increments revision and rotates the account namespace; returns the retired record for later explicit host credential cleanup |
| Label edit | Increments revision and invalidates the old lease, preserving the account binding; returns no retired account |
| Identical save | Preserves revision/account and committed bytes; returns no retired account |
| Remove/re-add | Removes the old record; re-add gets new IDs, so old ownership cannot resurrect |
| In-process lease | Private, non-deserializable epoch plus exact record; stale edit/remove fails, reload changes epoch |
| External change | File digest recheck invalidates freshness and refuses overwriting a stale disk snapshot; explicit reload required |
| Save | Candidate state, bounded serialization, short-lived nonblocking OS writer lock, existing synced temporary-file/atomic-replacement helper, then in-memory publication |
| Failed save | Original committed bytes and in-memory ownership survive; staging file is dropped; no delete/copy fallback |

The separate lock file is empty nonsecret metadata. Its OS handle exists only during a mutation, and dropping it releases the lock even after failure. Empty load creates no file/handle/service. There is no watcher, polling loop, background task, idle protocol session or second testing runtime. The shared existing native atomic-write implementation is only made crate-visible; its behavior and bounded Windows publication retries are unchanged. Endpoint/display helpers likewise gain crate visibility without changing their rules.

The `is_current` check is a **freshness check, not an execution permit**. An edit could follow a completed check. E2 runtime integration must bind consent, discovery, dispatch and credential publication to current ownership and handle cancellation/replacement using the existing execution boundary. This library does not certify concurrency of live calls or arbitrary external editors bypassing its advisory writer lock.

## 2. Research and dependency choices

[OpenCode's MCP documentation](https://opencode.ai/docs/mcp-servers/) supports directly named remote configuration and host-managed authentication. [Goose's extension documentation](https://github.com/aaif-goose/goose/blob/main/documentation/docs/getting-started/using-extensions.md) supports custom server configuration outside its directory. These support a package-free acquisition path; copying subprocess launch, arbitrary headers or inline credentials is outside Grain's current profile.

The [official MCP security guidance](https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices) treats SSRF and authorization ownership as separate concerns. This is why endpoint syntax admission does not enable a connection or claim DNS/redirect authorization. The next runtime unit must enforce destination policy across discovery, redirects and OAuth metadata before using credentials.

Reuse pinned `uuid` **1.21.0**, already used by the desktop, for random identities rather than implementing an ID generator. The root lock gains one UUID package; its existing `getrandom` is reused. Desktop lock changes only the core dependency edge and does not upgrade its packages. Root `getrandom` 0.4.3 and desktop 0.4.1 remain their pre-existing lock versions; identity correctness is tested against the actual root build and desktop compilation. This is version alignment with the existing host, not a claim that 1.21.0 is the newest release.

[Rust's `File::try_lock`](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) provides the nonblocking OS lock and closes the file on drop. It requires Rust 1.89 or newer; this workspace uses 1.96. [Tempfile persistence](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist) is reused through the existing atomic writer. No database, filesystem-lock dependency, custom random algorithm or always-alive coordinator is added.

## 3. Source audit and repairs

Graph minimal context/file summaries/semantic search/impact were used first. Semantic lookup lacked the registry target and the new file is not indexed yet; impact/review reported zero affected flows, which is not proof of isolation. Manual review traced strict Serde input, host-only construction, load validation, optimistic edit/remove, cross-writer publication, old helper visibility and both lock files.

Review tightened three behaviors before final acceptance: unchanged saves preserve the record; label-only edits invalidate stale leases without retiring an unchanged account binding; an externally changed file invalidates `is_current`, not only the next save. A concurrent writer regression uses two independently loaded registries and a barrier; exactly one may commit. A real Windows file-sharing lock exercises failure after staging/sync, preserving old bytes/record and dropping the pending file. Neither test substitutes a mocked publication path. Atomic publication is tested, not power-loss durability; crash/orphan recovery and Linux/macOS certification remain release work.

No active MCP SDK/auth/transport/tool-discovery logic or native execution policy changes. No old capability code or external extension repository is removed. The unrelated generated `src/app/bindings.ts` working-tree change remains present in the build and excluded from the commit. Existing SDK/CLI/backend warnings and the earlier IDNA parity/production orphan-reconciliation work remain separate.

## 4. Evidence and acceptance

Forward criterion **60 Pass** after final source audit and the following evidence. Its scope is direct input/persistence ownership only; it cannot certify E2 custom networking, authentication or UI. E2 remains open.

| Evidence | Result |
|---|---|
| Final locked workspace tests | **498 Pass**: 285 core + 4 integration + 9 CLI + 23 author checks + 54 pill + 79 SDK + 22 registry tools + 22 router; zero ignored/failed |
| Fresh post-audit storage logic repeat | All **13** new cases Pass, including unchanged/label/destination distinctions and actual lock failures |
| Unmodified maintained Node self-tests | **74 Pass**; no new scenario or fixture infrastructure |
| Broad application checkpoint before label-only refinement | `run-YTlRRG`: **12/12 Pass**, 59,884 ms scenario time, about 63 seconds including runtime setup; 17 host PID references |
| Final-build MCP account/ownership repeat | `run-2nsoO1`: Pass, thirteen distinct observations |
| Final-build Agent/native smoke | `run-JUeP4Q`: **2/2 Pass** |
| Final-build negative oracle | `run-AAqSva`: expected Fail/exit 1 at **Missing expected rejection**, cleanup Pass; earlier `run-KYJQk4` detects the same fault before refinement |
| Independent cleanup | `e2a-connection-cleanup.json`: five scopes, 23 PID references, zero owned processes/listeners/scratch/native grants/MCP grants/secrets/registration bindings; one PID was reused by an unrelated non-harness process and left untouched |
| Compilation/static | Locked real Tauri/CLI build, TypeScript/Vite production build, affected core/SDK Clippy exit 0 and scoped formatting/whitespace checks; existing warnings retained |

Final source fingerprint: `0219a2d081a54224ff9f97164ffbbffebcd4d2d9861b016d19556e877a56c8c2`. Host SHA-256: `e1b54f22001205d737af262ed88af7eaa072010f41cad48c6db88fb08197df80`. CLI SHA-256: `7e8ec04b71e8fef19d6eff5d27f59d357046645baae241984758ba1e29d3cf50`. Maintained runner fingerprint remains `fa4fd165f24642e364d3eda111adb10214fe5d3041f130bd0b3545bc2757c5c3`. Precommit base `f362fc51`. The broad pre-refinement checkpoint used source `d08a9035207c80527def1df14e6aa86f818bc1e7b143db1a7426f64ad7da98dd` and host `b79491e148a7717a511288539f6339e8e38f410c008d0f9e176e3c99bc57bbb7`; it is not misrepresented as a final-build run.

The new boundary is tested directly through the production core library and actual filesystem, never a substitute application. Supported input is a positive control; unknown/credential fields, unsafe URLs, stale leases, corrupt state, real locks and overflow are refusal controls. The existing deliberate application fault separately checks the retained baseline oracle, not the new custom runtime. After the final label-only repair, repeat the complete affected pure suite and final-build MCP/Agent/negative checks; unrelated real transport deadlines are not rerun because no transport/deadline algorithm changed. The broad suites remain maintained and required when their boundaries change.

Maintained fast command:

```powershell
cargo test --locked -p grain-core mcp_connections::
```

Thirteen Windows logic cases cover strict/secret-free input, unsafe endpoint/display refusal, no-resource empty load, isolated accounts/restart, edits/no-op preservation, label-only binding preservation, remove/re-add, corrupt/future/duplicate/oversized files, nonblocking writer lock/rollback, externally stale snapshots, concurrent writers, count/revision exhaustion and real Windows publication failure. Existing full workspace tests cover shared native atomic writing and MCP descriptor admission. The real-app `extension-contract` checkpoint is an affected baseline regression, **not runtime testing of the new unwired custom path**.

## 5. Remaining E2 work and retention

Next wire direct configured connections into the existing MCP runtime and host-managed setup. The route must use these host IDs rather than catalogue string IDs; keep catalog keys exact, reject unsafe DNS/redirect/metadata destinations, bind account consent and request lifetimes to current revisions, invalidate stale selections, explicitly clean retired grants and expose honest disconnected/setup/recovery status. Test actual import/edit/remove/restart/auth/denial against an owned peer with the maintained real-app harness. Then wire verified store MCP descriptors through the same connection ownership boundary with artifact/version provenance. No package or store install should be mandatory for direct configuration.

Public management design/visual approval stays E5; SDK/CLI descriptor authoring E3; publishing E4; broader runtime policy E6; measured mixed sources E7; public freeze and release certification E8. **Seven E2–E8 delivery stages still retain work.** The completed provisional E1 contract is not reopened simply because E2 is now being integrated.

Keep the SDK input declaration, pure core registry and tests as the implementation foundation for the immediately following runtime unit. Do not create a second registry/transport or copy these helpers into the backend. Ignored logs, completed run evidence and read-only inspection scripts are disposable evidence. No new harness fixture/service was added. Genuine Linear expiry **12 Deferred**, public client metadata activation **56 Pending**, the three earlier policy-blocked interrupted roots and the physical-removal hold remain unchanged. No manual test is assigned for this library unit.
