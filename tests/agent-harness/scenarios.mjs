// Stable IDs are report contracts. Numbered checks below are supporting coverage,
// not permission to change the manual acceptance ledger automatically.
export const scenarios = [
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
];

export function selectScenarios(suite) {
  if (!["smoke", "lifecycle", "idle", "native-failures", "all"].includes(suite))
    throw new Error(`Unknown suite: ${suite}`);
  return scenarios.filter(
    (scenario) =>
      suite === "all" ||
      scenario.suite === suite ||
      (suite === "lifecycle" && scenario.suite === "smoke"),
  );
}
