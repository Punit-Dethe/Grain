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

export function selectScenarios(suite) {
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
      "native-installation",
      "registry-recovery",
      "store",
      "all",
    ].includes(suite)
  )
    throw new Error(`Unknown suite: ${suite}`);
  return scenarios.filter(
    (scenario) =>
      suite === "all" ||
      (suite === "registry-recovery" &&
        scenario.id === "native.registry-refusal") ||
      scenario.suite === suite ||
      (suite === "lifecycle" && scenario.suite === "smoke"),
  );
}

scenarios.push(
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
