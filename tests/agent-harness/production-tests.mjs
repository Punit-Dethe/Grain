#!/usr/bin/env node
// Production-logic evidence. Deliberately separate from real-window acceptance.
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";
import { writeReport, waitFor } from "./support.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
const exec = promisify(execFile);
const groups = {
  registry: "extensions::tests::",
  agent: "agent::",
  native: "extension_host::tests::",
  execution: "action_exec::tests::",
  mcp: "grain_mcp::",
  auth: "grain_auth::",
};
let group = "all",
  output = join(here, ".runs");
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === "--group") group = process.argv[++i];
  else if (process.argv[i] === "--output") output = resolve(process.argv[++i]);
  else throw new Error(`Unknown option: ${process.argv[i]}`);
}
if (group !== "all" && !groups[group])
  throw new Error(`Unknown production-test group: ${group}`);
await mkdir(output, { recursive: true });
const root = await mkdtemp(join(output, "logic-"));
const report = {
  schema: 1,
  runId: randomUUID(),
  evidenceClass: "production-logic/normal-build",
  commit: (
    await exec("git", ["rev-parse", "HEAD"], { cwd: repo })
  ).stdout.trim(),
  dirtyFiles: (
    await exec("git", ["status", "--porcelain"], { cwd: repo })
  ).stdout
    .trim()
    .split(/\r?\n/)
    .filter(Boolean),
  startedAt: new Date().toISOString(),
  results: [],
  cleanup: { status: "Pass" },
};
let current;
let stopping;
let interrupted = false;
function stopOwned() {
  if (stopping) return stopping;
  const owned = current;
  if (!owned || owned.exitCode !== null || owned.signalCode !== null)
    return Promise.resolve();
  stopping = (async () => {
    if (process.platform === "win32") {
      try {
        await exec("taskkill.exe", ["/PID", String(owned.pid), "/T", "/F"], {
          timeout: 6000,
          windowsHide: true,
        });
      } catch (error) {
        if (owned.exitCode === null && owned.signalCode === null) throw error;
      }
    } else owned.kill("SIGKILL");
    await waitFor(
      "Owned Cargo process exit",
      () => owned.exitCode !== null || owned.signalCode !== null,
      { timeoutMs: 6000 },
    );
  })();
  // Signal callbacks cannot await; finally awaits and records cleanup errors.
  stopping.catch(() => {});
  return stopping;
}
const cancel = () => {
  interrupted = true;
  void stopOwned();
};
process.once("SIGINT", cancel);
process.once("SIGTERM", cancel);
try {
  for (const [id, filter] of Object.entries(groups).filter(
    ([id]) => group === "all" || id === group,
  )) {
    if (interrupted) throw new Error("Production suite interrupted");
    console.log(`RUN logic.${id}`);
    const started = performance.now();
    const env = { ...process.env };
    delete env.TAURI_CONFIG;
    delete env.GRAIN_AGENT_HARNESS_ROOT;
    const registry = id === "registry";
    const cargoArgs = [
      "test",
      "--locked",
      ...(registry ? ["-p", "grain-core"] : []),
      "--lib",
      filter,
    ];
    if (process.platform === "win32" && !registry) {
      delete env.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER;
      // A TOML argv array preserves paths containing spaces; Cargo's runner
      // environment string splits whitespace rather than interpreting quotes.
      cargoArgs.unshift(
        "--config",
        `target.x86_64-pc-windows-msvc.runner=${JSON.stringify(["powershell.exe", "-NoProfile", "-File", join(repo, "scripts/run-rust-test.ps1")])}`,
      );
      env.TMP ||= "C:\\Windows\\Temp";
      const metadata = JSON.parse(
        (
          await exec(
            "cargo",
            ["metadata", "--format-version", "1", "--no-deps"],
            {
              cwd: join(repo, "src-tauri"),
              env,
              timeout: 30000,
            },
          )
        ).stdout,
      );
      if (metadata.target_directory.replace(/[\\/]+$/, "").length <= 12) {
        delete env.LOCALAPPDATA;
        delete env.TEMP;
      }
    }
    let tail = "";
    const result = {
      id: `logic.${id}`,
      description: `${registry ? "Core registry" : "Normal backend"} lib tests filtered by ${filter}`,
      status: "Not run",
    };
    try {
      const exitCode = await new Promise((resolve, reject) => {
        stopping = undefined;
        current = spawn("cargo", cargoArgs, {
          cwd: registry ? repo : join(repo, "src-tauri"),
          env,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        const capture = (chunk) => {
          tail = (tail + chunk.toString()).slice(-131072);
        };
        current.stdout.on("data", capture);
        current.stderr.on("data", capture);
        const deadline = setTimeout(() => {
          void stopOwned();
          reject(new Error("Cargo group exceeded its 15-minute deadline"));
        }, 900000);
        current.once("error", (error) => {
          clearTimeout(deadline);
          reject(error);
        });
        current.once("exit", (code) => {
          clearTimeout(deadline);
          current = null;
          resolve(code);
        });
      });
      const summary = [
        ...tail.matchAll(
          /test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g,
        ),
      ].at(-1);
      if (
        exitCode !== 0 ||
        !summary ||
        Number(summary[1]) === 0 ||
        Number(summary[2]) !== 0
      )
        throw new Error(
          `Cargo exit ${exitCode}; require at least one passing test and no failures`,
        );
      result.status = "Pass";
      result.passed = Number(summary[1]);
      result.ignored = Number(summary[3]);
    } catch (error) {
      result.status = error.code === "ENOENT" ? "Blocked" : "Fail";
      result.error = String(error.message);
    }
    result.elapsedMs = Math.round(performance.now() - started);
    await mkdir(join(root, "evidence"), { recursive: true });
    await writeFile(join(root, "evidence", `${id}.log`), tail);
    report.results.push(result);
    console.log(
      `${result.status.toUpperCase()} ${result.id}${result.passed ? `: ${result.passed} tests` : `: ${result.error}`}`,
    );
    if (result.status !== "Pass") break;
  }
} catch (error) {
  report.results.push({
    id: "logic.runner",
    status: "Fail",
    error: error.message,
  });
} finally {
  try {
    if (current) await stopOwned();
    if (stopping) await stopping;
  } catch (error) {
    report.cleanup = { status: "Fail", error: error.message };
  }
  process.removeListener("SIGINT", cancel);
  process.removeListener("SIGTERM", cancel);
  report.completedAt = new Date().toISOString();
  for (const id of Object.keys(groups).filter(
    (id) => group === "all" || id === group,
  ))
    if (!report.results.some((result) => result.id === `logic.${id}`))
      report.results.push({ id: `logic.${id}`, status: "Not run" });
  await writeReport(root, report);
  console.log(`Report: ${join(root, "evidence/report.md")}`);
}
process.exitCode =
  report.cleanup.status !== "Pass" ||
  report.results.some((result) => result.status === "Fail")
    ? 1
    : report.results.some((result) => result.status !== "Pass")
      ? 2
      : 0;
