// Only the pinned official runner launches this finite real-application adapter.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile, realpath, writeFile } from "node:fs/promises";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import {
  cases,
  localServerUrl,
  assertServerOwner,
} from "./conformance-support.mjs";
import { assertWithin } from "./support.mjs";

const here = dirname(fileURLToPath(import.meta.url));
assert.ok(
  process.env.GRAIN_CONFORMANCE_ROOT,
  "Official runner context is required",
);
const root = await realpath(process.env.GRAIN_CONFORMANCE_ROOT);
assertWithin(await realpath(join(here, ".runs")), root);
const control = JSON.parse(await readFile(join(root, "control.json"), "utf8"));
const testCase = cases.find((item) => item.id === control.id);
assert.ok(
  testCase && control.schema === 1 && /^[a-f0-9-]{36}$/.test(control.runId),
);
assert.equal(process.env.MCP_CONFORMANCE_SCENARIO, testCase.scenario);
assert.equal(process.env.MCP_CONFORMANCE_PROTOCOL_VERSION, testCase.version);
assert.equal(process.argv.length, 3);
const url = localServerUrl(process.argv[2], testCase.scenario);
await assertServerOwner(Number(url.port), control.pid);
await writeFile(
  join(root, "binding.json"),
  JSON.stringify({ ...control, url: url.href }),
);
const child = spawn(
  process.execPath,
  [
    join(here, "run.mjs"),
    "--scenario",
    testCase.id,
    "--conformance-root",
    root,
    "--output",
    join(root, "clients"),
  ],
  { cwd: resolve(here, "../.."), windowsHide: true, stdio: "inherit" },
);
process.exitCode = await new Promise((resolve, reject) => {
  child.once("error", reject);
  child.once("exit", (code) => resolve(code ?? 1));
});
