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
  rm,
  realpath,
} from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { createServer, connect } from "node:net";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { chromium } from "@playwright/test";
import { scenarios, selectScenarios } from "./scenarios.mjs";
import { startMcpFixture } from "./mcp-fixture.mjs";
import { mcpHandlers } from "./mcp.mjs";
import { mcpAuthHandlers } from "./mcp-auth.mjs";
import { configuredMcpHandlers } from "./mcp-configured.mjs";
import { configuredRuntimeHandlers } from "./mcp-configured-runtime.mjs";
import { configuredAuthHandlers } from "./mcp-configured-auth.mjs";
import {
  mcpIndependenceHandlers,
  MCP_INDEPENDENCE_IDS,
} from "./mcp-independence.mjs";
import { workflowHandlers } from "./workflow.mjs";
import { liveHandlers, LIVE_ENDPOINT, LIVE_REPOSITORY } from "./mcp-live.mjs";
import { hfHandlers, HF_ENDPOINT, HF_DOCUMENT } from "./mcp-hf-live.mjs";
import {
  linearHandlers,
  LINEAR_ENDPOINT,
  linearIpcDeadline,
  safeLinearError,
  waitForLinearCancellation,
} from "./mcp-linear.mjs";
import {
  cases as conformanceCases,
  startConformanceRelay,
} from "./conformance-support.mjs";
import { startModel } from "./model.mjs";
import { verifyBuild, runnerFingerprint } from "./stamp.mjs";
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
  if (["--list", "--help", "--linear-sign-in"].includes(option))
    options[option.slice(2)] = true;
  else if (
    [
      "--suite",
      "--scenario",
      "--binary",
      "--output",
      "--fault",
      "--conformance-root",
    ].includes(option)
  ) {
    if (!process.argv[i + 1] || process.argv[i + 1].startsWith("--"))
      throw new Error(`${option} requires a value`);
    options[option.slice(2)] = process.argv[++i];
  } else throw new Error(`Unknown option: ${option}`);
}
if (options.help) {
  console.log(
    "node tests/agent-harness/run.mjs [--list] [--suite NAME | --scenario ID] [--binary path] [--output directory] [--linear-sign-in]\nWindows real Agent/WebView2 acceptance. Build with tests/agent-harness/build.ps1. See --list for cases and suites. The controlled TLS peers need Python with cryptography. Linear browser tests require explicit opt-in.",
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
const linearPreflightSelected = selected.some(
  (scenario) => scenario.suite === "mcp-linear-preflight",
);
const linearHumanSelected = selected.some((scenario) =>
  ["mcp-linear-sign-in", "mcp-linear-contracts"].includes(scenario.suite),
);
const linearGuardsSelected = selected.some(
  (scenario) => scenario.suite === "mcp-linear-guards",
);
const linearSelected =
  linearPreflightSelected || linearHumanSelected || linearGuardsSelected;
if (linearHumanSelected !== !!options["linear-sign-in"])
  throw new Error(
    "Linear browser testing requires --suite mcp-linear-sign-in or mcp-linear-contracts with --linear-sign-in; refused in other suites",
  );
if (linearHumanSelected && !process.stdin.isTTY)
  throw new Error(
    "Linear browser testing requires an interactive terminal for browser approval",
  );
let conformanceBinding;
if (selected.some((scenario) => scenario.suite === "mcp-conformance")) {
  assert.ok(
    options["conformance-root"] && selected.length === 1,
    "Use the maintained official conformance runner",
  );
  const owned = await realpath(options["conformance-root"]);
  assertWithin(await realpath(join(here, ".runs")), owned);
  conformanceBinding = JSON.parse(
    await readFile(join(owned, "binding.json"), "utf8"),
  );
  assert.equal(conformanceBinding.id, selected[0].id);
  assert.equal(conformanceBinding.schema, 1);
  assert.ok(conformanceCases.some((item) => item.id === selected[0].id));
} else
  assert.equal(
    options["conformance-root"],
    undefined,
    "Conformance context is confined to official cases",
  );
if (
  options.fault &&
  ![
    "wrong-mcp-type",
    "supported-mcp-excluded",
    "short-mcp-preview",
    "accepted-mcp-catalog",
    "missing-live-evidence",
    "skip-linear-cancel",
    "wrong-mcp-account",
    "wrong-mcp-peer-account",
    "missing-mcp-issuer-rotation",
    "missing-mcp-refresh-failure",
    "missing-mcp-cimd",
    "missing-mcp-destination-refusal",
    "missing-configured-cleanup",
    "wrong-configured-owner",
    "wrong-configured-auth-account",
    "skip-configured-client-rotation",
    "wrong-mcp-refresh-account",
    "skip-fixed-port-conflict",
    "abandoned-mcp-credential",
    "lost-mcp-account",
    "skip-mcp-disable",
    "skip-mcp-client-change",
    "abandoned-mcp-client-secret",
    "wrong-workflow-receipt",
  ].includes(options.fault)
)
  throw new Error("Unknown oracle fault");
if (
  options.fault === "skip-linear-cancel" &&
  (!linearPreflightSelected || selected.length !== 1)
)
  throw new Error(
    "skip-linear-cancel requires one isolated Linear preflight scenario",
  );
if (
  options.fault === "wrong-workflow-receipt" &&
  options.scenario !== "agent.workflow-mcp"
)
  throw new Error(
    "wrong-workflow-receipt requires an isolated write workflow scenario",
  );
if (
  options.fault === "missing-live-evidence" &&
  !["mcp.live-read-disable", "mcp.hf-nested-read"].includes(options.scenario)
)
  throw new Error(
    "missing-live-evidence requires one isolated public live read scenario",
  );
for (const [fault, scenario] of [
  ["wrong-mcp-peer-account", "mcp.auth-provider-independence"],
  ["missing-mcp-issuer-rotation", "mcp.auth-issuer-binding"],
  ["missing-mcp-refresh-failure", "mcp.auth-temporary-recovery"],
  ["missing-mcp-cimd", "mcp.auth-client-metadata"],
  ["missing-mcp-destination-refusal", "mcp.destination-boundary"],
  ["missing-configured-cleanup", "mcp.configured-storage"],
  ["wrong-configured-owner", "mcp.configured-execution"],
  ["wrong-configured-auth-account", "mcp.configured-auth-ownership"],
  ["skip-configured-client-rotation", "mcp.configured-auth-client-rotation"],
  ["wrong-mcp-refresh-account", "mcp.auth-refresh-recovery"],
  ["skip-fixed-port-conflict", "mcp.auth-fixed-port-conflict"],
  ["lost-mcp-account", "mcp.auth-close-cancellation"],
  ["skip-mcp-disable", "mcp.auth-shutdown"],
  ["skip-mcp-client-change", "mcp.auth-client-configuration"],
  ["abandoned-mcp-client-secret", "mcp.auth-client-configuration"],
  ["wrong-mcp-account", "mcp.auth-fixture"],
  ["abandoned-mcp-credential", "mcp.auth-fixture"],
  ["wrong-mcp-type", "mcp.transport-contract"],
  ["supported-mcp-excluded", "mcp.mixed-catalog"],
  ["short-mcp-preview", "mcp.response-preview"],
  ["accepted-mcp-catalog", "mcp.catalog-budgets"],
]) {
  if (options.fault === fault && options.scenario !== scenario)
    throw new Error(`${fault} requires --scenario ${scenario}`);
}
const runId = randomUUID();
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
let child, browser, model, main, mcpProvider, mcpPeer;
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
  if (linearHumanSelected) {
    logTail =
      "Live Linear host output omitted to protect private diagnostics.\n";
    return;
  }
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
  const timeoutMs = linearIpcDeadline(
    command,
    args,
    linearHumanSelected || linearGuardsSelected,
  );
  try {
    return await Promise.race([
      main.evaluate(
        ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
        { command, args },
      ),
      new Promise((_, reject) => {
        deadline = setTimeout(
          () =>
            reject(
              new Error(
                `${command} exceeded its ${timeoutMs / 1000}-second IPC deadline`,
              ),
            ),
          timeoutMs,
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
  await activate(page.locator("button.agent-notch-close"));
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
async function request(instruction = "hello", { timeoutMs = 20000 } = {}) {
  await closePanel();
  await invoke("agent_harness_submit", { instruction });
  const page = await panel();
  await page
    .locator(".agc-confirm-actions .agc-action-btn")
    .waitFor({ state: "visible", timeout: timeoutMs });
  assert.equal(
    (await status()).agent.pendingApproval,
    true,
    "Host must own the approval before clicking it",
  );
  return page;
}
async function scriptedReply() {
  await invoke("agent_harness_submit", { instruction: "hello" });
  const page = await panel();
  await page.getByText("Harness ready", { exact: true }).waitFor();
}
const handlers = {
  "agent.model-reply": scriptedReply,
  async "agent.close-model"() {
    const start = model.journal.length;
    await invoke("agent_harness_submit", { instruction: "model_wait" });
    await waitFor("Held model request", () =>
      model.journal.slice(start).some((entry) => entry.state === "received"),
    );
    await closePanel();
    await waitFor("Held model cancelled", () =>
      model.journal.slice(start).some((entry) => entry.state === "cancelled"),
    );
    await scriptedReply();
  },
};
let hostBinary;
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
    async () => !(await portOpen(cdpPort)),
    { timeoutMs: 10000 },
  );
  assert.equal(child.exitCode, 0, `Host ${pid} did not exit normally`);
}
async function restartHost(beforeLaunch) {
  const oldSession = hostSession;
  const snapshot = await status();
  priorEvents.splice(0, priorEvents.length, ...snapshot.events);
  await stopHost();
  if (beforeLaunch) await beforeLaunch();
  cdpPort = await unusedPort();
  await launchHost();
  assert.equal(hostSession, oldSession + 1);
  const after = await status();
  assert.equal(after.agent.pendingApproval, false);
  assert.equal(after.agent.active, false);
}
const configuredMcpSuite = configuredMcpHandlers({
  root,
  main: () => main,
  invoke,
  status,
  restartHost,
  panel,
  closePanel,
  fault: options.fault,
});
Object.assign(handlers, configuredMcpSuite.handlers);
const configuredRuntimeSuite = configuredRuntimeHandlers({
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  provider: () => mcpProvider,
  model: () => model,
});
Object.assign(handlers, configuredRuntimeSuite.handlers);
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
const liveSuite = liveHandlers({
  invoke,
  panel,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  model: () => model,
  log: () => logTail,
  fault: options.fault,
});
Object.assign(handlers, liveSuite.handlers);
const hfSuite = hfHandlers({
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  model: () => model,
  log: () => logTail,
  fault: options.fault,
});
Object.assign(handlers, hfSuite.handlers);
const mcpAuthSuite = mcpAuthHandlers({
  main: () => main,
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  provider: () => mcpProvider,
  model: () => model,
  fault: options.fault,
  vaultCount: async (
    clientSecret = false,
    registration = false,
    configured = false,
  ) => {
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
        ...(clientSecret ? ["-McpClientSecret"] : []),
        ...(registration ? ["-McpRegistration"] : []),
        ...(configured ? ["-McpConfigured"] : []),
        ...(linearSelected ? ["-McpLinear"] : []),
      ],
      { timeout: 10000, windowsHide: true, maxBuffer: 4096 },
    );
    const value = JSON.parse(output.stdout.trim());
    assert.equal(
      value.kind,
      registration
        ? "mcp-registration-inventory"
        : clientSecret
          ? "mcp-client-secret-inventory"
          : "mcp-vault-inventory",
    );
    return value.count;
  },
});
Object.assign(handlers, mcpAuthSuite.handlers);
const configuredAuthSuite = configuredAuthHandlers({
  fault: options.fault,
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  provider: () => mcpProvider,
  model: () => model,
  vaultCount: (secret = false, registration = false) =>
    mcpAuthSuite.vaultCount(secret, registration, true),
});
Object.assign(handlers, configuredAuthSuite.handlers);

const linearSuite = linearHandlers({
  invoke,
  waitFor,
  restartHost,
  vaultCount: mcpAuthSuite.vaultCount,
  fault: options.fault,
  humanCancelled: () =>
    waitForLinearCancellation(process.stdin, cancellation.signal),
});
Object.assign(handlers, linearSuite.handlers);
const mcpIndependenceSuite = mcpIndependenceHandlers({
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  restartHost,
  provider: (peer) => (peer ? mcpPeer : mcpProvider),
  model: () => model,
  fault: options.fault,
  vaultCount: mcpAuthSuite.vaultCount,
});
Object.assign(handlers, mcpIndependenceSuite.handlers);
const workflowSuite = workflowHandlers({
  root,
  invoke,
  status,
  request,
  activate,
  waitFor,
  closePanel,
  panel,
  restartHost,
  provider: () => mcpProvider,
  model: () => model,
  fault: options.fault,
});
Object.assign(handlers, workflowSuite.handlers);
const conformanceEvidence = [];
for (const testCase of conformanceCases) {
  handlers[testCase.id] = async () => {
    const observation = {
      stage: testCase.id,
      status: "Running",
      protocolVersion: testCase.version,
    };
    conformanceEvidence.push(observation);
    const control = (operation) => invoke("agent_harness_mcp", { operation });
    await control("enable");
    try {
      if (testCase.scenario === "initialize") {
        const found = await control("discover");
        assert.equal(found.tool_count, 0);
        assert.deepEqual(found.tools, []);
        assert.equal(
          mcpProvider.journal.filter((item) => item.method === "initialize")
            .length,
          1,
        );
        assert.equal(
          mcpProvider.journal.filter((item) => item.method === "tools/list")
            .length,
          1,
        );
        assert.equal(
          mcpProvider.journal.filter((item) => item.method === "tools/call")
            .length,
          0,
        );
      } else {
        const start = model.journal.length;
        const page = await request("mcp_conformance");
        assert.equal(
          mcpProvider.journal.filter((item) => item.method === "tools/call")
            .length,
          0,
          "Official tool escaped approval",
        );
        await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
        await waitFor(
          "Actual official tool result",
          async () => !(await status()).agent.active,
        );
        assert.equal(
          model.journal
            .slice(start)
            .filter((item) => item.mcpConformanceVerified).length,
          1,
        );
        assert.equal(
          mcpProvider.journal.filter((item) => item.method === "tools/call")
            .length,
          1,
        );
        await page
          .getByText("Harness verified official MCP sum: 8", { exact: false })
          .first()
          .waitFor({ state: "visible", timeout: 10000 });
        assert.ok(
          mcpProvider.journal.some(
            (item) => item.wireVersion === testCase.version,
          ),
          "Expected official wire version not observed",
        );
        if (testCase.version === "2026-07-28")
          assert.equal(
            mcpProvider.journal.filter((item) => item.method === "initialize")
              .length,
            0,
          );
        else
          assert.ok(
            mcpProvider.journal.some((item) => item.method === "initialize"),
          );
      }
      assert.equal(mcpProvider.activeSessions, 0);
      assert.ok(
        !mcpProvider.journal.some(
          (item) => item.phase === "error" || item.phase === "upstream-error",
        ),
        "Conformance relay failed",
      );
      Object.assign(observation, {
        status: "Pass",
        wireCalls: testCase.scenario === "initialize" ? 0 : 1,
        approvalRequired: testCase.scenario !== "initialize",
        activeSessions: 0,
        actualResultVerified: testCase.scenario !== "initialize",
      });
    } finally {
      await closePanel();
      await control("disable");
    }
  };
}
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
  report.injectedFault = options.fault ?? null;
  if (linearSelected) {
    if (linearHumanSelected)
      report.evidenceClass =
        "real-application/human-browser-consent/live-issuer";
    report.linearPreflight = {
      endpoint: LINEAR_ENDPOINT,
      evidenceClass: linearHumanSelected
        ? "real-application/human-browser-consent/live-issuer"
        : "real-application/live-issuer/SDK-consent-preflight",
      limitations: [
        linearHumanSelected
          ? "Human browser authentication, scoped grant metadata and discovery/contracts only; no account tool execution or actual expiry/refresh certification; metadata alone does not accept numbered checks."
          : "No browser sign-in, token exchange, account read or live refresh certification; metadata alone does not accept numbered checks.",
      ],
      privacy: {
        metadataOnly: true,
        privateConsentUrlRetained: false,
        privateDiagnosticsOmitted: linearHumanSelected,
        screenshotsPermitted: !linearHumanSelected,
      },
    };
  }
  if (selected.some((scenario) => scenario.suite === "mcp-live"))
    report.liveMcp = {
      endpoint: LIVE_ENDPOINT,
      repository: LIVE_REPOSITORY,
      evidenceClass: "real-application/live-provider/scripted-model",
      limitations: [
        "Public documentation reads only; no OAuth, account, private repository, live-model relevance or universal provider certification.",
        "Backend attempt counts are conservative client dispatch observations, not independent remote-server receipts.",
      ],
    };
  if (selected.some((scenario) => scenario.suite === "mcp-hf-live"))
    report.liveMcp = {
      endpoint: HF_ENDPOINT,
      document: HF_DOCUMENT,
      evidenceClass: "real-application/live-provider/scripted-model",
      limitations: [
        "One fixed anonymous public text read only; no OAuth, writes, private content or live-model relevance certification.",
        "Dispatch observations are client-side; no independent remote-server receipt.",
      ],
    };
  model = await startModel({ fault: options.fault });
  await writeFile(
    join(root, ".grain-agent-harness.json"),
    JSON.stringify({
      schema: 1,
      runId,
      modelPort: model.port,
      mcpLiveDeepwiki: selected.some(
        (scenario) => scenario.suite === "mcp-live",
      ),
      mcpLiveHuggingface: selected.some(
        (scenario) => scenario.suite === "mcp-hf-live",
      ),
      mcpAuth: selected.some((scenario) =>
        ["mcp-auth", "mcp-configured-auth"].includes(scenario.suite),
      ),
      mcpLiveLinear: linearSelected,
      mcpLinearConsent: linearHumanSelected || linearGuardsSelected,
    }),
  );
  if (
    selected.some((scenario) =>
      [
        "mcp",
        "mcp-configured-runtime",
        "mcp-configured-auth",
        "mcp-auth",
        "mcp-conformance",
        "agent-workflow",
        "agent-interruption",
      ].includes(scenario.suite),
    )
  ) {
    try {
      await exec("python", [join(here, "auth-tls.py"), root], {
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
  if (conformanceBinding)
    mcpProvider = await startConformanceRelay(root, conformanceBinding);
  else if (
    selected.some((scenario) =>
      [
        "mcp",
        "mcp-configured-runtime",
        "mcp-configured-auth",
        "mcp-auth",
        "agent-workflow",
        "agent-interruption",
      ].includes(scenario.suite),
    )
  ) {
    mcpProvider = await startMcpFixture(root, {
      authenticated: selected.some((scenario) =>
        ["mcp-auth", "mcp-configured-auth"].includes(scenario.suite),
      ),
      wrongAccount: [
        "wrong-mcp-account",
        "wrong-configured-auth-account",
      ].includes(options.fault),
      wrongNestedType: options.fault === "wrong-mcp-type",
      supportedExcluded: options.fault === "supported-mcp-excluded",
      shortPreview: options.fault === "short-mcp-preview",
      acceptedCatalog: options.fault === "accepted-mcp-catalog",
      wrongWorkflowReceipt: options.fault === "wrong-workflow-receipt",
    });
  }
  if (mcpProvider) {
    if (selected.some((scenario) => MCP_INDEPENDENCE_IDS.includes(scenario.id)))
      mcpPeer = await startMcpFixture(root, {
        authenticated: true,
        wrongAccount: options.fault === "wrong-mcp-peer-account",
      });
    await writeFile(
      join(root, ".grain-agent-harness.json"),
      JSON.stringify({
        schema: 1,
        runId,
        modelPort: model.port,
        mcpPort: mcpProvider?.port,
        mcpPeerPort: mcpPeer?.port,
        mcpAuth: selected.some((scenario) =>
          ["mcp-auth", "mcp-configured-auth"].includes(scenario.suite),
        ),
        mcpIssuerRotation: selected.some(
          (scenario) => scenario.id === "mcp.auth-issuer-binding",
        ),
      }),
    );
  }
  cdpPort = await unusedPort();
  hostBinary = binary;
  await launchHost();
  report.adapter.webViewVersion = browser.version();
  const firstStatus = await status();
  if (!linearSelected)
    await assert.rejects(
      invoke("agent_harness_mcp", {
        operation: "authorization",
        target: "linear",
      }),
      /Live Linear preflight is not enabled/,
    );
  if (!linearHumanSelected && !linearGuardsSelected)
    for (const operation of [
      "consent_ready",
      "open_authorization",
      "grant_metadata",
      "catalog_contracts",
    ])
      await assert.rejects(
        invoke("agent_harness_mcp", { operation, target: "linear" }),
        /Live Linear (preflight|browser consent) is not enabled/,
      );
  if (!mcpPeer)
    await assert.rejects(
      invoke("agent_harness_mcp", {
        operation: "authorization",
        target: "peer",
      }),
      /MCP (peer|account) fixture is not enabled/,
    );
  if (!mcpProvider && !report.liveMcp)
    await assert.rejects(
      invoke("agent_harness_mcp", { operation: "discover" }),
      /MCP fixture is not enabled/,
    );
  if (report.liveMcp)
    await assert.rejects(
      invoke("agent_harness_mcp", { operation: "discover" }),
      /disabled in Grain Settings/,
    );
  assert.equal(firstStatus.runId, runId);
  assert.equal(firstStatus.applicationId, "com.grain.agent-harness");
  assert.equal(
    resolve(firstStatus.profile.replace(/^\\\\\?\\/, "")),
    resolve(root, "data"),
    "Host did not use the disposable profile",
  );
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
      const eventStart = (await status()).events.length;
      await handlers[scenario.id]();
      await closePanel();
      await waitFor(
        "Scenario cleanup baseline",
        async () => !(await status()).agent.active,
      );
      const end = await status();
      result.events = linearHumanSelected ? [] : end.events.slice(eventStart);
      result.model = linearHumanSelected ? [] : model.journal.slice(modelStart);
      result.observations = [
        ...configuredMcpSuite.takeEvidence(),
        ...configuredRuntimeSuite.takeEvidence(),
        ...configuredAuthSuite.takeEvidence(),
        ...mcpSuite.takeEvidence(),
        ...liveSuite.takeEvidence(),
        ...hfSuite.takeEvidence(),
        ...linearSuite.takeEvidence(),
        ...mcpAuthSuite.takeEvidence(),
        ...mcpIndependenceSuite.takeEvidence(),
        ...workflowSuite.takeEvidence(),
        ...conformanceEvidence.splice(0),
      ];
      assert.ok(
        !result.model.some((entry) => entry.state === "error"),
        "Scripted provider rejected the real Agent request",
      );
      assert.ok(
        !mcpProvider?.journal.some(
          (entry) =>
            entry.phase === "error" || entry.phase === "upstream-error",
        ),
        "Controlled MCP peer rejected a real request",
      );
      assert.ok(
        !mcpProvider?.oauth?.journal.some(
          (entry) =>
            entry.phase === "oauth-error" || entry.phase === "unexpected-route",
        ),
        "Controlled MCP OAuth issuer rejected a real request",
      );
      assert.ok(
        !mcpPeer?.journal.some((entry) => entry.phase === "error"),
        "Second MCP peer rejected real traffic",
      );
      assert.ok(
        !mcpPeer?.oauth?.journal.some((entry) =>
          ["oauth-error", "unexpected-route"].includes(entry.phase),
        ),
        "Second OAuth issuer rejected real traffic",
      );
      result.status = "Pass";
    } catch (error) {
      result.status = error instanceof Blocked ? "Blocked" : "Fail";
      result.error = linearHumanSelected
        ? safeLinearError(error)
        : String(error.message ?? error).slice(0, 2500);
      result.model = linearHumanSelected ? [] : model.journal.slice(modelStart);
      result.observations = [
        ...(result.observations ?? []),
        ...configuredMcpSuite.takeEvidence(),
        ...configuredRuntimeSuite.takeEvidence(),
        ...configuredAuthSuite.takeEvidence(),
        ...mcpSuite.takeEvidence(),
        ...liveSuite.takeEvidence(),
        ...hfSuite.takeEvidence(),
        ...linearSuite.takeEvidence(),
        ...mcpAuthSuite.takeEvidence(),
        ...mcpIndependenceSuite.takeEvidence(),
        ...workflowSuite.takeEvidence(),
        ...conformanceEvidence.splice(0),
      ];
      if (
        [
          "mcp",
          "mcp-conformance",
          "mcp-live",
          "mcp-hf-live",
          "mcp-linear-preflight",
          "mcp-linear-guards",
          "mcp-linear-sign-in",
          "mcp-linear-contracts",
          "mcp-auth",
          "agent-workflow",
          "agent-interruption",
        ].includes(scenario.suite)
      ) {
        for (const observation of result.observations) {
          if (observation.status === "Running") {
            observation.status = result.status;
            observation.error = result.error;
          }
        }
      }
      if (!linearHumanSelected)
        try {
          result.snapshot = await status();
        } catch {
          /* host might have exited */
        }
      if (!linearHumanSelected && main && !main.isClosed()) {
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
      const page = linearHumanSelected ? null : await findWindow("agent-panel");
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
    error: linearHumanSelected
      ? safeLinearError(error)
      : String(error.message ?? error).slice(0, 2500),
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
  if (mcpProvider) {
    report.mcpFixture = {
      port: mcpProvider.port,
      requests: mcpProvider.journal,
      activeSessionsBeforeShutdown: mcpProvider.activeSessions,
      heldCallsBeforeShutdown: mcpProvider.heldCalls,
      delayedRepliesBeforeShutdown: mcpProvider.delayedReplies,
      limitations: [
        conformanceBinding
          ? "Pinned official server via owned TLS relay; named tool-only subset, no live provider, OAuth, account persistence or full conformance/tier certification."
          : mcpProvider.oauth
            ? "Controlled SDK OAuth and real scoped vault/callbacks; external browser consent and live-provider certification remain separate."
            : "Unauthenticated controlled HTTPS peer; no live provider, OAuth, account persistence or official conformance certification.",
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
  if (mcpProvider?.oauth || linearSelected) {
    if (mcpProvider?.oauth)
      report.mcpOAuthFixture = { requests: mcpProvider.oauth.journal };
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

          ...(linearSelected ? ["-McpLinear"] : []),
        ],
        { timeout: 10000, windowsHide: true, maxBuffer: 4096 },
      );
      const value = JSON.parse(output.stdout.trim());
      assert.equal(value.kind, "mcp-vault-cleanup");
      assert.equal(value.remaining, 0);
      assert.equal(value.clientSecretsRemaining, 0);
      assert.equal(value.registrationsRemaining, 0);
      report.mcpVaultCleanup = value;
      if (selected.some((x) => ["mcp-configured-auth"].includes(x.suite))) {
        const cleaned = await exec(
          "powershell.exe",
          [
            "-NoProfile",
            "-File",
            join(here, "auth-cleanup.ps1"),
            "-Root",
            root,
            "-RunId",
            runId,

            "-McpConfigured",
          ],
          { timeout: 10000, windowsHide: true, maxBuffer: 4096 },
        );
        const configured = JSON.parse(cleaned.stdout.trim());
        assert.equal(configured.remaining, 0);
        assert.equal(configured.clientSecretsRemaining, 0);
        assert.equal(configured.registrationsRemaining, 0);
        report.configuredMcpVaultCleanup = configured;
      }
    } catch (error) {
      errors.push(`Owned MCP credential cleanup: ${error.message}`);
    }
  }
  if (mcpPeer) {
    report.mcpPeerFixture = {
      port: mcpPeer.port,
      requests: mcpPeer.journal,
      activeSessionsBeforeShutdown: mcpPeer.activeSessions,
      heldCallsBeforeShutdown: mcpPeer.heldCalls,
      delayedRepliesBeforeShutdown: mcpPeer.delayedReplies,
    };
    report.mcpPeerOAuthFixture = { requests: mcpPeer.oauth.journal };
    try {
      await mcpPeer.close();
      await poll(
        "Second MCP listener release",
        async () => !(await portOpen(mcpPeer.port)),
        { timeoutMs: 3000 },
      );
    } catch (error) {
      errors.push(`Second MCP provider cleanup: ${error.message}`);
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
  report.cleanup = {
    status: errors.length ? "Fail" : "Pass",
    errors: linearHumanSelected ? errors.map(safeLinearError) : errors,
  };
  report.completedAt = new Date().toISOString();
  // Keep reports/screenshots only. Delete only verified children of this unique run.
  for (const name of ["data", "mcp-tls"]) {
    try {
      await rm(assertWithin(root, join(root, name)), {
        recursive: true,
        force: true,
        maxRetries: 3,
        retryDelay: 200,
      });
    } catch (error) {
      report.cleanup.status = "Fail";
      report.cleanup.errors.push(
        linearHumanSelected
          ? safeLinearError(error)
          : `Scratch cleanup: ${error.message}`,
      );
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
