// Installation/consent scenarios operate on the real main and Agent WebViews.
// Only maintained permission-free fixture paths under the owned profile are used.
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";

export async function fixturePackage(here, revision, revised = false) {
  const project = JSON.parse(
    await readFile(join(here, "fixtures/lifecycle/manifest.json"), "utf8"),
  );
  const { entry, ...manifest } = project;
  manifest.contributes.actions = manifest.contributes.actions.filter(
    (action) => action.id === "hello",
  );
  if (revised) {
    manifest.name = "Harness Lifecycle Revised";
    manifest.contributes.actions[0].title = "Harness fast hello (revised)";
  }
  manifest.entry_source = (
    await readFile(join(here, "fixtures/lifecycle/main.js"), "utf8")
  ).replace('"one"', JSON.stringify(revision));
  return { manifest, payloads: {} };
}

export function installationHandlers(ctx) {
  const {
    root,
    here,
    status,
    fixture,
    request,
    greeting,
    closePanel,
    activate,
    waitFor,
    events,
    restartHost,
  } = ctx;
  let evidence = [];
  async function note(stage) {
    const value = await status();
    evidence.push({
      stage,
      session: value.hostSession,
      registryAvailable: value.registryAvailable,
      enabled: value.fixtureEnabled,
      approved: value.fixtureApproved,
      owner: value.fixtureOwner,
      workerCount: value.worker.count,
      pendingApproval: value.agent.pendingApproval,
    });
  }
  async function imported(revision, revised = false) {
    await writeFile(
      assertWithin(root, join(root, "fixture.grainpack")),
      JSON.stringify(await fixturePackage(here, revision, revised)),
    );
    await fixture("import");
    await note(`import:${revision}`);
  }
  async function project(folder, revision) {
    const pack = await fixturePackage(here, revision);
    const { entry_source, ...manifest } = pack.manifest;
    await mkdir(assertWithin(root, join(root, folder, "dist")), {
      recursive: true,
    });
    await writeFile(
      assertWithin(root, join(root, folder, "manifest.json")),
      JSON.stringify({ ...manifest, entry: "dist/main.js" }),
    );
    await writeFile(
      assertWithin(root, join(root, folder, "dist/main.js")),
      entry_source,
    );
  }
  async function management(name = "Harness Lifecycle") {
    // Remount after fixture-only setup commands; ordinary import handlers refresh
    // their own cards. Keep an already open consent sheet intact until tested.
    await activate(ctx.main().locator('.nav-item[data-page="agent"]'));
    await ctx
      .main()
      .locator('[data-page-panel="agent"]')
      .waitFor({ state: "visible", timeout: 10000 });
    // Embedded production assets hide the developer navigation entry. Navigate
    // the existing router to its real management page; no renderer substitute.
    await ctx.main().evaluate(() => {
      window.location.hash = "/extensions/installed";
    });
    const button = ctx
      .main()
      .getByRole("switch", { name: `Enable ${name}`, exact: true });
    return { button };
  }
  async function consent(
    name = "Harness Lifecycle",
    title = "Harness fast hello",
  ) {
    const { button } = await management(name);
    await activate(button);
    const sheet = ctx.main().getByRole("dialog");
    await sheet.waitFor({ state: "visible", timeout: 10000 });
    assert.match(await sheet.innerText(), /It provides these tools/);
    assert.ok((await sheet.innerText()).includes(title));
    assert.equal((await status()).fixtureEnabled, false);
    assert.equal((await status()).worker.count, 0, "Consent spawned a worker");
    return sheet;
  }
  async function allow(
    name = "Harness Lifecycle",
    title = "Harness fast hello",
  ) {
    const sheet = await consent(name, title);
    await activate(
      sheet.getByRole("button", { name: "Allow and enable", exact: true }),
    );
    await waitFor(
      "Committed consent enablement",
      async () => (await status()).fixtureEnabled,
    );
    assert.equal(
      await ctx.main().getByRole("dialog").count(),
      0,
      "Consent required a second sheet",
    );
    await note("consent:allowed");
  }
  async function refusal(page, before) {
    await activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await waitFor(
      "Stale approval refusal",
      async () =>
        !(await status()).agent.active &&
        !(await status()).agent.pendingApproval,
    );
    assert.equal(
      events(await status(), "dispatched").length,
      events(before, "dispatched").length,
      "Old approval dispatched after owner changed",
    );
    assert.match(
      await page.locator("body").innerText(),
      /no longer approved|changed|expired|unavailable/i,
    );
    await note("approval:refused");
  }
  async function restart(stage, beforeLaunch) {
    const before = (await status()).hostSession;
    await restartHost(beforeLaunch);
    assert.equal(
      (await status()).hostSession,
      before + 1,
      "Restart did not launch a new owned host",
    );
    await note(stage);
  }
  return {
    allow,
    project,
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "native.cli-package-ownership"() {
        const { packagedOwnership } = await import("./packaging.mjs");
        evidence.push(
          await packagedOwnership({
            ...ctx,
            cli: ctx.cli(),
            allow,
            note,
            record: (step) => evidence.push(step),
          }),
        );
      },
      async "native.consent-persistence"() {
        await imported("installed-one");
        const sheet = await consent();
        await activate(
          sheet.getByRole("button", { name: "Cancel", exact: true }),
        );
        assert.equal((await status()).fixtureEnabled, false);
        assert.equal((await status()).fixtureApproved, false);
        await restart("restart:cancelled");
        assert.equal((await status()).fixtureEnabled, false);
        assert.equal((await status()).fixtureApproved, false);
        await allow();
        await greeting("installed-one");
        await restart("restart:enabled");
        assert.equal((await status()).fixtureEnabled, true);
        await greeting("installed-one");
        const before = await status();
        const pending = await request();
        await fixture("disable");
        await refusal(pending, before);
        await restart("restart:disabled");
        assert.equal((await status()).fixtureEnabled, false);
        const { button } = await management();
        await activate(button);
        await waitFor(
          "Re-enable saved approval",
          async () => (await status()).fixtureEnabled,
        );
        assert.equal(await ctx.main().getByRole("dialog").count(), 0);
        await greeting("installed-one");
      },
      async "native.stale-review"() {
        await imported("review-one");
        const sheet = await consent();
        await imported("review-two", true);
        await activate(
          sheet.getByRole("button", { name: "Allow and enable", exact: true }),
        );
        await waitFor("Stale review error", async () =>
          /changed after review/.test(
            await ctx.main().locator("body").innerText(),
          ),
        );
        assert.equal((await status()).fixtureEnabled, false);
        assert.equal((await status()).fixtureApproved, false);
        assert.equal((await status()).worker.count, 0);
        // Remount the normal route to refresh cards after out-of-band import.
        await activate(ctx.main().locator('.nav-item[data-page="agent"]'));
        await allow(
          "Harness Lifecycle Revised",
          "Harness fast hello (revised)",
        );
        await greeting("review-two");
      },
      async "native.owner-restoration"() {
        await imported("installed-one");
        await allow();
        await greeting("installed-one");
        let before = await status();
        let pending = await request();
        await imported("installed-two");
        await refusal(pending, before);
        await greeting("installed-two");
        await restart("restart:same-version");
        assert.equal((await status()).fixtureOwner, "installed");
        await greeting("installed-two");
        before = await status();
        pending = await request();
        await project("fixture", "developer-a");
        await fixture("register");
        await refusal(pending, before);
        await fixture("enable");
        await greeting("developer-a");
        before = await status();
        pending = await request();
        await project("fixture-b", "developer-b");
        await fixture("load_second");
        await refusal(pending, before);
        await fixture("enable");
        await greeting("developer-b");
        await restart("restart:developer-b");
        assert.equal((await status()).fixtureOwner, "fixture-b");
        await greeting("developer-b");
        before = await status();
        pending = await request();
        await fixture("unload");
        await refusal(pending, before);
        assert.equal((await status()).fixtureOwner, "installed");
        assert.equal((await status()).fixtureEnabled, true);
        await greeting("installed-two");
        await restart("restart:restored");
        assert.equal((await status()).fixtureOwner, "installed");
        await greeting("installed-two");
      },
      async "native.enablement-approval"() {
        await project("fixture", "approval-one");
        await fixture("load");
        let before = await status();
        let pending = await request();
        await fixture("disable");
        await fixture("enable");
        await refusal(pending, before);
        await greeting("approval-one");
        before = await status();
        pending = await request();
        await project("fixture", "approval-two");
        await fixture("register");
        await fixture("enable");
        await refusal(pending, before);
        await greeting("approval-two");
      },
      async "native.registry-refusal"() {
        await imported("preserved");
        await allow();
        await closePanel();
        const path = assertWithin(root, join(root, "data/extensions.json"));
        const saved = await readFile(path);
        for (const bytes of [
          Buffer.from("{broken registry"),
          Buffer.from('{"tool_only_migration_version":999,"records":{}}'),
        ]) {
          // Write only after the owned host exits, never into a live or ordinary profile.
          await restart("restart:invalid-registry", () =>
            writeFile(path, bytes),
          );
          assert.equal((await status()).registryAvailable, false);
          assert.deepEqual(
            await readFile(path),
            bytes,
            "Startup overwrote an invalid registry",
          );
          await assert.rejects(fixture("import"), /registry unavailable/);
          assert.match(ctx.log(), /extensions registry failed to load/);
          assert.equal((await status()).worker.count, 0);
          await restart("restart:restored-registry", () =>
            writeFile(path, saved),
          );
          assert.equal((await status()).registryAvailable, true);
          assert.equal((await status()).fixtureEnabled, true);
          await greeting("preserved");
          await closePanel();
        }
      },
    },
  };
}
