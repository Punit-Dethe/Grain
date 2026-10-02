#!/usr/bin/env node
// Real WebView2 only. Never starts Chromium or installs mock Tauri APIs.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  writeFile,
  copyFile,
  rm,
} from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { createServer, connect } from "node:net";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { chromium } from "@playwright/test";
import { scenarios, selectScenarios } from "./scenarios.mjs";
import { installationHandlers } from "./installation.mjs";
import { storeHandlers } from "./store.mjs";
import { registryHandlers } from "./registry.mjs";
import { foundationHandlers } from "./foundation.mjs";
import { authenticationHandlers } from "./authentication.mjs";
import { accountOwnershipHandlers } from "./auth-ownership.mjs";
import { accountRefreshHandlers } from "./auth-refresh.mjs";
import { accountScheduleHandlers } from "./auth-schedules.mjs";
import { startAuthFixture } from "./auth-fixture.mjs";
import { startMcpFixture } from "./mcp-fixture.mjs";
import { mcpHandlers } from "./mcp.mjs";
import { startStore, STORE_PUBLIC_KEY } from "./store-fixture.mjs";
import { startModel } from "./model.mjs";
import { developerReload } from "./developer.mjs";
import { verifyBuild, verifyCli, runnerFingerprint } from "./stamp.mjs";
import {
  waitFor as poll,
  Blocked,
  assertWithin,
  writeReport,
} from "./support.mjs";

const exec = promisify(execFile);
const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
const options = { suite: "smoke" };
for (let i = 2; i < process.argv.length; i++) {
  const option = process.argv[i];
  if (["--list", "--help", "--focus-click"].includes(option))
    options[option.slice(2)] = true;
  else if (
    ["--suite", "--scenario", "--binary", "--output", "--fault"].includes(
      option,
    )
  ) {
    if (!process.argv[i + 1] || process.argv[i + 1].startsWith("--"))
      throw new Error(`${option} requires a value`);
    options[option.slice(2)] = process.argv[++i];
  } else throw new Error(`Unknown option: ${option}`);
}
if (options.help) {
  console.log(
    "node tests/agent-harness/run.mjs [--list] [--suite smoke|lifecycle|idle|native-failures|native-foundation|native-auth|native-auth-schedules|mcp|native-installation|registry-recovery|store|all] [--scenario ID] [--binary path] [--output directory] [--focus-click]\nWindows real Agent/WebView2 acceptance. --focus-click exercises owned native header activation only for agent.reopen-escape. Build first with tests/agent-harness/build.ps1. No live account or model key required. native-auth and mcp additionally require Python cryptography for run-local TLS.",
  );
  process.exit(0);
}
if (options.list) {
  for (const scenario of scenarios)
    console.log(`${scenario.id} [${scenario.suite}] ${scenario.description}`);
  process.exit(0);
}
const selected = options.scenario
  ? scenarios.filter((scenario) => scenario.id === options.scenario)
  : selectScenarios(options.suite);
if (!selected.length) throw new Error(`Unknown scenario: ${options.scenario}`);
if (options["focus-click"] && options.scenario !== "agent.reopen-escape")
  throw new Error("--focus-click requires --scenario agent.reopen-escape");
if (
  options.fault &&
  ![
    "wrong-greeting",
    "missing-escape",
    "successful-error",
    "skip-restart",
    "stale-package",
    "unclosed-store",
    "lost-pointer",
    "stringified-number",
    "lost-migration-archive",
    "wrong-account",
    "abandoned-auth",
    "wrong-owner",
    "unchanged-auth-declaration",
    "uncancelled-login",
    "accepted-partial-consent",
    "released-refresh",
    "wrong-mcp-type",
    "supported-mcp-excluded",
    "short-mcp-preview",
    "accepted-mcp-catalog",
  ].includes(options.fault)
)
  throw new Error("Unknown oracle fault");
for (const [fault, scenario] of [
  ["wrong-mcp-type", "mcp.transport-contract"],
  ["supported-mcp-excluded", "mcp.mixed-catalog"],
  ["short-mcp-preview", "mcp.response-preview"],
  ["accepted-mcp-catalog", "mcp.catalog-budgets"],
  ["released-refresh", "native.auth-refresh-logout"],
  ["uncancelled-login", "native.auth-cancellation"],
  ["accepted-partial-consent", "native.auth-expiry"],
]) {
  if (options.fault === fault && options.scenario !== scenario)
    throw new Error(`${fault} requires --scenario ${scenario}`);
}
if (
  options.fault === "wrong-owner" &&
  options.scenario !== "native.auth-owners"
)
  throw new Error("wrong-owner requires --scenario native.auth-owners");
if (
  options.fault === "unchanged-auth-declaration" &&
  options.scenario !== "native.auth-binding"
)
  throw new Error(
    "unchanged-auth-declaration requires --scenario native.auth-binding",
  );
if (
  options.fault === "wrong-account" &&
  options.scenario !== "native.auth-fixture"
)
  throw new Error("wrong-account requires --scenario native.auth-fixture");
if (
  options.fault === "abandoned-auth" &&
  options.scenario !== "native.auth-fixture"
)
  throw new Error("abandoned-auth requires --scenario native.auth-fixture");
if (
  options.fault === "stringified-number" &&
  options.scenario !== "native.typed-contract"
)
  throw new Error(
    "stringified-number requires --scenario native.typed-contract",
  );
if (
  options.fault === "lost-migration-archive" &&
  options.scenario !== "native.legacy-migration"
)
  throw new Error(
    "lost-migration-archive requires --scenario native.legacy-migration",
  );
if (
  options.fault === "missing-escape" &&
  options.scenario !== "agent.reopen-escape"
)
  throw new Error("missing-escape requires --scenario agent.reopen-escape");
if (
  options.fault === "successful-error" &&
  options.scenario !== "native.reply-failures"
)
  throw new Error("successful-error requires --scenario native.reply-failures");
if (
  options.fault === "skip-restart" &&
  options.scenario !== "native.consent-persistence"
)
  throw new Error(
    "skip-restart requires --scenario native.consent-persistence",
  );
if (
  options.fault === "stale-package" &&
  options.scenario !== "native.cli-package-ownership"
)
  throw new Error(
    "stale-package requires --scenario native.cli-package-ownership",
  );
const runId = randomUUID();
if (
  options.fault === "lost-pointer" &&
  options.scenario !== "native.registry-preservation"
)
  throw new Error(
    "lost-pointer requires --scenario native.registry-preservation",
  );
if (
  options.fault === "unclosed-store" &&
  options.scenario !== "store.close-offline"
)
  throw new Error("unclosed-store requires --scenario store.close-offline");
const output = resolve(options.output ?? join(here, ".runs"));
await mkdir(output, { recursive: true });
const root = await mkdtemp(join(output, "run-"));
const report = {
  schema: 1,
  runId,
  evidenceClass: "real-application/scripted-model",
  startedAt: new Date().toISOString(),
  platform: process.platform,
  suite: options.scenario ? "single" : options.suite,
  requestedScenario: options.scenario ?? null,
  attempt: 1,
  adapter: {
    kind: "webview2-cdp",
    node: process.version,
    architecture: process.arch,
    focusClick: Boolean(options["focus-click"]),
  },
  commit: "unknown",
  dirtyFiles: [],
  results: [],
  cleanup: { status: "Not run" },
  limitations: [
    "Typed instruction seam skips summon shortcut/OS capture/microphone.",
    "Native pill is not launched. Recording/pill/shared-input portion of check 37 is not covered.",
    "Local scripted provider tests deterministic Agent mechanics, not live-model relevance or provider account compatibility.",
    "Checks in scenario metadata are partial supporting coverage; manual ledger is unchanged.",
  ],
};
let child, browser, model, main, store, authProvider, mcpProvider;
let interrupted = false;
const cancellation = new AbortController();
const waitFor = (description, operation, options = {}) =>
  poll(description, operation, { signal: cancellation.signal, ...options });
let logTail = "";
const redact = (text) =>
  text
    .replace(/\b[a-f0-9]{64}\b/gi, "[redacted]")
    .replace(/Bearer\s+\S+/gi, "Bearer [redacted]");
const appendLog = (chunk) => {
  logTail = (logTail + redact(chunk.toString())).slice(-65536);
};
const shutdownSignal = () => {
  interrupted = true;
  cancellation.abort(new Error("Run interrupted"));
};
process.once("SIGINT", shutdownSignal);
process.once("SIGTERM", shutdownSignal);

async function portOpen(port) {
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port });
    socket.setTimeout(300);
    socket.once("connect", () => {
      socket.destroy();
      resolve(true);
    });
    socket.once("error", () => {
      socket.destroy();
      resolve(false);
    });
    socket.once("timeout", () => {
      socket.destroy();
      resolve(false);
    });
  });
}
async function unusedPort() {
  const listener = createServer();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolve);
  });
  const port = listener.address().port;
  await new Promise((resolve) => listener.close(resolve));
  return port;
}
async function invoke(command, args = {}) {
  if (interrupted && command !== "agent_harness_shutdown")
    throw new Error("Run interrupted");
  if (!main || child.exitCode !== null || child.signalCode !== null)
    throw new Error("Harness host is not running");
  let deadline;
  try {
    return await Promise.race([
      main.evaluate(
        ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
        { command, args },
      ),
      new Promise((_, reject) => {
        deadline = setTimeout(
          () =>
            reject(new Error(`${command} exceeded its 10-second IPC deadline`)),
          10000,
        );
      }),
    ]);
  } finally {
    clearTimeout(deadline);
  }
}
const priorEvents = [];
let hostSession = 0;
const status = async () => {
  const value = await invoke("agent_harness_status");
  value.events = [
    ...priorEvents,
    ...value.events.map((event) => ({ ...event, session: hostSession })),
  ];
  assert.ok(value.events.length <= 4096, "Restart evidence buffer overflowed");
  assert.ok(value.events.length <= 4096, "Restart evidence buffer overflowed");
  value.hostSession = hostSession;
  return value;
};
const fixture = (operation) => invoke("agent_harness_fixture", { operation });
const events = (snapshot, phase, action) =>
  snapshot.events.filter(
    (event) => event.phase === phase && (!action || event.action === action),
  );
async function findWindow(label) {
  for (const context of browser.contexts()) {
    for (const page of context.pages()) {
      if (page.isClosed()) continue;
      try {
        const currentLabel = await page.evaluate(
          () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
        );
        if (
          typeof label === "string"
            ? currentLabel === label
            : label.test(currentLabel ?? "")
        )
          return page;
      } catch {
        /* target still initializes */
      }
    }
  }
  return null;
}
async function panel() {
  return waitFor("Agent panel", () => findWindow("agent-panel"));
}
async function activate(locator) {
  await locator.waitFor({ state: "visible", timeout: 10000 });
  await waitFor("Enabled Agent control", () => locator.isEnabled());
  // WebView2 screen coordinates can race native resize/entrance animation.
  // Activate the actual enabled DOM button and its production React handler.
  await locator.evaluate((button) => button.click());
}
async function closePanel() {
  const page = await findWindow("agent-panel");
  if (!page) return;
  await activate(page.locator("button.agc-c-x, button.agc-close"));
  await waitFor(
    "Agent window destruction",
    async () => page.isClosed() || !(await findWindow("agent-panel")),
  );
  await waitFor(
    "Agent run release",
    async () =>
      !(await status()).agent.active && !(await status()).agent.pendingApproval,
  );
}
async function request(instruction = "hello") {
  await closePanel();
  await invoke("agent_harness_submit", { instruction });
  const page = await panel();
  await page
    .locator(".agc-confirm-actions .agc-action-btn")
    .waitFor({ state: "visible", timeout: 20000 });
  assert.equal(
    (await status()).agent.pendingApproval,
    true,
    "Host must own the approval before clicking it",
  );
  return page;
}
async function greeting(revision = "one") {
  const before = await status();
  const page = await request();
  assert.equal(
    events(await status(), "dispatched").length,
    events(before, "dispatched").length,
    "Tool dispatched before approval",
  );
  await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
  await waitFor("Approved greeting", async () => {
    const snapshot = await status();
    return (
      events(snapshot, "completed", "hello").length ===
        events(before, "completed", "hello").length + 1 &&
      !snapshot.agent.active
    );
  });
  await page
    .getByText(`Harness hello (${revision})`, { exact: false })
    .first()
    .waitFor({ state: "visible", timeout: 15000 });
  const after = await status();
  assert.equal(
    events(after, "dispatched", "hello").length,
    events(before, "dispatched", "hello").length + 1,
    "Approval must dispatch once",
  );
  assert.equal(
    after.worker.current.pending,
    0,
    "Worker retained a pending reply after completion",
  );
  return after;
}
async function slowStart() {
  const before = await status();
  const page = await request("slow_hello");
  await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
  const active = await waitFor("Slow native dispatch", async () => {
    const snapshot = await status();
    return (
      events(snapshot, "dispatched", "slow_hello").length ===
        events(before, "dispatched", "slow_hello").length + 1 &&
      snapshot.worker.current?.pending === 1 &&
      snapshot
    );
  });
  return { before, active, page };
}

async function failureCall(
  action,
  expectedOutcome,
  { retire = true, uiText } = {},
) {
  const before = await status();
  const modelStart = model.journal.length;
  const page = await request(action);
  const approvedAt = performance.now();
  await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
  const after = await waitFor(
    `${action} classified and Agent released`,
    async () => {
      const snapshot = await status();
      return (
        events(snapshot, "outcome", action).length ===
          events(before, "outcome", action).length + 1 &&
        !snapshot.agent.active &&
        !snapshot.agent.pendingApproval &&
        snapshot
      );
    },
    { timeoutMs: 30000 },
  );
  const outcome = events(after, "outcome", action).at(-1);
  assert.equal(
    outcome.outcome,
    expectedOutcome,
    `${action} reported the wrong certainty`,
  );
  const dispatches = events(after, "dispatched", action).slice(
    events(before, "dispatched", action).length,
  );
  assert.equal(
    dispatches.length,
    1,
    `${action} must dispatch once, with no automatic retry`,
  );
  const worker = dispatches[0].worker;
  if (retire) {
    assert.equal(
      after.worker.current,
      null,
      `${action} retained the failed worker`,
    );
    assert.equal(
      after.tokenCount,
      baselineTokens,
      `${action} leaked its token`,
    );
    assert.ok(
      events(after, "retired").some((event) => event.worker === worker),
      `${action} failed to retire its exact owner`,
    );
    await waitFor(
      "Retired native supervisor destruction",
      async () => !(await findWindow(/^extension-host-/)),
    );
  } else {
    assert.equal(
      after.worker.current.pending,
      0,
      `${action} retained a pending reply`,
    );
  }
  assert.equal(
    after.fixtureEnabled,
    true,
    "Ordinary failure disabled the extension",
  );
  assert.equal(
    model.journal
      .slice(modelStart)
      .flatMap((entry) => entry.returned)
      .filter((name) => name.startsWith("act__")).length,
    1,
    "Model action was silently replayed",
  );
  const text = await page.locator("body").innerText();
  assert.ok(
    !text.includes("HARNESS_PRIVATE_ERROR_MARKER"),
    "Private worker error leaked into the UI",
  );
  if (uiText)
    assert.ok(
      text.includes(uiText),
      `${action} omitted its truthful visible notice`,
    );
  const invoked = events(after, "invoked", action).at(-1);
  return {
    before,
    after,
    worker,
    page,
    outcome,
    invoked,
    approvedAt,
    elapsedAfterApprovalMs: performance.now() - approvedAt,
  };
}
const inputEvidence = [];
function takeInputEvidence() {
  return inputEvidence.splice(0);
}
async function escapePanel(page) {
  if (options.fault !== "missing-escape") {
    let output;
    let failure;
    try {
      output = await exec(
        "powershell.exe",
        [
          "-NoProfile",
          "-File",
          join(here, "native-input.ps1"),
          "-OwnerPid",
          String(child.pid),
          ...(options["focus-click"] ? ["-FocusByClick"] : []),
        ],
        { timeout: 10000, windowsHide: true, maxBuffer: 16384 },
      );
    } catch (error) {
      output = error;
      failure = error;
    }
    // Preserve guarded refusals as well as accepted input. A successful SendInput
    // call is not proof that the shortcut handler received the key.
    let observation;
    try {
      observation = JSON.parse(String(output.stdout ?? "").trim());
    } catch (error) {
      inputEvidence.push({
        schema: 1,
        kind: "native-escape-diagnostic-missing",
      });
      // Compilation/startup failures may predate the adapter's finally block.
      // Retain their original error rather than replacing it with a JSON error.
      throw failure ?? error;
    }
    assert.equal(observation.schema, 1);
    assert.equal(observation.kind, "native-escape");
    assert.ok(inputEvidence.length < 64, "Native input evidence overflowed");
    inputEvidence.push(observation);
    if (failure) throw failure;
    assert.equal(observation.accepted, 2);
    assert.equal(observation.foregroundBefore, true);
    assert.equal(observation.heldModifiers, "");
    assert.equal(observation.escapeHeld, false);
    if (options["focus-click"]) {
      assert.equal(observation.focusClickAccepted, 2);
      assert.equal(observation.physicalCoordinates, true);
    }
  } else {
    inputEvidence.push({ kind: "native-escape-withheld", schema: 1 });
  }
  await waitFor("Escape window destruction", () => page.isClosed(), {
    timeoutMs: 8000,
  });
  await waitFor("Escape releases Agent", async () => {
    const value = await status();
    return !value.agent.active && !value.agent.pendingApproval;
  });
}
async function noLateSuccess(before, worker) {
  // Wait beyond the fixture's real 15-second handler, not a shortened fake clock.
  const started = performance.now();
  await waitFor(
    "Interrupted-call observation window",
    () => performance.now() - started >= 16000,
    { timeoutMs: 18000, intervalMs: 250 },
  );
  const snapshot = await status();
  assert.equal(
    events(snapshot, "completed", "slow_hello").length,
    events(before, "completed", "slow_hello").length,
    "Interrupted call reported a late success",
  );
  assert.equal(
    events(snapshot, "dispatched", "slow_hello").length,
    events(before, "dispatched", "slow_hello").length + 1,
    "Interrupted call was automatically repeated",
  );
  assert.ok(
    events(snapshot, "retired").some((event) => event.worker === worker),
    "Interrupted generation did not retire",
  );
}
async function changeRevision(revision) {
  const source = await readFile(
    join(here, "fixtures/lifecycle/main.js"),
    "utf8",
  );
  const fixtureSource = source.replace('"one"', JSON.stringify(revision));
  await writeFile(
    assertWithin(root, join(root, "fixture/dist/main.js")),
    options.fault === "wrong-greeting"
      ? fixtureSource.replace(
          "Harness hello (",
          "Intentional incorrect greeting (",
        )
      : fixtureSource,
  );
  if (options.fault === "successful-error") {
    const path = assertWithin(root, join(root, "fixture/dist/main.js"));
    await writeFile(
      path,
      (await readFile(path, "utf8")).replace(
        'error: { class: "network", message: "HARNESS_PRIVATE_ERROR_MARKER" }',
        'ok: { body: "Intentional error-to-success oracle fault" }',
      ),
    );
  }
  if (options.fault === "stringified-number") {
    const path = assertWithin(root, join(root, "fixture/dist/main.js"));
    await writeFile(
      path,
      (await readFile(path, "utf8")).replace(
        "JSON.stringify(args)",
        "JSON.stringify({ ...args, count: String(args.count) })",
      ),
    );
  }
}

const handlers = {
  async "native.reply-failures"() {
    for (const [action, classification, retire, uiText] of [
      [
        "tool_error",
        "toolReportedError",
        false,
        "Partial effects may have occurred",
      ],
      [
        "thrown_error",
        "toolReportedError",
        false,
        "Partial effects may have occurred",
      ],
      ["malformed_result", "resultUnavailable", false, "invalid action result"],
      ["lost_reply", "unknownOutcome", true, "could not confirm its result"],
    ]) {
      const call = await failureCall(action, classification, {
        retire,
        uiText,
      });
      const fresh = await greeting();
      if (retire) assert.notEqual(fresh.worker.current.identity, call.worker);
    }
  },
  async "native.readiness-failure"() {
    await writeFile(
      assertWithin(root, join(root, "fixture/dist/main.js")),
      "self.close();\n",
    );
    const before = await status();
    const page = await request();
    await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    const after = await waitFor(
      "Unready worker failure",
      async () => {
        const value = await status();
        return (
          events(value, "outcome", "hello").length ===
            events(before, "outcome", "hello").length + 1 &&
          !value.agent.active &&
          value
        );
      },
      { timeoutMs: 10000 },
    );
    assert.equal(events(after, "outcome", "hello").at(-1).outcome, "failed");
    assert.equal(
      events(after, "dispatched").length,
      events(before, "dispatched").length,
      "Unready worker dispatched a tool",
    );
    const spawned = events(after, "spawned").slice(
      events(before, "spawned").length,
    );
    assert.equal(spawned.length, 1, "Startup was retried or never attempted");
    assert.ok(
      events(after, "retired").some(
        (event) => event.worker === spawned[0].worker,
      ),
    );
    assert.equal(after.worker.current, null);
    assert.equal(after.tokenCount, baselineTokens);
    assert.equal(after.fixtureEnabled, true);
    await waitFor(
      "Unready supervisor destruction",
      async () => !(await findWindow(/^extension-host-/)),
    );
    assert.match(await page.locator("body").innerText(), /not dispatched/);
    await changeRevision("one");
    await greeting();
  },
  async "native.source-drift"() {
    const warm = await greeting();
    const before = await status();
    const page = await request();
    await changeRevision("two"); // Deliberately no load or developer reload.
    await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await waitFor(
      "Source drift refusal",
      async () => !(await status()).agent.active,
    );
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
      "Old approval executed changed source",
    );
    assert.match(
      await page.locator("body").innerText(),
      /no longer approved or available|changed/,
    );
    const fresh = await greeting("two");
    assert.notEqual(
      fresh.worker.current.identity,
      warm.worker.current.identity,
      "New source used the obsolete warm worker",
    );
  },
  async "native.absolute-deadline"() {
    const call = await failureCall("deadline_hello", "unknownOutcome", {
      uiText: "could not confirm its result",
    });
    const elapsed = call.outcome.elapsedMs - call.invoked.elapsedMs;
    assert.ok(
      elapsed >= 19500 && elapsed <= 23000,
      `Native absolute deadline took ${elapsed}ms`,
    );
    assert.ok(
      call.elapsedAfterApprovalMs <= 24000,
      "Approval-to-failure exceeded its local acceptance tolerance",
    );
    console.log(
      `OBSERVED native absolute deadline ${elapsed}ms from invocation`,
    );
    const fresh = await greeting();
    assert.notEqual(fresh.worker.current.identity, call.worker);
    // Observe beyond the original handler's 25s deadline, including recovery.
    await waitFor(
      "Late deadline observation",
      () => performance.now() - call.approvedAt >= 27000,
      { timeoutMs: 9000, intervalMs: 200 },
    );
    const after = await status();
    assert.equal(
      events(after, "dispatched", "deadline_hello").length,
      events(call.before, "dispatched", "deadline_hello").length + 1,
    );
    assert.equal(
      events(after, "completed", "deadline_hello").length,
      events(call.before, "completed", "deadline_hello").length,
    );
    assert.equal(
      after.worker.current.identity,
      fresh.worker.current.identity,
      "Old timeout cleanup killed the replacement",
    );
  },
  async "native.result-budgets"() {
    for (const [action, classification] of [
      ["oversized_result", "resultUnavailable"],
      ["oversized_raw", "unknownOutcome"],
    ]) {
      const call = await failureCall(action, classification, {
        uiText:
          action === "oversized_raw"
            ? "could not confirm its result"
            : "exceeding its supported budget",
      });
      const fresh = await greeting();
      assert.notEqual(fresh.worker.current.identity, call.worker);
    }
  },
  async "native.invalid-input"() {
    for (const instruction of [
      "invalid_arguments",
      "missing_arguments",
      "wrong_arguments",
      "oversized_arguments",
      "malformed_arguments",
    ]) {
      await closePanel();
      const before = await status();
      const modelStart = model.journal.length;
      await invoke("agent_harness_submit", { instruction });
      const page = await panel();
      await waitFor(
        "Invalid-input Agent completion",
        async () =>
          model.journal.length >= modelStart + 3 &&
          !(await status()).agent.active,
      );
      const after = await status();
      assert.equal(after.agent.pendingApproval, false);
      assert.equal(
        after.worker.current,
        null,
        "Invalid arguments woke a worker",
      );
      assert.equal(
        events(after, "dispatched").length,
        events(before, "dispatched").length,
      );
      const text = await page.locator("body").innerText();
      assert.match(
        text,
        /undeclared parameter|arguments|Invalid or repeated model tool-call identity/,
      );
      assert.ok(
        !text.includes("HARNESS_PRIVATE_ARGUMENT_MARKER"),
        "Rejected parameter key leaked into the UI",
      );
    }
    await greeting();
  },
  async "native.cold-warm"() {
    const cold = await status();
    assert.equal(cold.worker.current, null);
    const first = await greeting();
    const second = await greeting();
    assert.equal(
      second.worker.current.identity,
      first.worker.current.identity,
      "Warm greeting restarted the worker",
    );
  },
  async "agent.decline"() {
    const before = await status();
    const page = await request();
    await activate(page.locator(".agc-confirm-actions .agc-cancel-btn"));
    await waitFor("Decline release", async () => {
      const value = await status();
      return !value.agent.active && !value.agent.pendingApproval;
    });
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
    );
    await greeting();
  },
  async "agent.close-pending"() {
    const before = await status();
    await request();
    await closePanel();
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
    );
    await greeting();
  },
  async "agent.typed-approval"() {
    const before = await status();
    const page = await request();
    await activate(page.locator("button.agc-c-followup"));
    const input = page.locator("textarea").first();
    await input.fill("yes");
    // Like button activation, exercise the real React handler without a CDP
    // native focus/resize race. Native Escape has its own OS input case.
    await input.evaluate((element) =>
      element.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Enter",
          code: "Enter",
          bubbles: true,
          cancelable: true,
        }),
      ),
    );
    await waitFor("Typed approval completion", async () => {
      const value = await status();
      return (
        events(value, "completed", "hello").length ===
          events(before, "completed", "hello").length + 1 && !value.agent.active
      );
    });
    assert.equal(
      events(await status(), "dispatched", "hello").length,
      events(before, "dispatched", "hello").length + 1,
    );
  },
  async "agent.stale-approval"() {
    const before = await status();
    const page = await request();
    await changeRevision("two");
    await developerReload(root);
    await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await waitFor("Stale approval release", async () => {
      const value = await status();
      return !value.agent.active && !value.agent.pendingApproval;
    });
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
      "Stale approval dispatched changed code",
    );
    await greeting("two");
  },
  async "agent.escape-slow"() {
    const slow = await slowStart();
    await escapePanel(slow.page);
    await greeting();
    await noLateSuccess(slow.before, slow.active.worker.current.identity);
  },
  async "agent.reopen-escape"() {
    const before = await status();
    for (let cycle = 0; cycle < 10; cycle++) {
      // No added handoff sleep: close/reopen exercises deferred OS cleanup.
      await request();
      await closePanel();
      const replacement = await request();
      await escapePanel(replacement);
      assert.equal(
        events(await status(), "dispatched").length,
        events(before, "dispatched").length,
        "Closing a replacement approval dispatched a tool",
      );
    }
    await greeting();
  },
  async "agent.close-model"() {
    await invoke("agent_harness_submit", { instruction: "model_wait" });
    const page = await panel();
    await waitFor(
      "Delayed provider request",
      () => model.journal.at(-1)?.state === "received",
    );
    await closePanel();
    await waitFor("Provider cancellation", () =>
      model.journal.some((entry) => entry.state === "cancelled"),
    );
    assert.equal(page.isClosed(), true);
    await greeting();
  },
  async "agent.close-slow"() {
    const slow = await slowStart();
    await closePanel();
    const fresh = await greeting();
    assert.notEqual(
      fresh.worker.current.identity,
      slow.active.worker.current.identity,
    );
    await noLateSuccess(slow.before, slow.active.worker.current.identity);
  },
  async "native.ten-replacements"() {
    const generations = new Set();
    for (let cycle = 0; cycle < 10; cycle++) {
      const result = await greeting();
      assert.ok(
        !generations.has(result.worker.current.identity),
        "Replacement reused an old worker token",
      );
      generations.add(result.worker.current.identity);
      await closePanel();
      await fixture("unload");
      await waitFor("Unloaded worker/token cleanup", async () => {
        const value = await status();
        return value.worker.count === 0 && value.tokenCount === baselineTokens;
      });
      await fixture("load");
    }
    await greeting();
  },
  async "native.disable-slow"() {
    const slow = await slowStart();
    await fixture("disable");
    await fixture("enable");
    const fresh = await greeting();
    assert.notEqual(
      fresh.worker.current.identity,
      slow.active.worker.current.identity,
    );
    await noLateSuccess(slow.before, slow.active.worker.current.identity);
    assert.equal(
      (await status()).worker.current.identity,
      fresh.worker.current.identity,
      "Old cleanup killed the replacement",
    );
  },
  async "native.hot-reload"() {
    let last = await greeting();
    for (const revision of ["two", "three"]) {
      await changeRevision(revision);
      const reload = await developerReload(root);
      assert.equal(reload.enabled, true);
      const fresh = await greeting(revision);
      assert.notEqual(
        fresh.worker.current.identity,
        last.worker.current.identity,
      );
      last = fresh;
    }
    const slow = await slowStart();
    await changeRevision("four");
    await developerReload(root);
    await greeting("four");
    await noLateSuccess(slow.before, slow.active.worker.current.identity);
    await fixture("disable");
    await changeRevision("five");
    const disabled = await developerReload(root);
    assert.equal(disabled.enabled, false);
    assert.equal(disabled.restartedWorker, false);
    assert.equal((await status()).worker.current, null);
    await fixture("enable");
    await greeting("five");
  },
  async "native.real-idle"() {
    const warm = await greeting();
    await closePanel();
    const started = performance.now();
    await waitFor(
      "Production idle reaper",
      async () => !(await status()).worker.current,
      { timeoutMs: 170000, intervalMs: 1000 },
    );
    assert.ok(
      performance.now() - started >= 115000,
      "Worker retired substantially before the production idle threshold",
    );
    console.log(
      `OBSERVED idle retirement after ${Math.round(performance.now() - started)}ms`,
    );
    const fresh = await greeting();
    assert.notEqual(
      fresh.worker.current.identity,
      warm.worker.current.identity,
    );
    await closePanel();
    const near = performance.now();
    await waitFor(
      "Near-idle-boundary wait",
      () => performance.now() - near >= 110000,
      { timeoutMs: 115000, intervalMs: 500 },
    );
    const slow = await slowStart();
    console.log(
      "OBSERVED near-boundary slow call dispatch; awaiting its result",
    );
    await waitFor(
      "Near-boundary slow completion",
      async () =>
        events(await status(), "completed", "slow_hello").length ===
        events(slow.before, "completed", "slow_hello").length + 1,
      { timeoutMs: 20000 },
    );
    assert.equal(
      (await status()).worker.current.identity,
      fresh.worker.current.identity,
      "Active call was reaped at the idle boundary",
    );
  },
};
let hostBinary, cliIdentity;
async function launchHost() {
  const binary = hostBinary;
  child = spawn(binary, [], {
    cwd: repo,
    windowsHide: true,
    env: {
      ...process.env,
      GRAIN_AGENT_HARNESS_ROOT: root,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${cdpPort}`,
      WEBVIEW2_USER_DATA_FOLDER: join(root, "data/webview"),
      RUST_LOG:
        "warn,handy_app_lib=info,handy_app_lib::shortcut::handy_keys=debug",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", appendLog);
  child.stderr.on("data", appendLog);
  child.once("error", (error) => {
    logTail += String(error);
  });
  report.hostPid = child.pid;
  browser = await waitFor(
    "Owned WebView2 debugging endpoint",
    async () => {
      if (child.exitCode !== null)
        throw new Blocked(
          `Harness host exited (${child.exitCode}); ${logTail.slice(-1500)}`,
        );
      try {
        return await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`, {
          timeout: 1000,
        });
      } catch {
        return null;
      }
    },
    { timeoutMs: 40000, intervalMs: 250 },
  );
  main = await waitFor("Real main WebView", () => findWindow("main"));
  hostSession++;
  (report.hostPids ??= []).push(child.pid);
  const snapshot = await status();
  assert.equal(snapshot.runId, runId);
  assert.equal(snapshot.applicationId, "com.grain.agent-harness");
  assert.equal(
    resolve(snapshot.profile.replace(/^\\\\\?\\/, "")),
    resolve(root, "data"),
  );
  assert.deepEqual(snapshot.shortcutBindings, []);
}
async function stopHost() {
  const pid = child.pid;
  try {
    await invoke("agent_harness_shutdown");
  } catch {
    /* host exits before IPC reply */
  }
  await waitFor(
    "Owned host exit",
    () => child.exitCode !== null || child.signalCode !== null,
    { timeoutMs: 10000 },
  );
  await browser.close();
  browser = null;
  main = null;
  await waitFor(
    "Old host endpoint release",
    async () => !(await portOpen(cdpPort)) && !(await portOpen(17124)),
    { timeoutMs: 10000 },
  );
  assert.equal(child.exitCode, 0, `Host ${pid} did not exit normally`);
}
async function restartHost(beforeLaunch) {
  if (options.fault === "skip-restart") return;
  const oldSession = hostSession;
  const snapshot = await status();
  priorEvents.splice(0, priorEvents.length, ...snapshot.events);
  await stopHost();
  if (beforeLaunch) await beforeLaunch();
  cdpPort = await unusedPort();
  await launchHost();
  assert.equal(hostSession, oldSession + 1);
  const after = await status();
  assert.equal(after.worker.count, 0);
  assert.equal(after.tokenCount, baselineTokens);
  assert.equal(after.agent.pendingApproval, false);
  assert.equal(after.agent.active, false);
}
const installation = installationHandlers({
  root,
  repo,
  here,
  cli: () => cliIdentity,
  fault: options.fault,
  status,
  fixture,
  request,
  greeting,
  closePanel,
  activate,
  waitFor,
  events,
  restartHost,
  main: () => main,
  log: () => logTail,
});
Object.assign(handlers, installation.handlers);
const storeSuite = storeHandlers({
  root,
  invoke,
  status,
  waitFor,
  fixture,
  greeting,
  restartHost,
  store: () => store,
  allow: installation.allow,
  project: installation.project,
  fault: options.fault,
  main: () => main,
  activate,
});
Object.assign(handlers, storeSuite.handlers);
const registrySuite = registryHandlers({
  root,
  here,
  status,
  fixture,
  greeting,
  restartHost,
  waitFor,
  imported: installation.imported,
  allow: installation.allow,
  fault: options.fault,
});
Object.assign(handlers, registrySuite.handlers);
const foundationSuite = foundationHandlers({
  root,
  here,
  status,
  fixture,
  request,
  greeting,
  activate,
  waitFor,
  events,
  restartHost,
  imported: installation.imported,
  allow: installation.allow,
  model: () => model,
  fault: options.fault,
});
Object.assign(handlers, foundationSuite.handlers);
const authenticationSuite = authenticationHandlers({
  fault: options.fault,
  root,
  invoke,
  main: () => main,
  provider: () => authProvider,
  model: () => model,
  status,
  request,
  activate,
  waitFor,
  events,
  restartHost,
});
Object.assign(handlers, authenticationSuite.handlers);
const accountContext = {
  fault: options.fault,
  root,
  repo,
  invoke,
  main: () => main,
  provider: () => authProvider,
  model: () => model,
  cli: () => cliIdentity,
  vaultCount: async () => {
    const output = await exec(
      "powershell.exe",
      [
        "-NoProfile",
        "-File",
        join(here, "auth-cleanup.ps1"),
        "-Root",
        root,
        "-RunId",
        runId,
        "-InventoryOnly",
      ],
      { timeout: 10000, windowsHide: true, maxBuffer: 4096 },
    );
    const value = JSON.parse(output.stdout.trim());
    assert.equal(value.kind, "native-vault-inventory");
    assert.ok(
      Number.isInteger(value.count) && value.count >= 0 && value.count <= 256,
    );
    return value.count;
  },
  closePanel,
  panel,
  status,
  request,
  activate,
  waitFor,
  events,
  restartHost,
};
const accountOwnershipSuite = accountOwnershipHandlers(accountContext);
Object.assign(handlers, accountOwnershipSuite.handlers);
const accountSchedules = accountScheduleHandlers(
  accountContext,
  accountOwnershipSuite.controls,
);
Object.assign(handlers, accountSchedules.handlers);
const accountRefresh = accountRefreshHandlers(
  accountContext,
  accountOwnershipSuite.controls,
);
Object.assign(handlers, accountRefresh.handlers);
const mcpSuite = mcpHandlers({
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  panel,
  provider: () => mcpProvider,
  model: () => model,
  log: () => logTail,
});
Object.assign(handlers, mcpSuite.handlers);
let baselineTokens = 0;
let cdpPort;
try {
  const version = await exec("git", ["rev-parse", "HEAD"], { cwd: repo });
  report.commit = version.stdout.trim();
  report.runnerFingerprint = await runnerFingerprint();
  const dirty = await exec("git", ["status", "--porcelain"], { cwd: repo });
  report.dirtyFiles = dirty.stdout.trim().split(/\r?\n/).filter(Boolean);
  if (process.platform !== "win32")
    throw new Blocked(
      "This real-application adapter requires Windows WebView2; other platforms have no adapter yet",
    );
  let binary = options.binary;
  if (!binary) {
    const metadata = await exec(
      "cargo",
      ["metadata", "--format-version", "1", "--no-deps"],
      { cwd: join(repo, "src-tauri"), maxBuffer: 4 * 1024 * 1024 },
    );
    binary = join(
      JSON.parse(metadata.stdout).target_directory,
      "debug/grain-agent-harness.exe",
    );
  }
  binary = resolve(binary);
  if (basename(binary).toLowerCase() !== "grain-agent-harness.exe")
    throw new Blocked(
      "Only the dedicated grain-agent-harness.exe may be launched",
    );
  try {
    await access(binary);
  } catch {
    throw new Blocked(
      "Harness executable is missing; run tests/agent-harness/build.ps1 first",
    );
  }
  if (await portOpen(17124))
    throw new Blocked(
      "Harness events port 17124 is already in use; run this suite serially",
    );
  let build;
  try {
    build = await verifyBuild(binary);
  } catch (error) {
    throw new Blocked(`${error.message}. Run tests/agent-harness/build.ps1.`);
  }
  report.binary = {
    path: binary,
    sha256: build.binarySha256,
    sourceFingerprint: build.sourceFingerprint,
    buildCommit: build.commit,
    builtAt: build.builtAt,
  };
  if (
    selected.some((scenario) =>
      [
        "native.cli-package-ownership",
        "native.auth-owners",
        "native.auth-cancellation",
      ].includes(scenario.id),
    )
  ) {
    try {
      cliIdentity = await verifyCli();
    } catch (error) {
      throw new Blocked(`${error.message}. Run tests/agent-harness/build.ps1.`);
    }
    report.cli = cliIdentity;
  }
  report.injectedFault = options.fault ?? null;
  model = await startModel();
  if (selected.some((scenario) => scenario.suite === "store")) {
    store = await startStore(here);
    report.storeFixture = {
      port: store.port,
      publicKey: STORE_PUBLIC_KEY,
      limitations: [
        "Fixed test publishing anchor; production root signatures/rotation excluded.",
      ],
    };
  }
  await writeFile(
    join(root, ".grain-agent-harness.json"),
    JSON.stringify({
      schema: 1,
      runId,
      modelPort: model.port,
      storePort: store?.port,
    }),
  );
  for (const [suite, purpose] of [
    ["native-auth", "native"],
    ["mcp", "mcp"],
  ]) {
    if (!selected.some((scenario) => scenario.suite === suite)) continue;
    try {
      await exec("python", [join(here, "auth-tls.py"), root, purpose], {
        timeout: 10000,
        windowsHide: true,
        maxBuffer: 4096,
      });
    } catch {
      throw new Blocked(
        "HTTPS fixtures need Python with cryptography for disposable TLS",
      );
    }
  }
  if (selected.some((scenario) => scenario.suite === "native-auth")) {
    authProvider = await startAuthFixture(root, {
      wrongAccount: options.fault === "wrong-account",
    });
  }
  if (selected.some((scenario) => scenario.suite === "mcp")) {
    mcpProvider = await startMcpFixture(root, {
      wrongNestedType: options.fault === "wrong-mcp-type",
      supportedExcluded: options.fault === "supported-mcp-excluded",
      shortPreview: options.fault === "short-mcp-preview",
      acceptedCatalog: options.fault === "accepted-mcp-catalog",
    });
  }
  if (authProvider || mcpProvider) {
    await writeFile(
      join(root, ".grain-agent-harness.json"),
      JSON.stringify({
        schema: 1,
        runId,
        modelPort: model.port,
        storePort: store?.port,
        authPort: authProvider?.port,
        mcpPort: mcpProvider?.port,
      }),
    );
  }
  await mkdir(join(root, "fixture/dist"), { recursive: true });
  await copyFile(
    join(here, "fixtures/lifecycle/manifest.json"),
    join(root, "fixture/manifest.json"),
  );
  await changeRevision("one");
  cdpPort = await unusedPort();
  hostBinary = binary;
  await launchHost();
  report.adapter.webViewVersion = browser.version();
  const firstStatus = await status();
  if (!authProvider)
    await assert.rejects(
      invoke("agent_harness_auth", { operation: "status" }),
      /Native auth fixture is not enabled/,
    );
  if (!mcpProvider)
    await assert.rejects(
      invoke("agent_harness_mcp", { operation: "discover" }),
      /MCP fixture is not enabled/,
    );
  assert.equal(firstStatus.runId, runId);
  assert.equal(firstStatus.applicationId, "com.grain.agent-harness");
  assert.equal(firstStatus.eventsPort, 17124);
  assert.equal(
    resolve(firstStatus.profile.replace(/^\\\\\?\\/, "")),
    resolve(root, "data"),
    "Host did not use the disposable profile",
  );
  baselineTokens = firstStatus.tokenCount;
  assert.deepEqual(
    firstStatus.shortcutBindings,
    [],
    "Harness retained ordinary accelerators",
  );
  // The harness already owns an empty production shortcut manager. This must
  // be idempotent, including after the real main renderer completes onboarding.
  await invoke("initialize_shortcuts");
  assert.deepEqual((await status()).shortcutBindings, []);
  for (const scenario of selected) {
    if (interrupted) throw new Error("Run interrupted");
    const started = performance.now();
    const modelStart = model.journal.length;
    // A hard process/terminal loss cannot run finally. Persist partial evidence
    // with an explicit unverified cleanup verdict, never an aggregate Pass.
    report.activeScenario = scenario.id;
    await writeReport(root, report);
    console.log(`RUN ${scenario.id}`);
    const result = {
      id: scenario.id,
      description: scenario.description,
      supportingChecks: scenario.checks,
      status: "Not run",
    };
    try {
      await closePanel();
      await fixture("unload").catch((error) => {
        if (!String(error).includes("not a load-unpacked")) throw error;
      });
      if ((await status()).fixtureInstalled) await fixture("remove_installed");
      await changeRevision("one");
      await copyFile(
        join(here, "fixtures/lifecycle/manifest.json"),
        join(root, "fixture/manifest.json"),
      );
      if (
        scenario.id !== "native.legacy-migration" &&
        scenario.suite !== "native-auth" &&
        scenario.suite !== "mcp" &&
        !["native-installation", "registry-recovery", "store"].includes(
          scenario.suite,
        )
      )
        await fixture("load");
      const eventStart = (await status()).events.length;
      await handlers[scenario.id]();
      if (scenario.suite === "store") await invoke("store_close");
      await closePanel();
      await fixture("unload").catch((error) => {
        if (!String(error).includes("not a load-unpacked")) throw error;
      });
      if ((await status()).fixtureInstalled) await fixture("remove_installed");
      await waitFor("Scenario cleanup baseline", async () => {
        const value = await status();
        return (
          value.worker.count === 0 &&
          value.tokenCount === baselineTokens &&
          !value.agent.active &&
          !value.agent.pendingApproval &&
          !(await findWindow(/^extension-host-/))
        );
      });
      const end = await status();
      result.events = end.events.slice(eventStart);
      result.model = model.journal.slice(modelStart);
      result.observations = [
        ...takeInputEvidence(),
        ...installation.takeEvidence(),
        ...storeSuite.takeEvidence(),
        ...registrySuite.takeEvidence(),
        ...foundationSuite.takeEvidence(),
        ...authenticationSuite.takeEvidence(),
        ...accountOwnershipSuite.takeEvidence(),
        ...accountSchedules.takeEvidence(),
        ...accountRefresh.takeEvidence(),
        ...mcpSuite.takeEvidence(),
      ];
      assert.ok(
        !result.model.some((entry) => entry.state === "error"),
        "Scripted provider rejected the real Agent request",
      );
      result.status = "Pass";
    } catch (error) {
      result.status = error instanceof Blocked ? "Blocked" : "Fail";
      result.error = String(error.message ?? error).slice(0, 2500);
      result.model = model.journal.slice(modelStart);
      result.observations = [
        ...(result.observations ?? []),
        ...takeInputEvidence(),
        ...installation.takeEvidence(),
        ...storeSuite.takeEvidence(),
        ...registrySuite.takeEvidence(),
        ...foundationSuite.takeEvidence(),
        ...authenticationSuite.takeEvidence(),
        ...accountOwnershipSuite.takeEvidence(),
        ...accountSchedules.takeEvidence(),
        ...accountRefresh.takeEvidence(),
        ...mcpSuite.takeEvidence(),
      ];
      if (scenario.suite === "mcp") {
        for (const observation of result.observations) {
          if (observation.status === "Running") {
            observation.status = result.status;
            observation.error = result.error;
          }
        }
      }
      try {
        result.snapshot = await status();
      } catch {
        /* host might have exited */
      }
      if (main && !main.isClosed()) {
        try {
          result.mainText = (await main.locator("body").innerText()).slice(
            0,
            4000,
          );
          await mkdir(join(root, "evidence"), { recursive: true });
          await main.screenshot({
            path: join(root, "evidence", `${scenario.id}-main.png`),
          });
        } catch {
          /* renderer diagnostic is best effort */
        }
      }
      const page = await findWindow("agent-panel");
      if (page) {
        try {
          await mkdir(join(root, "evidence"), { recursive: true });
          await page.screenshot({
            path: join(root, "evidence", `${scenario.id}.png`),
          });
        } catch {
          /* bounded diagnostic best effort */
        }
      }
    }
    result.elapsedMs = Math.round(performance.now() - started);
    report.results.push(result);
    delete report.activeScenario;
    await writeReport(root, report);
    console.log(
      `${result.status.toUpperCase()} ${result.id} (${result.elapsedMs}ms)${result.error ? `: ${result.error}` : ""}`,
    );
    // Stop after a failure rather than certify later scenarios on contaminated state.
    if (result.status !== "Pass") break;
  }
} catch (error) {
  report.results.push({
    id: "harness.prerequisites",
    status: error instanceof Blocked ? "Blocked" : "Fail",
    error: String(error.message ?? error).slice(0, 2500),
  });
} finally {
  const errors = [];
  if (child && child.exitCode === null && child.signalCode === null) {
    try {
      await invoke("agent_harness_shutdown");
    } catch {
      /* reply may disappear on graceful exit */
    }
    try {
      await poll(
        "Host exit",
        () => child.exitCode !== null || child.signalCode !== null,
        { timeoutMs: 6000 },
      );
    } catch {
      try {
        await exec("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
          timeout: 6000,
        });
        await poll(
          "Forced host exit",
          () => child.exitCode !== null || child.signalCode !== null,
          { timeoutMs: 3000 },
        );
      } catch (error) {
        errors.push(`Owned process cleanup failed: ${error.message}`);
      }
    }
  }
  if (browser) {
    try {
      await browser.close();
    } catch (error) {
      errors.push(`CDP cleanup: ${error.message}`);
    }
  }
  if (model) {
    try {
      await model.close();
    } catch (error) {
      errors.push(`Model cleanup: ${error.message}`);
    }
  }
  if (store) {
    report.storeRequests = store.journal;
    try {
      await store.close();
    } catch (error) {
      errors.push(`Store cleanup: ${error.message}`);
    }
    try {
      await poll(
        "Store fixture listener release",
        async () => !(await portOpen(store.port)),
        { timeoutMs: 3000 },
      );
    } catch (error) {
      errors.push(error.message);
    }
  }
  if (authProvider) {
    report.nativeAuthFixture = {
      port: authProvider.port,
      requests: authProvider.journal,
      limitations: [
        "Controlled consent replaces external browser handoff; live-provider acceptance remains pending.",
      ],
    };
    try {
      await authProvider.close();
      await poll(
        "Native auth fixture listener release",
        async () => !(await portOpen(authProvider.port)),
        { timeoutMs: 3000 },
      );
    } catch (error) {
      errors.push(`Auth provider cleanup: ${error.message}`);
    }
    try {
      const output = await exec(
        "powershell.exe",
        [
          "-NoProfile",
          "-File",
          join(here, "auth-cleanup.ps1"),
          "-Root",
          root,
          "-RunId",
          runId,
        ],
        { timeout: 10000, windowsHide: true, maxBuffer: 4096 },
      );
      const result = JSON.parse(output.stdout.trim());
      assert.equal(result.kind, "native-vault-cleanup");
      assert.equal(result.remaining, 0);
      report.nativeVaultCleanup = result;
    } catch (error) {
      errors.push(`Owned credential cleanup: ${error.message}`);
    }
  }
  if (mcpProvider) {
    report.mcpFixture = {
      port: mcpProvider.port,
      requests: mcpProvider.journal,
      activeSessionsBeforeShutdown: mcpProvider.activeSessions,
      heldCallsBeforeShutdown: mcpProvider.heldCalls,
      delayedRepliesBeforeShutdown: mcpProvider.delayedReplies,
      limitations: [
        "Unauthenticated controlled HTTPS peer; no live provider, OAuth, account persistence or official conformance certification.",
      ],
    };
    try {
      await mcpProvider.close();
      await poll(
        "MCP fixture listener release",
        async () => !(await portOpen(mcpProvider.port)),
        { timeoutMs: 3000 },
      );
    } catch (error) {
      errors.push(`MCP provider cleanup: ${error.message}`);
    }
  }
  if (cdpPort) {
    try {
      await poll(
        "WebView2 endpoint release",
        async () => !(await portOpen(cdpPort)),
        { timeoutMs: 6000, intervalMs: 300 },
      );
    } catch (error) {
      errors.push(error.message);
    }
  }
  if (child) {
    try {
      await poll(
        "Harness event listener release",
        async () => !(await portOpen(17124)),
        { timeoutMs: 3000 },
      );
    } catch (error) {
      errors.push(error.message);
    }
  }
  report.cleanup = { status: errors.length ? "Fail" : "Pass", errors };
  report.completedAt = new Date().toISOString();
  // Keep reports/screenshots only. Delete only verified children of this unique run.
  for (const name of [
    "data",
    "fixture",
    "fixture-b",
    "fixture.grainpack",
    "auth-tls",
    "mcp-tls",
    "auth-fixture.grainpack",
    "auth-peer.grainpack",
    "auth-fixture-a",
    "auth-fixture-b",
    "auth-fixture-a.grainpack",
    "auth-fixture-b.grainpack",
  ]) {
    try {
      await rm(assertWithin(root, join(root, name)), {
        recursive: true,
        force: true,
        maxRetries: 3,
        retryDelay: 200,
      });
    } catch (error) {
      report.cleanup.status = "Fail";
      report.cleanup.errors.push(`Scratch cleanup: ${error.message}`);
    }
  }
  await mkdir(join(root, "evidence"), { recursive: true });
  await writeFile(join(root, "evidence/host.log"), logTail);
  for (const scenario of selected)
    if (!report.results.some((result) => result.id === scenario.id))
      report.results.push({ id: scenario.id, status: "Not run" });
  try {
    if (
      report.runnerFingerprint &&
      (await runnerFingerprint()) !== report.runnerFingerprint
    )
      throw new Error(
        "Harness definitions changed during execution; repeat from stable inputs",
      );
  } catch (error) {
    report.results.push({
      id: "harness.identity",
      status: "Fail",
      error: error.message,
    });
  }
  await writeReport(root, report);
  process.removeListener("SIGINT", shutdownSignal);
  process.removeListener("SIGTERM", shutdownSignal);
  console.log(`Report: ${join(root, "evidence/report.md")}`);
}
process.exitCode =
  report.cleanup.status !== "Pass" ||
  report.results.some((result) => result.status === "Fail")
    ? 1
    : report.results.some((result) =>
          ["Blocked", "Not run"].includes(result.status),
        )
      ? 2
      : 0;
