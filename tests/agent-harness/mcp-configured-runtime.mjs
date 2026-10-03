// Direct host-managed records through the SAME production Agent/MCP runtime.
// The exact reserved descriptor maps only to the existing owned TLS peer in
// agent-harness builds. No extension package or alternate execution path.
import assert from "node:assert/strict";

export const CONFIGURED_ENDPOINT =
  "https://configured.grain-harness.example/mcp";
const definition = (
  name = "Configured harness",
  url = CONFIGURED_ENDPOINT,
  type = "none",
) => JSON.stringify({ name, url, authentication: { type } });

export function configuredRuntimeHandlers(ctx) {
  let evidence = [];
  const list = () => ctx.invoke("mcp_connections_list");
  const add = () =>
    ctx.invoke("mcp_connection_import", { definitionJson: definition() });
  const enable = (record, enabled = true) =>
    ctx.invoke("mcp_connection_set_enabled", {
      id: record.id,
      expectedRevision: record.revision,
      enabled,
    });
  const replace = (record, definitionJson) =>
    ctx.invoke("mcp_connection_replace", {
      id: record.id,
      expectedRevision: record.revision,
      definitionJson,
    });
  const remove = (record) =>
    ctx.invoke("mcp_connection_remove", {
      id: record.id,
      expectedRevision: record.revision,
    });
  const calls = () =>
    ctx.provider().journal.filter((x) => x.method === "tools/call").length;
  const providerId = (record) => `configured-${record.id}`;
  async function stage(name, operation) {
    try {
      await operation();
      evidence.push({
        stage: name,
        status: "Pass",
        activeSessions: ctx.provider().activeSessions,
      });
    } catch (error) {
      evidence.push({
        stage: name,
        status: "Fail",
        error: String(error.message ?? error),
      });
      throw new Error(`Configured runtime ${name}: ${error.message ?? error}`, {
        cause: error,
      });
    }
  }
  async function reset() {
    await ctx.closePanel();
    await ctx.invoke("extension_set_developer_mode", { enabled: true });
    for (const record of await list()) await remove(record);
    ctx.provider().configure({
      lifecycle: "stateless",
      reply: "json",
      result: "normal",
      catalog: "mixed",
      revision: "one",
    });
  }
  async function read(
    record,
    instruction = "mcp_configured_read",
    mutation = null,
    expected = "mcpVerified",
  ) {
    const before = calls(),
      start = ctx.model().journal.length;
    const page = await ctx.request(instruction);
    assert.equal(calls(), before, "Configured tool escaped approval");
    if (mutation) await mutation();
    await ctx.activate(page.locator(".agc-confirm-actions .agc-action-btn"));
    await ctx.waitFor(
      "Configured Agent completes",
      async () => !(await ctx.status()).agent.active,
    );
    const receipts = ctx
      .model()
      .journal.slice(start)
      .filter((x) => x[expected]);
    assert.equal(
      receipts.length,
      1,
      "Missing/duplicated configured outcome receipt",
    );
    assert.equal(
      receipts[0].configuredOwner,
      `mcp.${providerId(record)}`,
      "Configured owner receipt changed",
    );
    assert.equal(
      calls(),
      before + (expected === "mcpRefused" ? 0 : 1),
      "Configured call was absent, unapproved or replayed",
    );
    await ctx.waitFor(
      "Configured protocol session disposed",
      () => ctx.provider().activeSessions === 0,
    );
    assert.equal(
      (await ctx.status()).worker.count,
      0,
      "Custom MCP started a native worker",
    );
    await ctx.closePanel();
  }
  async function restarted() {
    const session = (await ctx.status()).hostSession;
    await ctx.restartHost();
    assert.equal((await ctx.status()).hostSession, session + 1);
  }
  return {
    takeEvidence() {
      const result = evidence;
      evidence = [];
      return result;
    },
    handlers: {
      async "mcp.configured-execution"() {
        await reset();
        let record = await add();
        await stage("inactive-until-exact-enable", async () => {
          assert.equal(record.state, "inactive");
          await assert.rejects(
            ctx.invoke("mcp_test_provider", { id: providerId(record) }),
            /disabled/,
          );
          assert.equal(calls(), 0);
          await enable(record);
          assert.equal((await list())[0].state, "enabled");
          const tools = await ctx.invoke("mcp_test_provider", {
            id: providerId(record),
          });
          assert.equal(tools.provider_id, providerId(record));
          assert.equal(tools.tool_count, 2);
        });
        for (const lifecycle of ["stateless", "legacy"])
          for (const reply of ["json", "sse"]) {
            await stage(
              `${lifecycle}-${reply}-actual-approved-read`,
              async () => {
                ctx.provider().configure({ lifecycle, reply });
                await read(record);
              },
            );
          }
        await stage("restart-and-host-enable-isolation", async () => {
          await restarted();
          assert.equal((await list())[0].state, "enabled");
          await read(record);
          await enable(record, false);
          assert.equal((await list())[0].state, "inactive");
          await assert.rejects(
            ctx.invoke("mcp_test_provider", { id: providerId(record) }),
            /disabled/,
          );
          await enable(record);
          const stale = record;
          record = await replace(
            record,
            definition("Renamed configured harness"),
          );
          assert.equal(record.state, "enabled");
          await assert.rejects(enable(stale), /changed; reload/);
          await read(record);
        });
        await stage("destination-and-oauth-edits-stay-inactive", async () => {
          record = await replace(
            record,
            definition("Changed", "https://other.example.com/mcp"),
          );
          assert.equal(record.state, "inactive");
          await assert.rejects(
            ctx.invoke("mcp_test_provider", { id: providerId(record) }),
            /disabled/,
          );
          record = await replace(
            record,
            definition(
              "OAuth configured harness",
              CONFIGURED_ENDPOINT,
              "oauth",
            ),
          );
          await assert.rejects(enable(record), /remains inactive/);
          await assert.rejects(
            ctx.invoke("mcp_connect_provider", { id: providerId(record) }),
            /unknown MCP provider/,
          );
          await assert.rejects(
            ctx.invoke("mcp_test_provider", { id: providerId(record) }),
            /disabled/,
          );
          assert.equal((await list())[0].state, "inactive");
          await remove(record);
          assert.deepEqual(await list(), []);
        });
      },
      async "mcp.configured-approval-ownership"() {
        for (const transition of [
          "noop",
          "peer-remove",
          "label",
          "disable",
          "remove",
          "destination",
          "oauth",
          "developer-off",
        ]) {
          await reset();
          const records = [await add(), await add()].sort((a, b) =>
            a.id.localeCompare(b.id),
          );
          const [record, peer] = records;
          await enable(record);
          await enable(peer);
          await stage(`pending-${transition}`, async () => {
            const mutation = async () => {
              if (transition === "noop") await replace(record, definition());
              if (transition === "peer-remove") await remove(peer);
              if (transition === "label")
                await replace(record, definition("Changed label"));
              if (transition === "disable") await enable(record, false);
              if (transition === "remove") await remove(record);
              if (transition === "destination")
                await replace(
                  record,
                  definition(
                    "New destination",
                    "https://other.example.com/mcp",
                  ),
                );
              if (transition === "oauth")
                await replace(
                  record,
                  definition("OAuth", CONFIGURED_ENDPOINT, "oauth"),
                );
              if (transition === "developer-off")
                await ctx.invoke("extension_set_developer_mode", {
                  enabled: false,
                });
            };
            await read(
              record,
              "mcp_configured_read",
              mutation,
              ["noop", "peer-remove"].includes(transition)
                ? "mcpVerified"
                : "mcpRefused",
            );
          });
        }
        await reset();
      },
      async "mcp.configured-active-cancellation"() {
        for (const [lifecycle, reply, transition] of [
          ["stateless", "json", "disable"],
          ["legacy", "json", "remove"],
          ["stateless", "sse", "label"],
          ["legacy", "sse", "developer-off"],
        ]) {
          await reset();
          let record = await add();
          await enable(record);
          await stage(`held-${lifecycle}-${reply}-${transition}`, async () => {
            ctx.provider().configure({ lifecycle, reply, result: "held" });
            const before = calls(),
              start = ctx.model().journal.length;
            const page = await ctx.request("mcp_configured_unknown");
            assert.equal(calls(), before);
            await ctx.activate(
              page.locator(".agc-confirm-actions .agc-action-btn"),
            );
            await ctx.waitFor(
              "Actual configured reply held",
              () => ctx.provider().heldCalls === 1,
            );
            // An unchanged metadata save must not cancel active work.
            await replace(record, definition());
            assert.equal(ctx.provider().heldCalls, 1);
            assert.equal((await ctx.status()).agent.active, true);
            if (transition === "disable") await enable(record, false);
            if (transition === "remove") await remove(record);
            if (transition === "label")
              record = await replace(record, definition("After cancellation"));
            if (transition === "developer-off")
              await ctx.invoke("extension_set_developer_mode", {
                enabled: false,
              });
            await ctx.waitFor(
              "Configured cancelled Agent stops",
              async () => !(await ctx.status()).agent.active,
            );
            await ctx.waitFor(
              "Held connection and session disposed",
              () =>
                ctx.provider().heldCalls === 0 &&
                ctx.provider().activeSessions === 0,
            );
            const receipts = ctx
              .model()
              .journal.slice(start)
              .filter((x) => x.mcpUnknownVerified);
            assert.equal(
              receipts.length,
              1,
              "Wrong configured post-dispatch classification",
            );
            assert.equal(
              receipts[0].configuredOwner,
              `mcp.${providerId(record)}`,
            );
            assert.equal(
              calls(),
              before + 1,
              "Cancelled configured call replayed",
            );
            ctx.provider().attemptLateReply();
            await ctx.closePanel();
            await ctx.invoke("extension_set_developer_mode", { enabled: true });
            if (transition === "remove") record = await add();
            await enable(record);
            ctx.provider().configure({ result: "normal" });
            await read(record);
          });
        }
        await reset();
      },
    },
  };
}
