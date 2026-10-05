import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";
const ID = "com.grain.harness.lifecycle";

export function storeHandlers(ctx) {
  const { invoke, status, waitFor, fixture, greeting, restartHost } = ctx;
  let observations = [];
  const store = () => ctx.store();
  const install = (version = "0.1.0") =>
    invoke("store_install", { id: ID, version });
  async function openStore() {
    await ctx.main().evaluate(() => {
      window.location.hash = "/extensions/store";
    });
  }
  async function closeStore() {
    await ctx.activate(ctx.main().locator('.nav-item[data-page="agent"]'));
    await ctx
      .main()
      .locator('[data-page-panel="agent"]')
      .waitFor({ state: "visible" });
    await waitFor(
      "Unmounted store released index",
      async () => !(await status()).store.indexResident,
    );
  }
  async function storeCard() {
    const card = ctx
      .main()
      .getByRole("button", { name: "Preview Harness Lifecycle", exact: true });
    await card.waitFor({ state: "visible" });
    return card;
  }
  async function browse() {
    const view = await invoke("store_browse");
    observations.push({
      stage: "browse",
      status: view.status,
      canInstall: view.can_install,
      entries: view.entries.map((entry) => ({
        id: entry.id,
        version: entry.version,
      })),
    });
    return view;
  }
  async function fresh() {
    const view = await browse();
    assert.equal(view.status, "fresh");
    assert.equal(view.can_install, true);
    assert.equal(view.entries.length, 1);
    assert.equal(view.entries[0].id, ID);
    assert.equal(view.entries[0].readme, store().listingHash);
    assert.equal(
      await invoke("store_readme", { sha256: store().listingHash }),
      store().listingText,
    );
    observations.push({
      stage: "explicit-description",
      status: "Pass",
      sha256: store().listingHash,
    });
  }
  async function setup() {
    await store().configure();
    await fresh();
    await install();
    const snapshot = await status();
    assert.equal(snapshot.fixtureTrust, "verified");
    assert.equal(snapshot.fixtureEnabled, false);
    assert.equal(snapshot.worker.count, 0);
    await ctx.allow();
    await greeting("store-one");
  }
  function tracked(operation) {
    let result;
    const pending = operation().then(
      () => {
        result = { succeeded: true };
      },
      (error) => {
        result = { error: String(error) };
      },
    );
    return { pending, result: () => result };
  }
  async function expectRefusal(trackedCall, pattern) {
    await waitFor("Store operation settled", () => trackedCall.result());
    assert.ok(
      trackedCall.result().error,
      "Superseded store operation succeeded",
    );
    assert.match(trackedCall.result().error, pattern);
    observations.push({ stage: "refused", error: trackedCall.result().error });
  }
  return {
    takeEvidence() {
      const value = observations;
      observations = [];
      return value;
    },
    handlers: {
      async "store.close-offline"() {
        await store().configure();
        for (let cycle = 0; cycle < 10; cycle++) {
          store().hold("/index.json");
          await openStore();
          await waitFor("Held store refresh", () => store().heldCount === 1);
          await ctx
            .main()
            .getByText("Loading the extension store…", { exact: true })
            .waitFor({ state: "visible" });
          if (ctx.fault !== "unclosed-store") await closeStore();
          await waitFor(
            "Cancelled HTTP response released",
            () => store().heldCount === 0,
          );
          assert.equal((await status()).store.indexResident, false);
          await openStore();
          await storeCard();
          assert.equal((await status()).store.canInstall, true);
          await closeStore();
        }
        await store().configure();
        await openStore();
        const card = await storeCard();
        await ctx.activate(
          card.getByRole("button", { name: "Install", exact: true }),
        );
        await waitFor(
          "UI store installation",
          async () => (await status()).fixtureInstalled,
        );
        await card
          .getByRole("button", { name: "Installed", exact: true })
          .waitFor({ state: "visible" });
        assert.equal((await status()).worker.count, 0);
        assert.equal((await status()).fixtureEnabled, false);
        await closeStore();
        await ctx.allow();
        await greeting("store-one");
        await restartHost();
        await greeting("store-one");
        await store().configure("store-two", "0.2.0");
        await fresh();
        await invoke("store_close");
        store().offline();
        await openStore();
        const offlineCard = await storeCard();
        await ctx
          .main()
          .getByText(
            "Offline — showing the last verified catalogue. Installs are paused.",
            { exact: true },
          )
          .waitFor({ state: "visible" });
        assert.equal(
          await offlineCard
            .getByRole("button", { name: "Update", exact: true })
            .isDisabled(),
          true,
        );
        assert.equal((await status()).store.canInstall, false);
        await assert.rejects(install("0.2.0"), /offline|expired/);
        await greeting("store-one");
        await closeStore();
        store().offline(false);
        await openStore();
        assert.equal(
          await (
            await storeCard()
          )
            .getByRole("button", { name: "Update", exact: true })
            .isEnabled(),
          true,
        );
        await closeStore();
        assert.equal((await status()).store.indexResident, false);
        observations.push({ stage: "ten-close-reopen-cycles", count: 10 });
      },
      async "store.pending-mutations"() {
        await setup();
        await store().configure("store-two", "0.2.0");
        for (const mutation of ["disable", "remove_installed"]) {
          await fresh();
          store().hold("blob");
          const pending = tracked(() => install("0.2.0"));
          await waitFor("Held store artifact", () => store().heldCount === 1);
          await fixture(mutation);
          store().release();
          await expectRefusal(pending, /changed|superseded|revision/i);
          await restartHost();
          const snapshot = await status();
          assert.equal(snapshot.fixtureEnabled, false);
          if (mutation === "disable") {
            assert.equal(snapshot.fixtureVersion, "0.1.0");
            await fixture("enable");
            await greeting("store-one");
          } else assert.equal(snapshot.fixtureInstalled, false);
          observations.push({
            stage: `restart:${mutation}`,
            session: snapshot.hostSession,
            enabled: snapshot.fixtureEnabled,
            installed: snapshot.fixtureInstalled,
            version: snapshot.fixtureVersion,
          });
        }
        await fresh();
        await install("0.2.0");
        await ctx.allow();
        await greeting("store-two");
        await restartHost();
        await greeting("store-two");
        const registryPath = assertWithin(
          ctx.root,
          join(ctx.root, "data/extensions.json"),
        );
        const installedRecord = JSON.parse(await readFile(registryPath, "utf8"))
          .records[ID];
        await ctx.project("fixture", "store-developer");
        await fixture("register");
        // The unchanged tool declaration retains its existing declaration
        // consent; source-specific call approval still occurs in greeting().
        assert.equal((await status()).fixtureApproved, true);
        await fixture("enable");
        await greeting("store-developer");
        await store().configure("store-three", "0.3.0");
        for (const mutation of ["disable", "remove_installed"]) {
          await fresh();
          store().hold("blob");
          const pending = tracked(() => install("0.3.0"));
          await waitFor(
            "Held developer store artifact",
            () => store().heldCount === 1,
          );
          await fixture(mutation);
          store().release();
          await expectRefusal(pending, /changed|superseded|revision/i);
          await restartHost();
          const snapshot = await status();
          assert.equal(snapshot.fixtureOwner, "fixture");
          if (mutation === "disable") {
            assert.equal(snapshot.fixtureEnabled, false);
            assert.equal(snapshot.fixtureInstalled, true);
            assert.deepEqual(
              JSON.parse(await readFile(registryPath, "utf8")).records[ID].dev
                .replaced,
              installedRecord,
              "Pending update changed the parked installed owner",
            );
            await fixture("enable");
          } else {
            assert.equal(snapshot.fixtureEnabled, true);
            assert.equal(snapshot.fixtureInstalled, false);
          }
          await greeting("store-developer");
          observations.push({
            stage: `restart:developer:${mutation}`,
            session: snapshot.hostSession,
            enabled: snapshot.fixtureEnabled,
            installed: snapshot.fixtureInstalled,
            owner: snapshot.fixtureOwner,
          });
        }
        await fixture("unload");
        assert.equal((await status()).fixtureInstalled, false);
        assert.equal((await status()).fixtureEnabled, false);
      },
      async "store.integrity-close"() {
        await store().configure();
        await fresh();
        store().corruptBlob();
        await assert.rejects(install(), /hash mismatch/i);
        assert.equal((await status()).fixtureInstalled, false);
        store().corruptBlob(false);
        store().hold("blob");
        const pending = tracked(install);
        await waitFor(
          "Held close-cancel artifact",
          () => store().heldCount === 1,
        );
        await invoke("store_close");
        await expectRefusal(pending, /superseded|closed/i);
        await waitFor(
          "Closed artifact request released",
          () => store().heldCount === 0,
        );
        assert.equal((await status()).fixtureInstalled, false);
        await fresh();
        const count = store().journal.filter((item) =>
          item.path.startsWith("/blob/"),
        ).length;
        store().badSignature();
        const view = await browse();
        assert.equal(view.can_install, false);
        assert.equal(view.status, "offline");
        await assert.rejects(install(), /offline|expired/i);
        assert.equal(
          store().journal.filter((item) => item.path.startsWith("/blob/"))
            .length,
          count,
        );
        store().badSignature(false);
        await fresh();
        await install();
        await ctx.allow();
        await greeting("store-one");
        observations.push({
          stage: "signature-and-hash-refusals",
          recovered: true,
        });
      },
    },
  };
}
