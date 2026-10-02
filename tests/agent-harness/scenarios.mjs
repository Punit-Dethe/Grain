// Stable IDs are report contracts. Numbered checks below are supporting coverage,
// not permission to change the manual acceptance ledger automatically.
export const scenarios = [
  {
    id: "native.typed-contract",
    suite: "native-foundation",
    checks: [22],
    description:
      "Real native typed/optional arguments round-trip; changed declaration without reload refuses the old approval",
  },
  {
    id: "native.legacy-migration",
    suite: "native-foundation",
    checks: [2],
    description:
      "Real upgrade/restarts quarantine legacy privileges and preserve edited prompt/binding archives exactly once",
  },
  {
    id: "native.cold-warm",
    suite: "smoke",
    checks: [3, 4],
    description:
      "Agent searches/loads a single tool, waits for approval, executes once, then reuses the warm worker",
  },
  {
    id: "agent.decline",
    suite: "smoke",
    checks: [],
    description: "Declined approval never dispatches the native tool",
  },
  {
    id: "agent.typed-approval",
    suite: "lifecycle",
    checks: [29],
    description:
      "Typed yes resumes the exact pending call once through the real follow-up field",
  },
  {
    id: "agent.stale-approval",
    suite: "lifecycle",
    checks: [27],
    description:
      "Reload invalidates the displayed approval; clicking the old confirmation cannot dispatch",
  },
  {
    id: "agent.escape-slow",
    suite: "lifecycle",
    checks: [29],
    description:
      "Windows Escape with the owned Agent focused interrupts a slow call and permits a fresh session",
  },
  {
    id: "agent.reopen-escape",
    suite: "lifecycle",
    checks: [29, 30],
    description:
      "Ten immediate close/reopen pairs preserve native Escape and never execute discarded approvals",
  },
  {
    id: "agent.close-pending",
    suite: "lifecycle",
    checks: [30],
    description:
      "Closing a pending confirmation discards it and a fresh session recovers",
  },
  {
    id: "agent.close-model",
    suite: "lifecycle",
    checks: [30],
    description:
      "Closing during a delayed model reply cancels the request and a fresh session recovers",
  },
  {
    id: "agent.close-slow",
    suite: "lifecycle",
    checks: [29],
    description:
      "Real close button interrupts a slow dispatched tool without a late success or replay",
  },
  {
    id: "native.ten-replacements",
    suite: "lifecycle",
    checks: [5, 6],
    description:
      "Ten unload/load cycles replace workers and retain one dispatch per approved greeting",
  },
  {
    id: "native.disable-slow",
    suite: "lifecycle",
    checks: [25],
    description:
      "Disable during a slow call retires only its worker; re-enabled fast call recovers",
  },
  {
    id: "native.hot-reload",
    suite: "lifecycle",
    checks: [27, 37],
    description:
      "Source reload replaces idle/in-flight workers, preserves disabled state, and recovers Agent calls",
  },
  {
    id: "native.real-idle",
    suite: "idle",
    checks: [35],
    description:
      "Production idle reaper retires the worker; a new call recovers; near-boundary slow call survives",
  },
  {
    id: "native.reply-failures",
    suite: "native-failures",
    checks: [21],
    description:
      "Lost replies, declared/thrown errors and malformed envelopes have honest, private classifications and no replay",
  },
  {
    id: "native.readiness-failure",
    suite: "native-failures",
    checks: [21],
    description:
      "A worker that never authenticates fails before dispatch, retires its token and permits a fresh request",
  },
  {
    id: "native.source-drift",
    suite: "native-failures",
    checks: [26],
    description:
      "Warm source changed without reload invalidates the old approval; a fresh request executes the new source",
  },
  {
    id: "native.absolute-deadline",
    suite: "native-failures",
    checks: [24, 32],
    description:
      "A real 25-second tool exceeds the production 20-second absolute deadline, retires once and recovers without replay",
  },
  {
    id: "native.result-budgets",
    suite: "native-failures",
    checks: [33, 34],
    description:
      "Decoded and raw oversized replies fail at distinct boundaries, retire their workers and recover on the shared listener",
  },
  {
    id: "native.invalid-input",
    suite: "native-failures",
    checks: [],
    description:
      "Undeclared model arguments fail before approval or worker startup and private parameter keys remain hidden",
  },
  {
    id: "native.consent-persistence",
    suite: "native-installation",
    checks: [40, 52],
    description:
      "Real consent Cancel/Allow, disabled/enabled restarts, pending-call disable and committed state recovery",
  },
  {
    id: "native.stale-review",
    suite: "native-installation",
    checks: [38],
    description:
      "Replacing a package invalidates the real open permission sheet; a fresh sheet shows current declarations",
  },
  {
    id: "native.owner-restoration",
    suite: "native-installation",
    checks: [28, 39, 53],
    description:
      "Same-version import replacement and installed/developer A/B/restoration survive actual host restarts and refuse old approvals",
  },
  {
    id: "native.enablement-approval",
    suite: "native-installation",
    checks: [19],
    description:
      "Disable/re-enable and source replacement refuse the previous call approval, then fresh greetings recover",
  },
  {
    id: "native.registry-refusal",
    suite: "native-installation",
    checks: [49],
    description:
      "Malformed/future registry startup preserves the file, reports unavailable operations, then a restored isolated profile recovers",
  },
  {
    id: "native.cli-package-ownership",
    suite: "native-installation",
    checks: [28, 39, 53],
    description:
      "Real CLI build/doctor/pack, same-version replacement, old flat/version files, developer A/B and restored installed bytes across restart; accounts excluded",
  },
  {
    id: "native.registry-preservation",
    suite: "registry-recovery",
    checks: [49],
    description:
      "Opaque account pointer, disabled and quarantined records survive restart; explicit validated recovery restores actual tools",
  },
  {
    id: "native.registry-save-failure",
    suite: "registry-recovery",
    checks: [49],
    description:
      "Real Windows publication locks refuse enable durably and disable live with honest save failure, then explicit recovery survives restart",
  },
  {
    id: "store.close-offline",
    suite: "store",
    checks: [18],
    description:
      "Ten held-refresh close/reopen cycles, signed install/consent/Agent call, restart and cached offline installation refusal",
  },
  {
    id: "store.pending-mutations",
    suite: "store",
    checks: [36],
    description:
      "Disable/removal supersede a held signed update; explicit fresh installation and actual Agent results recover across restart",
  },
  {
    id: "store.integrity-close",
    suite: "store",
    checks: [],
    description:
      "Invalid catalog signature, corrupt artifact hash and store-close cancellation refuse publication, then actual tools recover",
  },
];

scenarios.push({
  id: "native.auth-fixture",
  suite: "native-auth",
  checks: [],
  description:
    "Guarded native OAuth fixture: real consent, PKCE, scoped OS vault, approved A/B reads, stale approval refusal, restart and disconnect",
});
scenarios.push(
  {
    id: "native.auth-binding",
    suite: "native-auth",
    checks: [41],
    description:
      "Real vault legacy/unbound credential refusal, four reviewed declaration changes, fresh grants and restart reads",
  },
  {
    id: "native.auth-owners",
    suite: "native-auth",
    checks: [45, 53],
    description:
      "Actual CLI builds and installed/developer A/B account ownership, reload, stale approval refusal, restart, restoration and parked uninstall",
  },
);

scenarios.push({
  id: "mcp.legacy-http-probe",
  suite: "mcp",
  checks: [8, 17],
  description:
    "Generic HTTP 400 discovery rejection, one fresh legacy handshake per operation, actual approved JSON/SSE nested result and session cleanup",
});

// Official runner owns the server prerequisite. Normal --suite all excludes
// these entries; conformance.mjs invokes each finite case with its owned binding.
scenarios.push(
  {
    id: "mcp.conformance-initialize",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official legacy initialization through Grain production discovery, no actions",
  },
  {
    id: "mcp.conformance-tools-legacy",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official legacy numeric tool through real Agent discovery, selected schema and approval",
  },
  {
    id: "mcp.conformance-tools-modern",
    suite: "mcp-conformance",
    checks: [8, 17],
    description:
      "Official modern numeric tool through real Agent discovery, selected schema and approval",
  },
);

export function selectScenarios(suite) {
  if (suite === "mcp-foundation")
    return scenarios.filter((scenario) =>
      ["mcp", "mcp-auth"].includes(scenario.suite),
    );
  if (suite === "native-auth-schedules")
    return scenarios.filter((scenario) =>
      [
        "native.auth-cancellation",
        "native.auth-switch",
        "native.auth-failed-switch",
        "native.auth-expiry",
      ].includes(scenario.id),
    );
  if (
    ![
      "smoke",
      "lifecycle",
      "idle",
      "native-failures",
      "native-foundation",
      "native-auth",
      "mcp",
      "mcp-live",
      "mcp-auth",
      "native-installation",
      "registry-recovery",
      "store",
      "all",
    ].includes(suite)
  )
    throw new Error(`Unknown suite: ${suite}`);
  return scenarios.filter(
    (scenario) =>
      (suite === "all" &&
        !["mcp-conformance", "mcp-live"].includes(scenario.suite)) ||
      (suite === "registry-recovery" &&
        scenario.id === "native.registry-refusal") ||
      scenario.suite === suite ||
      (suite === "lifecycle" && scenario.suite === "smoke"),
  );
}

scenarios.push(
  {
    id: "mcp.auth-fixture",
    suite: "mcp-auth",
    checks: [12, 13, 17, 31],
    description:
      "Actual SDK discovery/DCR/PKCE/vault, approved A/B nested reads, old account refusal and restart; controlled provider prerequisite",
  },
  {
    id: "mcp.auth-denied-cancelled",
    suite: "mcp-auth",
    checks: [9],
    description:
      "Denied/cancelled SDK login, actual reusable callback listener, zero token exchange and fresh authenticated reads",
  },
  {
    id: "mcp.auth-late-callback",
    suite: "mcp-auth",
    checks: [10],
    description:
      "Cancel-sign-in, disable and Developer Mode off refuse issued old callbacks before/after restart; fresh B grant/read recovers",
  },
  {
    id: "mcp.live-read-disable",
    suite: "mcp-live",
    checks: [8],
    description:
      "Live public DeepWiki discovery, approved fixed-repository read, disabled stale/fresh refusal and restart recovery",
  },
  {
    id: "mcp.live-response-recovery",
    suite: "mcp-live",
    checks: [20],
    description:
      "Live public normal/large documentation reads through production bounds, actual Agent approval and fresh recovery",
  },
  {
    id: "native.auth-cancellation",
    suite: "native-auth",
    checks: [42],
    description:
      "Six real callback/held-exchange cancellation schedules: reload, disable, disconnect, late callback refusal and fresh restart reads",
  },
  {
    id: "native.auth-switch",
    suite: "native-auth",
    checks: [43],
    description:
      "Explicit disconnect A / connect B, stale approval refuses without dispatch, actual B read and restart",
  },
  {
    id: "native.auth-failed-switch",
    suite: "native-auth",
    checks: [50],
    description:
      "Cancelled, denied and failed code exchange preserve selected A across actual reads/restarts; successful B switch refuses stale approval",
  },
  {
    id: "native.auth-expiry",
    suite: "native-auth",
    checks: [51],
    description:
      "Partial consent refused, real short-lived grants expire without refresh and reconnect, real refresh recovers once without login",
  },
);

scenarios.push({
  id: "native.auth-refresh-logout",
  suite: "native-auth",
  checks: [44],
  description:
    "Actual held refresh cannot restore logout; reconnect/restart identify replacement account; a second provider reads independently in the same host",
});

scenarios.push(
  {
    id: "mcp.transport-contract",
    suite: "mcp",
    checks: [8, 17],
    description:
      "Real MCP JSON/SSE stateless discovery and legacy handshake, nested wire/results, approval, restart and stale disable refusal (controlled peer only)",
  },
  {
    id: "mcp.mixed-catalog",
    suite: "mcp",
    checks: [23],
    description:
      "Real MCP mixed pages, unsupported selection/empty catalog, repeated cursor refusal, changed approval and fresh recovery",
  },
  {
    id: "mcp.response-preview",
    suite: "mcp",
    checks: [20],
    description:
      "Real JSON/SSE large UTF-8 and structured previews retain explicit byte bounds, omission notices and fresh-call recovery",
  },
  {
    id: "mcp.transport-bounds",
    suite: "mcp",
    checks: [20],
    description:
      "Separate modern/legacy declared/chunked JSON, SSE data/comments, error-body overflow and dropped replies classify unknown, never replay and recover",
  },
  {
    id: "mcp.close-cancellation",
    suite: "mcp",
    checks: [31],
    description:
      "Modern/legacy JSON/SSE approved held calls and pending approvals close through real Agent UI; late replies cannot contaminate fresh sessions",
  },
  {
    id: "mcp.catalog-budgets",
    suite: "mcp",
    checks: [20, 23],
    description:
      "Management Test and actual Agent reject tool/duplicate/cursor/page/metadata/cumulative-wire limits independently, expose no actions and recover",
  },
  {
    id: "mcp.http-deadline",
    suite: "mcp",
    checks: [20, 31],
    description:
      "Real 45-second JSON/SSE HTTP deadlines in modern/legacy approved reads remain unknown without replay, release replies and recover",
  },
  {
    id: "mcp.discovery-deadline",
    suite: "mcp",
    checks: [20, 31],
    description:
      "Actual 90-second discovery deadline bounds successive sub-45s pages across modern/legacy, dispatches nothing, retires timers/sessions and recovers",
  },
);
