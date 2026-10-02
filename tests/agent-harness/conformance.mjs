// Serial official checks + real-app acceptance. No bare SDK/client replacement.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import {
  access,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { connect } from "node:net";
import {
  cases,
  CONFORMANCE_VERSION,
  verifyOfficialChecks,
  initializeFixtureBlocked,
} from "./conformance-support.mjs";
import { hashFile, runnerFingerprint } from "./stamp.mjs";
import { assertWithin, waitFor, writeReport } from "./support.mjs";

const here = dirname(fileURLToPath(import.meta.url)),
  repo = resolve(here, "../..");
const cache = join(here, ".build/conformance"),
  exec = promisify(execFile);
const options = {};
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === "--install") options.install = true;
  else if (["--scenario", "--fault", "--suite"].includes(process.argv[i]))
    options[process.argv[i].slice(2)] = process.argv[++i];
  else throw new Error("Unknown conformance option");
}
assert.ok(
  options.fault === undefined || options.fault === "missing-official-check",
);
const selected = options.scenario
  ? cases.filter((item) => item.id === options.scenario)
  : [
      ...cases.filter((item) => item.scenario === "tools_call"),
      ...cases.filter((item) => item.scenario === "initialize"),
    ].filter(
      (item) => options.suite !== "tools" || item.scenario === "tools_call",
    );
assert.ok(
  options.suite === undefined || ["all", "tools"].includes(options.suite),
);
assert.ok(selected.length > 0);
if (options.fault)
  assert.ok(
    options.scenario && selected[0].scenario === "tools_call",
    "Official fault requires one tool-call scenario",
  );
if (options.install) {
  assert.equal(options.fault, undefined);
  await mkdir(cache, { recursive: true });
  for (const name of ["package.json", "package-lock.json"])
    await copyFile(join(here, "conformance", name), join(cache, name));
  // A lockfile and npm integrity verification pin all transitive test packages.
  const result = await exec(
    "cmd.exe",
    ["/d", "/s", "/c", "npm.cmd ci --ignore-scripts --no-audit --no-fund"],
    {
      cwd: cache,
      windowsHide: true,
      timeout: 120000,
      maxBuffer: 65536,
    },
  );
  console.log(result.stdout.trim());
  process.exit(0);
}
const packagePath = join(
  cache,
  "node_modules/@modelcontextprotocol/conformance/package.json",
);
await access(packagePath);
assert.equal(
  JSON.parse(await readFile(packagePath, "utf8")).version,
  CONFORMANCE_VERSION,
  "Install the exact maintained conformance version",
);
assert.equal(
  await hashFile(join(cache, "package-lock.json")),
  await hashFile(join(here, "conformance/package-lock.json")),
  "Conformance cache lock differs; reinstall",
);
const cli = join(
  cache,
  "node_modules/@modelcontextprotocol/conformance/dist/index.js",
);
const reportRoot = await mkdtemp(join(here, ".runs/conformance-"));
const report = {
  schema: 1,
  runId: randomUUID(),
  evidenceClass: "official-conformance/real-application",
  commit: (
    await exec("git", ["rev-parse", "HEAD"], { cwd: repo })
  ).stdout.trim(),
  startedAt: new Date().toISOString(),
  results: [],
  cleanup: { status: "Not run", errors: [] },
  official: {
    version: CONFORMANCE_VERSION,
    cliSha256: await hashFile(cli),
    lockSha256: await hashFile(join(cache, "package-lock.json")),
  },
  runnerFingerprint: await runnerFingerprint(),
  limitations: [
    "Explicit tool-only subset, not SDK tier/full conformance, OAuth, live providers or Agent relevance certification.",
  ],
};
let active,
  interrupted = false;
const onSignal = () => {
  interrupted = true;
};
process.once("SIGINT", onSignal);
process.once("SIGTERM", onSignal);
async function portOpen(port) {
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port });
    socket.setTimeout(500);
    const finish = (value) => {
      socket.destroy();
      resolve(value);
    };
    socket.once("connect", () => finish(true));
    socket.once("error", () => finish(false));
    socket.once("timeout", () => finish(true));
  });
}
try {
  for (const testCase of selected) {
    assert.equal(interrupted, false, "Conformance run interrupted");
    const root = await mkdtemp(join(reportRoot, "case-"));
    const result = {
      id: testCase.id,
      scenario: testCase.scenario,
      protocolVersion: testCase.version,
      root,
      status: "Running",
    };
    report.results.push(result);
    await writeReport(reportRoot, report);
    console.log("RUN " + testCase.id + " (official " + testCase.version + ")");
    let timer;
    try {
      const out = join(root, "official");
      let tail = "";
      const exitCode = await new Promise((resolve, reject) => {
        active = spawn(
          process.execPath,
          [
            cli,
            "client",
            "--command",
            "node tests/agent-harness/conformance-client.mjs",
            "--scenario",
            testCase.scenario,
            "--spec-version",
            testCase.version,
            "--timeout",
            "60000",
            "--output-dir",
            out,
          ],
          {
            cwd: repo,
            env: { ...process.env, GRAIN_CONFORMANCE_ROOT: root },
            windowsHide: true,
            stdio: ["ignore", "pipe", "pipe"],
          },
        );
        active.stdout.on("data", (chunk) => {
          tail = (tail + chunk).slice(-65536);
        });
        active.stderr.on("data", (chunk) => {
          tail = (tail + chunk).slice(-65536);
        });
        active.once("error", reject);
        active.once("exit", (code) => resolve(code));
        // Publish before the official runner starts its adapter. One finite case.
        writeFile(
          join(root, "control.json"),
          JSON.stringify({
            schema: 1,
            runId: report.runId,
            id: testCase.id,
            pid: active.pid,
          }),
        ).catch(reject);
        timer = setTimeout(
          () =>
            reject(
              new Error(
                "Owned official runner exceeded 90 seconds; cleanup unverified",
              ),
            ),
          90000,
        );
      });
      result.exitCode = exitCode;
      await writeFile(join(root, "official.log"), tail);
      const officialDirs = await readdir(out);
      assert.equal(
        officialDirs.length,
        1,
        "Missing/ambiguous official case output",
      );
      const checks = JSON.parse(
        await readFile(join(out, officialDirs[0], "checks.json"), "utf8"),
      );
      const clientDirs = await readdir(join(root, "clients"));
      assert.equal(clientDirs.length, 1);
      const application = JSON.parse(
        await readFile(
          join(root, "clients", clientDirs[0], "evidence/report.json"),
          "utf8",
        ),
      );
      assert.equal(application.results.length, 1);
      assert.equal(application.results[0].id, testCase.id);
      assert.equal(
        application.cleanup.status,
        "Pass",
        "Real app cleanup did not pass",
      );
      assert.equal(application.runnerFingerprint, report.runnerFingerprint);
      const binding = JSON.parse(
        await readFile(join(root, "binding.json"), "utf8"),
      );
      await waitFor(
        "Owned official server port closes",
        async () => !(await portOpen(Number(new URL(binding.url).port))),
        { timeoutMs: 5000 },
      );
      Object.assign(result, {
        officialChecks: checks,
        applicationReport: join(
          root,
          "clients",
          clientDirs[0],
          "evidence/report.json",
        ),
        binary: application.binary,
        observations: application.results[0].observations,
        officialServerPort: Number(new URL(binding.url).port),
        cleanup: "Pass",
      });
      if (
        testCase.scenario === "initialize" &&
        initializeFixtureBlocked(application, checks)
      ) {
        result.status = "Blocked";
        result.error =
          "Pinned official initialize fixture returns HTTP 200 with an empty discover result; Grain correctly refuses it. No initialize check was emitted.";
        console.log("BLOCKED " + testCase.id + ": " + result.error);
        break;
      }
      assert.equal(exitCode, 0, "Official runner failed: " + tail.slice(-4000));
      assert.equal(
        application.results[0].status,
        "Pass",
        "Real application acceptance did not pass",
      );
      // Remove required evidence after a genuinely completed app/server run.
      // The same ordinary acceptance oracle must detect it.
      verifyOfficialChecks(
        options.fault === "missing-official-check"
          ? checks.filter((check) => check.id !== testCase.check)
          : checks,
        testCase,
      );
      result.status = "Pass";
      console.log(
        "PASS " +
          testCase.id +
          ": " +
          checks.filter((check) => check.status === "SUCCESS").length +
          " official checks + real Grain acceptance",
      );
    } catch (error) {
      result.status = "Fail";
      result.error = error.message;
      console.log("FAIL " + testCase.id + ": " + error.message);
      break;
    } finally {
      clearTimeout(timer);
      if (active && active.exitCode === null && active.signalCode === null) {
        report.cleanup.errors.push(
          "Official runner still active; forced owned-process termination does not certify child cleanup",
        );
        await exec("taskkill.exe", ["/PID", String(active.pid), "/T", "/F"], {
          windowsHide: true,
          timeout: 10000,
        }).catch((error) => report.cleanup.errors.push(error.message));
      }
      active = null;
      for (const name of ["control.json", "binding.json"])
        await rm(assertWithin(root, join(root, name)), { force: true });
      await writeReport(reportRoot, report);
    }
  }
} finally {
  for (const testCase of selected)
    if (!report.results.some((item) => item.id === testCase.id))
      report.results.push({ id: testCase.id, status: "Not run" });
  if ((await runnerFingerprint()) !== report.runnerFingerprint)
    report.cleanup.errors.push("Harness definitions changed during execution");
  if (
    report.results.some(
      (result) => result.cleanup !== "Pass" && result.status !== "Not run",
    )
  )
    report.cleanup.errors.push(
      "A started case has no verified application/server cleanup",
    );
  report.cleanup.status = report.cleanup.errors.length ? "Fail" : "Pass";
  report.completedAt = new Date().toISOString();
  await writeReport(reportRoot, report);
  process.removeListener("SIGINT", onSignal);
  process.removeListener("SIGTERM", onSignal);
  console.log("Report: " + join(reportRoot, "evidence/report.md"));
}
process.exitCode =
  report.cleanup.status !== "Pass" ||
  report.results.some((result) => result.status === "Fail")
    ? 1
    : report.results.some((result) => result.status !== "Pass")
      ? 2
      : 0;
