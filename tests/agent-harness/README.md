# Grain Agent acceptance harness

Start with the frontend unit tests and affected Rust tests. On Windows use
the existing Cargo runner at `scripts/run-rust-test.ps1` for desktop library
tests. Use the real-app harness for an application boundary that the normal
tests cannot cover. The configured MCP suites are the current checkpoint for
custom connections. The signed store, native package fixtures and worker
transport were retired on 8 November. See the [retirement log](../../docs/Extensions%202.0/STORE-RETIREMENT.md)
for implementation checkpoints and links to historical evidence.

Build with `pwsh -NoProfile -File tests/agent-harness/build.ps1`. Use
`node tests/agent-harness/run.mjs --list` for the current scenario inventory.
The `smoke`, `mcp-agent-workflows`, `mcp-configured`, and
`mcp-configured-runtime` suites cover the MCP-only cutover. The controlled TLS
fixtures need Python with `cryptography` on `PATH`. Older MCP model fixtures
that still wait for manual approval require adaptation to automatic dispatch.

## Quick start on Windows

Use the normal Grain build toolchain, Node 22+, PowerShell, WebView2 and the
Windows SDK. Install the workspace dependencies before building. Owned TLS
fixtures require Python with `cryptography`.

```powershell
pwsh -NoProfile -File tests/agent-harness/build.ps1
node tests/agent-harness/run.mjs --list
node tests/agent-harness/run.mjs --suite smoke
node tests/agent-harness/run.mjs --suite mcp-agent-workflows
node tests/agent-harness/run.mjs --suite mcp-configured
node tests/agent-harness/run.mjs --suite mcp-configured-runtime
node tests/agent-harness/run.mjs --suite mcp-configured-auth
node tests/agent-harness/production-tests.mjs --group mcp
node --test tests/agent-harness/runner.test.mjs
```

The builder stamps the executable and source. Rebuild after source changes.
The runner refuses an ordinary `handy.exe` or a binary that differs from the
stamp. The production logic runner accepts `agent`, `execution`, `mcp`, and
`all`. These results cover library behavior. The runtime suites exercise the
real Tauri app.

The harness uses its own identifier, profile and scoped MCP vault entries.
It disables ordinary global shortcuts and avoids microphone or account access
in self-contained runs. Use an idle Windows desktop when testing the actual
Agent window. On Windows, close the app using the debug target before building
if it holds a runtime DLL open.

Each run creates a unique folder under `.runs/` or the parent passed via
`--output`. The runner cleans up owned processes, scratch data and credentials
and retains the report, bounded logs and failure evidence. Keep live-account
reports out of Git. Exit codes are 0 for passing scenarios and cleanup, 1 for
failure, and 2 for a blocked prerequisite. A failure leaves later scenarios
unrun. Use the [interruption inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory)
before cleaning up an interrupted run.

### Terminal-only MCP probe (GitHub test profile)

**15 October mixed-provider acceptance:** seven real Harbor journeys passed with
GitHub and Notion enabled, including combined reads and disposable writes/fresh
verification. Reused this probe; no new maintained runner/scenario was introduced.
Ignored `.runs/toolcalling-mixed-20261015/` retains the disposable staging adapter,
reports/fixtures and independent GitHub checks. Resolve diagnostic action names
from each task's offered/loaded canonical IDs: a previous exported catalogue can
be stale. Nonempty replies alone are not acceptance. All temporary model/key
fields were restored; ordinary settings/credentials were unchanged and owned
processes exited. Retire the adapter after review. Hints remain debug opt-in:
the matched ordinary read was slower with hints. See progress for exact verdicts.

The debug app recognizes `GRAIN_AGENT_PROBE_FILE` as a one-shot request file.
This reuses its headless setup and production MCP/Agent functions. It creates no
browser, microphone, native worker server or testing service. Release builds and
the separate `agent-harness` binary do not contain this entry point. It refuses
the ordinary app identity: use a debug build with the GitHub test configuration.
Close the GitHub test app first; the probe borrows its saved accounts/model without changing
provider selection. Keep request/output files in ignored `.runs/` evidence; never
commit account-bound catalogue snapshots or credentials.
Validate the JSON report/error and trace, not only the parent GUI process exit
code: a returned `error` never counts as an Agent Pass.

```powershell
# From src-tauri/, build once after source changes (no installer or Vite server).
# Use the same short-target environment as scripts/run-tauri.ts on Windows,
# restoring the calling shell afterwards.
$taskOldLocal = $env:LOCALAPPDATA
$taskOldTemp = $env:TEMP
$taskOldConfig = $env:TAURI_CONFIG
try {
  $env:LOCALAPPDATA = $null
  $env:TEMP = $null
  $env:TAURI_CONFIG = Get-Content tauri.github-test.conf.json -Raw
  cargo build --bin handy
} finally {
  $env:LOCALAPPDATA = $taskOldLocal
  $env:TEMP = $taskOldTemp
  $env:TAURI_CONFIG = $taskOldConfig
}

# Create request.json with UTF-8 encoding in your owned .runs/ folder.
# Catalogue only: {"mode":"catalog"}
# Agent: {"mode":"agent","execute":true,"prompt":"Use Notion to search for Grain. Do not create, edit or delete anything."}
$env:GRAIN_AGENT_PROBE_FILE = 'C:\absolute\owned-run\request.json'
$env:RUST_LOG = 'handy_app_lib::agent=info,handy_app_lib::capability=info,warn'
& 'C:\gt\debug\handy.exe' > report.json 2> trace.log
Remove-Item Env:GRAIN_AGENT_PROBE_FILE
```

Run serially. Requests are bounded, live Agent execution is explicit and the
process exits after its report or a three-minute deadline. A timeout is not proof
of non-execution; never retry a possible write automatically. `catalog` exports
supported schemas and the exact production lexical projection, plus coverage;
it does not prove that every provider capability was admitted. `agent` returns
the actual reply and logs offered schemas/call rounds through normal diagnostics.
This exercises **MCP/backend** behavior; frontend display and speech acceptance remain separate. Retire this probe if a maintained direct CLI later
provides the same coverage; disposable request/analysis files are not new suites.

**15 October adopted recovery policy:** normal debug/release Agent requests now
include at most 8 KiB of inert deferred tool names, then optional purpose metadata.
Their schemas remain unloaded until ordinary `search_tools` publishes them. The
`GRAIN_AGENT_DEFERRED_HINTS` experiment switch is retired; it no longer changes
the runtime. Initial ranked/eager schemas retain 24/32 KiB ceilings, with 64 KiB
for task recovery. No server, runner or alternate Agent loop. Historical trial
results below retain their original off/on conditions; use current source/default
path for new acceptance. The lexical-only initial-recall gate failed; do not call
mechanical exact-recovery coverage a model-accuracy result. See current progress.

Live follow-up: after the user restored 9router, the same-binary read-only trial
verified combined recovery and successful search/fetch. All three reports passed;
one initial control missed catalogue preparation and must not be treated as a
matched timing sample. Hints remain opt-in pending broader comparison; see the
progress record for the warm control and experimental measurements.

Harbor follow-up (15 October): four genuine read-only journeys verified the
saved Harbor model with fallback disabled. Three task passes; control full-page
read was **Fail** because search recovery never published fetch and the response
was partial. Nonempty reply/expected title/provider success alone cannot accept
that task. Hints passed both requests and used fewer calls; one sample per task
and condition does not close the adoption gate. Ignored `.runs/harbor-20261015/`
holds reports and a disposable profile-staging adapter, not another maintained
suite. Retire that adapter after evidence review. All changed alpha selection/
fallback/borrowed-key fields were restored while retaining SDK refresh updates;
normal settings/credentials were unchanged. No Computer Use or installer.

For an offline catalogue comparison, use the opt-in backend test, not another
ranker implementation. Its JSON case array has `id`, `split`, `query`, and
`requires` (all groups required; IDs within each group are alternatives). Tool
labels must exist in the exported supported catalogue. Use canonical IDs for
shared names; bare names must resolve uniquely or the test rejects the case.
The test runs production
projection, ranking and schema publication at K=4/6/8 and records actual packed
coverage. No model or provider calls occur. It does not replace live task checks.
Non-eager initial publication uses 24 KiB within the 64 KiB task ceiling. The
existing evaluator also checks required sets after oracle-chosen exact lookup
of names actually visible in the bounded directory, preserving previous schemas.
It does not run a model recovery journey. Record both initial and after-recovery
coverage and budgets when comparing older 32 KiB reports. Label required provider
prerequisites, such as Notion fetching
Markdown guidance before writing content, rather than only the final write tool.
Runner Pass does not mean retrieval quality Pass. Cases inspected while tuning
are regression evidence; reserve fresh cases for an independent adoption gate.

```powershell
# From src-tauri/, after the probe process exits (shared Windows DLLs).
$env:GRAIN_LEXICAL_CATALOG = 'C:\absolute\owned-run\catalog.json'
$env:GRAIN_LEXICAL_CASES = 'C:\absolute\owned-run\cases.json'
$env:GRAIN_LEXICAL_REPORT = 'C:\absolute\owned-run\retrieval.json'
cargo --config 'target.x86_64-pc-windows-msvc.runner=["powershell.exe","-NoProfile","-File","C:/absolute/repo/scripts/run-rust-test.ps1"]' test --lib exported_catalogue_required_sets_under_schema_budget -- --ignored
```

## Installed GitHub MCP checkpoint

Run from the repository root on Windows:

```powershell
node tests/agent-harness/github-alpha.mjs --preflight
node tests/agent-harness/github-alpha.mjs --catalogue
node tests/agent-harness/github-alpha.mjs --read
node tests/agent-harness/github-alpha.mjs --workflow
node --test tests/agent-harness/github-alpha.test.mjs
```

Build the isolated Windows app using the Build Test workflow's
github-mcp-test option. In Grain GitHub Test, enable Agent, select its model,
connect GitHub from Browse extensions and disable other connections. Close
the app before running the script. The GitHub CLI needs independent read
access to Punit-Dethe/grain-github-alpha-test/README.md for the read check.

This focused script starts the installed Grain GitHub Test app, attaches to its owned
WebView2 through loopback CDP and invokes production Tauri Agent commands. It
does not use Computer Use, screenshots, simulated clicks, fixtures or a model
relay. `--read` approves only the test repository's README getter, verifies the
returned content independently, restarts the owned app, then repeats the read.
Preflight and catalogue discovery are read-only. The catalogue check records
supported tool names, a count and a digest without executing tools. The README
check verifies the result through the independent GitHub CLI and repeats the
read after restarting the owned app. Existing test account credentials stay in
the isolated profile.

`--workflow` is an explicitly authorized write checkpoint on the disposable
`Punit-Dethe/grain-github-alpha-test` repository: one marked issue, update/close,
one marked branch and a synthetic file. Exact argument/title guards refuse other
actions and repeated writes. Independent CLI reads verify each effect and main's
unchanged commit. Test objects are retained in the report for inspection; failed
or uncertain work must not be replayed automatically. No account connect or
disconnect is automated by this mode. Its final exposure check requires a test build
with `agent-tool-trace` diagnostics; an older binary may pass effects while that
distinct check remains Blocked. New log entries contain tool identities/counts
only; the original log level is restored even when the batch fails.

Reports record the installed binary hash and per-stage verdicts under ignored
`.runs/github-alpha-*/evidence/`; subsequent runs also stamp this script. Only
the spawned host is stopped, with cleanup reported independently. Disposable
WebView directories and evidence can be removed after review; process cleanup
does not claim file deletion. Retain this small installed-app checkpoint
until the standard runner can target installed apps directly, then consolidate
it there rather than building another testing framework. This verifies the real
Agent/MCP command path, not physical shortcuts or visual interaction.

### Linear consent preflight (opt-in, no account)

`mcp-linear-preflight` contains two separate cases: `mcp.linear-cancel-disable` and `mcp.linear-cancel-disconnect`. Each uses actual production `rmcp` metadata discovery and Dynamic Client Registration against Linear, validates the private SDK authorization handoff, cancels with the named host operation, verifies the exact cancellation, rebinds the retired callback port and verifies a late connection is refused. A real host restart and fresh SDK attempt repeat that case. The suite is excluded from `all` and does not open the browser, submit a code, import an account, or execute a tool. DCR may create server-owned client registrations; local cleanup does not assert their deletion.

The guarded debug host admits exactly `grain-harness-linear`, `https://mcp.linear.app/mcp/readonly` and SDK scope `read`. Its marker cannot mix controlled MCP, native account, store or DeepWiki fixture modes. The alias and consent slot are unavailable without that marker and absent from ordinary builds. Even an unexpected completed callback does not permit tool execution through this preflight adapter. The original production Linear catalog entry is unchanged. Vault inventory/deletion is metadata-only, bounded, and targets that alias plus this run's UUID service; ordinary grants are never imported or cleared.

Test the cancellation oracle with `npm run test:agent -- --scenario mcp.linear-cancel-disable --fault skip-linear-cancel`. It must **Fail** at the five-second observation that cancellation settled, then retire the owned flow in `finally` and report cleanup Pass. This short observation does not replace the production five-minute authentication deadline. The fault is refused in other suites or multi-case selections. Reports retain scope/resource/PKCE and cleanup booleans, never authorization URL, state, client ID or grant contents.

This is preparation for checks 9/12/17, not their acceptance. The next account unit must provide a private browser handoff and a bounded human-sign-in wait, verify the actual granted scope/lifetime, suppress private workspace data in diagnostics, and validate a fixed harmless read against actual authenticated tool metadata before permitting its dispatch. A suitable nested schema is required for check 17; normal reads alone do not certify it. Real MCP expiry/refresh uses the issued grant's actual clock, not API lifetime assumptions or modified credentials. No personal sign-in is needed for the preflight command.

### Human Linear consent (explicit opt-in)

`mcp-linear-guards` verifies the new ready/open/grant controls in the real host, first without an available consent URL, then with actual SDK readiness and host cancellation. It never opens a browser. `mcp-linear-sign-in --linear-sign-in` is a separate interactive case; the flag is refused in other suites, and a noninteractive launch is refused before any profile/network/browser work. Both suites are excluded from `all`.

The account owner performs only two browser steps:

1. Cancel or close the first Linear consent page **without approving**. Type only `cancelled` and Enter in the runner terminal. The report distinguishes this human acknowledgement from the actual denied/cancelled backend outcome, callback release and zero vault entries. Accidentally approving this first flow fails the procedure.
2. Sign in to Linear in the second page and approve the requested **read-only** access. The SDK validates the real callback, performs the actual code exchange and stores the grant in this run's Windows vault namespace. The runner then verifies scope/lifetime metadata, authenticated tool discovery, real restart and persistence, and disconnects/cleans up. It **does not execute any tool** or send workspace results to a model.

Only the fixed alias Connect receives a 310-second IPC observation, around the unchanged production five-minute authentication deadline; its discovery receives 95 seconds around the unchanged 90-second production budget. Other commands/providers keep ten seconds. Cancellation input is bounded, abortable, accepts only the fixed acknowledgement, and releases its readline/input listeners and timer on every exit. A timeout or terminal close fails and retires the owned login; it never counts as successful consent.

The harness validates the grant **before every vault publication**, including refresh: exact issuer, read-only SDK-granted scopes, a nonempty bearer token and bounded client/timestamp/lifetime metadata. If the token response omits scope, the locked SDK's RFC 6749 requested-scope resolution is recorded explicitly; it is not reported as an explicit token-response scope. Unknown expiry remains `expiryKnown: false`, never an assumed lifetime. Reports contain only scope source, issued/expiry seconds, refresh availability and supported-tool counts. They omit authorization URLs, token/client values, tool names/descriptions, host logs, model/event bodies, snapshots and screenshots in human mode. Provider/error bodies are replaced by fixed stage diagnostics. Local scope cleanup remains independent of remote grant revocation; remove the test authorization from Linear's account settings if you also want remote revocation.

This procedure prepares live acceptance; it does not by itself close 9/12/17. Actual browser/exchange/discovery behavior must be run with the account owner, reviewed and repeated. A credentials badge or a discovered tool count is insufficient to certify harmless Agent execution, nested compatibility or post-expiry refresh. Tool calls remain blocked until a separately reviewed fixed read allowance can be based on real authenticated schemas. Credentials/profile are deleted when the run completes, so no idle test host or grant is left for a later coding phase. The `oauth2` harness-only dependency only names the SDK's already-locked v5 token trait; it adds no new library version, client or OAuth implementation.

## Pinned official conformance through Grain

Build the stamped real harness application first; use Windows, Node and Python `cryptography`, with no concurrent native compilation. The separate private package locks `@modelcontextprotocol/conformance` to **0.2.0-alpha.12**. Installation uses locked `npm ci --ignore-scripts` in ignored `.build/conformance/`; it adds no application dependency.

```powershell
node tests/agent-harness/conformance.mjs --install
node tests/agent-harness/conformance.mjs --suite tools
node tests/agent-harness/conformance.mjs
node tests/agent-harness/conformance.mjs --scenario mcp.conformance-tools-modern --fault missing-official-check
```

The tools subset expects **exit 0**: separate legacy 2025-11-25 and modern 2026-07-28 tool cases Pass. The complete entry currently expects **exit 2**: those two Pass and standalone initialization Blocked because the pinned raw fixture sends an invalid empty discovery reply. The fault expects **exit 1**, missing required official evidence detected after a real successful call and cleanup Pass. Check the Node exit code and JSON, not just an outer shell's generic nonzero code. A Blocked/empty/skipped/warning result is never a Pass.

Each official case has its own actual server, isolated real application and owned-PID HTTPS relay. The relay preserves wire bodies/status/metadata and checks the upstream listener PID before every exchange; it accepts no user account, redirect or arbitrary endpoint. Grain performs production discovery, selected-tool loading and actual Agent UI approval. Exactly one `add_numbers` call with 5 and 3 must return the real result 8; both official numeric and wire-schema checks, application acceptance and independent cleanup must pass.

Stable IDs are `mcp.conformance-tools-legacy`, `mcp.conformance-tools-modern` and `mcp.conformance-initialize`. These require the separately owned official binding; plain `run.mjs` cannot silently substitute a synthetic peer. Ordinary `--suite all` selects all 54 self-contained cases. Reports retain raw official checks, application children, identities, case verdicts and cleanup under `.runs/conformance-*/`. Keep init Blocked until a compatible fixture is audited; do not force a different production negotiation mode.

The [focused conformance audit](../../docs/Extensions%202.0/MCP-CONFORMANCE-AUDIT.md) records the pin, evidence, blocked prerequisite and retirement conditions for the temporary relay. The [retention inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) preserves the earlier interrupted-root cleanup exception. This named subset is not full SDK-tier, OAuth, live-provider or whole B2 acceptance.

## Controlled real-app MCP suite

The opt-in public live suite is documented separately below. Its external availability does not affect the self-contained suite.

Requires the stamped real harness application and Python `cryptography` for owned TLS. No user account, model API key or production trust-store change is required. A fixed debug-only provider uses only its run's exact loopback endpoint and CA; it refuses account commands. The external peer supplies wire replies, not Agent/executor mocks.

```powershell
node tests/agent-harness/run.mjs --suite mcp
node tests/agent-harness/run.mjs --scenario mcp.response-preview --fault short-mcp-preview
```

| ID                       | Separate assertions                                                                                                                                                                                                                                               |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcp.legacy-http-probe`  | Generic-ID HTTP 400 discovery rejection, one fresh legacy handshake per operation, JSON/SSE management and approved nested reads, exact result, no call replay, session deletion and zero retained sessions.                                                      |
| `mcp.transport-contract` | Modern/legacy JSON/SSE, two-page nested reads, one approved call, no idle negotiation, actual restart and stale disable refusal.                                                                                                                                  |
| `mcp.mixed-catalog`      | Supported/excluded schemas, excluded selection with zero dispatch, empty supported catalog, repeated cursor and changed-approval refusal.                                                                                                                         |
| `mcp.response-preview`   | Multi-byte text/large structured data, bounded model preview, distinct model/UI omission notices and fresh recovery in JSON/SSE.                                                                                                                                  |
| `mcp.transport-bounds`   | Twelve named lifecycle/overflow/drop variants: declared/chunked JSON, SSE data/small comments, error body and lost reply. Honest unknown result, one call, no replay and fresh read after each.                                                                   |
| `mcp.close-cancellation` | Four approved held calls, actual panel close/HTTP disconnect/session cleanup, attempted late reply, fresh result isolation; two pending-approval closes never dispatch.                                                                                           |
| `mcp.catalog-budgets`    | Twenty-four lifecycle/framing/limit combinations: management Test and actual Agent independently refuse tool counts, rejected-definition duplicates, cursor/page limits, aggregate metadata and received bytes; zero action exposure/dispatch and fresh recovery. |
| `mcp.http-deadline`      | Four real 45-second approved-call timeouts; unknown result, one call, measured duration, response/session release, discarded late reply and fresh recovery.                                                                                                       |
| `mcp.discovery-deadline` | Modern/SSE and legacy/JSON each finish two 35-second pages, then stop the third at the real 90-second absolute deadline; no tool dispatch, no retained delay timer/session and fresh recovery.                                                                    |

Use `--list` for the current inventory. Official and live suites require their own prerequisites. Full MCP transport waits use the production clocks.

```powershell
node tests/agent-harness/run.mjs --scenario mcp.catalog-budgets --fault accepted-mcp-catalog
```

The catalog fault supplies an ordinary complete catalog where rejection is required. Expect the first precise management refusal assertion to fail, zero tool calls and cleanup Pass. It is admitted only for that scenario.

Reports now checkpoint completed cases before/after each scenario, preserving `activeScenario` and cleanup Not run until final teardown. A hard process loss can leave owned scratch/credentials behind; no partial report certifies cleanup or the full run. Follow the [hard-interruption inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) before explicit recovery. The initial interrupted MCP run and policy-blocked cleanup are retained in the [catalog/deadline audit](../../docs/Extensions%202.0/MCP-CATALOG-DEADLINE-AUDIT.md).

[Catalog audit](../../docs/Extensions%202.0/MCP-CATALOG-TRANSPORT-AUDIT.md), [bounds/cancellation audit](../../docs/Extensions%202.0/MCP-BOUNDS-CANCELLATION-AUDIT.md), and [retention/cleanup inventory](../../docs/Extensions%202.0/AGENT-TEST-HARNESS.md#harness-retention-and-cleanup-inventory) give evidence identities and exclusions. Controlled partial coverage never certifies live MCP OAuth, account persistence or official conformance. Retain fixtures as regression tests; delete owned profiles, keys, credentials, processes and response handles each run. Feature-gated test seams have documented retirement conditions.

Implementation/design references: [Playwright WebView2 automation](https://playwright.dev/docs/webview2), [Microsoft WebView2 environment options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl), [Tauri's distinction between mock-runtime and real-application tests](https://v2.tauri.app/develop/tests/), [Windows SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) and [foreground window restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow). These support the adapter choice; Grain's isolation and evidence policies are local requirements.

## Opt-in public live MCP acceptance

Build first, leave the desktop idle and run serially after native compilation. This uses the real application and production MCP adapter with a scripted model, normal TLS trust, and no account. The debug provider is fixed to `https://mcp.deepwiki.com/mcp`; only `read_wiki_structure` and `read_wiki_contents` for public `modelcontextprotocol/rust-sdk` are admitted. Questions, private repositories, extra arguments, credentials and arbitrary endpoints are refused before dispatch. No DeepWiki provider is added to the ordinary product catalog.

```powershell
node tests/agent-harness/run.mjs --suite mcp-live
node tests/agent-harness/run.mjs --scenario mcp.live-read-disable --fault missing-live-evidence
node tests/agent-harness/run.mjs --scenario mcp.legacy-http-probe
```

The live suite expects exit 0; service or desktop failures retain their actual verdict. The fault expects exit 1 after a genuine approved read, because required result evidence is deliberately withheld from the verifier; cleanup must pass. No automatic retry masks a failure.

| ID                           | Separate assertions                                                                                                                                                                                                                                                                                      |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcp.live-read-disable`      | Live discovery; actual approved result; disabled stale approval and fresh request both refuse with zero backend attempts; re-enable and actual restart restore reading. The fresh refusal retains an unrelated harmless native directory entry; empty-directory text-only fallback is outside this case. |
| `mcp.live-response-recovery` | Actual ordinary read, large public documentation request, bounded preview or honest unusable/unknown result, then a fresh actual ordinary read. No blind replay.                                                                                                                                         |

Reports contain fixed endpoint/repository, per-stage result kind/byte count, model evidence and client-side attempt observations. Attempt counts are conservative backend observations, not independent remote receipts. No raw documentation is retained in the JSON evidence. These cases exclude OAuth, private accounts, nested live schemas, live-model judgment and universal provider certification. Controlled overflow fixtures supply exact byte-limit evidence; an unknown live response does not prove remote failure or exact received payload size. See the [focused live audit](../../docs/Extensions%202.0/MCP-LIVE-READ-AUDIT.md). Both cases are excluded from ordinary `all`.

## Controlled MCP provider independence

```powershell
npm run test:agent -- --suite mcp-independence
node tests/agent-harness/run.mjs --scenario mcp.auth-provider-independence --fault wrong-mcp-peer-account
node tests/agent-harness/run.mjs --scenario mcp.auth-fixed-port-conflict --fault skip-fixed-port-conflict
```

The two positive cases share the real host and existing peer implementation. A second server starts only for these selected cases, with a separate origin/listener and client/code/token maps. Both actual SDK dynamic login futures remain open together; completing one and cancelling the other must leave only the completed grant enabled. The cancelled issued callback is refused. Approved reads prove the actual A/B accounts independently, including restart, disabled stale approval and removal of only one provider. The second case expects an exact refusal while the first preregistered flow owns port 31938, then serial login/read/restart recovery on that released fixed port. A conflict is never counted as authentication.

Each read asserts one wire call to its selected provider, zero calls to the other, exact nested returned JSON/account and a model receipt naming that provider. Agent reads remain serial; login futures may overlap independent approved reads. Neither a real model nor a personal account is required for these mechanical checks. External browser consent/live-account compatibility remains separate. Both fault commands must Fail/exit 1 with cleanup Pass. See the [focused provider audit](../../docs/Extensions%202.0/MCP-PROVIDER-INDEPENDENCE-AUDIT.md). Metadata-only vault cleanup admits exactly the four fixed keys in the current run namespace; both servers, handoff generations and disposable profiles are retired.

## Controlled SDK MCP OAuth acceptance

`mcp-auth` uses the actual locked Rust SDK, production protected-resource/issuer discovery, public dynamic registration, S256 PKCE/resource-bound exchange, real callback listener and scoped Windows vault. The fixed debug-only Account and Client targets require `mcpAuth` and its run-owned HTTPS peer. It never accepts an arbitrary provider, token, callback destination or certificate. The private handoff consumes the SDK's actual URL and the fixture's actual redirect; it replaces external browser consent only. No personal account or OS trust-store modification is required. Python `cryptography` supplies the existing disposable TLS setup. The Client target uses the same owned resource/issuer with three fixed preregistered IDs and fake secrets. It exercises the production credential settings and secret vault; it does not certify provider independence.

| ID                              | Independently asserted coverage                                                                                                                                                                                                                                                      |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mcp.auth-fixture`              | Actual A/B grants, selected nested read, no call before approval, exact account/result, old A generation refusal with zero calls, actual A/B restarts and disconnect.                                                                                                                |
| `mcp.auth-denied-cancelled`     | Denied callback and real Cancel sign-in command finish with zero exchange/grant, immediately reusable exact callback ports, then fresh approved A/B reads.                                                                                                                           |
| `mcp.auth-late-callback`        | Cancel sign-in, Disable and Developer Mode off each refuse the issued old redirect, with no exchange/grant/resurrection after actual restart; new B login/read/restart succeeds.                                                                                                     |
| `mcp.auth-close-cancellation`   | Modern/legacy crossed with JSON/SSE: actual approved held read and pending approval close; bounded local cleanup, late result refusal, preserved scoped grant/enablement and exact fresh/restart account A reads without new authorization.                                          |
| `mcp.auth-shutdown`             | Provider Disable and Developer Mode off each cross modern/legacy JSON/SSE: actual held receipt, bounded unknown outcome/UI, no replay, session deletion, disabled discovery refusal, preserved grant, stale confirmation refusal and fresh/restart A reads without signing in again. |
| `mcp.auth-client-configuration` | Empty-secret public and actual confidential login; public ID change, same-ID secret rotation/removal invalidate old grants/approvals; unsupported missing secret refuses, fresh/restart A/B reads recover with exact scopes and no DCR.                                              |

Build first; run serially on an idle desktop:

```powershell
node tests/agent-harness/run.mjs --suite mcp-auth
node tests/agent-harness/run.mjs --suite mcp-foundation
node tests/agent-harness/run.mjs --scenario mcp.auth-fixture --fault wrong-mcp-account
node tests/agent-harness/run.mjs --scenario mcp.auth-fixture --fault abandoned-mcp-credential
node tests/agent-harness/run.mjs --scenario mcp.auth-close-cancellation
node tests/agent-harness/run.mjs --scenario mcp.auth-close-cancellation --fault lost-mcp-account
node tests/agent-harness/run.mjs --scenario mcp.auth-shutdown --fault skip-mcp-disable
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault skip-mcp-client-change
node tests/agent-harness/run.mjs --scenario mcp.auth-client-configuration --fault abandoned-mcp-client-secret
```

All six faults must report Fail/exit 1 and cleanup Pass. Wrong-account changes the actual authenticated provider result; it must fail the account oracle. Abandoned-credential stops after one real grant; independent `mcpVaultCleanup` must report `deleted: 1`, `remaining: 0`. Lost-account invokes the actual production Disconnect command after an approved held read closes; the preservation assertion must fail rather than silently reconnect. Skip-disable leaves the actual held call enabled; the immediate provider-state assertion must fail before any queue timeout. Skip-client-change leaves the prior grant intact; the immediate grant-removal assertion must fail. Abandoned-client-secret stops after the real confidential secret is stored; independent cleanup must report `clientSecretsDeleted: 1` and `clientSecretsRemaining: 0`. Inventory/cleanup reuses `auth-cleanup.ps1 -Mcp`, matching only the exact run service and four fixed account keys and their scoped client-secret service; it never reads blobs. Normal success requires zero remaining scoped grants/client secrets/sessions/sockets and removed owned scratch material.

`mcp-foundation` selects the nine existing transport cases and these ten cases in the same real host, exercising their coexistence. It preserves genuine 45/90-second deadlines and independent scenario/stage assertions; it stops on first failure. Nineteen cases reuse the maintained peer implementation; independence alone starts a second owned issuer/listener. No production service is introduced. The OAuth evidence journal has a fixed 2,048-entry ceiling and reserves one constant terminal-error entry; its exhaustion self-test verifies failure without rethrow or private-request leakage.

Only actual bearer lookup and exact returned nested JSON establish account evidence. The model oracle distinguishes exact Cancelled/not-dispatched refusal, exact dispatched account uncertainty and exact successful account/result; unrelated failures cannot certify any of them. The close case requires four cancelled wire calls and twelve actual successful reads across four real restarts. Shutdown requires eight cancelled receipts, eight stale confirmations with zero calls, and 24 successful fresh/restart A reads, with no new authorization exchanges. Evidence contains bounded account/stage/counter flags, never authorization URLs, codes, PKCE verifiers or tokens. The client case requires ten actual successful reads, four stale refusals with zero dispatch, five restarts, exact secret inventory/rotation and one actual unsupported-secret rejection with zero issued token. External browser UX, live OAuth/refresh and failed-switch preservation remain separate. The independence cases are documented above; the controlled expiry and refresh cases are documented below. See the [SDK/late-login audit](../../docs/Extensions%202.0/MCP-AUTH-FIXTURE-AUDIT.md), [authenticated cancellation audit](../../docs/Extensions%202.0/MCP-AUTH-CANCELLATION-AUDIT.md) and [shutdown audit](../../docs/Extensions%202.0/MCP-AUTH-SHUTDOWN-AUDIT.md) and the [client-configuration audit](../../docs/Extensions%202.0/MCP-CLIENT-CONFIGURATION-AUDIT.md) for exact boundaries.

## Agent read, write and interruption

```powershell
node tests/agent-harness/run.mjs --suite mcp-agent-workflows
node tests/agent-harness/run.mjs --scenario agent.workflow-mcp --fault wrong-workflow-receipt
```

The controlled cases verify read, exact write and independent verification.
They also cover closing before or after a write, retaining a successful
receipt across model failure, and refusing replay after an unknown result.
The receipt fault should fail the independent oracle and pass cleanup.

## Controlled MCP expiry and refresh recovery

```powershell
npm run test:agent -- --suite mcp-refresh
npm run test:agent -- --scenario mcp.auth-refresh-recovery --fault wrong-mcp-refresh-account
```

The positive suite uses two separately reported cases in one real host. Recovery issues a real two-second grant, restarts, waits issuer expiry and performs an actually approved A read through SDK refresh. The rotated replacement lasts 40 seconds, above the SDK's 30-second proactive buffer; another real restart/full expiry proves generation 2 uses the persisted replacement. A long-lived replacement survives a third restart and read without another refresh or browser login. Refusal separately verifies no refresh token and actual `invalid_grant`: zero tool dispatch/new login, exact current host failure, then explicit reconnect to B and approved fresh/restart reads. No clock or stored credential is edited; only the external owned issuer defines its short test lifetimes.

The fault changes the actual refreshed bearer account and must exit 1 with cleanup Pass. The issuer and acceptance oracles have client/resource/rotation/account and privacy negative tests. See the [focused refresh audit](../../docs/Extensions%202.0/MCP-REFRESH-RECOVERY-AUDIT.md) for final report identities, retained candidate/oracle repair and the current generic reconnect-error finding.

These controlled cases do not accept live check 12. Explicit isolated Linear consent, actual issued lifetime and restart have since passed through the opt-in human suites; genuine expiry/refresh remains pending. Linear has no nested object inputs in the inspected supported catalog; the separate anonymous Hugging Face suite supplies that prerequisite. No ordinary credentials are copied and these local fixture commands must not target an ordinary account. See the [current audit](../../docs/Extensions%202.0/MCP-NESTED-LIVE-READ-AUDIT.md).
