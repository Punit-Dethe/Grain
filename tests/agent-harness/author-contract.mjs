#!/usr/bin/env node
// Generated-author-contract evidence, separate from real-window acceptance.
// Uses the stamped CLI and installed compiler/bundler; no install or network.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { buildSync, stop } from "esbuild";
import { verifyCli, runnerFingerprint } from "./stamp.mjs";
import { assertWithin, writeReport } from "./support.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
assert.equal(process.argv.length, 2, "author-contract takes no arguments");
const cli = await verifyCli();
const output = join(here, ".runs");
await mkdir(output, { recursive: true });
const root = await mkdtemp(join(output, "author-"));
const project = assertWithin(root, join(root, "author-contract"));
const report = {
  schema: 1,
  runId: randomUUID(),
  evidenceClass: "generated-author-contract/non-visual",
  commit: cli.commit,
  cli,
  runnerFingerprint: await runnerFingerprint(),
  startedAt: new Date().toISOString(),
  results: [],
  cleanup: { status: "Not run" },
  limitations: [
    "Compilation and generated bundle checks do not prove real host permission enforcement, provider behavior or public API freeze.",
    "Generated project and compiler fixtures are retained as disposable evidence under this owned .runs/author-* directory.",
  ],
};
const exec = promisify(execFile);
const command = (binary, args, cwd) =>
  exec(binary, args, {
    cwd,
    windowsHide: true,
    timeout: 30000,
    maxBuffer: 256 * 1024,
  });
const check = () =>
  command(
    process.execPath,
    [
      join(repo, "node_modules/typescript/bin/tsc"),
      "--noEmit",
      "--pretty",
      "false",
    ],
    project,
  );
let stage;
try {
  stage = "generated-project";
  await command(cli.binary, ["init", "Author Contract"], root);
  await check(); // The unmodified shipped example must type-check.
  report.results.push({ id: stage, status: "Pass" });

  stage = "public-api-positive-and-negative-types";
  const contract = `import type { GrainCapability, GrainToolHandler, GrainToolResult } from "../grain";

const result: GrainToolResult = { ok: { title: "Result", body: "Done", details: [{ label: "id", value: "123" }] } };
const handler: GrainToolHandler = async (args, context) => {
  const key: string | null = context.idempotencyKey;
  if (args.fail === true) return { error: { class: "network", message: "private diagnostic" } };
  return { ok: key === null ? null : { body: key } };
};
grain.actions({ read: handler, plain: () => ({ body: "Compatibility" }), text: () => "Text", empty: () => null, result: () => result });
const capability: GrainCapability = "storage";
void capability;
void grain.storage.set("own-key", { nested: [1, true, null] });
void grain.net.fetch("https://api.example.com/items", { method: "GET", auth: true });
void grain.auth.status().then(connection => {
  if (connection) {
    const scopes: string[] = connection.granted_scopes;
    void scopes;
    // @ts-expect-error raw account tokens are not author API
    void connection.access_token;
  }
});
grain.actions({ immutable: (args, context) => {
  // @ts-expect-error the host's invocation context is read-only
  context.idempotencyKey = "replacement";
  // @ts-expect-error no ambient selected text in invocation context
  void context.selectedText;
  // @ts-expect-error validated arguments cannot contain callable values
  args.callback = () => "bad";
  return { ok: { body: "Done" } };
} });
// @ts-expect-error screen capture belongs to the agent
grain.screen.capture();
// @ts-expect-error no extension prompt customization
grain.prompt.set("replacement");
// @ts-expect-error no extension daemon event subscription
grain.on("TranscriptionComplete", () => {});
// @ts-expect-error no retired capability grant
const retired: GrainCapability = "capture";
// @ts-expect-error daemon events are not exported to extension projects
type InternalEvent = import("../grain").DaemonEvent;
// @ts-expect-error an outcome cannot declare both success and error
const ambiguous: GrainToolResult = { ok: {}, error: { class: "network" } };
// @ts-expect-error extension-authored follow-up cannot be resumed by the host
const followup: GrainToolResult = { needsInteraction: { kind: "confirm" } };
// @ts-expect-error display title must be text
const malformed: GrainToolResult = { ok: { title: 42 } };
// @ts-expect-error unsupported error class
const badClass: GrainToolResult = { error: { class: "grant_permission" } };
// @ts-expect-error envelope metadata must not accompany a success branch
const extraEnvelope: GrainToolResult = { ok: {}, source: "spoof" };
// @ts-expect-error an absent envelope is not an undefined success branch
const undefinedEnvelope: GrainToolResult = { ok: undefined };
`;
  const path = assertWithin(root, join(project, "src/contract-check.ts"));
  await writeFile(path, contract);
  await check(); // Every expect-error must encounter a real compiler diagnostic.
  report.results.push({ id: stage, status: "Pass", negativeCases: 15 });

  stage = "compiler-negative-control";
  await writeFile(
    path,
    contract.replace(
      "// @ts-expect-error raw account tokens are not author API",
      "// Deliberate unguarded negative control",
    ),
  );
  await assert.rejects(check(), (error) => {
    assert.equal(error.code, 2, "Expected a TypeScript compilation refusal");
    assert.match(
      error.stdout,
      /TS2339: Property 'access_token' does not exist/,
    );
    return true;
  });
  await writeFile(path, contract);
  report.results.push({ id: stage, status: "Pass", expectedCompilerExit: 2 });

  stage = "generated-bundle";
  const bundle = buildSync({
    absWorkingDir: project,
    entryPoints: ["src/main.ts"],
    bundle: true,
    format: "iife",
    platform: "browser",
    target: "es2020",
    write: false,
  });
  let handlers;
  runInNewContext(
    bundle.outputFiles[0].text,
    {
      grain: {
        actions: (value) => {
          handlers = value;
        },
      },
    },
    { timeout: 1000 },
  );
  assert.deepEqual(Object.keys(handlers), ["hello"]);
  assert.deepEqual(
    JSON.parse(
      JSON.stringify(await handlers.hello({}, { idempotencyKey: null })),
    ),
    {
      ok: { title: "Author Contract", body: "Hello from this tool." },
    },
  );
  await writeFile(
    assertWithin(root, join(project, "bundle.js")),
    bundle.outputFiles[0].text,
  );
  const pkg = JSON.parse(await readFile(join(project, "package.json"), "utf8"));
  assert.equal(pkg.scripts.check, "tsc --noEmit");
  assert.ok(pkg.scripts.build.startsWith("tsc --noEmit && "));
  report.results.push({ id: stage, status: "Pass" });
} catch (error) {
  report.results.push({
    id: stage,
    status: "Fail",
    error: error.message,
    // Generated, account-free source only. Keep compiler stage failures attributable.
    diagnostics:
      typeof error.stdout === "string"
        ? error.stdout.slice(0, 8192)
        : undefined,
  });
  process.exitCode = 1;
} finally {
  try {
    await stop(); // Release any bundler resources before recording completion.
    report.cleanup = { status: "Pass", credentials: "None", listeners: "None" };
  } catch (error) {
    report.cleanup = { status: "Fail", error: error.message };
    process.exitCode = 1;
  }
  report.finishedAt = new Date().toISOString();
  await writeReport(root, report);
  console.log(
    JSON.stringify({ root, results: report.results, cleanup: report.cleanup }),
  );
}
