# MCP catalog and transport acceptance audit

**Date:** 2 October 2026. **Branch:** `extensions/tool-only-retirement`. **Baseline:** `e9fa1a44`, plus the scoped working changes and pre-existing unrelated `src/app/bindings.ts` edit recorded by each report. This is the first B2 acceptance unit on the existing implementation. The backend lockfile still resolves `rmcp` **3.1.4**; the manifest's compatible requirement is `3.0.0`. No dependency was upgraded.

**Verdict:** check **23** is accepted as reviewed real-application automation against a controlled HTTPS MCP peer. Ledger: **39 Pass / 14 Pending** (15 human, 24 reviewed automated). Checks 8/17 have supporting controlled evidence, but their live-provider prerequisites remain open. Checks 20/31, official conformance, the remainder of B2 and all seven release gates remain open. This audit does not certify MCP authentication or universal server compatibility.

## What this unit establishes

The maintained `mcp` suite starts a disposable external HTTPS peer and the real isolated Grain app. Its scripted model uses production `search_tools`, selected `load_extension`, the actual Agent confirmation control and Grain's MCP executor. The peer independently checks received arguments; the model independently checks the returned result and the host's untrusted-data label. No direct tool execution or approval bypass is added.

`mcp.transport-contract` covers four combinations: modern `server/discover` and legacy initialize/initialized/session cleanup, each with JSON and SSE results. A two-page mixed catalog exposes two supported tools and excludes two unsupported schemas. Test, Agent search, selected loading and execution-time revalidation each traverse both pages. Modern requests retain negotiated metadata; legacy operations create and delete their own sessions. Only the selected action becomes callable. Nested object/array/boolean/integer/null and escaped text values arrive unchanged; each approval produces exactly one actual `tools/call`.

The same case restarts the actual process, verifies the fixed peer remains enabled and reads again. Disabling the provider while approval is pending prevents dispatch. Re-enabling and requesting a new call succeeds. Enablement itself creates no network request.

`mcp.mixed-catalog` verifies the developer Test result, exclusion warnings and a supported approved read. A model explicitly attempts to load an excluded tool; no schema or approval is exposed and no provider call occurs. An entirely unsupported catalog returns zero supported tools. A repeated paging cursor refuses incomplete discovery. Changing a supported tool description after confirmation invalidates the old digest before dispatch; a new approval succeeds.

## Isolation and admission review

- The fixed `grain-harness` provider, `NoAuthFixture` registration variant, command and TLS helper exist only under `agent-harness`. That feature already refuses release compilation. Normal builds retain the six authenticated hosted providers.
- The launch marker requires a bounded UUID and distinct model/store/auth/MCP/event ports. It supplies only a port; no arbitrary server URL, filesystem path, provider ID or tool command is accepted by the new control. The command also checks the main window, separate app identifier and owned profile. The marker's absence refuses MCP controls and hides the fixed provider.
- The endpoint is always `https://127.0.0.1:<owned-port>/mcp`. Redirects remain disabled, proxies are disabled only for this client, and certificate/hostname verification remains enabled. Its root-only trust uses the owned disposable CA, with a separately signed server leaf. The CA private key is never written, the leaf key stays in the disposable root and neither OS trust store is modified. Certificate reads are bounded and canonicalized within that root.
- The peer is explicitly unauthenticated: status says `fixture_no_auth`, `connected` remains false, and normal account connect/disconnect/client-credential operations refuse it. No fake grant or credential is seeded. The fixture asserts it receives no Authorization header. These results make no claim about OAuth or account preservation.
- Production operation tickets/serialization, enablement checks, schema filtering, exact tool-set digest, offline argument validator, confirmation, one-shot dispatch, HTTP/SSE byte limits, cancellation and close paths remain in use. The test branch replaces only the external authenticated peer prerequisite with this fixed unauthenticated peer. The digest includes its resolved endpoint. Ordinary provider serialization is unchanged.
- Test bootstrap now preserves only this run's explicitly enabled MCP fixture across restart; ordinary providers remain excluded. It never auto-enables the fixture. Native permissions, auth declarations and account ownership are unchanged.
- Fixture requests, journal, input bodies and accepted mode values are bounded. Shutdown closes owned sockets/listeners, checks zero protocol sessions, releases host/CDP/events resources and deletes the owned TLS/profile files. Cleanup failure fails the run.

Graph-first exploration returned no matching MCP/harness search nodes, so scoped source reads were used. Impact analysis identified 15 additional files and classified the surface as high risk; change/review context also identified sparse graph test links. Zero reported affected flows was not treated as proof of zero impact. Direct review followed the production capability loader/executor, bounded/cancellable wrappers, provider controls, marker, TLS construction, restart bootstrap, scripted oracle and runner cleanup. No blocking finding remains in this accepted unit; untested whole-block work is listed below.

## Failures retained and repaired

| Run | Finding | Disposition |
|---|---|---|
| `run-SvA6vw` | Disposable self-signed certificate rejected by Windows/platform verifier; zero MCP requests | Use a separate CA/leaf and reqwest's verified root-only test client. Production trust unchanged. |
| `run-0xfuJq` | Fixture expected `Grain`; actual client identifier is `grain` | Correct the fixture to the inspected production `client_info` contract. |
| `run-gB1JcT`, `run-JgcNi5` | Oracle omitted the production untrusted-result prefix | Require the existing label and compare the real JSON result; do not remove the label. |
| `run-7lmxRW` | All four transport combinations passed, but harness bootstrap cleared MCP enablement on restart | Preserve only the opted-in peer setting across owned restarts. No product persistence failure is claimed. |
| `run-1SycBP` | First complete clean suite before the final fault option was added | Both cases Pass, cleanup Pass. Retained separately from final runner identity. |
| `run-4TcGGF`, `run-9PNKpG` | Both cases Pass before strengthening the search-metadata oracle | Retained earlier runner identity; final runs below also require exact supported metadata IDs. |

None of these failures was converted into Pass by retries inside a scenario. Each change was followed by a new owned run. The final numeric-type fault rejects the actual returned string after exactly one call; it cannot be mistaken for a successful typed result.

## Final evidence

Reports are local ignored artifacts under `tests/agent-harness/.runs/<run>/evidence/`. JSON retains wire-method/cursor/metadata/typed-result assertions and per-stage observations; Markdown provides the run summary. Commit and dirty-source identity are explicit. Reproduce from this maintained runner rather than reusing a report from another binary.

| Evidence | Result |
|---|---|
| `run-gysFYv` | Both MCP cases Pass; 6,000 ms combined scenario time; 8 approved wire calls total; zero sessions; cleanup Pass |
| `run-C2iw9m` | Fresh-profile repeat: both Pass; 5,948 ms combined; 8 wire calls; zero sessions; cleanup Pass |
| `run-g1HrbH` | Deliberately stringified nested result: expected Fail / exit 1, one wire call, no replay, cleanup Pass |
| `run-hNrIL9` | Deliberately made excluded schema supported: expected Fail / exit 1 before any tool call, cleanup Pass |
| `logic-Q0ICci` | 46 normal-build production MCP tests Pass; none ignored; cleanup Pass |
| `run-t754O6` | All eight native-auth cases Pass before whitespace-only Rust formatting; cleanup Pass; fixed MCP controls refuse without the MCP marker |
| `run-R3Nevl` | Both ordinary smoke cases Pass; cleanup Pass; fixed MCP controls refuse without the MCP marker |
| Harness isolation tests | Four Pass, including marker MCP-port collision/refusal cases |
| Runner self-tests | 21 Pass, including returned-value corruption and excluded-schema admission checks |
| Build/static checks | Frontend types and real-app production assets built; normal Rust check and harness Clippy passed with retained warnings; scoped whitespace check passed |

Native regression `run-t754O6` used the same final runner and pre-format host/source identities `af54d8939d051d53973d602ce376a815e6d593ff2858a494b0348d5ba5a32f55` / `67246894d8b247636c5ba3d94edaafd2f45dd1502c273fe0c775aa82dbb4dc93`. Only a line-wrap formatting fix followed; both MCP cases, both fault checks and ordinary smoke were repeated after rebuilding. The earlier strengthened-oracle runs `run-tVKPdQ` / `run-8OtQzv` and both faults also have their expected verdicts and cleanup Pass, with that pre-format binary.

Final host SHA-256: `4d15908f876395782f88ff6cb3f61e6c0c44c59701d60757bc1f9b6231f8e14b`. Source fingerprint: `aa6e9d744e811405e6d3ee1275a3fd4ea0c71d672cde48f8bd837f718562fe53`. Final runner fingerprint: `71898f6d06913a0aeba7de9fbcc3a17fc08a9cdc6663f7409ea5a826fbed9b6e`. The native-account retest still requires independent final deletion of four discarded developer credentials, with zero remaining; that existing production reconciliation gap remains open. Historical unrelated generated bindings have 211 added / 206 removed lines and remain outside the commit.

## Reproduction

Run from `C:\Projects\Grain\grain`; Node dependencies and Python `cryptography` are required. Build before runtime; run suites serially. The runner launches and shuts down its own real app and never attaches to the user's ordinary instance.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.transport-contract --fault wrong-mcp-type
node tests/agent-harness/run.mjs --scenario mcp.mixed-catalog --fault supported-mcp-excluded
node tests/agent-harness/production-tests.mjs --group mcp
```

The two fault commands must exit 1 with the named failed assertion and cleanup Pass. A different failure or exit zero is not the expected proof.

## Sources and reuse decisions

The actual peer shapes and negotiation behavior were checked against the locked [official Rust SDK 3.1.4 model](https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/rmcp-v3.1.4/crates/rmcp/src/model.rs) and local downloaded source, rather than inferred from a newer SDK. This fixture implements only the small controlled peer contract required for acceptance; Grain continues using the SDK.

[Goose's extension manager](https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/agents/extension_manager/mod.rs) separates application integration from its MCP client and owns tool identities. That supports retaining Grain's admission/ownership layer around the official SDK; it does not justify importing Goose's wider resource/UI/context features.

The [reqwest 0.13.2 client implementation](https://raw.githubusercontent.com/seanmonstar/reqwest/v0.13.2/src/async_impl/client.rs) distinguishes merged platform roots from explicitly provided roots. The fixture uses the latter with certificate verification enabled. Production clients retain their original defaults.

The [official conformance runner](https://github.com/modelcontextprotocol/conformance/blob/main/README.md) supports scenario/context-driven client entry points and version/requirements selection. Its skip and expected-failure behavior means exit zero alone is insufficient. No conformance command has run in this unit; pinned requirements and per-check verdicts remain mandatory work, rather than renaming this controlled suite as conformance.

## Remaining B2 work and handoff

Next extend this peer for bounded JSON/SSE/chunk/comment/catalog floods, slow/held calls, late replies and no-replay proof through the real app. Then add the guarded production-wrapper conformance entry and pin requirements/tooling. Audit and repeat that unit before MCP authentication B3.

Keep checks 8/17/20/31 Pending until their complete mapped requirements are verified, including safe live-provider or account-persistence portions where required. One eligible live provider and harmless read remain necessary; the historical Linear negotiation failure is unresolved. No user sign-in batch is assigned by this unit. B3's seven checks and B4's three workflow checks remain open. No feature implementation, SDK upgrade, native OAuth refactor or physical privilege-removal sweep starts here.
