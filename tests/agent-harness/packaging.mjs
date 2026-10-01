// Real Grain CLI + local esbuild. Fixed fixture code; no dependency install,
// external account, runtime watcher, arbitrary command, or ordinary profile.
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { readFile, writeFile, mkdir, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { assertWithin, waitFor as poll } from "./support.mjs";
import { hashFile } from "./stamp.mjs";
import { fixturePackage } from "./installation.mjs";

const exec = promisify(execFile);
const FIXTURE_ID = "com.grain.harness.lifecycle";

export async function prepareCliProject({
  root,
  repo,
  here,
  folder,
  revision,
}) {
  assert.ok(["fixture", "fixture-b"].includes(folder));
  const project = assertWithin(root, join(root, folder));
  const pack = await fixturePackage(here, revision);
  const { entry_source, ...manifest } = pack.manifest;
  manifest.icon = "icon.png";
  await mkdir(assertWithin(root, join(project, "src")), { recursive: true });
  await writeFile(
    join(project, "manifest.json"),
    JSON.stringify({ ...manifest, entry: "dist/main.js" }),
  );
  await writeFile(join(project, "src/main.js"), entry_source);
  // Existing, fully decoded 512px Grain icon, used only for a disposable fixture.
  await copyFile(
    join(repo, "src-tauri/icons/icon.png"),
    join(project, "icon.png"),
  );
  await writeFile(
    join(project, "package.json"),
    JSON.stringify({ private: true, scripts: { build: "node build.mjs" } }),
  );
  const esbuildUrl = pathToFileURL(
    join(repo, "node_modules/esbuild/lib/main.js"),
  ).href;
  await writeFile(
    join(project, "build.mjs"),
    `import { build } from ${JSON.stringify(esbuildUrl)};\nawait build({ entryPoints: ['src/main.js'], bundle: true, format: 'iife', platform: 'browser', target: 'es2020', outfile: 'dist/main.js', sourcemap: true, logLevel: 'silent' });\n`,
  );
  return project;
}

async function cliCommand(binary, args, cwd, waitFor) {
  const child = spawn(binary, args, {
    cwd,
    windowsHide: true,
    env: {
      ...process.env,
      npm_config_offline: "true",
      npm_config_audit: "false",
      npm_config_fund: "false",
      npm_config_update_notifier: "false",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "",
    overflow = false,
    launchError;
  const append = (chunk) => {
    output += chunk.toString();
    if (output.length > 32768) {
      overflow = true;
      output = output.slice(0, 32768);
    }
  };
  child.stdout.on("data", append);
  child.stderr.on("data", append);
  child.once("error", (error) => {
    launchError = error;
  });
  try {
    await waitFor(
      "Owned extension CLI completion",
      () =>
        launchError ||
        overflow ||
        child.exitCode !== null ||
        child.signalCode !== null,
      { timeoutMs: 30000 },
    );
    if (launchError) throw launchError;
    assert.equal(overflow, false, "CLI output exceeded the evidence budget");
    assert.equal(
      child.exitCode,
      0,
      `CLI ${args[0]} failed: ${output.slice(-2000)}`,
    );
    return {
      command: args[0],
      pid: child.pid,
      exitCode: child.exitCode,
      outputBytes: Buffer.byteLength(output),
    };
  } finally {
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      await exec("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        windowsHide: true,
        timeout: 6000,
      });
      await poll(
        "Owned CLI tree exit",
        () => child.exitCode !== null || child.signalCode !== null,
        { timeoutMs: 3000 },
      );
    }
  }
}

export async function packagedOwnership(ctx) {
  const {
    root,
    repo,
    here,
    cli,
    fixture,
    request,
    greeting,
    status,
    events,
    restartHost,
    activate,
    waitFor,
    allow,
    note,
    fault,
  } = ctx;
  assert.ok(cli, "CLI identity must be verified before starting the scenario");
  const steps = [];
  async function packed(folder, revision) {
    const project = await prepareCliProject({
      root,
      repo,
      here,
      folder,
      revision,
    });
    // pack runs the real npm build itself; doctor follows so it sees built bytes.
    const output = assertWithin(root, join(root, "fixture.grainpack"));
    for (const args of [["pack", "--output", output], ["doctor"]]) {
      const step = {
        revision,
        ...(await cliCommand(cli.binary, args, project, waitFor)),
      };
      steps.push(step);
      ctx.record({ stage: "cli:command", ...step });
    }
    const bytes = await readFile(output);
    const pack = JSON.parse(bytes);
    assert.equal(pack.manifest.id, FIXTURE_ID);
    assert.equal(pack.manifest.version, "0.1.0");
    assert.deepEqual(pack.manifest.permissions, []);
    assert.equal(pack.manifest.contributes.authentication, undefined);
    assert.equal(pack.manifest.contributes.actions.length, 1);
    assert.equal(pack.manifest.contributes.actions[0].risk, "confirm");
    assert.ok(pack.payloads.iconPng, "CLI did not embed the submission icon");
    assert.ok(
      pack.manifest.entry_source.includes(revision),
      "CLI bundled obsolete source",
    );
    return bytes;
  }
  async function refuse(page, before) {
    await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await waitFor(
      "Packaged old approval refusal",
      async () =>
        !(await status()).agent.active &&
        !(await status()).agent.pendingApproval,
    );
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
    );
    assert.match(
      await page.locator("body").innerText(),
      /no longer approved|changed|expired|unavailable/i,
    );
  }
  const first = await packed("fixture", "packaged-one");
  await fixture("import");
  await allow();
  await greeting("packaged-one");
  let before = await status(),
    pending = await request();
  const second = await packed("fixture", "packaged-two");
  assert.notDeepEqual(
    first,
    second,
    "Same-version compilation did not change bytes",
  );
  if (fault === "stale-package")
    await writeFile(join(root, "fixture.grainpack"), first);
  await fixture("import");
  await refuse(pending, before);
  await greeting("packaged-two");
  // Create both historical lookup candidates only while the owned host is gone.
  // Neither is a mutation of the live registry or its hash-owned current bytes.
  const flat = assertWithin(
    root,
    join(root, "data/extensions", `${FIXTURE_ID}.grainpack.json`),
  );
  const versioned = assertWithin(
    root,
    join(root, "data/extensions", FIXTURE_ID, "0.1.0/pack.grainpack.json"),
  );
  await restartHost(async () => {
    await writeFile(flat, first);
    await writeFile(versioned, first);
  });
  await greeting("packaged-two");
  await note("cli:legacy-files-ignored");
  const registryPath = assertWithin(root, join(root, "data/extensions.json"));
  const installedRecord = JSON.parse(await readFile(registryPath, "utf8"))
    .records[FIXTURE_ID];
  assert.equal(installedRecord.enabled, true);
  await packed("fixture", "cli-developer-a");
  // Keep the original installed approval through the complete round trip.
  // Restoring the same visible owner must not resurrect its old call identity.
  before = await status();
  pending = await request();
  await fixture("register");
  await fixture("enable");
  await fixture("unload");
  await refuse(pending, before);
  assert.deepEqual(
    JSON.parse(await readFile(registryPath, "utf8")).records[FIXTURE_ID],
    installedRecord,
    "Restoration changed the saved installed record",
  );
  await note("cli:original-installed-approval-refused-after-restoration");
  await greeting("packaged-two");
  before = await status();
  pending = await request();
  await fixture("register");
  await fixture("enable");
  await refuse(pending, before);
  await greeting("cli-developer-a");
  before = await status();
  pending = await request();
  await packed("fixture-b", "cli-developer-b");
  await fixture("load_second");
  await fixture("enable");
  await refuse(pending, before);
  await greeting("cli-developer-b");
  await restartHost();
  assert.equal((await status()).fixtureOwner, "fixture-b");
  await greeting("cli-developer-b");
  before = await status();
  pending = await request();
  await fixture("unload");
  await refuse(pending, before);
  assert.equal((await status()).fixtureOwner, "installed");
  assert.equal((await status()).fixtureEnabled, true);
  await greeting("packaged-two");
  await restartHost();
  await greeting("packaged-two");
  assert.deepEqual(
    await readFile(flat),
    first,
    "Legacy flat bytes were replaced/deleted",
  );
  assert.deepEqual(
    await readFile(versioned),
    first,
    "Legacy version bytes were replaced/deleted",
  );
  assert.equal(
    await hashFile(cli.binary),
    cli.binarySha256,
    "CLI executable changed during execution",
  );
  const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
  return {
    cliSteps: steps,
    initialPackSha256: sha(first),
    replacementPackSha256: sha(second),
    iconSha256: await hashFile(join(repo, "src-tauri/icons/icon.png")),
    legacyFilesPreserved: true,
    installedRecordPreserved: true,
    accountCoverage: "Not exercised: no OAuth fixture",
  };
}
