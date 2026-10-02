// Real native-account acceptance; only the fixed, isolated fixture is mutable.
import assert from "node:assert/strict";
import { mkdir, writeFile, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  authPackage,
  AUTH_FIXTURE_ID,
  PRIVATE_MARKER,
} from "./auth-fixture.mjs";
import { assertWithin } from "./support.mjs";
import { cliCommand } from "./packaging.mjs";
import { developerReload } from "./developer.mjs";

export function accountOwnershipHandlers(ctx) {
  let evidence = [];
  let declarationChange;
  const auth = (operation) => ctx.invoke("agent_harness_auth", { operation });
  const exchanges = () =>
    ctx.provider().journal.filter((e) => e.phase === "exchange").length;
  const reads = () =>
    ctx.provider().journal.filter((e) => e.phase === "read").length;
  async function imported(change) {
    await writeFile(
      assertWithin(ctx.root, join(ctx.root, "auth-fixture.grainpack")),
      JSON.stringify(
        authPackage(ctx.provider().port, { owner: "installed", change }),
      ),
    );
    await auth("import");
    declarationChange = change;
  }
  async function review() {
    if ((await auth("status")).enabled) return;
    await ctx.activate(ctx.main().locator('.nav-item[data-page="agent"]'));
    await ctx.main().evaluate(() => {
      window.location.hash = "/extensions/installed";
    });
    await ctx.activate(
      ctx.main().getByRole("switch", {
        name: "Enable Harness Native Account",
        exact: true,
      }),
    );
    const sheet = ctx.main().getByRole("dialog");
    await sheet.waitFor({ state: "visible", timeout: 10000 });
    const text = await sheet.innerText();
    assert.ok(text.includes("Connect Harness OAuth"));
    assert.ok(text.includes("fixture.read") && text.includes("127.0.0.1"));
    if (declarationChange === "scopes")
      assert.ok(text.includes("fixture.extra"));
    if (declarationChange === "hosts") assert.ok(text.includes("localhost"));
    assert.equal((await auth("status")).enabled, false);
    assert.equal((await auth("status")).worker.count, 0);
    await ctx.activate(
      sheet.getByRole("button", { name: "Allow and enable", exact: true }),
    );
    await ctx.waitFor(
      "Reviewed native declaration",
      async () => (await auth("status")).enabled,
    );
  }
  async function connect(account) {
    const declaration = authPackage(ctx.provider().port, {
      change: declarationChange,
    }).manifest.contributes.authentication;
    ctx.provider().configure({
      account,
      clientId: declaration.clientId,
      scope: declaration.scopes.join(" "),
      tokenPath: new URL(declaration.tokenEndpoint).pathname,
    });
    const pending = auth("connect").then(
      (value) => ({ value }),
      (error) => ({ error }),
    );
    try {
      const url = await ctx.waitFor(
        "Owned sign-in handoff",
        async () => await auth("authorization"),
      );
      await ctx.provider().authorize(url);
      const result = await pending;
      if (result.error) throw result.error;
      assert.equal(result.value.state, "connected");
      assert.deepEqual(result.value.scopes, declaration.scopes);
      assert.deepEqual(result.value.granted_scopes, declaration.scopes);
      assert.deepEqual(result.value.api_hosts, declaration.apiHosts);
      evidence.push({ stage: "connected", account });
    } catch (error) {
      await auth("disconnect").catch(() => {});
      await pending;
      throw error;
    }
  }
  async function read(account, owner, refused = false) {
    const before = await ctx.status(),
      start = ctx.model().journal.length;
    const readStart = reads(),
      exchangeStart = exchanges();
    const instruction = `account_read_${account.toLowerCase()}_${owner.replaceAll("-", "_")}`;
    let page;
    if (refused) {
      await ctx.closePanel();
      await ctx.invoke("agent_harness_submit", { instruction });
      page = await ctx.panel();
      await ctx.waitFor(
        "Unusable account refuses tool loading",
        async () =>
          ctx
            .model()
            .journal.slice(start)
            .some((e) => e.accountRefused) &&
          !(await ctx.status()).agent.active,
      );
    } else {
      page = await ctx.request(instruction);
    }
    assert.equal(reads(), readStart, "Read escaped approval");
    if (!refused)
      await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Native account read settles",
      async () => !(await ctx.status()).agent.active,
    );
    const verified = ctx
      .model()
      .journal.slice(start)
      .filter((e) => e.accountVerified).length;
    if (refused) {
      assert.equal(
        reads(),
        readStart,
        "Invalid credential authorized an API read",
      );
      assert.equal(
        exchanges(),
        exchangeStart,
        "Invalid credential authorized a refresh",
      );
      assert.equal(
        verified,
        0,
        "Unusable credential was accepted as a real result",
      );
      assert.equal(
        ctx
          .model()
          .journal.slice(start)
          .filter((e) => e.accountRefused).length,
        1,
        "Actual native account refusal was not observed",
      );
      assert.equal(
        ctx.events(await ctx.status(), "dispatched", "account_read").length,
        ctx.events(before, "dispatched", "account_read").length,
      );
      assert.match(
        await page.locator("body").innerText(),
        /failed|reauthoriz|reconnect|not connected|refus/i,
      );
    } else {
      assert.equal(reads(), readStart + 1);
      assert.equal(
        ctx
          .provider()
          .journal.filter((e) => e.phase === "read")
          .at(-1).account,
        account,
      );
      assert.equal(verified, 1, "Real account/owner result did not match");
      assert.equal(
        ctx.events(await ctx.status(), "dispatched", "account_read").length,
        ctx.events(before, "dispatched", "account_read").length + 1,
      );
    }
    assert.ok(
      !(await page.locator("body").innerText()).includes(PRIVATE_MARKER),
    );
    evidence.push({
      stage: refused ? "credential-refused" : "owner-read",
      account,
      owner,
      apiReadDelta: reads() - readStart,
    });
  }
  async function stale(page, before, stage) {
    const readStart = reads();
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Old owner approval refused",
      async () => !(await ctx.status()).agent.active,
    );
    assert.equal(
      ctx.events(await ctx.status(), "dispatched", "account_read").length,
      ctx.events(before, "dispatched", "account_read").length,
      "Obsolete owner dispatched",
    );
    assert.equal(reads(), readStart);
    assert.match(
      await page.locator("body").innerText(),
      /no longer approved|changed|expired|unavailable/i,
    );
    evidence.push({ stage, dispatchDelta: 0 });
  }
  async function restart(account, owner) {
    const exchangeStart = exchanges();
    await ctx.restartHost();
    const state = await auth("status");
    assert.equal(state.owner, owner);
    assert.equal(state.connection.state, "connected");
    assert.equal(state.worker.count, 0);
    await read(account, owner);
    assert.equal(
      exchanges(),
      exchangeStart,
      "Restart silently reauthenticated",
    );
    evidence.push({ stage: "owner-restart", owner, account });
  }
  async function project(owner) {
    assert.ok(["developer-a", "developer-b"].includes(owner));
    const folder =
      owner === "developer-a" ? "auth-fixture-a" : "auth-fixture-b";
    const root = assertWithin(ctx.root, join(ctx.root, folder));
    const pack = authPackage(ctx.provider().port, {
      owner:
        ctx.fault === "wrong-owner" && owner === "developer-a"
          ? "installed"
          : owner,
    });
    const { entry_source, ...manifest } = pack.manifest;
    manifest.icon = "icon.png";
    await mkdir(join(root, "src"), { recursive: true });
    await copyFile(
      join(ctx.repo, "src-tauri/icons/icon.png"),
      join(root, "icon.png"),
    );
    await writeFile(
      join(root, "manifest.json"),
      JSON.stringify({ ...manifest, entry: "dist/main.js" }),
    );
    await writeFile(join(root, "src/main.js"), entry_source);
    await writeFile(
      join(root, "package.json"),
      JSON.stringify({ private: true, scripts: { build: "node build.mjs" } }),
    );
    const esbuild = pathToFileURL(
      join(ctx.repo, "node_modules/esbuild/lib/main.js"),
    ).href;
    await writeFile(
      join(root, "build.mjs"),
      `import {build} from ${JSON.stringify(esbuild)}; await build({entryPoints:['src/main.js'],bundle:true,format:'iife',platform:'browser',target:'es2020',outfile:'dist/main.js',logLevel:'silent'});`,
    );
    const output = assertWithin(
      ctx.root,
      join(ctx.root, `${folder}.grainpack`),
    );
    for (const args of [["pack", "--output", output], ["doctor"]])
      evidence.push({
        stage: "auth-cli",
        owner,
        ...(await cliCommand(ctx.cli().binary, args, root, ctx.waitFor)),
      });
  }
  return {
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "native.auth-binding"() {
        await imported();
        try {
          await review();
          await connect("A");
          await read("A", "installed");
          for (const operation of ["seed_unbound", "seed_legacy"]) {
            await auth(operation);
            assert.equal(
              (await auth("status")).connection.state,
              "needs_reauthorization",
            );
            await read("A", "installed", true);
            await ctx.restartHost();
            assert.equal(
              (await auth("status")).connection.state,
              "needs_reauthorization",
            );
            await read("A", "installed", true);
            await connect("A");
            await read("A", "installed");
            await restart("A", "installed");
            evidence.push({ stage: "legacy-reconnect", format: operation });
          }
          for (const change of ["client", "token", "scopes", "hosts"]) {
            const before = await ctx.status();
            const pending = await ctx.request("account_read_a_installed");
            await imported(
              ctx.fault === "unchanged-auth-declaration" ? undefined : change,
            );
            await stale(pending, before, `declaration:${change}`);
            await review();
            assert.equal(
              (await auth("status")).connection.state,
              "needs_reauthorization",
            );
            await read("A", "installed", true);
            await connect("A");
            await read("A", "installed");
            await restart("A", "installed");
            evidence.push({ stage: "declaration-reconnect", field: change });
            await imported();
            await review();
            assert.equal(
              (await auth("status")).connection.state,
              "needs_reauthorization",
            );
            await connect("A");
            await read("A", "installed");
          }
        } finally {
          await auth("remove");
        }
      },
      async "native.auth-owners"() {
        await imported();
        try {
          await review();
          await connect("A");
          await read("A", "installed");
          let before = await ctx.status(),
            pending = await ctx.request("account_read_a_installed");
          await project("developer-a");
          await auth("load_a");
          await auth("enable");
          await stale(pending, before, "installed-to-a");
          assert.equal((await auth("status")).owner, "developer-a");
          assert.equal((await auth("status")).connection.state, "disconnected");
          await read("A", "developer-a", true);
          await connect("B");
          await read("B", "developer-a");
          const exchangeStart = exchanges();
          await auth("load_a");
          await auth("enable");
          await read("B", "developer-a");
          await developerReload(ctx.root, AUTH_FIXTURE_ID);
          await read("B", "developer-a");
          assert.equal(
            exchanges(),
            exchangeStart,
            "Same directory reload lost its account",
          );
          before = await ctx.status();
          pending = await ctx.request("account_read_b_developer_a");
          await project("developer-b");
          await auth("load_b");
          await auth("enable");
          await stale(pending, before, "a-to-b");
          assert.equal((await auth("status")).connection.state, "disconnected");
          await read("B", "developer-b", true);
          await connect("A");
          await read("A", "developer-b");
          await restart("A", "developer-b");
          before = await ctx.status();
          pending = await ctx.request("account_read_a_developer_b");
          await auth("unload");
          await stale(pending, before, "b-to-installed");
          await read("A", "installed");
          await restart("A", "installed");
          // A newly loaded directory must not recover its retired B session.
          await auth("load_a");
          await auth("enable");
          assert.equal((await auth("status")).connection.state, "disconnected");
          await connect("B");
          await read("B", "developer-a");
          const storedBefore = await ctx.vaultCount();
          await auth("remove_installed");
          assert.equal(
            await ctx.vaultCount(),
            storedBefore - 1,
            "Parked uninstall must delete only its installed grant",
          );
          assert.equal((await auth("status")).installed, false);
          assert.equal((await auth("status")).owner, "developer-a");
          await read("B", "developer-a");
          await restart("B", "developer-a");
          await auth("unload");
          evidence.push({
            stage: "parked-uninstall",
            activeAccount: "B",
            restoredInstalled: false,
          });
        } finally {
          await auth("remove");
        }
      },
    },
  };
}
