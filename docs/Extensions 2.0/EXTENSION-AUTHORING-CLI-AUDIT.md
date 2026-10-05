# E3a: native and MCP authoring checkpoint

**5 October 2026 — forward 67 accepted within provisional authoring scope.** E3 remains in progress. The [authoring guide](../../crates/grain-ext-cli/README.md) documents the actual commands and limitations. E2's shared runtime, SDK/OAuth adapters and host identities are unchanged.

## Implementation and scope

The existing CLI distinguishes native `manifest.json` projects from remote `mcp.json` projects and refuses roots containing both. Native scaffolds retain the reduced generated tool API, typed handler/result/context contract, restricted host services and existing build/hot-reload pipeline. Both scaffolds include provisional `DESCRIPTION.md`; this does not migrate catalog README/media fields or certify listing rendering.

`init --mcp-url <https-url>` creates metadata and documentation only. OAuth is the default; `--authentication none` is explicit. No JavaScript, Node package, daemon-event types, worker or local server command is generated. `doctor` uses the existing source scanner and the host's exact pure descriptor parser. `pack` emits canonical `<id>-<version>.mcp.json` without network/DNS/registration/consent. Credentials, headers, client IDs, trust, enablement and host-owned identity fields remain rejected. The 8 KiB limit and API/schema/destination grammar stay identical to E1/E2.

The checker adds a path dependency on the existing `grain-core` validator rather than copying endpoint/schema policy or moving settled runtime code. The lock diff adds only that local edge: no new external package/version or service. SDK exports and desktop auth/transport/dispatch are unchanged. A future leaf-validator extraction should be driven by measured CLI size/build cost; no memory reduction is claimed here.

`dev` refuses MCP projects before connecting or starting watchers. MCP `submit` explicitly refuses until submission/listing acceptance. Native submission remains the historical source-pointer workflow, not newly certified publishing. Direct user-configured connections remain package-free; this CLI does not add local descriptor import or store UI.

## Focused audit and repair

Graph-first exploration had no useful authoring file summary, so targeted source inspection followed. Review guidance was broad/truncated (up to 500 nodes across 38 files) with imperfect test linkage; it is not execution evidence. The scoped source audit covered flags, project-kind detection, canonical/bounded safe reads, malformed/ambiguous roots, generated exports/examples, cleanup guards, artifact publication, retained native build/dev/submit, dependencies and maintained case ownership.

The audit repaired output handling: a built artifact cannot replace either root definition, including the other extension kind's definition and canonical aliases/case variants. Otherwise `pack --output manifest.json` could make an MCP project ambiguous. Refusal tests prove preserved source and prior artifact. Invalid scaffolds remove only their newly created directory; existing projects are not overwritten. Unsupported endpoints/auth/schema/API/metadata, unknown fields, stdio, oversized/invalid JSON and forbidden credentials are refused. The compiler negative control still detects raw-token access; generated native code compiles and its bundle returns the declared hello result.

No remaining finding blocks this authoring unit. E3b must address structured TOML submissions, exact source identity, DESCRIPTION/media bounds/provenance and documentation migration; producer/consumer rollout must be coordinated with E4. Public SDK freeze and release remain gated.

## Verification and identity

- `cargo test --locked -p grain-ext-cli -p grain-extension-checks --lib`: **35/35 Pass** (11 CLI, 24 checker).
- Node runner/client metadata: **81/81 Pass**. Generated native project/type/negative-control/bundle: **4/4 Pass**, with 15 expected compile refusals.
- Real acceptance build includes TypeScript and actual embedded frontend/backend. Normal Vite production build passes in 7.57 seconds. Scoped Clippy passes with two existing CLI conversion warnings; the acceptance host retains twelve existing feature-build warnings. Format/scoped whitespace checks pass.

| Evidence | Result and scope |
|---|---|
| `author-dx6evl` | Generated native contract checks above; non-visual, no real host certification. |
| `run-9A1Pjk` | **3/3 store cases Pass**. Actual stamped CLI generates/checks/packs a descriptor; its exact bytes enter the existing signed fixture, install inactive, obtain explicit enablement and complete approved Agent read/restart. Existing metadata/account/revocation cases also pass. |
| `run-kpQNvY` | **12/12 extension-contract Pass**, including native CLI packaging, typed/public results, compatibility/migration, account independence, store integrity and smoke; separate verdicts retained. |
| `run-xVgKK9` | Fresh **1/1 store.mcp-management Pass** on the same final app/CLI; repeat artifact has the same content hash. |

Desktop evidence is actual Windows x64 Grain/Wry/WebView2 154.0.4258.53 with controlled model/owned peers. It does not certify live consent, production publishing or genuine-model effectiveness. Final identities:

- Dirty base: `eb217f824d523965db5fb0c440a2bcf51f80d70e`.
- App/CLI source: `12fe0ab145061ffbd4ca38e16fb91ff5beb4078b165f6f590d0f16e4a435fb40`.
- App SHA256: `ec5ee374fcb51da5eb37db0a4eadaab7f66375d4825f98829a8eaf21c00331fa`.
- CLI SHA256: `e3723dab72e435f2ae9710130126603dd50b88c99f866a0224f4aba81cdf6afd`.
- Desktop runner: `691b526b3738def98e06c89a6ff1d6a55ecca12e057f7a4c0a03a4871b437707`.
- MCP artifact: 268 bytes, SHA256 `c361bb342eee134a4c6c3865e65652577a933e013a72e9666eb2241f39e60fc6`.

All three desktop scopes record cleanup Pass and zero remaining grant/client-secret/registration inventories. Independent read-only inspection checks three scopes, 25 host PID references, six new MCP CLI command references and all marker fixture ports against executable/root ownership. No owned profiles/TLS/MCP author folders/processes/listeners remain. No process was killed by the inspector; ordinary profiles/accounts remain untouched.

## Retention and handoff

Keep CLI/checker/tests, the author guide and the minimal addition to the existing store management case. It reuses the scoped CLI helper, signing fixture, peers and report format: no new engine, command bridge, model instruction, fault framework or scenario. `mcp-author` is a named disposable folder removed during normal teardown. Ignored `.runs/e3a-*` logs/inspectors/reports and the generated nonsecret author-contract folder are disposable evidence. Inventory stays **108 scenarios / 93 self-contained / 81 Node self-tests**.

Next E3b completes submission/listing metadata and author documentation using existing tooling. E4 aligns the separate registry/protected publishing; E5 owns UI. **Six E3–E8 stages retain work.** Baseline remains **52 Pass / 1 Deferred (12)**; permanent client hosting/live acceptance **56 Pending**; release gates and physical obsolete-code removal hold remain. No new manual test batch.
