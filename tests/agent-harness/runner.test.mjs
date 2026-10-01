import assert from "node:assert/strict";
import test from "node:test";
import {
  mkdtemp,
  readFile,
  rm,
  mkdir,
  writeFile,
  rename,
} from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { nextReply, startModel, FIXTURE_ID } from "./model.mjs";
import { scenarios, selectScenarios } from "./scenarios.mjs";
import { assertWithin, waitFor, writeReport } from "./support.mjs";

const exec = promisify(execFile);
const here = dirname(fileURLToPath(import.meta.url));
const body = (results = [], actions = []) => ({
  model: "harness-scripted",
  messages: [
    { role: "user", content: "Harness request: hello" },
    ...results.map((content) => ({ role: "tool", content })),
  ],
  tools: ["search_tools", "load_extension", ...actions].map((name) => ({
    type: "function",
    function: { name, description: "Harness fast hello" },
  })),
});

test("scenario IDs are unique and each suite is explicit", () => {
  assert.equal(
    new Set(scenarios.map((scenario) => scenario.id)).size,
    scenarios.length,
  );
  assert.equal(selectScenarios("smoke").length, 2);
  assert.equal(selectScenarios("store").length, 3);
  assert.equal(selectScenarios("registry-recovery").length, 3);
  assert.equal(selectScenarios("all").length, scenarios.length);
  assert.throws(() => selectScenarios("made-up"), /Unknown suite/);
});

test(
  "native Escape refusal records diagnostics without sending input",
  { skip: process.platform !== "win32" },
  async () => {
    // The Node test process owns no Grain Assist window. Never borrow another
    // application's PID or synthesize input for this refusal check.
    await assert.rejects(
      exec(
        "powershell.exe",
        [
          "-NoProfile",
          "-File",
          join(here, "native-input.ps1"),
          "-OwnerPid",
          String(process.pid),
        ],
        { timeout: 10000, windowsHide: true, maxBuffer: 16384 },
      ),
      (error) => {
        assert.match(error.stderr, /Owned visible Agent window missing/);
        const observation = JSON.parse(error.stdout.trim());
        assert.equal(observation.schema, 1);
        assert.equal(observation.kind, "native-escape");
        assert.equal(observation.accepted, 0);
        assert.equal(observation.focusClickAccepted, 0);
        assert.equal(observation.foregroundBefore, false);
        assert.equal(observation.foregroundAfter, false);
        assert.equal(observation.foregroundRequested, false);
        return true;
      },
    );
  },
);

test(
  "owned registry lock denies publication and releases after cancellation",
  { skip: process.platform !== "win32" },
  async () => {
    const { withRegistryLock } = await import("./registry.mjs");
    const root = await mkdtemp(join(tmpdir(), "grain-registry-lock-"));
    const path = join(root, "data/extensions.json");
    const moved = join(root, "data/released.json");
    const cancellation = new AbortController();
    try {
      await mkdir(join(root, "data"));
      await writeFile(
        join(root, ".grain-agent-harness.json"),
        JSON.stringify({ schema: 1, runId: randomUUID(), modelPort: 9000 }),
      );
      await writeFile(path, "{}");
      await assert.rejects(
        withRegistryLock(
          {
            root,
            here,
            waitFor: (name, operation, options) =>
              waitFor(name, operation, {
                ...options,
                signal: cancellation.signal,
              }),
          },
          async () => {
            await assert.rejects(rename(path, moved), (error) =>
              ["EPERM", "EACCES", "EBUSY"].includes(error.code),
            );
            cancellation.abort();
            throw new Error("deliberate operation cancellation");
          },
        ),
        /deliberate operation cancellation/,
      );
      // Cleanup must complete even though ordinary scenario observations abort.
      await rename(path, moved);
      assert.equal(await readFile(moved, "utf8"), "{}");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  },
);

test("store fixture signs exact bytes with a distinct fixed test anchor", async () => {
  const { STORE_PUBLIC_KEY, signStoreBytes } =
    await import("./store-fixture.mjs");
  const { createPublicKey, createHash, verify } = await import("node:crypto");
  const decoded = Buffer.from(STORE_PUBLIC_KEY, "base64");
  const key = createPublicKey({
    key: Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      decoded.subarray(10),
    ]),
    format: "der",
    type: "spki",
  });
  const data = Buffer.from('{"fixture":true}');
  const sig = Buffer.from(
    signStoreBytes(data).split("\n")[1],
    "base64",
  ).subarray(10);
  assert.equal(
    verify(null, createHash("blake2b512").update(data).digest(), key, sig),
    true,
  );
  assert.equal(
    verify(null, createHash("blake2b512").update("changed").digest(), key, sig),
    false,
  );
  const backend = await readFile(
    resolve(here, "../../src-tauri/src/grain_agent_harness.rs"),
    "utf8",
  );
  assert.ok(
    backend.includes(STORE_PUBLIC_KEY),
    "Runner and host test anchors differ",
  );
  const production = await readFile(
    resolve(here, "../../crates/grain-core/src/trust.rs"),
    "utf8",
  );
  assert.ok(
    !production.includes(STORE_PUBLIC_KEY),
    "Test key leaked into production trust",
  );
});

test("owned store transport preserves byte length and releases a held request", async () => {
  const { startStore } = await import("./store-fixture.mjs");
  const store = await startStore(here);
  try {
    await store.configure();
    const base = `http://127.0.0.1:${store.port}`;
    const index = await (await fetch(`${base}/index.json`)).json();
    const blob = await (
      await fetch(`${base}/blob/${store.hash}.grainpack`)
    ).arrayBuffer();
    assert.equal(blob.byteLength, index.entries[0].size);
    store.corruptBlob();
    const corrupt = await (
      await fetch(`${base}/blob/${store.hash}.grainpack`)
    ).arrayBuffer();
    assert.equal(corrupt.byteLength, blob.byteLength);
    assert.notDeepEqual(Buffer.from(corrupt), Buffer.from(blob));
    store.hold("/index.json");
    const pending = fetch(`${base}/index.json`);
    await waitFor("Held fixture request", () => store.heldCount === 1);
    store.release();
    assert.equal((await pending).status, 200);
    await waitFor("Released fixture request", () => store.heldCount === 0);
  } finally {
    await store.close();
  }
});

test("scripted provider enforces search, selective load and offered-action discipline", () => {
  const first = nextReply(body());
  assert.equal(first.tool_calls[0].function.name, "search_tools");
  assert.equal(
    JSON.parse(first.tool_calls[0].function.arguments).extension_id,
    FIXTURE_ID,
  );
  assert.equal(
    nextReply(body(["search result"])).tool_calls[0].function.name,
    "load_extension",
  );
  assert.throws(
    () => nextReply(body([], ["act__unexpected"])),
    /Initial Agent frame/,
  );
  assert.throws(() => nextReply(body(["search", "loaded"])), /exactly one/);
  assert.throws(
    () => nextReply(body(["search", "loaded"], ["act__one", "act__two"])),
    /exactly one/,
  );
  assert.equal(
    nextReply(body(["search", "loaded"], ["act__hello"])).tool_calls[0].function
      .name,
    "act__hello",
  );
});

test("declined/failed tool result produces no automatic tool replay", () => {
  const result = nextReply(body(["search", "loaded", "Declined by user"]));
  assert.equal(result.tool_calls, undefined);
  assert.match(result.content, /Declined by user/);
});

test("failure suite is isolated and private error text is rejected by the model oracle", () => {
  assert.equal(selectScenarios("native-failures").length, 6);
  assert.ok(
    selectScenarios("lifecycle").every(
      (scenario) => scenario.suite !== "native-failures",
    ),
  );
  assert.throws(
    () => nextReply(body(["search", "loaded", "HARNESS_PRIVATE_ERROR_MARKER"])),
    /Private worker error/,
  );
});

test("invalid-input requests use real selected schema with distinct rejected payloads", () => {
  for (const instruction of [
    "invalid_arguments",
    "missing_arguments",
    "wrong_arguments",
    "oversized_arguments",
    "malformed_arguments",
  ]) {
    const request = body(["search", "loaded"], ["act__input_echo"]);
    request.messages[0].content = `Harness request: ${instruction}`;
    request.tools.at(-1).function.description = "Harness input echo";
    const reply = nextReply(request);
    assert.equal(reply.tool_calls[0].function.name, "act__input_echo");
    const raw = reply.tool_calls[0].function.arguments;
    if (instruction === "malformed_arguments")
      assert.throws(() => JSON.parse(raw));
    if (instruction === "oversized_arguments")
      assert.ok(Buffer.byteLength(raw) > 65536);
    if (instruction === "wrong_arguments")
      assert.equal(JSON.parse(raw).text, 42);
    if (instruction === "missing_arguments")
      assert.deepEqual(JSON.parse(raw), {});
    if (instruction === "invalid_arguments")
      assert.ok(
        Object.hasOwn(JSON.parse(raw), "HARNESS_PRIVATE_ARGUMENT_MARKER"),
      );
  }
});

test("actual local HTTP fixture records rejection and releases its listener", async () => {
  const model = await startModel();
  try {
    const endpoint = `http://127.0.0.1:${model.port}/v1/chat/completions`;
    const good = await fetch(endpoint, {
      method: "POST",
      body: JSON.stringify(body()),
    });
    assert.equal(good.status, 200);
    assert.equal(
      (await good.json()).choices[0].message.tool_calls[0].function.name,
      "search_tools",
    );
    const wrong = await fetch(endpoint, {
      method: "POST",
      body: JSON.stringify(body([], ["act__unselected"])),
    });
    assert.equal(wrong.status, 400);
    assert.equal(model.journal.at(-1).state, "error");
  } finally {
    await model.close();
  }
  await assert.rejects(fetch(`http://127.0.0.1:${model.port}`));
});

test("path ownership refuses roots and escaping targets", () => {
  const root = resolve(tmpdir(), "harness-root");
  assert.throws(() => assertWithin(root, root), /child/);
  assert.throws(() => assertWithin(root, join(root, "../outside")), /child/);
  assert.equal(
    assertWithin(root, join(root, "evidence")),
    join(root, "evidence"),
  );
});

test("a failed observation is a failed assertion, never a passing timeout", async () => {
  await assert.rejects(
    waitFor("intentional failure", () => false, {
      timeoutMs: 30,
      intervalMs: 5,
    }),
    /intentional failure timed out/,
  );
});

test("cancellation stops observation rather than accepting a partial result", async () => {
  const cancellation = new AbortController();
  const timer = setTimeout(
    () => cancellation.abort(new Error("Owned run cancelled")),
    10,
  );
  try {
    await assert.rejects(
      waitFor("cancelled run", () => false, {
        timeoutMs: 5000,
        intervalMs: 5,
        signal: cancellation.signal,
      }),
      /Owned run cancelled/,
    );
  } finally {
    clearTimeout(timer);
  }
});

test("report preserves Blocked/Not run and identifies partial evidence", async () => {
  const root = await mkdtemp(join(tmpdir(), "grain-harness-report-"));
  try {
    await writeReport(root, {
      runId: "test",
      evidenceClass: "test-only",
      commit: "test",
      cleanup: { status: "Pass" },
      results: [
        { id: "one", status: "Blocked", error: "prerequisite missing" },
        { id: "two", status: "Not run" },
      ],
    });
    const report = JSON.parse(
      await readFile(join(root, "evidence/report.json"), "utf8"),
    );
    assert.equal(report.results[0].status, "Blocked");
    assert.equal(report.results[1].status, "Not run");
    assert.match(
      await readFile(join(root, "evidence/report.md"), "utf8"),
      /manual check statuses are unchanged/,
    );
  } finally {
    assertWithin(tmpdir(), root);
    await rm(root, { recursive: true, force: true });
  }
});

test("missing application binary exits nonzero and emits blocked evidence", async () => {
  const root = await mkdtemp(join(tmpdir(), "grain-harness-blocked-"));
  try {
    const execution = await exec(process.execPath, [
      join(here, "run.mjs"),
      "--binary",
      join(root, "grain-agent-harness.exe"),
      "--output",
      root,
    ]).then(
      (value) => ({ ...value, code: 0 }),
      (error) => error,
    );
    assert.equal(execution.code, 2);
    const reportPath = execution.stdout
      .match(/Report: (.+report\.md)/)?.[1]
      .trim()
      .replace(/report\.md$/, "report.json");
    assert.ok(reportPath);
    assertWithin(root, reportPath);
    const report = JSON.parse(await readFile(reportPath, "utf8"));
    assert.equal(report.results[0].status, "Blocked");
    assert.match(
      report.results[0].error,
      process.platform === "win32"
        ? /Harness executable is missing/
        : /requires Windows WebView2/,
    );
    assert.ok(
      report.results.slice(1).every((result) => result.status === "Not run"),
    );
  } finally {
    assertWithin(tmpdir(), root);
    await rm(root, { recursive: true, force: true });
  }
});

test("installation suite retains distinct owners and declares no privileges", async () => {
  const { fixturePackage } = await import("./installation.mjs");
  const first = await fixturePackage(here, "installed-one");
  const second = await fixturePackage(here, "installed-two", true);
  assert.equal(selectScenarios("native-installation").length, 6);
  assert.equal(first.manifest.version, second.manifest.version);
  assert.deepEqual(first.manifest.permissions, []);
  assert.equal(first.manifest.contributes.authentication, undefined);
  assert.equal(first.manifest.contributes.actions.length, 1);
  assert.equal(first.manifest.contributes.actions[0].risk, "confirm");
  assert.notEqual(first.manifest.entry_source, second.manifest.entry_source);
  assert.match(first.manifest.entry_source, /"installed-one"/);
  assert.match(second.manifest.entry_source, /"installed-two"/);
  assert.notEqual(first.manifest.name, second.manifest.name);
});

test("CLI fixture uses only the maintained local build and fixed owner paths", async () => {
  const { prepareCliProject } = await import("./packaging.mjs");
  const root = await mkdtemp(join(tmpdir(), "grain-cli-fixture-"));
  try {
    const project = await prepareCliProject({
      root,
      here,
      repo: resolve(here, "../.."),
      folder: "fixture-b",
      revision: "cli-test",
    });
    const manifest = JSON.parse(
      await readFile(join(project, "manifest.json"), "utf8"),
    );
    assert.equal(manifest.id, "com.grain.harness.lifecycle");
    assert.equal(manifest.icon, "icon.png");
    assert.equal(manifest.entry, "dist/main.js");
    assert.deepEqual(manifest.permissions, []);
    const pkg = JSON.parse(
      await readFile(join(project, "package.json"), "utf8"),
    );
    assert.equal(pkg.scripts.build, "node build.mjs");
    assert.equal(pkg.dependencies, undefined);
    assert.match(
      await readFile(join(project, "src/main.js"), "utf8"),
      /"cli-test"/,
    );
    const png = await readFile(join(project, "icon.png"));
    assert.equal(png.readUInt32BE(16), 512);
    assert.equal(png.readUInt32BE(20), 512);
    await assert.rejects(
      prepareCliProject({
        root,
        here,
        repo: resolve(here, "../.."),
        folder: "../escape",
        revision: "never",
      }),
    );
  } finally {
    await rm(assertWithin(tmpdir(), root), { recursive: true, force: true });
  }
});
