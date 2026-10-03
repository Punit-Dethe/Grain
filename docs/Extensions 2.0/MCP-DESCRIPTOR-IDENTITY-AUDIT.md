# Provisional MCP descriptor and host identity: E1b audit

Date: 3 October 2026. Branch: `extensions/tool-only-retirement`.

## Scope

E1b adds an author-owned, bounded remote MCP descriptor and a separate host-owned connection/account/source identity. The existing development catalog validates through this contract, preserving its exact endpoints, authentication policy and deployed vault account names. No custom import command, settings UI, store package admission, account migration or additional MCP client is introduced. E1 remains open for the remaining compatibility/configuration contract; the public API freezes at E8.

Execution: graph/source inventory and primary-source comparison → shared types and catalog bridge → negative validation and catalog logic tests → real-application MCP regression → focused audit/repair and fresh repeat → independent cleanup. Graph search did not cover these new types. Change detection reported risk 0.60 and credential-store/endpoint gaps, while review context reported low risk and zero impact. Those contradictory summaries are not certification; source inspection and affected auth/lifecycle tests are authoritative here.

## Research and decisions

- [Goose's pinned extension configuration](https://github.com/aaif-goose/goose/blob/591edd47cf2cfea4957d720c607cf2a4def8673d/crates/goose/src/agents/extension.rs) distinguishes server-backed and native/platform implementations. Grain keeps transport metadata separate from its host account state. Its subprocess/environment/platform surfaces are outside this slice.
- [OpenCode's MCP configuration and account commands](https://opencode.ai/docs/mcp-servers/) provide a useful remote URL and explicit account-management precedent. Grain deliberately does not copy embedded credential/header fields into publisher metadata. A publisher descriptor and user account setup have different owners.
- [Official MCP registry server metadata, pinned](https://github.com/modelcontextprotocol/registry/blob/bf4e88cbe8d1a635c06144ccea1d24cb52fa6186/docs/reference/server-json/generic-server-json.md) separates remote transport metadata from server identity/version. Grain's smaller profile is not an automatic registry/OpenCode/Goose JSON importer.
- [Published MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization) treats the resource URI and credential handling as authorization concerns. Meaningful path suffixes remain distinct; schema/package/API versions are independent of SDK protocol negotiation.
- [The existing URL parser's official API](https://docs.rs/url/2.5.8/url/struct.Url.html) supplies parsing and normalization. `grain-core` reuses backend-pinned `url = 2.5.8` instead of implementing another parser. The separate root lock gains URL/IDNA dependencies; the desktop lock only gains the existing URL dependency edge. No DNS lookup or network request occurs during validation.

The separate locks currently resolve root `idna_adapter 1.2.2`/ICU normalizer `2.3.0` and desktop `1.2.1`/`2.1.1`. This unit preserves the desktop dependency versions and tests catalog parity, not arbitrary IDNA conformance across those tables. E3 authoring validation must check CLI/desktop acceptance parity on packaged custom descriptors; E8 records the supported dependency/profile matrix. Do not silently upgrade the desktop's transport dependencies to make the lock files look alike.

## Author profile

```json
{
  "schema": 1,
  "id": "com.example.calendar",
  "name": "Calendar",
  "description": "Read calendar events.",
  "version": "1.0.0",
  "grainApi": "^1.0",
  "transport": { "type": "streamable-http", "url": "https://mcp.example.com/mcp" },
  "authentication": { "type": "oauth" }
}
```

This is metadata, not an API key or an installed/enabled connection. `none` is also an explicit authentication metadata variant; it cannot authorize an OAuth downgrade or silently clear an account. Only the host chooses an account and grants execution. SDK transport, issuer discovery, registration, PKCE, resource binding, refresh and vault ownership remain in the already integrated adapters.

| Field or boundary | Contract |
|---|---|
| Input | At most 8 KiB before JSON parsing; unknown, duplicate, missing and incorrectly typed fields refused, including nested transport/auth fields. No permissive extension map. |
| Schema/API | Schema exactly 1; provisional API expression exactly `^1.0`. No guessed semver-range evaluator or forward-schema acceptance. |
| ID/version | Existing canonical extension-ID and filename-safe version validators. Version is package/profile metadata; this does not claim a complete semver parser. Catalog `1.0.0` denotes this provisional descriptor profile, not the remote server's software version. |
| Display | Nonempty trimmed text; name 120 bytes, single-paragraph description 2,048 bytes. Controls/hidden result characters refused. Long listing content is later DESCRIPTION work. |
| Transport | Remote HTTPS Streamable HTTP only. URL at most 2,048 bytes before and after normalization; DNS-form host, no IP literal/local-name suffix, userinfo, query, fragment, backslash, raw whitespace, zero/invalid port or repaired empty authority. |
| Canonical resource | Scheme/host/default port and URL-parser path normalization; root slash omitted, meaningful path case/slash retained. IDNA handled by the maintained parser; canonical DNS label validation reuses the SDK helper. |
| Error | Small typed contract errors; rejected URLs, credentials and JSON are not echoed. No browser, credential store or tool invocation is started by parsing. |
| Excluded author fields | Credentials, tokens, client registration, headers, source/trust, account/connection ID, enablement, tools, process command/env, script/settings/prompts/capture/capability grants. |

**Endpoint syntax acceptance is not network admission.** A public-looking DNS name can resolve to private addresses or redirect elsewhere. E2 must apply reviewed DNS/address/redirect/rebinding and consent checks before arbitrary custom endpoints can reach an HTTP client. Current runtime inputs remain the fixed catalog plus separately guarded, owned harness overrides. No new arbitrary-URL route exists.

## Host ownership

`ConnectionIdentity` has private fields and no deserializer; its source is never loaded from author JSON. Acquisition code must construct it after validating its actual source. A `Store` label/hash is recorded provenance, not signature verification.

| Source | Account key and ownership |
|---|---|
| Development catalog | Connection/account must both equal the provider ID. Vault key stays exactly that ID, preserving existing grant, secret and client-registration service/account pairs. No migration or sign-in reset. |
| Configured | `mcp:v1:configured:{connection}:{account}`. Distinct instance/account IDs produce distinct keys, independent of labels or endpoint spelling. No configured runtime/persistence exists yet. |
| Store | `mcp:v1:store:{extensionId}:{connection}:{account}`. Canonical extension ID/version and 64 lowercase SHA-256 digits required; signature/source verification still belongs to installation. |

Connection/account scope IDs are 1–64 bytes of lowercase ASCII letters/digits/hyphens, starting/ending alphanumeric. They cannot inject delimiters. Store artifact/version changes preserve the stable account key but change recorded provenance. E2 must invalidate approval/generation on owner, descriptor, endpoint, authentication or artifact change and revalidate issuer/resource ownership; key stability alone does not authorize credential reuse. Source and credential service namespaces must remain separate through import/install/update/remove. This unit does not certify that future lifecycle implementation.

The catalog bridge was inspected at every vault constructor: ordinary status resolves fixed catalog entries; connect/disconnect/session paths resolve the requested provider first; the isolated Linear observer uses a fixed constant. The internal canonical-ID assertion is not reached by an unknown user provider ID. Existing client-secret/registration functions keep their same provider account key. Provider controls, tool identifiers/digests, discovery policy, approval and tool dispatch are unchanged.

## Audit findings and repair

1. A negative test proved Serde's internally tagged **unit** OAuth variant accepted `authentication.headers` despite the enum annotation. Empty **struct** variants now enforce the same JSON shape while refusing extra fields for both `oauth` and `none`; root/nested secret and ownership forgery matrices pass. The initial run had eight Pass/one Fail and is not acceptance.
2. The URL parser accepts some repaired authority forms and DNS-looking punctuation. Require a present nonempty raw authority and reuse canonical SDK DNS-label validation; local/IP/userinfo aliases and malformed ports are refused.
3. Unicode paths can expand during percent encoding. Enforce the URL limit after normalization as well as before; accepted normalized descriptors round-trip under the same limits. Meaningful `/mcp/` remains different from `/mcp`, including GitHub's actual endpoint.
4. Keep host provenance non-deserializable and catalog account names exact. Tests cover delimiter/boundary refusals, mismatched catalog ownership, cross-source/instance/account separation and stable keys with changed store provenance. No user JSON is treated as a trusted host source.

No unresolved issue blocks this bounded metadata/catalog bridge. The network/import/update and public hosting prerequisites above remain explicit work, not implicit Pass results.

## Verification and retained work

Reproducible commands, real application suites serial:

```powershell
cargo test --locked -p grain-core -p grain-sdk -p grain-ext-cli
cargo clippy --locked -p grain-core -p grain-sdk -p grain-ext-cli --all-targets
node --test tests/agent-harness/*.test.mjs
node tests/agent-harness/production-tests.mjs --group mcp
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --scenario mcp.auth-provider-independence
node tests/agent-harness/run.mjs --suite smoke
# Expected Fail/exit 1 for wrong-account corruption; cleanup must Pass:
node tests/agent-harness/run.mjs --scenario mcp.auth-provider-independence --fault wrong-mcp-peer-account
```

Final source fingerprint: `a67f25f9d13ec3a992e0e3c9d09e505c978b483ded952a98eac134917087a3f5`.
Real-host SHA-256: `8f1b033aeec7a1c827a0a37a926060faabbf7236972e388f4ce9ffc2491d4133`.
CLI SHA-256: `4445717b8a200d3c9246981c259a671d03a7e1c57a86fc104058a809d5c729e0`.
Runner fingerprint: `bd883e464b19d56d4c98a2a888b07432e1df0befa0050dbc0ac19a209db65db4`.

Build/evidence records parent `4a146fe9` plus this scoped diff and the preserved unrelated generated `src/app/bindings.ts` churn. Windows, Node 24.11.1; actual SDK/Tauri/vault paths use controlled issuers and scripted-model fixtures, not a new live account. Parsing/identity tests do not substitute for MCP adapter/account acceptance, and controlled account tests do not certify live expiry.

| Evidence under ignored `.runs/` | Verdict and scope |
|---|---|
| `e1b-workspace-tests.log` | 271 core unit + four integration + 73 SDK + nine CLI tests Pass, including the nine new descriptor/identity matrices. No ignored tests in these groups. |
| `logic-IRewPt` | 59 normal-build MCP backend tests Pass, including fixed catalog metadata/keys and unknown-provider refusal. |
| `e1b-feature-catalog-tests.log` | Same catalog endpoint/auth/key/round-trip invariant Pass with all harness-feature catalog entries included. |
| `e1b-node-tests.log` | All 73 maintained Node harness self-tests Pass. |
| `author-dI8XGX` | Fresh generated native project, supported API/15 negative types, compiler negative control and bundle stages Pass. No listener/account created. |
| `e1b-build.log`, `e1b-backend-check.log` | Stamped CLI/real debug application, frontend type-check and embedded Vite build, and normal backend check Pass. Existing warnings remain. |
| `e1b-clippy.log` | Normal all-target core/SDK/CLI Clippy completes. Four pre-existing SDK manifest-test/CLI diagnostics; no changed-line warning or clean strict-lint claim. Scoped Rust formatting and whitespace checks plus changed Markdown Prettier Pass. Whole-repository whitespace checking still flags the excluded generated bindings churn. |
| `run-WOlpA4` | All 22 actual-app MCP foundation cases Pass, including thirteen auth cases, nine transport/lifecycle cases, four actual 45-second HTTP deadlines and two actual 90-second discovery deadlines. 49 recorded host PID references; cleanup/vault counts zero. |
| `run-hSXCxz` | Fresh provider/account-independence repeat Pass: 13 observations across three recorded host lifetimes, real approved reads/restarts and independent accounts. |
| `run-qkWxoK` | Both ordinary Agent/native smoke cases Pass. |
| `run-40Nf9n` | Deliberate wrong-account corruption correctly Fail/exit 1 at `Selected provider returned another account` (`A !== B`); cleanup Pass. This is negative-control evidence, not positive acceptance. |
| `e1b-descriptor-cleanup.json` | Independent read-only inspection of all four runtime scopes: 54 host PID references, zero owned processes/listeners/scratch and zero scoped grant/secret/registration credentials. One non-harness reused PID was observed and left untouched; a later read found it no longer present. |

**Forward criterion 58 Pass**, limited to strict descriptor/host identity admission and the unchanged catalog/account bridge. Baseline stays **52 Pass / 1 Deferred (12)**; forward **54/55/57/58 Pass**, full **56 Pending for permanent public OAuth hosting/provider acceptance**. Inventory stays **92 IDs / 77 self-contained / 73 Node self-tests**. No new runtime scenario was necessary for this pure contract slice; existing affected actual-app coverage was repeated after the audit repairs.

Maintain the SDK/core types and matrix tests plus the catalog bridge regressions. No new test engine, debug command, listener, cache or background worker was needed. Ignored `.runs/` reports, logs and read-only inspectors are disposable evidence; no such artifact is imported by production. Existing controlled peers remain maintained harness fixtures. Three earlier policy-blocked interrupted roots stay untouched. Unrelated generated `src/app/bindings.ts` changes and the separate `grain-extensions/` checkout are preserved.

Physical obsolete-code removal, public OAuth identity activation, live Linear expiry, E2 custom runtime/source acquisition, E3 authoring/packaging, E4 publishing, E5 management, E6 runtime policy, E7 mixed-source measurements and E8 release certification remain outside this acceptance. No new manual account batch is required for E1b.
