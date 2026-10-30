import assert from "node:assert/strict";
import { CONFIGURED_ENDPOINT } from "./mcp-configured-runtime.mjs";
import { verifyMcpRefresh } from "./mcp-auth.mjs";
import { MCP_CLIENTS, MCP_CLIENT_SECRETS } from "./mcp-oauth-fixture.mjs";

export const CONFIGURED_AUTH_ENDPOINT =
  "https://configured-auth.grain-harness.example/mcp";
const definition = (
  name = "Configured account",
  url = CONFIGURED_AUTH_ENDPOINT,
  type = "oauth",
) => JSON.stringify({ name, url, authentication: { type } });

export function configuredAuthHandlers(ctx) {
  let evidence = [];
  const list = () => ctx.invoke("mcp_connections_list");
  const args = (record) => ({
    id: record.id,
    expectedRevision: record.revision,
  });
  const add = () =>
    ctx.invoke("mcp_connection_import", { definitionJson: definition() });
  const state = (record) => ctx.invoke("mcp_connection_status", args(record));
  const remove = (record) => ctx.invoke("mcp_connection_remove", args(record));
  const replace = (record, text) =>
    ctx.invoke("mcp_connection_replace", {
      ...args(record),
      definitionJson: text,
    });
  const enable = (record, enabled) =>
    ctx.invoke("mcp_connection_set_enabled", { ...args(record), enabled });
  const disconnect = (record) =>
    ctx.invoke("mcp_connection_disconnect", args(record));
  const setClient = (record, clientId, clientSecret = "") =>
    ctx.invoke("mcp_connection_set_client_credentials", {
      ...args(record),
      clientId,
      clientSecret,
    });
  const clearClient = (record) =>
    ctx.invoke("mcp_connection_clear_client_credentials", args(record));
  const oauth = () => ctx.provider().oauth;
  const calls = () =>
    ctx.provider().journal.filter((x) => x.method === "tools/call");
  const tokens = () => oauth().journal.filter((x) => x.phase === "token");
  async function stage(name, operation) {
    try {
      await operation();
      evidence.push({
        stage: name,
        status: "Pass",
        sessions: ctx.provider().activeSessions,
      });
    } catch (error) {
      evidence.push({ stage: name, status: "Fail", error: error.message });
      throw new Error(`Configured OAuth ${name}: ${error.message}`, {
        cause: error,
      });
    }
  }
  async function reset() {
    await ctx.closePanel();
    await ctx.invoke("extension_set_developer_mode", { enabled: true });
    for (const record of await list()) await remove(record);
    assert.equal(await ctx.vaultCount(), 0);
    assert.equal(await ctx.vaultCount(true), 0);
    assert.equal(await ctx.vaultCount(false, true), 0);
    oauth().retireGrants();
    oauth().retireRegistrations();
    ctx.provider().configure({
      lifecycle: "stateless",
      reply: "json",
      catalog: "mixed",
      result: "normal",
      probeRejection: false,
    });
    oauth().configure({
      account: "A",
      denied: false,
      expiresIn: 1200,
      refreshable: false,
      refreshExpiresIn: 1200,
      rejectRefresh: false,
      refreshFailure: "none",
      metadataUnavailable: false,
      metadataClientSupported: false,
      dynamicRegistration: true,
      authorizationServer: null,
      resourceOrigin: null,
      destinationMode: "valid",
      secretVersion: 0,
    });
  }
  async function begin(record, account = "A", options = {}) {
    oauth().configure({ account, ...options });
    const pending = ctx.invoke("mcp_connection_connect", args(record)).then(
      (value) => ({ value }),
      (error) => ({ error: String(error) }),
    );
    try {
      const url = await ctx.waitFor("Configured SDK consent", () =>
        ctx.invoke("agent_harness_configured_consent", {
          id: `configured-${record.id}`,
        }),
      );
      const callback = await oauth().authorize(url);
      return { pending, callback };
    } catch (error) {
      await enable(record, false).catch(() => {});
      await pending;
      throw error;
    }
  }
  async function finish(flow) {
    assert.equal((await oauth().callback(flow.callback)).status, 200);
    const result = await flow.pending;
    assert.equal(result.error, undefined, result.error);
  }
  const connect = async (record, account = "A", options = {}) =>
    finish(await begin(record, account, options));
  async function read(
    record,
    account = "A",
    mutation = null,
    refused = false,
    disabled = false,
  ) {
    const baseline = calls().length,
      start = ctx.model().journal.length;
    const page = await ctx.request(
      `mcp_configured_${disabled ? "disabled" : "account"}_${account.toLowerCase()}`,
    );
    assert.equal(calls().length, baseline);
    if (mutation) await mutation();
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Configured authenticated Agent stops",
      async () => !(await ctx.status()).agent.active,
    );
    const receipts = ctx.model().journal.slice(start);
    if (refused) {
      assert.equal(receipts.filter((x) => x.mcpAccountRefused).length, 1);
      assert.equal(calls().length, baseline);
    } else {
      const accepted = receipts.filter((x) => x.mcpAccountVerified);
      assert.equal(accepted.length, 1, "Missing configured account receipt");
      assert.equal(accepted[0].mcpAccountVerified, account);
      assert.equal(accepted[0].mcpAccountProvider, `configured-${record.id}`);
      assert.equal(calls().length, baseline + 1);
      assert.equal(calls().at(-1).account, account);
    }
    await ctx.waitFor(
      "Configured authenticated sessions disposed",
      () => ctx.provider().activeSessions === 0,
    );
    assert.equal((await ctx.status()).worker.count, 0);
    await ctx.closePanel();
  }
  return {
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "mcp.configured-auth-client-rotation"() {
        await reset();
        const record = await add();
        oauth().configure({ dynamicRegistration: false });
        const registered = oauth().journal.filter(
          (x) => x.phase === "registered",
        ).length;
        await stage("preregistered-public-client-and-restart", async () => {
          for (const id of ["", "x".repeat(513), "bad\nclient"])
            await assert.rejects(setClient(record, id), /client ID/);
          await assert.rejects(
            setClient(record, MCP_CLIENTS.publicOne, "x".repeat(4097)),
            /client secret/,
          );
          await setClient(record, MCP_CLIENTS.publicOne);
          assert.equal(await ctx.vaultCount(false, true), 1);
          assert.equal(await ctx.vaultCount(), 0);
          assert.equal((await state(record)).client_id_configured, true);
          assert.equal((await state(record)).enabled, false);
          await connect(record);
          await read(record);
          const exchanged = tokens().length;
          await ctx.restartHost();
          assert.equal((await state(record)).client_id_configured, true);
          await read(record);
          assert.equal(tokens().length, exchanged);
        });
        let account = "A";
        for (const item of [
          {
            name: "public-id-rotation",
            id: MCP_CLIENTS.publicTwo,
            account: "B",
            secrets: 0,
          },
          {
            name: "confidential-client",
            id: MCP_CLIENTS.confidential,
            secret: MCP_CLIENT_SECRETS[0],
            account: "A",
            secrets: 1,
            version: 0,
          },
          {
            name: "same-id-secret-rotation",
            id: MCP_CLIENTS.confidential,
            secret: MCP_CLIENT_SECRETS[1],
            account: "B",
            secrets: 1,
            version: 1,
          },
          {
            name: "return-to-public-removes-secret",
            id: MCP_CLIENTS.publicOne,
            account: "A",
            secrets: 0,
          },
        ]) {
          await stage(item.name, async () => {
            await read(
              record,
              account,
              async () => {
                if (ctx.fault !== "skip-configured-client-rotation")
                  await setClient(record, item.id, item.secret);
                assert.equal(await ctx.vaultCount(), 0);
                assert.equal(await ctx.vaultCount(true), item.secrets);
                assert.equal((await state(record)).enabled, false);
                if (item.version !== undefined)
                  oauth().configure({ secretVersion: item.version });
              },
              true,
              true,
            );
            await connect(record, item.account);
            await read(record, item.account);
            const exchanged = tokens().length;
            const issued = tokens().at(-1);
            assert.equal(issued.configuredClient, true);
            assert.equal(
              issued.registration,
              item.secrets ? "confidential" : "public",
            );
            if (item.secrets) assert.equal(issued.secretVersion, item.version);
            await ctx.restartHost();
            assert.equal((await state(record)).client_id_configured, true);
            assert.equal(await ctx.vaultCount(true), item.secrets);
            await read(record, item.account);
            assert.equal(tokens().length, exchanged);
            account = item.account;
          });
        }
        await stage(
          "logout-retains-client-explicit-reset-recovers-dcr",
          async () => {
            await disconnect(record);
            assert.equal(await ctx.vaultCount(), 0);
            assert.equal(await ctx.vaultCount(false, true), 1);
            await ctx.restartHost();
            assert.equal((await state(record)).client_id_configured, true);
            await connect(record);
            await read(record, "A", () => clearClient(record), true, true);
            assert.equal(await ctx.vaultCount(), 0);
            assert.equal(await ctx.vaultCount(true), 0);
            assert.equal(await ctx.vaultCount(false, true), 0);
            assert.equal((await state(record)).client_id_configured, false);
            assert.equal((await state(record)).enabled, false);
            assert.equal(
              oauth().journal.filter((x) => x.phase === "registered").length,
              registered,
            );
            oauth().configure({ dynamicRegistration: true });
            await connect(record);
            await read(record);
            assert.equal(
              oauth().journal.filter((x) => x.phase === "registered").length,
              registered + 1,
            );
          },
        );
        await reset();
      },
      async "mcp.configured-auth-client-ownership"() {
        await reset();
        let [one, two] = [await add(), await add()].sort((a, b) =>
          a.id.localeCompare(b.id),
        );
        oauth().configure({ dynamicRegistration: false });
        await stage(
          "same-endpoint-client-isolation-and-discovery-refusal",
          async () => {
            await setClient(
              one,
              MCP_CLIENTS.confidential,
              MCP_CLIENT_SECRETS[0],
            );
            await setClient(two, MCP_CLIENTS.publicTwo);
            await connect(one, "A");
            await connect(two, "B");
            assert.equal(await ctx.vaultCount(), 2);
            assert.equal(await ctx.vaultCount(true), 1);
            assert.equal(await ctx.vaultCount(false, true), 2);
            for (const options of [
              { metadataUnavailable: true },
              { destinationMode: "remote-token" },
            ]) {
              oauth().configure(options);
              await assert.rejects(setClient(one, MCP_CLIENTS.publicOne));
              oauth().configure({
                metadataUnavailable: false,
                destinationMode: "valid",
              });
              assert.equal(await ctx.vaultCount(), 2);
              assert.equal(await ctx.vaultCount(true), 1);
              assert.equal(await ctx.vaultCount(false, true), 2);
              await read(one);
            }
            await clearClient(one);
            assert.equal((await state(one)).client_id_configured, false);
            assert.equal((await state(two)).client_id_configured, true);
            assert.equal(await ctx.vaultCount(), 1);
            assert.equal(await ctx.vaultCount(true), 0);
            assert.equal(await ctx.vaultCount(false, true), 1);
            await read(two, "B");
          },
        );
        await stage("pending-consent-client-save-and-clear", async () => {
          for (const transition of ["save", "clear"]) {
            await setClient(one, MCP_CLIENTS.publicOne);
            const baseline = tokens().length;
            const flow = await begin(one);
            if (transition === "save")
              await setClient(one, MCP_CLIENTS.publicTwo);
            else await clearClient(one);
            const result = await flow.pending;
            assert.match(result.error ?? "", /account or access changed/);
            await assert.rejects(oauth().callback(flow.callback));
            assert.equal(tokens().length, baseline);
            await setClient(one, MCP_CLIENTS.publicOne);
            await connect(one);
            await read(one);
            await read(two, "B");
          }
        });
        for (const [lifecycle, reply, transition] of [
          ["stateless", "json", "save"],
          ["legacy", "sse", "clear"],
        ]) {
          await stage(`held-client-${lifecycle}-${transition}`, async () => {
            ctx.provider().configure({ lifecycle, reply, result: "held" });
            const baseline = calls().length,
              start = ctx.model().journal.length;
            const page = await ctx.request("mcp_configured_unknown");
            await ctx.activate(
              page.locator(".agc-confirm-actions .agc-action-btn"),
            );
            await ctx.waitFor(
              "Configured client reply held",
              () => ctx.provider().heldCalls === 1,
            );
            if (transition === "save")
              await setClient(one, MCP_CLIENTS.publicTwo);
            else await clearClient(one);
            await ctx.waitFor(
              "Configured client change stops Agent",
              async () => !(await ctx.status()).agent.active,
            );
            await ctx.waitFor(
              "Configured client work disposed",
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
            assert.equal(calls().length, baseline + 1);
            assert.equal(await ctx.vaultCount(), 1);
            ctx.provider().attemptLateReply();
            await ctx.closePanel();
            ctx.provider().configure({ result: "normal" });
            await setClient(one, MCP_CLIENTS.publicOne);
            await connect(one);
            await read(one);
            await read(two, "B");
          });
        }
        await stage("revision-guard-and-metadata-retirement", async () => {
          const stale = one;
          one = await replace(one, definition("Renamed registration"));
          await assert.rejects(
            setClient(stale, MCP_CLIENTS.publicTwo),
            /changed; reload/,
          );
          await assert.rejects(clearClient(stale), /changed; reload/);
          assert.equal((await state(one)).client_id_configured, true);
          await read(one);
          one = await replace(
            one,
            definition("Anonymous", CONFIGURED_ENDPOINT, "none"),
          );
          assert.equal(await ctx.vaultCount(), 1);
          assert.equal(await ctx.vaultCount(false, true), 1);
          await assert.rejects(
            setClient(one, MCP_CLIENTS.publicOne),
            /anonymous/,
          );
          await assert.rejects(clearClient(one), /anonymous/);
          one = await replace(one, definition());
          assert.equal((await state(one)).client_id_configured, false);
          await remove(two);
          assert.equal(await ctx.vaultCount(), 0);
          assert.equal(await ctx.vaultCount(false, true), 0);
          await ctx.restartHost();
          assert.equal((await state(one)).client_id_configured, false);
        });
        await reset();
      },
      async "mcp.configured-auth-ownership"() {
        await reset();
        let [one, two] = [await add(), await add()].sort((a, b) =>
          a.id.localeCompare(b.id),
        );
        await stage("independent-sdk-accounts-and-restart", async () => {
          assert.equal((await state(one)).connected, false);
          await assert.rejects(
            enable(one, true),
            /Connect this configured MCP account/,
          );
          await connect(one, "A");
          await connect(two, "B");
          assert.equal(await ctx.vaultCount(), 2);
          await enable(one, false);
          assert.equal((await state(one)).connected, true);
          assert.equal((await state(one)).enabled, false);
          await enable(one, true);
          assert.equal((await state(one)).enabled, true);
          assert.equal(await ctx.vaultCount(), 2);
          await read(one, "A");
          await read(two, "B");
          const exchanged = tokens().length;
          await ctx.restartHost();
          assert.equal((await state(one)).connected, true);
          assert.equal((await state(two)).enabled, true);
          await read(one, "A");
          await read(two, "B");
          assert.equal(tokens().length, exchanged);
        });
        await stage("label-stale-approval-and-peer-retirement", async () => {
          const stale = one;
          await read(
            one,
            "A",
            async () => {
              one = await replace(one, definition("Renamed account"));
            },
            true,
          );
          await assert.rejects(
            ctx.invoke("mcp_connection_connect", args(stale)),
            /changed; reload/,
          );
          await assert.rejects(
            ctx.invoke("mcp_connection_status", args(stale)),
            /changed; reload/,
          );
          await assert.rejects(disconnect(stale), /changed; reload/);
          assert.equal(await ctx.vaultCount(), 2);
          await remove(two);
          assert.equal(await ctx.vaultCount(), 1);
          await read(one, "A");
        });
        await stage(
          "destination-and-auth-retire-before-activation",
          async () => {
            one = await replace(
              one,
              definition("Anonymous", CONFIGURED_ENDPOINT, "none"),
            );
            assert.equal(one.state, "inactive");
            assert.equal(await ctx.vaultCount(), 0);
            await assert.rejects(
              ctx.invoke("mcp_connection_connect", args(one)),
              /no account/,
            );
            one = await replace(one, definition());
            await connect(one);
            assert.equal(await ctx.vaultCount(), 1);
            one = await replace(
              one,
              definition("Auth removed", CONFIGURED_AUTH_ENDPOINT, "none"),
            );
            assert.equal(await ctx.vaultCount(), 0);
            assert.equal(one.state, "inactive");
            await remove(one);
            assert.deepEqual(await list(), []);
          },
        );
        await reset();
      },
      async "mcp.configured-auth-cancellation"() {
        for (const transition of [
          "disable",
          "label",
          "remove",
          "destination",
          "developer-off",
        ]) {
          await reset();
          let record = await add();
          await stage(`pending-consent-${transition}`, async () => {
            const baseline = tokens().length;
            const flow = await begin(record);
            if (transition === "disable") await enable(record, false);
            if (transition === "label")
              record = await replace(record, definition("Changed consent"));
            if (transition === "remove") await remove(record);
            if (transition === "destination")
              record = await replace(
                record,
                definition("Changed destination", CONFIGURED_ENDPOINT, "none"),
              );
            if (transition === "developer-off")
              await ctx.invoke("extension_set_developer_mode", {
                enabled: false,
              });
            const result = await flow.pending;
            assert.match(result.error ?? "", /account or access changed/);
            await assert.rejects(oauth().callback(flow.callback));
            assert.equal(tokens().length, baseline);
            assert.equal(await ctx.vaultCount(), 0);
            await ctx.invoke("extension_set_developer_mode", { enabled: true });
            if (transition === "remove") record = await add();
            if (transition === "destination")
              record = await replace(record, definition());
            await connect(record);
            await read(record);
            await disconnect(record);
            assert.equal((await state(record)).connected, false);
            assert.equal((await list())[0].state, "inactive");
          });
        }
        await reset();
      },
      async "mcp.configured-auth-refresh-cancellation"() {
        await reset();
        let record = await add();
        await stage("actual-expiry-refresh-and-restart", async () => {
          await connect(record, "A", { expiresIn: 2, refreshable: true });
          const issued = tokens().at(-1);
          await ctx.waitFor(
            "Actual configured issuer expiry",
            () => Date.now() >= issued.expiresAt,
          );
          await ctx.invoke("mcp_test_provider", {
            id: `configured-${record.id}`,
          });
          verifyMcpRefresh(
            oauth()
              .journal.filter((x) => x.phase === "refresh")
              .at(-1),
            "A",
            1,
          );
          const exchanged = tokens().length;
          await ctx.restartHost();
          await read(record);
          assert.equal(tokens().length, exchanged);
          assert.equal(await ctx.vaultCount(), 1);
        });
        for (const [lifecycle, reply, transition] of [
          ["stateless", "json", "disconnect"],
          ["legacy", "sse", "remove"],
        ]) {
          await stage(`held-${lifecycle}-${reply}-${transition}`, async () => {
            ctx.provider().configure({ lifecycle, reply, result: "held" });
            const baseline = calls().length,
              start = ctx.model().journal.length;
            const page = await ctx.request("mcp_configured_unknown");
            await ctx.activate(
              page.locator(".agc-confirm-actions .agc-action-btn"),
            );
            await ctx.waitFor(
              "Configured account reply held",
              () => ctx.provider().heldCalls === 1,
            );
            if (transition === "disconnect") await disconnect(record);
            else await remove(record);
            await ctx.waitFor(
              "Configured account cancellation stops",
              async () => !(await ctx.status()).agent.active,
            );
            await ctx.waitFor(
              "Configured account reply/session disposal",
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
            assert.equal(calls().length, baseline + 1);
            assert.equal(await ctx.vaultCount(), 0);
            ctx.provider().attemptLateReply();
            await ctx.closePanel();
            if (transition === "remove") record = await add();
            ctx.provider().configure({ result: "normal" });
            await connect(record);
            await read(record);
          });
        }
        await reset();
      },
    },
  };
}
