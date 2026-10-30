# Provisional native author API: contract and E1a audit

Date: 3 October 2026. Branch: `extensions/tool-only-retirement`.

## Scope and execution

This completes **E1a, public tool handler/result/context parity and generated-type isolation**, not all E1 or a public API freeze. A native extension here means an embedded JavaScript tool provider with the existing `scripted` manifest tier. It does not mean the retired executable-companion tier. MCP descriptors and host-owned connection/account/source identity remain the next E1 slice; custom connections, publishing, author configuration and final versioning still need their planned implementation and acceptance.

Execution: graph/source inspection and multiple primary references → SDK declarations/CLI generation → generated-project compiler and worker checks → actual Grain acceptance → focused audit/repair → fresh acceptance and independent cleanup. Graph search missed the manifest area, and impact/context output included unrelated nodes and contradictory risk summaries. Direct source inspection was necessary; graph-reported zero affected flows is not an acceptance finding.

## References and decisions

- [OpenCode public tool contract, pinned source](https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/plugin/src/tool.ts) separates validated arguments, invocation context and result types. Grain follows that separation using existing behavior. OpenCode's directories, agent/session IDs, permission APIs and attachments are not authority to add them to Grain.
- [Goose extension configuration, pinned source](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/agents/extension.rs) distinguishes transport-backed and built-in/platform implementations. Grain's common extension concept likewise does not require a native tool author to implement MCP. Goose's privileged platform extension surface is outside Grain's intended public contract.
- [MCP draft tools specification](https://modelcontextprotocol.io/specification/draft/server/tools), retrieved 3 October, separates schemas and results and treats annotations as untrusted. It also warns that server names are not unique across an aggregate. This informs the upcoming host-owned identity slice; it does not introduce draft-only caching, header or multi-round-trip features into Grain's locked SDK profile.

The SDK remains the source of copied TypeScript API declarations. Internal Rust daemon/pill wire types still exist for Grain's own consumers, but `grain-ext init` no longer reflects them into an author's `grain.d.ts`. No new crate, runtime engine, registry publication, API key or idle service is introduced. Removing obsolete internal capabilities physically remains on hold.

## Provisional author contract

| Surface | Public contract | Host ownership / limits |
|---|---|---|
| `grain.actions` | Map of declared action ID to `GrainToolHandler` | Only the exact selected, approved tool receives validated arguments. No ambient transcript, screen, selection or other-extension catalog. |
| `GrainToolArguments` | Object of JSON values | Manifest validation remains authoritative; a TypeScript declaration cannot grant access. |
| `GrainToolContext` | Read-only `idempotencyKey: string \| null` | Opaque prepared-write key; null when absent. Forward only if the service supports it. No automatic replay or guaranteed deduplication. No abort/context-capture API is invented. |
| `GrainToolResult` | Exactly one `{ ok: data/string/null }` or `{ error: { class?, message? } }` | Grain owns provenance, write receipts, rendering and certainty. Worker error messages are not exposed verbatim. |
| Plain return compatibility | Display object `{ title?, body?, details? }`, string or null | Existing runtime compatibility remains. Prefer the explicit envelope for custom scalar fields. |
| `GrainToolDisplay` / `GrainToolData` | Display text and label/value details; optional JSON fields inside `ok` | Display is bounded: title 160 bytes, body 4 KiB, up to 16 detail rows with 80-byte labels/600-byte values. Nested native JSON is not currently retained as structured model output. |
| Error classes | `auth`, `network`, `invalid_argument`, `not_found`, `rate_limited`, `cancelled`, `internal` | Coarse host-classified advice; partial effects remain possible. Auth failure does not silently start sign-in. |
| Own storage / brokered network / auth metadata / logging | Existing `grain.storage`, `grain.net`, `grain.auth`, `grain.log`, `extId`, granted `caps` | Same namespace/grant/vault enforcement as before. Account tokens are not public API. Network/auth configuration remains provisional, with runtime validation required. |
| Retired or unsupported | No daemon subscription, prompt editing, OS/Space access, screen/OCR/selection, executable companion, or author-minted follow-up interaction | Absent from the runtime/public types or rejected by existing manifest/host validation. Legacy parsers/tombstones are retained. |

Recommended result:

```typescript
grain.actions({
  hello: async (args, context) => ({
    ok: { title: "My service", body: "Completed", details: [{ label: "Item", value: "123" }] },
  }),
});
```

Return `{ error: { class: "network" } }` for a reported service failure; Grain supplies safe user text. Return `{ ok: null }` for an empty result. Boolean/array success payloads, multiple envelope branches and extra outer fields are not supported. `needsInteraction` is intentionally not advertised because the current host cannot resume extension-authored follow-up. A rejected result after dispatch is not proof that no action occurred.

Migration: generate a separate reference project with the current `grain-ext init`; copy its public declarations into an existing project without overwriting authored files. Handlers may keep a one-argument signature or plain display return. Use the second argument for the existing write key. Move custom scalar result data under `ok`. Replace internal event imports with explicit declared tool inputs. New scaffolds use `strict` plus `exactOptionalPropertyTypes`, provide `npm run check`, and type-check before the normal bundle build. Existing `grain-ext dev` does not yet add a watch-time compiler gate; that belongs to E3. The host wire version remains 1.0; this provisional surface is not the E8 public compatibility freeze.

## Audit findings and repairs

1. Generated `grain.d.ts` exported `DaemonEvent` and its internal dependent shapes. Replaced full event reflection with the SDK's author declaration and filtered tool capability union. Core/pill consumers are unchanged.
2. The scaffold returned `{ ok: ... }`, while `actions` accepted only plain title/body results and omitted the already-delivered context argument. Added explicit public handler/context/display/result/error types while preserving supported plain-return compatibility.
3. The worker recognized envelopes using truthiness. `{ ok: null }`, `{ ok: "" }`, `{ ok: false }` and null ambiguous branches could be wrapped as successful object data instead of reaching host classification. It now recognizes own envelope keys by presence. Malformed/unsupported replies remain subject to the host's existing strict classification; no authority or retry behavior changes.
4. Audit-added compiler checks reproduced an accepted `{ ok: undefined }` in candidate `author-58hpA5` (unused expected-error diagnostic). Strict optional properties closes that hole in generated projects; separating plain display shape from enveloped custom data also makes outer-field intent explicit. TypeScript remains structural and can be bypassed; it is not a security boundary.
5. Initial real-app candidate `run-lKexRA` passed the new contract and typed cases, then migration failed because the generic dialog locator matched both extension consent and an actual update dialog. Scoped consent to the exact extension's accessible name and consent class. No product updater change or forced application action. The full foundation rerun passed.

## Verification and evidence

Commands from the repository root, after the standard build:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/author-contract.mjs
node tests/agent-harness/run.mjs --suite native-foundation
node tests/agent-harness/run.mjs --scenario native.reply-failures
node tests/agent-harness/run.mjs --suite smoke
# Expected Fail/exit 1, wrong certainty; cleanup Pass:
node tests/agent-harness/run.mjs --scenario native.reply-failures --fault successful-error
node tests/agent-harness/production-tests.mjs --group execution
cargo test --locked -p grain-sdk -p grain-ext-cli
node node_modules/vitest/vitest.mjs run src/app/extension-runtime.test.ts
node --test tests/agent-harness/*.test.mjs
```

Final source fingerprint: `4a30ecb8f44423385491f2e9de1efddc76561c7d497a0a12bc38500938a9a184`.
Real host SHA-256: `3aed60df20eb0e6eda54ea08f7c37713ff7aadbfc7620f8bdf05fd5e52b8984e`.
CLI SHA-256: `358e404417cfa89b46e85bf0ec7038ae79b4ab03b5c5e647086d203a86e0d35e`.
Final real-app runner fingerprint: `bd883e464b19d56d4c98a2a888b07432e1df0befa0050dbc0ac19a209db65db4`.
Builds recorded parent `45a4db68` plus this scoped diff and preserved unrelated generated `src/app/bindings.ts` churn; that churn is not committed by this unit. Platform Windows, Node 24.11.1, installed TypeScript 5.6.3 and esbuild 0.25.11. Fresh npm installation against the scaffold's dependency ranges was not performed.

| Evidence under ignored `.runs/` | Verdict and scope |
|---|---|
| `author-D7yVZ7` (fresh repeat of `author-Yt0uo3`) | Four stages Pass: unmodified generated project compilation; supported types plus 15 individually annotated negative cases; unguarded raw-token-access compiler negative control (actual exit 2/TS2339); generated hello bundle registration/result. Cleanup Pass. These are non-visual compiler/bundle checks, not a Tauri substitute. |
| `run-rJl0g8` | Three real-app foundation cases Pass: seven result/context modes, nine typed-contract observations, ten migration/restart observations; seven actual host lifetimes. |
| `run-48suqK` | Fresh new result/context case Pass, seven observations, one actual host. |
| `run-pw14kx` | Lost reply, declared/thrown error and malformed-envelope classifications plus recovery/no-replay Pass. |
| `run-VRhK7N` | Both ordinary Agent smoke cases Pass. |
| `run-CP95IO` | Deliberate error-to-success corruption correctly Fail/exit 1 at `tool_error reported the wrong certainty`; cleanup Pass. Not positive acceptance. |
| `logic-9ceeWL` | Four normal production execution logic tests Pass. |
| `e1-rust-tests.log`, `e1-selftests.log` | 73 SDK + 9 CLI Rust tests and 73 Node harness self-tests Pass. Six production worker Vitest tests Pass separately. |
| `e1-author-build.log` | Stamped CLI and actual debug host build, frontend type-check and embedded Vite build Pass. |
| `e1-clippy.log` | Normal all-target SDK/CLI Clippy completes with four pre-existing diagnostics in unchanged manifest tests/DevClient code. Strict `-D warnings` was attempted and failed on existing SDK test warnings; no clean strict-lint claim. Scoped formatting, ESLint and whitespace checks Pass. |
| `e1-author-cleanup.json` | Independent read-only inspection of six completed scopes, including the failed candidate and deliberate fault: 12 recorded host PID references, zero owned processes/listeners/scratch, no PID reuse. These native suites use no account fixture. |

Candidate evidence is retained: `author-Kad5yH` was the initial 13-case compiler pass before the audit; `author-58hpA5` records the failed strengthened contract; `run-lKexRA` is not a passed suite. No numbered baseline result is rewritten from these candidates.

## Retention, limits and next step

Keep `author-contract.mjs`, the new actual-host case and their source tests as maintained checks. The generated projects/type probes/bundles/reports under `.runs/author-*`, runtime evidence, logs and read-only inspection helper are disposable artifacts, deliberately untracked. They contain no real credentials; no production runtime imports the compiler checker. Unused CLI reflection dependencies and broader author-tooling cleanup are recorded for E3, not silently expanded into this result-contract slice. The three older policy-blocked interrupted scratch roots remain untouched.

**Forward criterion 57 Pass** for this bounded E1a contract. Original baseline **52 Pass / 1 Deferred (12)** remains; forward **54/55 Pass**, **56 full activation Pending**. Inventory **92 IDs / 77 self-contained / 73 Node self-tests**; `native-foundation` now has three cases. This acceptance does not certify custom MCP configuration, schema/manifest public freeze, full generated-project doctor/pack/install/update, live provider behavior or measured model selection.

Next: E1b provisional MCP descriptor, connection/account/source identity and configuration/version/error ownership, followed by focused validation and audit before E2 connects those contracts to runtime/storage/import. E1 is partially complete; all eight E1–E8 delivery lanes still have work. Public OAuth hosting activation and the deferred live Linear expiry test remain separate prerequisites. No new manual batch or physical obsolete-code removal is assigned.
