// Only store feature boundaries. Protocol/account matrices stay in the existing
// production/component suites; this exercises real Tauri, Agent and OS vault.
import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";
import { withRegistryLock } from "./registry.mjs";
import { cliCommand } from "./packaging.mjs";
import { CONFIGURED_ENDPOINT } from "./mcp-configured-runtime.mjs";
import { CONFIGURED_AUTH_ENDPOINT } from "./mcp-configured-auth.mjs";
import { MCP_CLIENTS, MCP_CLIENT_SECRETS } from "./mcp-oauth-fixture.mjs";

const ID = "com.grain.harness.mcp",
  PEER = "com.grain.harness.mcp-peer";
export function storeMcpHandlers(ctx) {
  let evidence = [];
  const list = () => ctx.invoke("mcp_connections_list");
  const args = (record) => ({
    id: record.id,
    expectedRevision: record.revision,
  });
  const remove = (record) => ctx.invoke("mcp_connection_remove", args(record));
  const enable = (record, enabled = true) =>
    ctx.invoke("mcp_connection_set_enabled", { ...args(record), enabled });
  const state = (record) => ctx.invoke("mcp_connection_status", args(record));
  const disk = async () =>
    JSON.parse(
      await readFile(
        assertWithin(ctx.root, join(ctx.root, "data/mcp-connections.json")),
        "utf8",
      ),
    );
  const row = async (record) =>
    (await disk()).connections.find((x) => x.connectionId === record.id);
  const calls = () =>
    ctx.provider().journal.filter((x) => x.method === "tools/call");
  const oauth = () => ctx.provider().oauth;
  const update = async (record, version) => {
    await ctx.invoke("store_mcp_update", {
      connectionId: record.id,
      expectedRevision: record.revision,
      version,
    });
    return (await list()).find((x) => x.id === record.id);
  };
  async function stage(name, operation) {
    try {
      const value = await operation();
      evidence.push({ stage: name, status: "Pass" });
      return value;
    } catch (error) {
      evidence.push({ stage: name, status: "Fail", error: error.message });
      throw new Error(`Store MCP ${name}: ${error.message}`, { cause: error });
    }
  }
  async function browse() {
    const view = await ctx.invoke("store_browse");
    assert.equal(view.status, "fresh");
    assert.equal(view.can_install, true);
    assert.equal(view.entries.length, 0, "MCP cards require the later E5 UI");
  }
  async function publish(options = {}) {
    ctx.store().configureMcp(options);
    await browse();
  }
  async function install(id = ID) {
    await ctx.invoke("store_mcp_install", { id, version: "1.0.0" });
    return (await list()).at(-1);
  }
  async function reset() {
    await ctx.closePanel();
    await ctx.invoke("extension_set_developer_mode", { enabled: true });
    for (const record of await list()) await remove(record);
    for (const flags of [
      [false, false],
      [true, false],
      [false, true],
    ])
      assert.equal(await ctx.vaultCount(...flags), 0);
    oauth().retireGrants();
    oauth().retireRegistrations();
    oauth().configure({
      account: "A",
      denied: false,
      expiresIn: 1200,
      refreshable: false,
      metadataUnavailable: false,
      metadataClientSupported: false,
      dynamicRegistration: true,
      authorizationServer: null,
      resourceOrigin: null,
      destinationMode: "valid",
      secretVersion: 0,
    });
    ctx.provider().configure({
      lifecycle: "stateless",
      reply: "json",
      catalog: "mixed",
      result: "normal",
      revision: "one",
      probeRejection: false,
    });
  }
  async function read(
    record,
    mutation = null,
    expected = "mcpVerified",
    instruction = "mcp_configured_read",
  ) {
    const before = calls().length,
      start = ctx.model().journal.length;
    const page = await ctx.request(instruction);
    assert.equal(calls().length, before, "Unapproved store call escaped");
    if (mutation) await mutation(page);
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Store Agent finishes",
      async () => !(await ctx.status()).agent.active,
    );
    const receipts = ctx
      .model()
      .journal.slice(start)
      .filter((x) => x[expected]);
    assert.equal(receipts.length, 1, `Missing/duplicate ${expected}`);
    if (expected === "mcpAccountVerified")
      assert.equal(receipts[0].mcpAccountProvider, `configured-${record.id}`);
    else
      assert.equal(receipts[0].configuredOwner, `mcp.configured-${record.id}`);
    assert.equal(
      calls().length,
      before + (expected.endsWith("Refused") ? 0 : 1),
    );
    await ctx.waitFor(
      "Store session disposed",
      () => ctx.provider().activeSessions === 0,
    );
    assert.equal((await ctx.status()).worker.count, 0);
    await ctx.closePanel();
  }
  async function begin(record) {
    const pending = ctx.invoke("mcp_connection_connect", args(record)).then(
      () => ({}),
      (error) => ({ error: String(error) }),
    );
    const url = await ctx.waitFor("Store SDK consent", () =>
      ctx.invoke("agent_harness_configured_consent", {
        id: `configured-${record.id}`,
      }),
    );
    return { pending, callback: await oauth().authorize(url) };
  }
  async function connect(record) {
    const flow = await begin(record);
    assert.equal((await oauth().callback(flow.callback)).status, 200);
    assert.equal((await flow.pending).error, undefined);
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "store.mcp-management"() {
        await reset();
        await stage("cli-authored-descriptor-to-signed-store", async () => {
          const authorRoot = assertWithin(
            ctx.root,
            join(ctx.root, "mcp-author"),
          );
          await mkdir(authorRoot);
          const commands = [];
          commands.push(
            await cliCommand(
              ctx.cli().binary,
              [
                "init",
                "Store MCP harness",
                "--id",
                ID,
                "--mcp-url",
                CONFIGURED_ENDPOINT,
                "--authentication",
                "none",
              ],
              authorRoot,
              ctx.waitFor,
            ),
          );
          const project = join(authorRoot, "store-mcp-harness");
          const source = join(project, "mcp.json");
          const descriptor = JSON.parse(await readFile(source, "utf8"));
          descriptor.version = "1.0.0";
          descriptor.description = "Owned store MCP tools.";
          await writeFile(source, JSON.stringify(descriptor));
          for (const args of [["doctor"], ["pack"]])
            commands.push(
              await cliCommand(ctx.cli().binary, args, project, ctx.waitFor),
            );
          const bytes = await readFile(join(project, `${ID}-1.0.0.mcp.json`));
          await publish({ descriptorBytes: bytes });
          evidence.push({
            stage: "cli-authored-artifact",
            status: "Pass",
            commands,
            sha256: createHash("sha256").update(bytes).digest("hex"),
            bytes: bytes.length,
          });
        });
        let record;
        await stage("verified-inactive-acquisition-and-guards", async () => {
          const before = ctx.provider().journal.length;
          ctx.store().corruptBlob();
          await assert.rejects(install(), /hash/);
          assert.deepEqual(await list(), []);
          ctx.store().corruptBlob(false);
          record = await install();
          assert.equal(record.state, "inactive");
          assert.equal(
            ctx.provider().journal.length,
            before,
            "Install contacted MCP endpoint",
          );
          assert.ok((await row(record)).storeArtifact);
          await assert.rejects(install(), /already|changed/);
          await assert.rejects(
            ctx.invoke("mcp_connection_replace", {
              ...args(record),
              definitionJson: JSON.stringify({
                name: "Injected",
                url: CONFIGURED_ENDPOINT,
                authentication: { type: "none" },
              }),
            }),
            /verified catalogue/,
          );
          await enable(record);
          await read(record, async (page) => {
            await assert.rejects(
              page.evaluate(() =>
                window.__TAURI_INTERNALS__.invoke("store_mcp_install", {
                  id: "com.grain.harness.mcp",
                  version: "1.0.0",
                }),
              ),
              /only from Grain settings/,
            );
          });
          await ctx.restartHost();
          await read(record);
          assert.equal((await ctx.status()).store.indexResident, false);
        });
        await stage("version-specific-and-deprecation-policy", async () => {
          ctx.store().revokeMcp([ID], "0.0.1");
          await browse();
          assert.equal((await list())[0].state, "enabled");
          ctx.store().revokeMcp([ID], "1.0.0", "deprecated");
          await browse();
          assert.equal(
            (
              await ctx.invoke("mcp_test_provider", {
                id: `configured-${record.id}`,
              })
            ).tool_count,
            2,
          );
          await assert.rejects(
            update(record, "1.0.0"),
            /revoked or deprecated/,
          );
          await publish();
        });
        await stage(
          "metadata-update-invalidates-approval-preserves-account",
          async () => {
            const before = await row(record),
              stale = record;
            await read(
              record,
              async () => {
                await publish({
                  version: "2.0.0",
                  description: "Updated store listing.",
                });
                record = await update(record, "2.0.0");
              },
              "mcpRefused",
            );
            assert.equal(record.state, "inactive");
            assert.equal((await row(record)).accountId, before.accountId);
            await assert.rejects(enable(stale), /changed/);
            await enable(record);
            await read(record, async () => {
              const unchanged = await update(record, "2.0.0");
              assert.equal(unchanged.revision, record.revision);
            });
          },
        );
        await stage("failed-publication-preserves-old-descriptor", async () => {
          await publish({ version: "3.0.0" });
          const before = await row(record);
          await withRegistryLock(
            ctx,
            async () => {
              await assert.rejects(
                update(record, "3.0.0"),
                /save|filesystem|storage|changed/i,
              );
            },
            "mcp-connections.json",
          );
          assert.deepEqual(await row(record), before);
          assert.equal((await list())[0].state, "inactive");
          record = await update(record, "3.0.0");
          await enable(record);
          await read(record);
        });
        await stage("removed-download-cannot-resurrect-owner", async () => {
          await publish({ version: "4.0.0" });
          ctx.store().hold("blob");
          const pending = update(record, "4.0.0").then(
            () => ({}),
            (error) => ({ error: String(error) }),
          );
          await ctx.waitFor(
            "Held MCP descriptor",
            () => ctx.store().heldCount === 1,
          );
          await remove(record);
          ctx.store().release();
          assert.match((await pending).error ?? "", /changed/);
          assert.deepEqual(await list(), []);
          await publish();
          const next = await install();
          assert.notEqual(next.id, record.id);
          await enable(next);
          await ctx.restartHost();
          await read(next);
          await remove(next);
        });
        await reset();
      },
      async "store.mcp-accounts"() {
        await reset();
        await publish({
          url: CONFIGURED_AUTH_ENDPOINT,
          authentication: "oauth",
        });
        let record = await install();
        await stage("real-sdk-client-grant-and-restart", async () => {
          await ctx.invoke("mcp_connection_set_client_credentials", {
            ...args(record),
            clientId: MCP_CLIENTS.confidential,
            clientSecret: MCP_CLIENT_SECRETS[0],
          });
          await connect(record);
          for (const flags of [
            [false, false],
            [true, false],
            [false, true],
          ])
            assert.equal(await ctx.vaultCount(...flags), 1);
          await read(
            record,
            null,
            "mcpAccountVerified",
            "mcp_configured_account_a",
          );
          await ctx.restartHost();
          await read(
            record,
            null,
            "mcpAccountVerified",
            "mcp_configured_account_a",
          );
        });
        await stage(
          "settings-failure-prevents-account-publication",
          async () => {
            const before = await row(record);
            await publish({ version: "2.0.0" });
            await withRegistryLock(
              ctx,
              async () => {
                await assert.rejects(update(record, "2.0.0"), /enablement/);
              },
              "grain.settings.json",
            );
            assert.deepEqual(await row(record), before);
            assert.equal(
              await ctx.vaultCount(),
              1,
              "Failed pre-retirement save deleted grant",
            );
            record = await update(record, "2.0.0");
            assert.notEqual((await row(record)).accountId, before.accountId);
            for (const flags of [
              [false, false],
              [true, false],
              [false, true],
            ])
              assert.equal(await ctx.vaultCount(...flags), 0);
            assert.equal(record.state, "inactive");
            await enable(record);
            await read(record);
          },
        );
        await stage(
          "destination-change-needs-new-account-and-explicit-consent",
          async () => {
            const before = await row(record);
            await publish({
              version: "3.0.0",
              url: CONFIGURED_AUTH_ENDPOINT,
              authentication: "oauth",
            });
            record = await update(record, "3.0.0");
            assert.notEqual((await row(record)).accountId, before.accountId);
            await assert.rejects(enable(record), /Connect/);
            await connect(record);
            await read(
              record,
              null,
              "mcpAccountVerified",
              "mcp_configured_account_a",
            );
            await ctx.invoke("extension_set_developer_mode", {
              enabled: false,
            });
            await assert.rejects(
              ctx.invoke("store_mcp_update", {
                connectionId: record.id,
                expectedRevision: record.revision,
                version: "3.0.0",
              }),
              /Developer|developer/,
            );
            await ctx.invoke("extension_set_developer_mode", { enabled: true });
            await remove(record);
            assert.equal(await ctx.vaultCount(), 0);
          },
        );
        await reset();
      },
      async "store.mcp-revocation"() {
        await reset();
        await publish({
          url: CONFIGURED_AUTH_ENDPOINT,
          authentication: "oauth",
        });
        const record = await install();
        await connect(record);
        await stage(
          "signed-revocation-beats-held-call-and-pending-consent",
          async () => {
            ctx.provider().configure({ result: "held" });
            const before = calls().length,
              start = ctx.model().journal.length;
            const page = await ctx.request("mcp_configured_unknown");
            await ctx.activate(
              page.locator(".agc-confirm-actions .agc-action-btn"),
            );
            await ctx.waitFor(
              "Store read held",
              () => ctx.provider().heldCalls === 1,
            );
            await publish({
              id: PEER,
              url: CONFIGURED_AUTH_ENDPOINT,
              authentication: "oauth",
            });
            await ctx.invoke("store_mcp_install", {
              id: PEER,
              version: "1.0.0",
            });
            const peer = (await list()).find((x) => x.id !== record.id);
            const baseline = oauth().journal.filter(
              (x) => x.phase === "token",
            ).length;
            const flow = await begin(peer);
            ctx.store().revokeMcp([ID, PEER]);
            await withRegistryLock(
              ctx,
              async () => {
                await assert.rejects(ctx.invoke("store_browse"), /enablement/);
              },
              "grain.settings.json",
            );
            assert.match(
              (await flow.pending).error ?? "",
              /account or access changed/,
            );
            await assert.rejects(oauth().callback(flow.callback));
            assert.equal(
              oauth().journal.filter((x) => x.phase === "token").length,
              baseline,
            );
            await ctx.waitFor(
              "Revoked Agent stops",
              async () => !(await ctx.status()).agent.active,
            );
            await ctx.waitFor(
              "Revoked sessions disposed",
              () =>
                ctx.provider().activeSessions === 0 &&
                ctx.provider().heldCalls === 0,
            );
            assert.equal(
              ctx
                .model()
                .journal.slice(start)
                .filter((x) => x.mcpUnknownVerified).length,
              1,
            );
            assert.equal(calls().length, before + 1, "Revoked read replayed");
            ctx.provider().attemptLateReply();
            await ctx.closePanel();
            ctx.provider().configure({ result: "normal" });
            for (const item of [record, peer]) {
              await assert.rejects(enable(item), /revoked/);
              await assert.rejects(
                ctx.invoke("mcp_connection_connect", args(item)),
                /revoked/,
              );
              await assert.rejects(
                ctx.invoke("mcp_test_provider", {
                  id: `configured-${item.id}`,
                }),
                /disabled|revoked|changed/,
              );
            }
            await ctx.invoke("store_close");
            ctx.store().offline();
            await ctx.restartHost();
            for (const item of await list()) {
              assert.equal(item.state, "inactive");
              await assert.rejects(enable(item), /revoked/);
            }
            const configured = await ctx.invoke("mcp_connection_import", {
              definitionJson: JSON.stringify({
                name: "Independent configured MCP",
                url: CONFIGURED_ENDPOINT,
                authentication: { type: "none" },
              }),
            });
            await enable(configured);
            await read(configured);
            await remove(configured);
            for (const item of await list()) await remove(item);
            assert.equal(await ctx.vaultCount(), 0);
            ctx.store().offline(false);
          },
        );
        await reset();
      },
    },
  };
}
