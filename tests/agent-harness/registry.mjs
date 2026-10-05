import assert from "node:assert/strict";
import { readFile, writeFile, readdir, realpath } from "node:fs/promises";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { assertWithin, waitFor as cleanupWait } from "./support.mjs";

const ID = "com.grain.harness.lifecycle";
// Opaque, nonsecret registry metadata. No corresponding vault entry is created.
const POINTER = "a".repeat(32);

export async function withRegistryLock(
  ctx,
  operation,
  fileName = "extensions.json",
) {
  assert.ok(
    ["extensions.json", "mcp-connections.json", "grain.settings.json"].includes(
      fileName,
    ),
  );
  const path = assertWithin(ctx.root, join(ctx.root, "data", fileName));
  assertWithin(await realpath(ctx.root), await realpath(path));
  const child = spawn(
    "powershell.exe",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      join(ctx.here, "registry-lock.ps1"),
      "-Root",
      ctx.root,
      "-FileName",
      fileName,
    ],
    { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] },
  );
  let output = "",
    failure;
  child.on("error", (error) => {
    failure = error;
  });
  child.stdin.on("error", (error) => {
    failure = error;
  });
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (bytes) => {
      output = (output + bytes.toString()).slice(-4096);
    });
  }
  try {
    await ctx.waitFor(
      "Owned registry lock readiness",
      () => {
        if (failure) throw failure;
        if (child.exitCode !== null)
          throw new Error(`Lock helper exited: ${output}`);
        return output.includes("HARNESS_REGISTRY_LOCK_READY");
      },
      { timeoutMs: 10000 },
    );
    await operation();
  } finally {
    if (child.exitCode === null) child.stdin.end("release\n");
    try {
      await cleanupWait(
        "Owned registry lock release",
        () => child.exitCode !== null,
        { timeoutMs: 5000 },
      );
    } catch (error) {
      child.kill();
      await cleanupWait(
        "Owned lock helper exit",
        () => child.exitCode !== null || child.signalCode !== null,
        { timeoutMs: 5000 },
      );
      throw error;
    }
    assert.equal(child.exitCode, 0, `Lock helper failed: ${output}`);
  }
}

export function registryHandlers(ctx) {
  const { root, status, fixture, greeting, restartHost, imported, allow } = ctx;
  const path = assertWithin(root, join(root, "data/extensions.json"));
  const read = async () => JSON.parse(await readFile(path, "utf8"));
  let evidence = [];
  async function restart(stage, beforeLaunch) {
    const previous = (await status()).hostSession;
    await restartHost(beforeLaunch);
    const state = await status();
    assert.equal(state.hostSession, previous + 1);
    evidence.push({
      stage,
      session: state.hostSession,
      enabled: state.fixtureEnabled,
      installed: state.fixtureInstalled,
      workerCount: state.worker.count,
    });
    return state;
  }
  return {
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "native.registry-preservation"() {
        await imported("registry-preserved");
        await allow();
        await greeting("registry-preserved");
        const original = await read();
        const seeded = structuredClone(original);
        seeded.records[ID].authentication_session = POINTER;
        await restart("restart:opaque-pointer", () => {
          const value = structuredClone(seeded);
          if (ctx.fault === "lost-pointer")
            delete value.records[ID].authentication_session;
          return writeFile(path, JSON.stringify(value));
        });
        assert.equal(
          (await read()).records[ID].authentication_session,
          POINTER,
          "Restart lost the opaque native account pointer",
        );
        assert.deepEqual((await read()).records[ID], seeded.records[ID]);
        await greeting("registry-preserved");
        await fixture("disable");
        const disabled = (await read()).records[ID];
        const state = await restart("restart:disabled-pointer");
        assert.equal(state.fixtureEnabled, false);
        assert.equal(state.worker.count, 0);
        assert.deepEqual((await read()).records[ID], disabled);
        await fixture("enable");
        await greeting("registry-preserved");
        const record = (await read()).records[ID];
        const artifact = assertWithin(
          root,
          join(
            root,
            "data/extensions",
            ID,
            record.installed_version,
            record.artifact_sha256,
            "pack.grainpack.json",
          ),
        );
        assertWithin(await realpath(root), await realpath(artifact));
        const savedArtifact = await readFile(artifact);
        const corrupt = Buffer.from(savedArtifact);
        corrupt[corrupt.length - 2] ^= 1;
        const userData = assertWithin(
          root,
          join(root, "data/registry-recovery-user-data.txt"),
        );
        await writeFile(userData, "owned preservation sentinel");
        await restart("restart:corrupt-artifact-quarantined", () =>
          writeFile(artifact, corrupt),
        );
        const quarantine = await read();
        assert.match(quarantine.quarantined[ID], /verification|hash/i);
        assert.equal(quarantine.records[ID].enabled, false);
        assert.equal(quarantine.records[ID].actions_approved, undefined);
        assert.equal(quarantine.records[ID].authentication_session, POINTER);
        assert.equal((await status()).worker.count, 0);
        await assert.rejects(fixture("enable"), /verification|hash mismatch/i);
        const restored = await restart("restart:quarantine-preserved", () =>
          writeFile(artifact, savedArtifact),
        );
        assert.equal(restored.fixtureEnabled, false);
        assert.deepEqual((await read()).records[ID], quarantine.records[ID]);
        assert.equal(
          (await read()).quarantined[ID],
          quarantine.quarantined[ID],
        );
        assert.deepEqual(await readFile(artifact), savedArtifact);
        assert.equal(
          await readFile(userData, "utf8"),
          "owned preservation sentinel",
        );
        await assert.rejects(fixture("enable"), /needsActions/);
        assert.equal((await status()).fixtureEnabled, false);
        assert.equal((await status()).worker.count, 0);
        await imported("registry-recovered");
        assert.equal((await read()).quarantined?.[ID], undefined);
        await allow();
        await greeting("registry-recovered");
        await restart("restart:explicit-recovery");
        await greeting("registry-recovered");
        evidence.push({
          stage: "pointer-preservation",
          preserved: true,
          vaultCoverage:
            "Excluded: nonsecret opaque pointer with no credential entry",
        });
      },
      async "native.registry-save-failure"() {
        await imported("registry-save");
        await allow();
        await greeting("registry-save");
        await fixture("disable");
        const saved = await readFile(path);
        await withRegistryLock(ctx, async () => {
          await assert.rejects(
            fixture("enable"),
            /persist|denied|sharing|os error/i,
          );
          assert.equal((await status()).fixtureEnabled, false);
          assert.equal((await status()).worker.count, 0);
          assert.deepEqual(await readFile(path), saved);
          assert.equal(
            (await readdir(join(root, "data"))).filter(
              (name) =>
                name.startsWith(".grain-extension-") &&
                name.endsWith(".pending"),
            ).length,
            0,
          );
        });
        await restart("restart:failed-enable-preserved");
        assert.equal((await status()).fixtureEnabled, false);
        await fixture("enable");
        await greeting("registry-save");
        const enabled = await readFile(path);
        await withRegistryLock(ctx, async () => {
          await assert.rejects(
            fixture("disable"),
            /persist|denied|sharing|os error/i,
          );
          assert.equal((await status()).fixtureEnabled, false);
          assert.equal((await status()).worker.count, 0);
          assert.deepEqual(await readFile(path), enabled);
        });
        // A failed refusal-only save must not claim durable disablement.
        await restart("restart:failed-disable-reported");
        assert.equal((await status()).fixtureEnabled, true);
        await greeting("registry-save");
        await fixture("disable");
        await restart("restart:successful-disable");
        assert.equal((await status()).fixtureEnabled, false);
        await fixture("enable");
        await greeting("registry-save");
        evidence.push({
          stage: "bounded-save-failures",
          failedEnableRolledBack: true,
          failedDisableLiveOnly: true,
          explicitDisablePersisted: true,
        });
      },
    },
  };
}
