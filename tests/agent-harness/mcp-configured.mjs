// Real Tauri commands and real scoped disk; no transport or credentials are
// acquired by these metadata-only procedures. Runtime acceptance is separate.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { assertWithin } from "./support.mjs";

const definition = (
  name = "Harness custom MCP",
  url = "https://custom.example.com/mcp",
  type = "none",
) => JSON.stringify({ name, url, authentication: { type } });

export function configuredMcpHandlers(ctx) {
  const path = assertWithin(
    ctx.root,
    join(ctx.root, "data/mcp-connections.json"),
  );
  const list = () => ctx.invoke("mcp_connections_list");
  const add = (definitionJson = definition()) =>
    ctx.invoke("mcp_connection_import", { definitionJson });
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
  const disk = async () => JSON.parse(await readFile(path, "utf8"));
  let evidence = [];
  async function stage(name, operation) {
    try {
      const result = await operation();
      evidence.push({ stage: name, status: "Pass" });
      return result;
    } catch (error) {
      evidence.push({
        stage: name,
        status: "Fail",
        error: String(error.message ?? error),
      });
      throw new Error(`Configured MCP ${name}: ${error.message ?? error}`, {
        cause: error,
      });
    }
  }
  async function restart(operation) {
    const session = (await ctx.status()).hostSession;
    await ctx.restartHost(operation);
    assert.equal((await ctx.status()).hostSession, session + 1);
  }
  async function empty() {
    for (const record of await list()) await remove(record);
  }
  return {
    takeEvidence() {
      const value = evidence;
      evidence = [];
      return value;
    },
    handlers: {
      async "mcp.configured-ownership"() {
        await empty();
        let first, second;
        await stage("inactive-independent-imports", async () => {
          [first, second] = await Promise.all([add(), add()]);
          assert.notEqual(first.id, second.id);
          for (const view of [first, second]) {
            assert.match(view.id, /^[0-9a-f]{32}$/);
            assert.equal(view.revision, "1");
            assert.equal(view.state, "inactive");
            assert.deepEqual(Object.keys(view).sort(), [
              "authentication",
              "id",
              "name",
              "revision",
              "state",
              "url",
            ]);
          }
          const saved = await disk();
          assert.equal(
            new Set(saved.connections.map((x) => x.accountId)).size,
            2,
          );
          assert.equal((await ctx.status()).worker.count, 0);
          assert.ok(
            !(await ctx.invoke("mcp_provider_status")).some(
              (x) => x.id === first.id,
            ),
          );
          await assert.rejects(
            ctx.invoke("mcp_test_provider", { id: first.id }),
            /unknown MCP provider/i,
          );
        });
        await stage("noop-label-and-stale-revisions", async () => {
          const before = await readFile(path, "utf8");
          assert.deepEqual(await replace(first, definition()), first);
          assert.equal(await readFile(path, "utf8"), before);
          const account = (await disk()).connections.find(
            (x) => x.connectionId === first.id,
          ).accountId;
          const renamed = await replace(
            first,
            definition("Renamed custom MCP"),
          );
          assert.equal(renamed.revision, "2");
          assert.equal(
            (await disk()).connections.find((x) => x.connectionId === first.id)
              .accountId,
            account,
          );
          const snapshot = await readFile(path, "utf8");
          await assert.rejects(
            replace(first, definition("Stale")),
            /changed; reload/,
          );
          await assert.rejects(remove(first), /changed; reload/);
          for (const value of [
            "02",
            "+2",
            "2.0",
            "9007199254740993",
            "18446744073709551616",
          ])
            await assert.rejects(
              remove({ ...renamed, revision: value }),
              /changed; reload/,
            );
          assert.equal(await readFile(path, "utf8"), snapshot);
          first = renamed;
        });
        await stage("concurrent-edit-has-one-owner", async () => {
          const results = await Promise.allSettled([
            replace(first, definition("Concurrent A")),
            replace(first, definition("Concurrent B")),
          ]);
          assert.equal(
            results.filter((x) => x.status === "fulfilled").length,
            1,
          );
          assert.equal(
            results.filter((x) => x.status === "rejected").length,
            1,
          );
          assert.match(
            String(results.find((x) => x.status === "rejected").reason),
            /changed; reload/,
          );
          first = results.find((x) => x.status === "fulfilled").value;
          assert.equal(first.revision, "3");
          assert.deepEqual(
            (await list()).find((x) => x.id === first.id),
            first,
          );
        });
        await stage("destination-and-auth-account-rotation", async () => {
          let prior = (await disk()).connections.find(
            (x) => x.connectionId === first.id,
          ).accountId;
          first = await replace(
            first,
            definition("Moved", "https://other.example.com/mcp"),
          );
          let next = (await disk()).connections.find(
            (x) => x.connectionId === first.id,
          ).accountId;
          assert.notEqual(next, prior);
          prior = next;
          first = await replace(
            first,
            definition("Moved", "https://other.example.com/mcp", "oauth"),
          );
          next = (await disk()).connections.find(
            (x) => x.connectionId === first.id,
          ).accountId;
          assert.notEqual(next, prior);
          assert.equal(first.authentication, "oauth");
          await restart();
          assert.deepEqual(
            (await list()).find((x) => x.id === first.id),
            first,
          );
        });
        await stage("remove-readd-and-restart", async () => {
          await remove(first);
          await remove(second);
          assert.deepEqual(await list(), []);
          await assert.rejects(remove(first), /no longer available/);
          const fresh = await add();
          assert.notEqual(fresh.id, first.id);
          assert.notEqual(fresh.id, second.id);
          await remove(fresh);
          await restart();
          assert.deepEqual(await list(), []);
        });
      },
      async "mcp.configured-access"() {
        await empty();
        const saved = await add();
        const before = await readFile(path, "utf8");
        await stage("strict-nonsecret-import-refusals", async () => {
          const input = JSON.parse(definition());
          for (const bad of [
            { ...input, id: "linear" },
            { ...input, enabled: true },
            { ...input, command: "powershell" },
            { ...input, headers: { Authorization: "Bearer PRIVATE" } },
            {
              ...input,
              authentication: { type: "oauth", clientSecret: "PRIVATE" },
            },
            { ...input, url: "https://user:pass@custom.example.com/mcp" },
            { ...input, url: "https://127.0.0.1/mcp" },
            { ...input, url: "https://custom.example.com/mcp?token=PRIVATE" },
            { ...input, name: "x".repeat(4097) },
            [],
            null,
          ]) {
            await assert.rejects(add(JSON.stringify(bad)));
            await assert.rejects(replace(saved, JSON.stringify(bad)));
          }
          assert.equal(await readFile(path, "utf8"), before);
        });
        const calls = [
          [
            "mcp_connection_set_client_credentials",
            {
              id: saved.id,
              expectedRevision: saved.revision,
              clientId: "owned-client",
              clientSecret: "",
            },
          ],
          [
            "mcp_connection_clear_client_credentials",
            { id: saved.id, expectedRevision: saved.revision },
          ],
          [
            "mcp_connection_connect",
            { id: saved.id, expectedRevision: saved.revision },
          ],
          [
            "mcp_connection_status",
            { id: saved.id, expectedRevision: saved.revision },
          ],
          [
            "mcp_connection_disconnect",
            { id: saved.id, expectedRevision: saved.revision },
          ],
          [
            "mcp_connection_set_enabled",
            { id: saved.id, expectedRevision: saved.revision, enabled: true },
          ],
          ["mcp_connections_list", {}],
          ["mcp_connection_import", { definitionJson: definition() }],
          [
            "mcp_connection_replace",
            {
              id: saved.id,
              expectedRevision: saved.revision,
              definitionJson: definition("Changed"),
            },
          ],
          [
            "mcp_connection_remove",
            { id: saved.id, expectedRevision: saved.revision },
          ],
        ];
        await stage("developer-mode-off-refuses-all-commands", async () => {
          await ctx.invoke("extension_set_developer_mode", { enabled: false });
          try {
            for (const [command, args] of calls)
              await assert.rejects(
                ctx.invoke(command, args),
                /require Extension Developer Mode/,
              );
            assert.equal(await readFile(path, "utf8"), before);
          } finally {
            await ctx.invoke("extension_set_developer_mode", { enabled: true });
          }
        });
        await stage("non-main-window-refuses-all-commands", async () => {
          await ctx.fixture("load");
          const page = await ctx.request();
          try {
            for (const [command, args] of calls)
              await assert.rejects(
                page.evaluate(
                  ({ command, args }) =>
                    window.__TAURI_INTERNALS__.invoke(command, args),
                  { command, args },
                ),
                /available only from Grain settings/,
              );
            assert.equal(await readFile(path, "utf8"), before);
          } finally {
            await ctx.closePanel();
            await ctx.fixture("unload");
          }
        });
        await remove(saved);
      },
      async "mcp.configured-storage"() {
        await empty();
        const saved = await add();
        const valid = await readFile(path, "utf8");
        await stage("external-change-refuses-publication", async () => {
          const changed = JSON.parse(valid);
          changed.connections[0].definition.name = "External editor";
          const bytes = JSON.stringify(changed);
          await writeFile(path, bytes);
          try {
            await assert.rejects(add(), /changed; reload/);
            await assert.rejects(
              replace(saved, definition("Overwrite")),
              /changed; reload/,
            );
            await assert.rejects(remove(saved), /changed; reload/);
            assert.equal(await readFile(path, "utf8"), bytes);
          } finally {
            await writeFile(path, valid);
          }
        });
        await stage("corrupt-file-preserved-and-repair-restart", async () => {
          const broken = '{"schema":1,"connections":[';
          await restart(() => writeFile(path, broken));
          try {
            await assert.rejects(
              list(),
              /registry is invalid; its bytes are preserved/,
            );
            await assert.rejects(
              add(),
              /registry is invalid; its bytes are preserved/,
            );
            assert.equal(await readFile(path, "utf8"), broken);
          } finally {
            await restart(() => writeFile(path, valid));
          }
          assert.deepEqual(await list(), [saved]);
        });
        await stage("exact-large-revision-and-exhaustion", async () => {
          const atRevision = (value) => {
            const bytes = valid.replace(
              /"revision":\s*1([,}])/,
              `"revision":${value}$1`,
            );
            assert.notEqual(
              bytes,
              valid,
              "Owned revision fixture did not change",
            );
            return bytes;
          };
          try {
            await restart(() =>
              writeFile(path, atRevision("9007199254740993")),
            );
            const large = (await list())[0];
            assert.equal(large.revision, "9007199254740993");
            assert.equal(
              (await replace(large, definition("Large revision"))).revision,
              "9007199254740994",
            );
            await restart(() =>
              writeFile(path, atRevision("18446744073709551615")),
            );
            const maximum = (await list())[0];
            assert.equal(maximum.revision, "18446744073709551615");
            const before = await readFile(path, "utf8");
            await assert.rejects(
              replace(maximum, definition("Exhausted")),
              /revision is exhausted/,
            );
            assert.equal(await readFile(path, "utf8"), before);
          } finally {
            await restart(() => writeFile(path, valid));
          }
          assert.deepEqual(await list(), [saved]);
        });
        await stage("bounded-import-count", async () => {
          for (let i = 1; i < 32; i++) await add(definition(`Connection ${i}`));
          const before = await readFile(path, "utf8");
          await assert.rejects(add(), /exceeds its supported limit/);
          assert.equal(await readFile(path, "utf8"), before);
          assert.equal((await list()).length, 32);
          if (ctx.fault !== "missing-configured-cleanup") await empty();
          assert.deepEqual(
            await list(),
            [],
            "Configured cleanup left a saved connection",
          );
        });
      },
    },
  };
}
